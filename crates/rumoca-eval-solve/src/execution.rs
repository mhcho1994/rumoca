//! Ordered execution effects over canonical Solve scalar computation.
use rumoca_ir_solve as solve;
use solve::execution as ir;
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

#[derive(Clone)]
enum Value {
    Number(f64),
    Integer(u64),
    Text(String),
}
impl Value {
    fn number(&self) -> Result<f64, String> {
        match self {
            Self::Number(n) => Ok(*n),
            Self::Integer(n) => Ok(*n as f64),
            Self::Text(_) => Err("expected numeric value".into()),
        }
    }
    fn text(&self) -> Result<&str, String> {
        match self {
            Self::Text(value) => Ok(value),
            _ => Err("expected text value".into()),
        }
    }
    fn csv(&self) -> String {
        match self {
            Self::Number(n) => n.to_string(),
            Self::Integer(n) => n.to_string(),
            Self::Text(s) => quote(s),
        }
    }
}

pub struct CsvExecution {
    artifact: ir::ExecutionArtifact,
    root: PathBuf,
    files: BTreeMap<String, BufWriter<File>>,
    sequence: u64,
    finished: bool,
    /// The derived numerical program. Not from the wire: the caller derives it
    /// from the equation IR with the artifact's recorded profile and hands it
    /// in, so no solver's choices are serialized (D1).
    numerical: ir::NumericalProgram,
    time: f64,
    phase: String,
    y: Vec<f64>,
    p: Vec<f64>,
}

impl CsvExecution {
    pub fn start(
        artifact: ir::ExecutionArtifact,
        numerical: ir::NumericalProgram,
        root: &Path,
    ) -> Result<Self, String> {
        ir::validate(&artifact)?;
        std::fs::create_dir(root)
            .map_err(|e| format!("trace directory must be new: {}: {e}", root.display()))?;
        let mut state = Self {
            artifact,
            numerical,
            root: root.into(),
            files: BTreeMap::new(),
            sequence: 0,
            finished: false,
            time: 0.0,
            phase: String::new(),
            y: vec![],
            p: vec![],
        };
        let count = state
            .numerical
            .storage
            .iter()
            .filter(|v| v.role == "parameter")
            .count();
        state.p.resize(count, 0.0);
        for v in &state.numerical.storage {
            if v.role == "parameter" {
                *state.p.get_mut(v.index).ok_or("invalid parameter index")? = v.start;
            }
        }
        // The wire sink carries identity only; path/unit/kind are resolved
        // here and written denormalized, which is what the manifest is for.
        let manifest = serde_json::json!({
            "schema_version": 1,
            "dependency_digest": state.artifact.dependency_digest,
            "execution_revision": state.artifact.revision,
            "sinks": resolved_sinks(&state)?,
        });
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join("manifest.json"))
            .map_err(|e| e.to_string())?;
        serde_json::to_writer_pretty(&mut file, &manifest).map_err(|e| e.to_string())?;
        state.function("run_start")?;
        Ok(state)
    }

    fn function(&mut self, name: &str) -> Result<(), String> {
        let function = self
            .artifact
            .program
            .functions
            .get(name)
            .ok_or("missing function")?
            .clone();
        self.body(&function, &function.body.clone(), &mut BTreeMap::new())
    }

    /// Evaluate a program-local expression over declared locals.
    ///
    /// Leaves are locals and literals only -- the node type has no variable
    /// reference -- so this cannot reach model state except through a value a
    /// `snapshot.*` instruction already placed in `values`.
    fn expression(
        &mut self,
        function: &ir::Function,
        id: ir::ProgramExprId,
        values: &BTreeMap<String, Value>,
    ) -> Result<Value, String> {
        let node = function
            .expressions
            .get(id.0 as usize)
            .ok_or("program expression out of range")?;
        let value = match node {
            ir::ProgramExpr::Local { name } => get(values, name)?.clone(),
            ir::ProgramExpr::Real { value } => Value::Number(*value),
            ir::ProgramExpr::Integer { value } => {
                Value::Integer(u64::try_from(*value).map_err(|_| "negative integer literal")?)
            }
            ir::ProgramExpr::Boolean { value } => Value::Integer(u64::from(*value)),
            ir::ProgramExpr::Text { value } => Value::Text(value.clone()),
            ir::ProgramExpr::Unary { op, operand } => {
                let operand = self.expression(function, *operand, values)?.number()?;
                Value::Number(match op {
                    crate::UnaryOp::Neg => -operand,
                    other => {
                        return Err(format!("unary {other:?} is not admitted in a host program"));
                    }
                })
            }
            ir::ProgramExpr::Binary { op, lhs, rhs } => {
                let l = self.expression(function, *lhs, values)?.number()?;
                let r = self.expression(function, *rhs, values)?.number()?;
                Value::Number(match op {
                    crate::BinaryOp::Add => l + r,
                    crate::BinaryOp::Sub => l - r,
                    crate::BinaryOp::Mul => l * r,
                    crate::BinaryOp::Div => l / r,
                    other => {
                        return Err(format!(
                            "binary {other:?} is not admitted in a host program"
                        ));
                    }
                })
            }
            ir::ProgramExpr::Compare { op, lhs, rhs } => {
                let l = self.expression(function, *lhs, values)?.number()?;
                let r = self.expression(function, *rhs, values)?.number()?;
                let truth = match op {
                    crate::CompareOp::Eq => l == r,
                    crate::CompareOp::Ne => l != r,
                    crate::CompareOp::Lt => l < r,
                    crate::CompareOp::Le => l <= r,
                    crate::CompareOp::Gt => l > r,
                    crate::CompareOp::Ge => l >= r,
                };
                Value::Integer(u64::from(truth))
            }
        };
        if let Value::Number(number) = value {
            return self.finite(number);
        }
        Ok(value)
    }

    fn body(
        &mut self,
        function: &ir::Function,
        body: &[ir::Instruction],
        values: &mut BTreeMap<String, Value>,
    ) -> Result<(), String> {
        for op in body {
            if let Some((name, value)) = self.instruction(function, op, values)? {
                values.insert(name.clone(), value);
            }
        }
        Ok(())
    }

    fn instruction<'a>(
        &mut self,
        function: &ir::Function,
        op: &'a ir::Instruction,
        values: &mut BTreeMap<String, Value>,
    ) -> Result<Option<(&'a String, Value)>, String> {
        use ir::Instruction::*;
        let result = match op {
            Time { result } => Some((result, self::Value::Number(self.time))),
            Sequence { result } => Some((result, self::Value::Integer(self.sequence))),
            Phase { result } => Some((result, self::Value::Text(self.phase.clone()))),
            Value {
                result,
                trace_point,
            } => {
                // The derived program's observation map is keyed by the trace
                // point the program asked for (D2): one registration site.
                let obs = self
                    .numerical
                    .observations
                    .iter()
                    .find(|o| o.trace_point == *trace_point)
                    .ok_or("missing observation")?;
                let ops = ir::checked_scalar(&obs.instructions, self.y.len(), self.p.len())?;
                let value = crate::eval_row(&ops, &self.y, &self.p, self.time, None)
                    .map_err(|e| e.to_string())?;
                Some((result, self.finite(value)?))
            }
            Compute { result, expr } => {
                let value = self.expression(function, *expr, values)?;
                Some((result, value))
            }
            Open { sink } => {
                self.open_sink(sink)?;
                None
            }
            Write { sink, values: args } => {
                let row = args
                    .iter()
                    .map(|n| Ok(get(values, n)?.csv()))
                    .collect::<Result<Vec<_>, String>>()?
                    .join(",");
                let file = self.files.get_mut(sink).ok_or("CSV sink is not open")?;
                writeln!(file, "{row}").map_err(|e| e.to_string())?;
                None
            }
            Close { sink } => {
                // Also used by controlled-failure cleanup after partial setup.
                if let Some(mut file) = self.files.remove(sink) {
                    file.flush().map_err(|e| e.to_string())?;
                }
                None
            }
            If {
                condition,
                then_body,
                else_body,
            } => {
                let selected = if get(values, condition)?.number()? != 0.0 {
                    then_body
                } else {
                    else_body
                };
                self.body(function, selected, &mut values.clone())?;
                None
            }
            Call { function } => {
                self.function(function)?;
                None
            }
            Assert { condition, message } => {
                if get(values, condition)?.number()? == 0.0 {
                    // The message is a local holding text; report what it
                    // says, not what it is called.
                    let text = get(values, message)?.text()?;
                    return Err(format!("execution assertion: {text}"));
                }
                None
            }
        };
        Ok(result)
    }

    fn open_sink(&mut self, sink: &String) -> Result<(), String> {
        let decl = self
            .artifact
            .program
            .sinks
            .iter()
            .find(|s| &s.key == sink)
            .ok_or("missing sink")?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.root.join(&decl.filename))
            .map_err(|e| e.to_string())?;
        let mut out = BufWriter::new(file);
        writeln!(
            out,
            "{}",
            decl.columns
                .iter()
                .map(|column| quote(&column.name))
                .collect::<Vec<_>>()
                .join(",")
        )
        .map_err(|e| e.to_string())?;
        self.files.insert(sink.clone(), out);
        Ok(())
    }

    fn finite(&self, value: f64) -> Result<Value, String> {
        if value.is_finite() {
            Ok(Value::Number(value))
        } else {
            Err("non-finite executable observation/computation".into())
        }
    }
}

impl CsvExecution {
    pub fn publish(
        &mut self,
        time: f64,
        phase: &str,
        names: &[String],
        values: &[f64],
    ) -> Result<(), String> {
        self.time = time;
        self.phase = phase.into();
        self.y
            .resize(self.numerical.storage.len() - self.p.len(), 0.0);
        for v in &self.numerical.storage {
            if v.role == "parameter" {
                continue;
            }
            let i = names
                .iter()
                .position(|n| n == &v.name)
                .ok_or_else(|| format!("host did not reconstruct {}", v.name))?;
            *self
                .y
                .get_mut(v.index)
                .ok_or("invalid snapshot storage index")? =
                *values.get(i).ok_or("invalid snapshot width")?;
        }
        self.function("publish")?;
        self.sequence += 1;
        Ok(())
    }

    pub fn finish(&mut self) -> Result<(), String> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        let result = self.function("run_finish");
        // Even a failing finish instruction cannot keep later resources open.
        let mut cleanup = Ok(());
        for (_, mut file) in std::mem::take(&mut self.files) {
            if let Err(e) = file.flush() {
                cleanup = Err(e.to_string());
            }
        }
        result.and(cleanup)
    }
}

impl Drop for CsvExecution {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            eprintln!("execution cleanup failed: {error}");
        }
    }
}

fn get<'a>(values: &'a BTreeMap<String, Value>, name: &str) -> Result<&'a Value, String> {
    values
        .get(name)
        .ok_or_else(|| format!("undefined value {name}"))
}

fn quote(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.into()
    }
}

/// Resolve each wire sink against the derived program.
///
/// The wire form names identities only (SEV-014); a reader of the emitted
/// trace needs paths and units, and this is the one place that has both the
/// sink and the derived observations to join them.
fn resolved_sinks(state: &CsvExecution) -> Result<serde_json::Value, String> {
    let mut sinks = Vec::new();
    for sink in &state.artifact.program.sinks {
        let mut members = Vec::new();
        let mut path: Option<String> = None;
        for member in &sink.metadata.members {
            let observation = state
                .numerical
                .observations
                .iter()
                .find(|o| o.trace_point == member.trace_point)
                .ok_or_else(|| {
                    format!(
                        "sink {} names unobserved trace point {}",
                        sink.key, member.trace_point
                    )
                })?;
            // Structure, not text: the connector instance says what owns the
            // member and what the member is called.
            let owner = observation
                .owner
                .as_deref()
                .filter(|_| sink.metadata.connector.is_some());
            path = same_connector(path, owner, &sink.key)?;
            // A sink that is not connector instrumentation labels its column
            // with the observation's own name.
            let name = observation
                .member
                .clone()
                .unwrap_or_else(|| observation.name.clone());
            let unit = state
                .numerical
                .storage
                .iter()
                .find(|v| v.variable_id == Some(observation.variable_id))
                .and_then(|v| v.unit.clone());
            members.push(serde_json::json!({
                "trace_point": member.trace_point,
                "name": name,
                "unit": unit,
                "kind": observation.quantity,
            }));
        }
        let binding = sink.metadata.connector.as_ref();
        sinks.push(serde_json::json!({
            "key": sink.key,
            "filename": sink.filename,
            "columns": sink.columns,
            "metadata": {
                "connector": binding.map(|b| b.connector),
                "connector_path": path,
                "orientation": binding.map(|b| b.orientation),
                "members": members,
            },
        }));
    }
    Ok(serde_json::Value::Array(sinks))
}

/// One connector per sink, when the sink names a connector at all.
///
/// Split out so the resolution loop stays flat: a sink that mixes two
/// connectors is a pass defect, and saying so needs its own two lines.
fn same_connector(
    seen: Option<String>,
    owner: Option<&str>,
    key: &str,
) -> Result<Option<String>, String> {
    match (seen, owner) {
        (seen, None) => Ok(seen),
        (None, Some(owner)) => Ok(Some(owner.to_string())),
        (Some(seen), Some(owner)) if seen == owner => Ok(Some(seen)),
        (Some(seen), Some(owner)) => Err(format!("sink {key} mixes members of {seen} and {owner}")),
    }
}
