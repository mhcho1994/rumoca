//! `rumoca compile --emit-bitcode`, `rumoca bitcode ...`, and
//! `rumoca compile-bitcode`.
//!
//! These three commands are the external compiler interface: a model goes out
//! as `.rbc`, an arbitrary tool in any language edits it, and it comes back in
//! through validation and checked reconstruction.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand, ValueEnum};
use rumoca_bitcode::schema::RbcModel;
use rumoca_bitcode::{Encoding, ExportOptions};

use crate::compiler::CompilationResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum BitcodeFormat {
    /// Compact binary (default).
    #[default]
    Cbor,
    /// Human-readable, same schema.
    Json,
}

impl From<BitcodeFormat> for Encoding {
    fn from(format: BitcodeFormat) -> Self {
        match format {
            BitcodeFormat::Cbor => Encoding::Cbor,
            BitcodeFormat::Json => Encoding::Json,
        }
    }
}

#[derive(Args, Debug)]
pub struct BitcodeArgs {
    #[command(subcommand)]
    pub command: BitcodeCommand,
}

#[derive(Subcommand, Debug)]
pub enum BitcodeCommand {
    /// Link named equation modules into one artifact (no automatic wiring).
    Link(crate::bitcode_link::LinkArgs),
    /// Lower equations into editable public Solve programs.
    LowerExecution(crate::bitcode_execution::LowerArgs),
    /// Validate executable structure, freshness and backend capabilities.
    CheckExecution(BitcodeInspectArgs),
    /// Execute a saved program without re-lowering equations.
    Run(crate::bitcode_execution::RunArgs),
    /// Print a one-screen summary of an artifact.
    Inspect(BitcodeInspectArgs),
    /// Print an artifact as JSON, whatever encoding it used on disk.
    Dump(BitcodeDumpArgs),
    /// Validate an artifact without rebuilding a model from it.
    Check(BitcodeCheckArgs),
    /// Re-encode an artifact, e.g. binary to JSON and back.
    Convert(BitcodeConvertArgs),
    /// Prove an artifact round-trips: import it, re-export, and compare.
    RoundTrip(BitcodeRoundTripArgs),
    /// Print an artifact as a readable listing, resolving the expression graph.
    Disasm(BitcodeDisasmArgs),
    /// Write the parseable textual IR — the `.ll` to the `.rbc`.
    EmitText(BitcodeEmitTextArgs),
    /// Assemble the textual IR back into a binary artifact.
    Assemble(BitcodeAssembleArgs),
}

#[derive(Args, Debug)]
pub struct BitcodeEmitTextArgs {
    /// The `.rbc` artifact.
    pub input: PathBuf,
    /// Where to write the text. Standard output when omitted.
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Include each source file's full text.
    ///
    /// Off by default: it is most of the artifact's bytes and none of its
    /// semantics. Required for a byte-exact assemble back.
    #[arg(long)]
    pub sources: bool,
}

#[derive(Args, Debug)]
pub struct BitcodeAssembleArgs {
    /// The textual IR.
    pub input: PathBuf,
    /// Where to write the artifact.
    #[arg(short, long)]
    pub output: PathBuf,
    /// Encoding for the result.
    #[arg(long, value_enum, default_value_t = BitcodeFormat::Cbor)]
    pub format: BitcodeFormat,
}

#[derive(Args, Debug)]
pub struct BitcodeDisasmArgs {
    /// The `.rbc` artifact.
    pub input: PathBuf,
    /// Also print the flat expression table, the artifact's constant pool.
    #[arg(long)]
    pub expressions: bool,
    /// Annotate each equation with the source span it came from.
    #[arg(long)]
    pub provenance: bool,
    /// Print expression ids alongside the rendered form.
    #[arg(long)]
    pub ids: bool,
}

#[derive(Args, Debug)]
pub struct BitcodeInspectArgs {
    /// The `.rbc` artifact.
    pub input: PathBuf,
}

#[derive(Args, Debug)]
pub struct BitcodeDumpArgs {
    pub input: PathBuf,
    /// Write to a file instead of standard output.
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct BitcodeCheckArgs {
    pub input: PathBuf,
    /// Also reject nodes the schema could not represent.
    ///
    /// Plain `check` proves the artifact is internally consistent, which an
    /// artifact full of `Unsupported` nodes still is. This is the question a
    /// consumer actually has: can this be rebuilt into a model?
    #[arg(long)]
    pub strict: bool,
    /// Require complete scalar-port contracts and verified connection equations.
    /// Legacy connection annotations without port declarations are rejected.
    #[arg(long)]
    pub connections: bool,
}

#[derive(Args, Debug)]
pub struct BitcodeConvertArgs {
    pub input: PathBuf,
    #[arg(short, long)]
    pub output: PathBuf,
    #[arg(long, value_enum, default_value_t = BitcodeFormat::Cbor)]
    pub format: BitcodeFormat,
}

#[derive(Args, Debug)]
pub struct BitcodeRoundTripArgs {
    pub input: PathBuf,
    /// Write the re-exported artifact here for inspection.
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

#[derive(Args, Debug)]
#[command(arg_required_else_help = true)]
pub struct CompileBitcodeArgs {
    /// The `.rbc` artifact to compile.
    pub input: PathBuf,
    /// Code-generation target, as for `rumoca compile --target`.
    #[arg(long, value_name = "TARGET")]
    pub target: Option<String>,
    /// Output path for the generated artifact.
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Print the reconstructed model's shape and exit.
    #[arg(long)]
    pub summary: bool,
    /// Simulate the reconstructed model and emit its trace points.
    #[arg(long)]
    pub simulate: bool,
    /// Simulation end time.
    #[arg(long, default_value_t = 1.0, requires = "simulate")]
    pub t_end: f64,
    /// Fixed output interval. Omitted lets the runtime choose.
    #[arg(long, requires = "simulate")]
    pub dt: Option<f64>,
    /// Write the trace as CSV here instead of a table on standard output.
    #[arg(long, value_name = "FILE", requires = "simulate")]
    pub trace_out: Option<PathBuf>,
    /// Override a tunable parameter: `--param name=value`. Repeatable.
    #[arg(long = "param", value_name = "NAME=VALUE")]
    pub params: Vec<String>,
    /// Write reached scalar-domain evidence using the native interpreter.
    #[arg(long, requires = "simulate")]
    pub domain_diagnostics: Option<PathBuf>,
    /// Report runtime violations (non-finite values, declared min/max breaches)
    /// as machine-readable JSON, and exit non-zero when any is found.
    #[arg(long, requires = "simulate")]
    pub check: bool,
}

/// Parse one `--param name=value` pair.
fn parse_param(text: &str) -> Result<(String, f64)> {
    let (name, value) = text
        .split_once('=')
        .ok_or_else(|| anyhow::anyhow!("--param expects NAME=VALUE, got `{text}`"))?;
    let parsed = value
        .trim()
        .parse::<f64>()
        .with_context(|| format!("--param {name}: `{value}` is not a number"))?;
    Ok((name.trim().to_string(), parsed))
}

/// Write a compiled model as bitcode.
pub fn emit_bitcode(
    result: &CompilationResult,
    model_name: &str,
    path: &Path,
    format: BitcodeFormat,
    embed_sources: bool,
) -> Result<()> {
    let mut options = ExportOptions::default();
    options.embed_sources = embed_sources;
    let file = rumoca_bitcode::export(&result.dae, Some(&result.flat), model_name, &options)
        .context("export bitcode")?;
    rumoca_bitcode::write_file(path, &file, format.into())
        .with_context(|| format!("write {}", path.display()))?;
    eprintln!(
        "wrote bitcode v{} ({}) to {}",
        file.bitcode_version,
        Encoding::from(format).as_str(),
        path.display()
    );
    Ok(())
}

pub fn run_bitcode(args: BitcodeArgs) -> Result<()> {
    match args.command {
        BitcodeCommand::Link(args) => crate::bitcode_link::run(args),
        BitcodeCommand::LowerExecution(args) => crate::bitcode_execution::lower(args),
        BitcodeCommand::CheckExecution(args) => crate::bitcode_execution::check_path(&args.input),
        BitcodeCommand::Run(args) => crate::bitcode_execution::run(args),
        BitcodeCommand::Inspect(args) => run_inspect(&args.input),
        BitcodeCommand::Dump(args) => run_dump(&args.input, args.output.as_deref()),
        BitcodeCommand::Check(args) => run_check(&args.input, args.strict, args.connections),
        BitcodeCommand::Convert(args) => run_convert(&args.input, &args.output, args.format),
        BitcodeCommand::RoundTrip(args) => run_round_trip(&args.input, args.output.as_deref()),
        BitcodeCommand::EmitText(args) => {
            run_emit_text(&args.input, args.output.as_deref(), args.sources)
        }
        BitcodeCommand::Assemble(args) => run_assemble(&args.input, &args.output, args.format),
        BitcodeCommand::Disasm(args) => crate::bitcode_disasm::run_disasm(
            &args.input,
            crate::bitcode_disasm::DisasmOptions {
                expressions: args.expressions,
                provenance: args.provenance,
                ids: args.ids,
            },
        ),
    }
}

fn run_inspect(path: &Path) -> Result<()> {
    let (file, encoding) = rumoca_bitcode::read_file(path).map_err(anyhow::Error::from)?;
    let model = &file.model;
    let summary = &model.summary;
    println!("{} ({})", path.display(), encoding.as_str());
    println!("  bitcode version  {}", file.bitcode_version);
    println!("  producer         {}", file.producer);
    println!("  model            {}", model.name);
    println!();
    println!("  variables        {}", summary.variables);
    println!("    states         {}", summary.states);
    println!("    parameters     {}", summary.parameters);
    println!("    constants      {}", summary.constants);
    println!("    inputs         {}", summary.inputs);
    println!("    outputs        {}", summary.outputs);
    println!("    algebraics     {}", summary.algebraics);
    println!("    discrete real  {}", summary.discrete_reals);
    println!("    discrete value {}", summary.discrete_values);
    println!("  equations        {}", summary.equations);
    println!("  initial eqs      {}", summary.initial_equations);
    println!("  expressions      {}", summary.expressions);
    println!("  relations        {}", summary.relations);
    println!("  conditions       {}", summary.conditions);
    println!("  roots            {}", summary.roots);
    println!("  event actions    {}", summary.events);
    println!("  time events      {}", summary.time_events);
    println!("  components       {}", summary.components);
    println!("  connections      {}", summary.connections);
    println!("  trace points     {}", summary.trace_points);
    if !model.connections.is_empty() {
        println!();
        println!("  connections:");
        for connection in &model.connections {
            println!(
                "    [{}] {} <-> {}  ({:?})",
                connection.id,
                connection.left_connector,
                connection.right_connector,
                connection.quantity
            );
        }
    }
    Ok(())
}

fn run_dump(path: &Path, output: Option<&Path>) -> Result<()> {
    // Dump shows the artifact as it is on disk, including any field a newer
    // producer added that this build does not interpret.
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let json = rumoca_bitcode::dump_json(&bytes).map_err(anyhow::Error::from)?;
    match output {
        Some(output) => {
            std::fs::write(output, json).with_context(|| format!("write {}", output.display()))?;
            eprintln!("wrote {}", output.display());
        }
        None => println!("{json}"),
    }
    Ok(())
}

/// Expression nodes carrying a form the schema could not represent.
fn count_unsupported_nodes(model: &rumoca_bitcode::schema::RbcModel) -> usize {
    model
        .expressions
        .iter()
        .filter(|expression| {
            matches!(
                expression.node,
                rumoca_bitcode::schema::RbcExprNode::Unsupported { .. }
            )
        })
        .count()
}

fn run_check(path: &Path, strict: bool, connections: bool) -> Result<()> {
    let (file, _) = rumoca_bitcode::read_file(path).map_err(anyhow::Error::from)?;
    if file.execution.is_some() {
        crate::bitcode_execution::check(&file)?;
    }
    // `Unsupported` is a faithful carrier, not a corruption: the compiler
    // wrote it because bitcode v2 cannot express that expression form, and
    // it round-trips unchanged. Rejecting it *here* made such an artifact
    // readable but unwritable, so no external pass could ever instrument
    // the model -- which is the interchange promise the format exists for.
    // `compile-bitcode` still refuses it, because rebuilding a DAE from a
    // node whose meaning was never captured genuinely cannot be done.
    let mut options = rumoca_bitcode::validate::ValidateOptions::default();
    options.reject_unsupported = false;
    let unsupported = count_unsupported_nodes(&file.model);
    let checked = if connections {
        rumoca_bitcode::validate::validate_connection_contracts(&file.model, &options)
    } else {
        rumoca_bitcode::validate(&file.model, &options)
    };
    if strict && unsupported > 0 {
        eprintln!(
            "warning: {unsupported} expression node(s) record a form bitcode v2 cannot \
             express; the artifact round-trips, but `compile-bitcode` will refuse it"
        );
    }
    match checked {
        Ok(()) => {
            println!(
                "{}: valid bitcode v{}{}",
                path.display(),
                file.bitcode_version,
                if strict { " (strict)" } else { "" }
            );
            if !file.model.connectors.is_empty() {
                println!("  declared scalar connector contracts and connection laws checked");
            } else if !file.model.connection_sets.is_empty()
                || !file.model.connections.is_empty()
                || file.model.variables.iter().any(|v| v.connector.is_some())
            {
                println!(
                    "  connection laws NOT certified: missing complete port declarations (use --connections to require them)"
                );
            }
            Ok(())
        }
        Err(errors) => {
            for error in &errors {
                eprintln!("  - {error}");
            }
            bail!("{}: {} validation error(s)", path.display(), errors.len())
        }
    }
}

fn run_emit_text(input: &Path, output: Option<&Path>, sources: bool) -> Result<()> {
    let (file, _) = rumoca_bitcode::read_file(input).map_err(anyhow::Error::from)?;
    if file.execution.is_some() || !file.model.connectors.is_empty() {
        bail!(
            "text profile does not carry execution/connector declarations; use bitcode dump or convert"
        );
    }
    let text =
        rumoca_bitcode::text::print_text_with(&file, rumoca_bitcode::text::TextOptions { sources })
            .map_err(anyhow::Error::from)?;
    match output {
        Some(path) => {
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, &text).with_context(|| format!("write {}", path.display()))?;
            eprintln!(
                "wrote {} ({} bytes){}",
                path.display(),
                text.len(),
                if sources { "" } else { ", source text omitted" }
            );
        }
        None => print!("{text}"),
    }
    Ok(())
}

fn run_assemble(input: &Path, output: &Path, format: BitcodeFormat) -> Result<()> {
    let text =
        std::fs::read_to_string(input).with_context(|| format!("read {}", input.display()))?;
    let file = rumoca_bitcode::text::parse_text(&text).map_err(anyhow::Error::from)?;

    // Validated before it is written. The textual form is the one a person
    // edits by hand, so it is the one most likely to be internally
    // inconsistent — a dangling expression id, an equation reading a variable
    // that is not declared — and catching that here names the problem instead
    // of deferring it to whatever loads the artifact next.
    rumoca_bitcode::validate::validate(&file.model, &Default::default()).map_err(|errors| {
        anyhow::anyhow!(
            "assembled artifact is not valid:\n{}",
            errors
                .iter()
                .map(|e| format!("  - {e}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    })?;

    let bytes = rumoca_bitcode::encode(&file, format.into()).map_err(anyhow::Error::from)?;
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, bytes).with_context(|| format!("write {}", output.display()))?;
    eprintln!(
        "assembled {} -> {} ({})",
        input.display(),
        output.display(),
        Encoding::from(format).as_str()
    );
    Ok(())
}

fn run_convert(input: &Path, output: &Path, format: BitcodeFormat) -> Result<()> {
    // Changing how an artifact is stored must not change what it contains, so
    // this transcodes the raw document rather than round-tripping the typed
    // schema, which would drop fields this build does not know.
    let bytes = std::fs::read(input).with_context(|| format!("read {}", input.display()))?;
    let (_, from) = rumoca_bitcode::decode(&bytes).map_err(anyhow::Error::from)?;
    let converted =
        rumoca_bitcode::transcode(&bytes, format.into()).map_err(anyhow::Error::from)?;
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, converted).with_context(|| format!("write {}", output.display()))?;
    eprintln!(
        "converted {} ({}) -> {} ({})",
        input.display(),
        from.as_str(),
        output.display(),
        Encoding::from(format).as_str()
    );
    Ok(())
}

/// Prove `RBC -> DAE -> RBC` is faithful.
///
/// Re-exporting a reconstructed DAE and comparing it to the original artifact
/// is a stronger check than comparing counts: it compares every variable, its
/// attributes, every expression node, every equation, and every event. A
/// difference names the exact field that moved.
fn run_round_trip(path: &Path, output: Option<&Path>) -> Result<()> {
    let (original, _) = rumoca_bitcode::read_file(path).map_err(anyhow::Error::from)?;
    if original.execution.is_some() || !original.model.connectors.is_empty() {
        bail!(
            "DAE re-export would discard execution/connector declarations; use bitcode convert for a lossless container round-trip"
        );
    }
    let dae = rumoca_bitcode::import(&original).map_err(anyhow::Error::from)?;
    let again = rumoca_bitcode::export(&dae, None, &original.model.name, &ExportOptions::default())
        .map_err(anyhow::Error::from)?;

    if let Some(output) = output {
        rumoca_bitcode::write_file(output, &again, Encoding::Json).map_err(anyhow::Error::from)?;
    }

    let left = rumoca_bitcode::to_json(&original).map_err(anyhow::Error::from)?;
    let right = rumoca_bitcode::to_json(&again).map_err(anyhow::Error::from)?;
    if left == right {
        println!("{}: round-trip is byte-identical", path.display());
        return Ok(());
    }

    // Connections come from Flat, which import does not reconstruct, so they are
    // expected to differ. Compare everything else field by field.
    let differences = compare(&original.model, &again.model);
    if differences.is_empty() {
        println!(
            "{}: round-trip preserves every compared field (connections are Flat-derived and not reconstructed)",
            path.display()
        );
        return Ok(());
    }
    for difference in &differences {
        eprintln!("  - {difference}");
    }
    bail!(
        "{}: round-trip differs in {} place(s)",
        path.display(),
        differences.len()
    )
}

fn compare(left: &RbcModel, right: &RbcModel) -> Vec<String> {
    let mut differences = Vec::new();
    let mut check = |label: &str, a: String, b: String| {
        if a != b {
            differences.push(format!("{label}: {a} != {b}"));
        }
    };
    check(
        "variables",
        format!("{:?}", left.variables.len()),
        format!("{:?}", right.variables.len()),
    );
    check(
        "expressions",
        format!("{:?}", left.expressions.len()),
        format!("{:?}", right.expressions.len()),
    );
    check(
        "equations",
        format!("{:?}", left.equations.len()),
        format!("{:?}", right.equations.len()),
    );
    for (a, b) in left.variables.iter().zip(&right.variables) {
        check(
            &format!("variable {} name", a.id),
            a.name.clone(),
            b.name.clone(),
        );
        check(
            &format!("variable {} role", a.id),
            format!("{:?}", a.role),
            format!("{:?}", b.role),
        );
        check(
            &format!("variable {} unit", a.id),
            format!("{:?}", a.unit),
            format!("{:?}", b.unit),
        );
    }
    for (a, b) in left.expressions.iter().zip(&right.expressions) {
        check(
            &format!("expression {} node", a.id),
            format!("{:?}", a.node),
            format!("{:?}", b.node),
        );
    }
    for (a, b) in left.equations.iter().zip(&right.equations) {
        check(
            &format!("equation {} residual", a.id),
            format!("{:?}", a.residual),
            format!("{:?}", b.residual),
        );
        check(
            &format!("equation {} reads", a.id),
            format!("{:?}", a.reads),
            format!("{:?}", b.reads),
        );
    }
    for (a, b) in left.events.iter().zip(&right.events) {
        check(
            &format!("event {} action", a.id),
            format!("{:?}", a.action),
            format!("{:?}", b.action),
        );
    }
    differences
}

/// Read bitcode back into a checked DAE and continue compilation.
pub fn run_compile_bitcode(args: CompileBitcodeArgs) -> Result<()> {
    let (file, encoding) = rumoca_bitcode::read_file(&args.input).map_err(anyhow::Error::from)?;
    if file.execution.is_some() {
        bail!(
            "compile-bitcode would discard executable edits; use bitcode run --execution=require"
        );
    }
    eprintln!(
        "reading {} (bitcode v{}, {})",
        args.input.display(),
        file.bitcode_version,
        encoding.as_str()
    );

    let dae = rumoca_bitcode::import(&file).map_err(anyhow::Error::from)?;

    if args.summary {
        print_reconstructed(&file.model, &dae);
    }

    if args.simulate {
        return run_trace(&file.model, &dae, &args);
    }

    if let Some(target) = args.target.as_deref() {
        return run_target(&file.model, &dae, target, args.output.as_deref());
    }

    if !args.summary {
        print_reconstructed(&file.model, &dae);
    }
    Ok(())
}

/// Render a code-generation target from a reconstructed model.
///
/// Targets whose manifest declares `ir = "dae"` or `ir = "fmi"` render from a
/// DAE alone, so they work from bitcode unchanged — including `fmi2` and
/// `fmi3`. Targets over Flat, AST, or Algorithm Code need artifacts bitcode
/// does not carry, and are refused by name rather than approximated.
fn run_target(
    model: &RbcModel,
    dae: &rumoca_compile::compile::Dae,
    target: &str,
    output: Option<&Path>,
) -> Result<()> {
    use rumoca_compile::codegen::targets::{
        TargetBundle, TargetTemplateIr, render_dae_target_files, target_ir_is_dae_renderable,
    };

    let bundle = TargetBundle::load(target).with_context(|| format!("load target `{target}`"))?;
    let manifest = bundle
        .parse_manifest()
        .with_context(|| format!("parse target manifest for `{target}`"))?;

    if !target_ir_is_dae_renderable(manifest.ir) {
        bail!(
            "target `{target}` consumes {:?} IR, which bitcode does not carry. \
             Bitcode reconstructs a checked DAE, so targets declaring `ir = \"dae\"` \
             or `ir = \"fmi\"` render from it; {:?} additionally needs artifacts \
             produced earlier in compilation. Compile from Modelica source for that target.",
            manifest.ir,
            manifest.ir
        );
    }
    if manifest.ir == TargetTemplateIr::Fmi {
        bail!(
            "target `{target}` is an FMI packaging target. Its component lowering is \
             reachable from a DAE, but packaging additionally needs the artifact \
             session the `compile` path owns; that wiring is not done yet. \
             DAE-level targets work: try `--target dae-modelica`."
        );
    }

    let files = render_dae_target_files(&bundle, &manifest, dae, &model.name)
        .with_context(|| format!("render target `{target}`"))?;

    match output {
        Some(directory) => {
            std::fs::create_dir_all(directory)
                .with_context(|| format!("create {}", directory.display()))?;
            for file in &files {
                let path = directory.join(&file.path);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&path, &file.content)
                    .with_context(|| format!("write {}", path.display()))?;
                eprintln!("  wrote {}", path.display());
            }
            eprintln!("Target `{target}` rendered to {}", directory.display());
        }
        None => {
            // One file goes to stdout so a target can be piped; several would be
            // ambiguous, so ask for a directory instead.
            match files.as_slice() {
                [single] => print!("{}", single.content),
                _ => bail!(
                    "target `{target}` renders {} files; pass --output DIRECTORY",
                    files.len()
                ),
            }
        }
    }
    Ok(())
}

/// Simulate a reconstructed model and emit the trace points an external pass
/// asked for.
///
/// This is the end of the loop: the trace points were written by a tool that
/// never saw Rumoca's internals, and they select what a real solve reports.
fn run_trace(
    model: &RbcModel,
    dae: &rumoca_compile::compile::Dae,
    args: &CompileBitcodeArgs,
) -> Result<()> {
    use rumoca_sim::{SimOptions, simulate_with_diagnostics};

    // --check inspects every solver output, so it needs no instrumentation.
    if !args.check && model.trace_points.is_empty() {
        bail!(
            "this artifact declares no trace points; nothing to observe. \
             Run an instrumentation pass first, e.g. connector_logger.py, \
             or use --check to test declared properties over all variables"
        );
    }

    let mut options = SimOptions {
        t_end: args.t_end,
        dt: args.dt,
        ..SimOptions::default()
    };
    for pair in &args.params {
        options.param_overrides.push(parse_param(pair)?);
    }
    if args.check {
        eprintln!(
            "Simulating {} to t={} with property checks...",
            model.name, args.t_end
        );
    } else {
        eprintln!(
            "Simulating {} to t={} with {} trace point(s)...",
            model.name,
            args.t_end,
            model.trace_points.len()
        );
    }
    let execution = match args.domain_diagnostics.as_deref() {
        Some(path) => {
            rumoca_sim::execution::simulate_equations_with_domain_diagnostics(dae, &options, path)
        }
        None => simulate_with_diagnostics(dae, &options).map_err(|e| e.to_string()),
    };
    let sim = match execution {
        Ok(sim) => sim,
        Err(error) if args.check => {
            // For a search driver, a solve that refuses to run *is* the
            // finding: this parameter configuration made the model
            // mathematically invalid. Report it in the same structured shape as
            // a property violation instead of failing the process generically.
            return report_simulation_failure(&error.to_string(), &options.param_overrides);
        }
        Err(error) => return Err(anyhow::anyhow!("simulation failed: {error}")),
    };
    eprintln!(
        "Simulation complete: {} time points, {} variables",
        sim.times.len(),
        sim.names.len()
    );

    if args.check {
        return report_violations(model, &sim);
    }

    let plan = resolve_trace_plan(model, &sim.names);
    let missing: Vec<&TracePlanRow> = plan.iter().filter(|row| row.column.is_none()).collect();
    for row in &missing {
        // A trace point naming a variable the solver does not report is a real
        // gap, not something to silently drop: the user asked to observe it.
        eprintln!(
            "warning: trace point {} ({}) names `{}`, which the solver does not report",
            row.id, row.label, row.variable
        );
    }

    match args.trace_out.as_deref() {
        Some(path) => write_trace_csv(path, &plan, &sim),
        None => {
            print_trace_table(&plan, &sim);
            Ok(())
        }
    }
}

/// Report a refused solve as a structured finding.
fn report_simulation_failure(message: &str, parameters: &[(String, f64)]) -> Result<()> {
    let finding = serde_json::json!([{
        "kind": "simulation-failure",
        "detail": message,
        "parameters": parameters
            .iter()
            .map(|(name, value)| serde_json::json!({"name": name, "value": value}))
            .collect::<Vec<_>>(),
    }]);
    println!("{}", serde_json::to_string_pretty(&finding)?);
    std::process::exit(2);
}

/// Report runtime property violations as JSON.
///
/// Two properties are checked, both of which the model itself declares:
/// a value that stops being finite, and a value that leaves its declared
/// `min`/`max` range. Neither is inferred physics — a violation means the
/// model contradicted something its own author wrote down.
#[derive(serde::Serialize)]
struct Violation<'a> {
    kind: &'a str,
    variable: &'a str,
    time: f64,
    value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    bound: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
}

fn report_violations(model: &RbcModel, sim: &rumoca_sim::SimResult) -> Result<()> {
    let bounds = declared_bounds(model);
    let mut violations = Vec::new();

    for (column, name) in sim.names.iter().enumerate() {
        let Some(series) = sim.data.get(column) else {
            continue;
        };
        // One report per variable: a diverged trajectory is one bug, not one
        // per output point.
        if let Some(violation) =
            first_violation(name, series, &sim.times, bounds.get(name.as_str()))
        {
            violations.push(violation);
        }
    }

    let found = !violations.is_empty();
    println!("{}", serde_json::to_string_pretty(&violations)?);
    if found {
        // Non-zero so a search driver can treat "this configuration fails" as
        // a signal without parsing output.
        std::process::exit(2);
    }
    Ok(())
}

/// The first point at which one variable breaks a property it declares.
fn first_violation<'a>(
    name: &'a str,
    series: &[f64],
    times: &[f64],
    declared: Option<&DeclaredBounds>,
) -> Option<Violation<'a>> {
    for (index, value) in series.iter().enumerate() {
        let time = times.get(index).copied().unwrap_or(f64::NAN);
        let make = |kind, bound| Violation {
            kind,
            variable: name,
            time,
            value: *value,
            bound,
            source: declared.and_then(|declared| declared.source.clone()),
        };
        if value.is_nan() {
            return Some(make("nan", None));
        }
        if value.is_infinite() {
            return Some(make("infinite", None));
        }
        let Some(declared) = declared else { continue };
        match (declared.minimum, declared.maximum) {
            (Some(minimum), _) if *value < minimum => {
                return Some(make("below-min", Some(minimum)));
            }
            (_, Some(maximum)) if *value > maximum => {
                return Some(make("above-max", Some(maximum)));
            }
            _ => {}
        }
    }
    None
}

struct DeclaredBounds {
    minimum: Option<f64>,
    maximum: Option<f64>,
    source: Option<String>,
}

/// Collect `min`/`max` a variable declares, resolved to constants.
///
/// Only literal bounds are used. A bound that is itself an expression may
/// depend on a parameter this run overrode, and silently evaluating it here
/// would be a second, weaker evaluator.
fn declared_bounds(model: &RbcModel) -> std::collections::BTreeMap<&str, DeclaredBounds> {
    let literal = |id: Option<rumoca_bitcode::schema::ExprId>| -> Option<f64> {
        let index = id?.0 as usize;
        match &model.expressions.get(index)?.node {
            rumoca_bitcode::schema::RbcExprNode::Literal { value } => match value {
                rumoca_bitcode::schema::RbcLiteral::Real { value } => Some(*value),
                rumoca_bitcode::schema::RbcLiteral::Integer { value } => Some(*value as f64),
                rumoca_bitcode::schema::RbcLiteral::Enumeration { ordinal } => {
                    Some(*ordinal as f64)
                }
                _ => None,
            },
            _ => None,
        }
    };
    model
        .variables
        .iter()
        .filter_map(|variable| {
            let minimum = literal(variable.min);
            let maximum = literal(variable.max);
            (minimum.is_some() || maximum.is_some()).then(|| {
                (
                    variable.name.as_str(),
                    DeclaredBounds {
                        minimum,
                        maximum,
                        source: Some(format!(
                            "{}:{}:{}",
                            model
                                .sources
                                .get(variable.declaration.span.source.0 as usize)
                                .map(|s| s.name.rsplit('/').next().unwrap_or(&s.name).to_string())
                                .unwrap_or_default(),
                            variable.declaration.span.line,
                            variable.declaration.span.column
                        )),
                    },
                )
            })
        })
        .collect()
}

/// One resolved trace point: what to observe, and which solver column holds it.
struct TracePlanRow {
    id: u32,
    label: String,
    variable: String,
    quantity: String,
    unit: String,
    connection: String,
    column: Option<usize>,
}

fn resolve_trace_plan(model: &RbcModel, names: &[String]) -> Vec<TracePlanRow> {
    model
        .trace_points
        .iter()
        .map(|trace| {
            let variable = model
                .variables
                .get(trace.variable.0 as usize)
                .map(|variable| variable.name.clone())
                .unwrap_or_default();
            let connection = trace
                .connection
                .and_then(|id| model.connections.get(id.0 as usize))
                .map(|connection| {
                    format!(
                        "{} <-> {}",
                        connection.left_connector, connection.right_connector
                    )
                })
                .unwrap_or_default();
            TracePlanRow {
                id: trace.id.0,
                label: trace.label.clone(),
                column: names.iter().position(|name| *name == variable),
                variable,
                quantity: trace
                    .quantity
                    .map(|quantity| format!("{quantity:?}").to_lowercase())
                    .unwrap_or_default(),
                unit: trace.unit.clone().unwrap_or_default(),
                connection,
            }
        })
        .collect()
}

fn write_trace_csv(path: &Path, plan: &[TracePlanRow], sim: &rumoca_sim::SimResult) -> Result<()> {
    let mut out = String::from("time,trace_id,connection,variable,quantity,unit,value\n");
    for (index, time) in sim.times.iter().enumerate() {
        for row in plan {
            let Some(column) = row.column else { continue };
            let Some(value) = sim.data.get(column).and_then(|series| series.get(index)) else {
                continue;
            };
            out.push_str(&format!(
                "{time},{},{},{},{},{},{value}\n",
                row.id, row.connection, row.variable, row.quantity, row.unit
            ));
        }
    }
    std::fs::write(path, out).with_context(|| format!("write {}", path.display()))?;
    let observed = plan.iter().filter(|row| row.column.is_some()).count();
    eprintln!(
        "wrote {} ({} trace point(s) x {} time point(s))",
        path.display(),
        observed,
        sim.times.len()
    );
    Ok(())
}

/// Print the human-readable view of the same structured trace.
fn print_trace_table(plan: &[TracePlanRow], sim: &rumoca_sim::SimResult) {
    // The table is one presentation of the trace; --trace-out carries the
    // structured form.
    println!(
        "{:<9} {:<22} {:<14} {:<10} {:<6} {:>13}",
        "TIME", "CONNECTION", "VARIABLE", "QUANTITY", "UNIT", "VALUE"
    );
    let sample_every = (sim.times.len() / 10).max(1);
    for (index, time) in sim.times.iter().enumerate() {
        if index % sample_every != 0 && index + 1 != sim.times.len() {
            continue;
        }
        for row in plan {
            let Some(column) = row.column else { continue };
            let Some(value) = sim.data.get(column).and_then(|series| series.get(index)) else {
                continue;
            };
            let connection = if row.connection.is_empty() {
                row.variable.clone()
            } else {
                row.connection.clone()
            };
            println!(
                "{time:<9.4} {connection:<22} {:<14} {:<10} {:<6} {value:>13.6}",
                row.variable, row.quantity, row.unit
            );
        }
    }
}
fn print_reconstructed(model: &RbcModel, dae: &rumoca_compile::compile::Dae) {
    let (variables, equations) =
        dae.inspect(|view| (view.variable_count(), view.continuous_equation_count()));
    println!("reconstructed checked DAE from bitcode");
    println!("  model      {}", model.name);
    println!(
        "  variables  {variables} (bitcode declared {})",
        model.summary.variables
    );
    println!(
        "  equations  {equations} (bitcode declared {})",
        model.summary.equations
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use rumoca_bitcode::schema::*;

    /// A model with one traced connector variable.
    ///
    /// Built rather than written out: the struct-literal version named every
    /// field of `RbcModel` and `RbcVariable`, so it broke on every schema
    /// addition — four times in one week, each a mechanical repair.
    fn model_with_trace() -> RbcModel {
        use rumoca_bitcode::build::Builder;

        let mut builder = Builder::new("T");
        let torque = builder.variable("motor.flange.tau", RbcRole::Algebraic);
        builder.trace_point(torque, "flange torque", "test");
        let mut model = builder.finish();
        model.variables[torque.0 as usize].unit = Some("N.m".into());
        model.trace_points[0].quantity = Some(RbcQuantityKind::Flow);
        model.trace_points[0].unit = Some("N.m".into());
        model
    }

    #[test]
    fn a_trace_point_binds_to_the_solver_column_of_its_variable() {
        let model = model_with_trace();
        let names = vec!["time".to_string(), "motor.flange.tau".to_string()];
        let plan = resolve_trace_plan(&model, &names);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].column, Some(1));
        assert_eq!(plan[0].variable, "motor.flange.tau");
        assert_eq!(plan[0].unit, "N.m");
        assert_eq!(plan[0].quantity, "flow");
    }

    #[test]
    fn a_trace_point_the_solver_does_not_report_is_surfaced_not_dropped() {
        // The user asked to observe this quantity. Silently omitting it would
        // make a missing column look like a model with nothing to say.
        let model = model_with_trace();
        let names = vec!["time".to_string()];
        let plan = resolve_trace_plan(&model, &names);
        assert_eq!(plan.len(), 1, "the row must survive for the warning path");
        assert_eq!(plan[0].column, None);
    }
}
