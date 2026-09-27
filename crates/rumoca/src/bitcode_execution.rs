//! CLI boundaries for public, writable execution artifacts.
mod overrides;
use anyhow::{Context, Result, bail};
use clap::Args;
use rumoca_bitcode::schema::{self, RbcModel};
use rumoca_bitcode::{Encoding, RbcFile};
use rumoca_ir_solve as solve;
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};

#[derive(Debug, Args)]
/// No observation list. Observations are demanded by the program: a
/// `snapshot.value` names a trace point, and the numerical program is derived
/// from exactly the trace points the program references (D2).
pub struct LowerArgs {
    pub input: PathBuf,
    #[arg(short, long)]
    pub output: PathBuf,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    pub input: PathBuf,
    #[arg(long, default_value = "require")]
    pub execution: String,
    #[arg(long, default_value = "rk45")]
    pub backend: String,
    #[arg(long, default_value_t = 0.0)]
    pub start: f64,
    #[arg(long, default_value_t = 5.0)]
    pub stop: f64,
    #[arg(long, default_value_t = 0.1)]
    pub publish_interval: f64,
    #[arg(long, default_value_t = 1e-9)]
    pub rtol: f64,
    #[arg(long, default_value_t = 1e-11)]
    pub atol: f64,
    #[arg(long)]
    pub trace_root: PathBuf,
    /// Optional ordinary simulation result, for independent neutrality tests.
    #[arg(long)]
    pub result: Option<PathBuf>,
    /// Set a retained tunable scalar parameter without re-lowering.
    #[arg(long = "param")]
    pub parameters: Vec<String>,
    /// Override a scalar state start (refuses explicit initialization owners).
    #[arg(long = "initial")]
    pub initial_values: Vec<String>,
    /// Capture reached scalar domain violations with the native interpreter.
    #[arg(long)]
    pub domain_diagnostics: bool,
}

/// Canonical encoding of what a program depends on, for `dependency_digest`.
///
/// Hand-written, never `serde_json::to_vec`: a serializer's field order and
/// its `skip_serializing_if` decisions are not part of the model's meaning, and
/// v1 hashed both. Each record is length-prefixed so no two different inputs
/// can encode to the same bytes by concatenation.
fn encode(out: &mut Vec<u8>, part: &[u8]) {
    out.extend_from_slice(&(part.len() as u64).to_le_bytes());
    out.extend_from_slice(part);
}

/// The component this variable belongs to, plus its own name, encoded as two
/// length-prefixed records.
///
/// The two parts stay separate rather than being joined into one dotted
/// string: a joined string invites a reader to split it again, and
/// SPEC_0007's no-tokenizing rule is what that would violate. The hash
/// consumes them as bytes and never re-parses.
fn encode_component_path(out: &mut Vec<u8>, model: &RbcModel, variable: &schema::RbcVariable) {
    let component = variable
        .component
        .and_then(|id| model.components.iter().find(|c| c.id == id));
    match component {
        Some(component) => encode(out, component.path.as_bytes()),
        None => encode(out, b""),
    }
    encode(out, variable.name.as_bytes());
}

/// Hash the identities this program actually depends on.
///
/// v1 hashed the entire serialized model, so adding a trace point or rewriting
/// an unrelated residual invalidated a logging program that referenced
/// neither. This covers exactly the referenced trace points, the lowering
/// profile's expansion, and the two contract versions.
pub fn dependency_digest(
    model: &RbcModel,
    lowering: rumoca_ir_solve::execution::LoweringProfile,
    referenced: &std::collections::BTreeSet<u32>,
) -> Result<String> {
    let mut bytes = Vec::new();
    encode(&mut bytes, b"rumoca.execution.dependency.v1");
    encode(
        &mut bytes,
        &u32::from(rumoca_bitcode::schema::RBC_VERSION).to_le_bytes(),
    );
    encode(
        &mut bytes,
        &rumoca_ir_solve::execution::EXECUTION_VERSION.to_le_bytes(),
    );
    let expansion = lowering.expansion();
    encode(&mut bytes, expansion.name.as_bytes());
    encode(&mut bytes, expansion.backend_family.as_bytes());
    encode(&mut bytes, &[u8::from(expansion.scalar_only)]);
    // Referenced trace points, in id order so the set's own ordering cannot
    // change the digest.
    for id in referenced {
        let point = model
            .trace_points
            .iter()
            .find(|p| p.id.0 == *id)
            .with_context(|| format!("trace point {id} is referenced but not declared"))?;
        let variable = model
            .variables
            .iter()
            .find(|v| v.id == point.variable)
            .with_context(|| format!("trace point {id} names an unknown variable"))?;
        encode(&mut bytes, &id.to_le_bytes());
        encode_component_path(&mut bytes, model, variable);
        encode(&mut bytes, &variable.value_type.0.to_le_bytes());
        encode(&mut bytes, format!("{:?}", variable.causality).as_bytes());
        encode(
            &mut bytes,
            variable.unit.as_deref().unwrap_or("").as_bytes(),
        );
    }
    Ok(format!("sha1:{:x}", Sha1::digest(&bytes)))
}

/// Trace points a program references, through instructions and sink members.
pub fn referenced_trace_points(
    execution: &rumoca_ir_solve::execution::ExecutionArtifact,
) -> std::collections::BTreeSet<u32> {
    use rumoca_ir_solve::execution::Instruction;
    let mut found = std::collections::BTreeSet::new();
    for function in execution.program.functions.values() {
        let mut pending: Vec<&Instruction> = function.body.iter().collect();
        while let Some(instruction) = pending.pop() {
            match instruction {
                Instruction::Value { trace_point, .. } => {
                    found.insert(*trace_point);
                }
                Instruction::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    pending.extend(then_body.iter());
                    pending.extend(else_body.iter());
                }
                _ => {}
            }
        }
    }
    for sink in &execution.program.sinks {
        for member in &sink.metadata.members {
            found.insert(member.trace_point);
        }
    }
    found
}

pub fn check(file: &RbcFile) -> Result<()> {
    let execution = file
        .execution
        .as_ref()
        .context("artifact has no execution program; explicitly lower first")?;
    let referenced = referenced_trace_points(execution);
    if execution.dependency_digest
        != dependency_digest(&file.model, execution.lowering, &referenced)?
    {
        bail!(
            "stale execution: an identity this program depends on changed; explicitly re-lower and replay compatible execution passes"
        );
    }
    let mut options = rumoca_bitcode::validate::ValidateOptions::default();
    options.reject_unsupported = true;
    rumoca_bitcode::validate(&file.model, &options).map_err(|errors| {
        anyhow::anyhow!(
            "invalid equation artifact: {}",
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        )
    })?;
    // Analysis 5: resolve the program's references against the model. It runs
    // after the model itself validates, because an unresolvable reference into
    // an invalid model is the model's error to report first.
    rumoca_bitcode::validate::validate_execution_references(&file.model, execution).map_err(
        |errors| {
            anyhow::anyhow!(
                "program references the model incorrectly: {}",
                errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        },
    )?;
    let numerical = derive(file)?;
    rumoca_sim::execution::check(execution, &numerical).map_err(anyhow::Error::msg)
}

/// Derive the numerical program for an artifact from its equation IR and the
/// profile it recorded.
///
/// This is the whole of D1: the program is rebuilt here at load rather than
/// read from the file, so no solver's choices are serialized. Requested
/// observations come from the trace points the program references -- the
/// program says what it needs (D2).
pub fn derive(file: &RbcFile) -> Result<solve::execution::NumericalProgram> {
    let dae = rumoca_bitcode::import(file)?;
    let model = rumoca_sim::lower_dae_for_simulation(&dae, &Default::default())?;
    let referenced = match file.execution.as_ref() {
        Some(execution) => referenced_trace_points(execution),
        None => std::collections::BTreeSet::new(),
    };
    let requested = referenced
        .iter()
        .map(|id| {
            let point = file
                .model
                .trace_points
                .iter()
                .find(|p| p.id.0 == *id)
                .with_context(|| format!("trace point {id} is referenced but not declared"))?;
            let variable = file
                .model
                .variables
                .iter()
                .find(|v| v.id == point.variable)
                .with_context(|| format!("trace point {id} names an unknown variable"))?;
            Ok((*id, variable.id.0, variable.name.clone()))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut numerical =
        rumoca_sim::execution::lower(&model, &requested).map_err(anyhow::Error::msg)?;
    // The equation IR owns causality, unit and identity; the derived program
    // borrows them here rather than the wire carrying a second copy.
    for storage in &mut numerical.storage {
        if let Some(variable) = file.model.variables.iter().find(|v| v.name == storage.name) {
            storage.variable_id = Some(variable.id.0);
            storage.causality = serde_json::to_value(variable.causality)?
                .as_str()
                .context("invalid causality")?
                .into();
            storage.unit = variable.unit.clone();
        }
    }
    // The equation IR owns the physical role and the connector structure; the
    // derived program borrows both so an emitted manifest need not infer a
    // flow from a member's name, nor a path by splitting one.
    for observation in &mut numerical.observations {
        let Some(point) = file
            .model
            .trace_points
            .iter()
            .find(|p| p.id.0 == observation.trace_point)
        else {
            continue;
        };
        if let Some(quantity) = point.quantity {
            observation.quantity = Some(
                serde_json::to_value(quantity)?
                    .as_str()
                    .context("invalid quantity kind")?
                    .into(),
            );
        }
        for connector in &file.model.connectors {
            if let Some(binding) = connector
                .members
                .iter()
                .find(|member| member.variable == point.variable)
            {
                observation.owner = Some(connector.path.clone());
                observation.member = Some(binding.name.clone());
                break;
            }
        }
    }
    Ok(numerical)
}

pub fn lower(args: LowerArgs) -> Result<()> {
    let (mut file, _) = rumoca_bitcode::read_file(&args.input)?;
    let lowering = solve::execution::LoweringProfile::default();
    // Idempotent on an artifact that already carries a program with a matching
    // profile. v1 refused any artifact with a program at all, which made
    // re-lowering after an equation edit impossible without discarding the
    // authored program by hand.
    if let Some(existing) = file.execution.as_ref()
        && existing.lowering != lowering
    {
        bail!(
            "artifact records lowering profile {:?}; re-lowering to {lowering:?} would change the derivation",
            existing.lowering
        );
    }
    // A fresh artifact gets the three lifecycle functions, empty. They are the
    // program's entry points, so a pass has somewhere to append to.
    let program = file.execution.as_ref().map_or_else(
        || solve::execution::HostProgram {
            functions: ["run_start", "publish", "run_finish"]
                .into_iter()
                .map(|name| (name.to_string(), solve::execution::Function::default()))
                .collect(),
            sinks: vec![],
        },
        |existing| existing.program.clone(),
    );
    let revision = file.execution.as_ref().map_or(0, |e| e.revision);
    let passes = file
        .execution
        .as_ref()
        .map_or_else(Vec::new, |e| e.passes.clone());
    file.execution = Some(solve::execution::ExecutionArtifact {
        version: solve::execution::EXECUTION_VERSION,
        lowering,
        dependency_digest: String::new(),
        revision,
        passes,
        program,
    });
    // References resolve before the digest names them, so an unknown trace
    // point is reported as EX2-040 rather than as digest context.
    rumoca_bitcode::validate::validate_execution_references(
        &file.model,
        file.execution
            .as_ref()
            .expect("execution was just assigned"),
    )
    .map_err(|errors| {
        anyhow::anyhow!(
            "{}",
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        )
    })?;
    // The digest covers what the program references, so it is computed after
    // the program is in place.
    let referenced = referenced_trace_points(
        file.execution
            .as_ref()
            .expect("execution was just assigned"),
    );
    let digest = dependency_digest(&file.model, lowering, &referenced)?;
    if let Some(execution) = file.execution.as_mut() {
        execution.dependency_digest = digest;
    }
    check(&file)?;
    rumoca_bitcode::write_file(&args.output, &file, Encoding::Json)?;
    Ok(())
}

pub fn check_path(path: &Path) -> Result<()> {
    let (file, _) = rumoca_bitcode::read_file(path)?;
    check(&file)?;
    println!("valid execution v1; fresh derivation; native RK45 capable");
    Ok(())
}

pub fn run(args: RunArgs) -> Result<()> {
    if args.execution != "require" || args.backend != "rk45" {
        bail!(
            "unsupported execution mode/backend: public execution v1 requires --execution=require --backend=rk45"
        );
    }
    if !args.start.is_finite()
        || !args.stop.is_finite()
        || args.stop < args.start
        || !args.publish_interval.is_finite()
        || args.publish_interval <= 0.0
        || !args.rtol.is_finite()
        || args.rtol <= 0.0
        || !args.atol.is_finite()
        || args.atol <= 0.0
    {
        bail!("invalid simulation interval/tolerances");
    }
    let (file, _) = rumoca_bitcode::read_file(&args.input)?;
    check(&file)?;
    // Derive once, then apply overrides to the derived program in memory.
    // Nothing is written back: an override is run-local by construction now,
    // because there is no serialized program for it to persist into.
    let mut numerical = derive(&file)?;
    overrides::apply(
        &file.model,
        &mut numerical,
        &args.parameters,
        &args.initial_values,
    )?;
    let options = rumoca_sim::SimOptions {
        t_start: args.start,
        t_end: args.stop,
        dt: Some(args.publish_interval),
        rtol: args.rtol,
        atol: args.atol,
        solver_mode: rumoca_sim::SimSolverMode::RkLike,
        ..Default::default()
    };
    let runner = if args.domain_diagnostics {
        rumoca_sim::execution::run_with_domain_diagnostics
    } else {
        rumoca_sim::execution::run
    };
    let result = runner(
        file.execution.as_ref().context("missing executable")?,
        &numerical,
        &options,
        &args.trace_root,
    )
    .map_err(|message| {
        anyhow::anyhow!(serde_json::json!({"kind": "execution-failure", "detail": message}))
    })?;
    if let Some(path) = args.result {
        std::fs::write(
            path,
            serde_json::to_vec(
                &serde_json::json!({"times": result.times, "names": result.names, "data": result.data}),
            )?,
        )?;
    }
    println!(
        "executed saved program; {} published times",
        result.times.len()
    );
    Ok(())
}
