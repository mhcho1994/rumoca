//! Single authoritative checker for a v2 host program.
//!
//! The analyses run in a fixed order, each assuming the previous one passed:
//! name resolution, type, effect ordering, termination, then reference
//! resolution against the model. The order is the same discipline
//! `rumoca-ir-galec::validate` uses for a bounded block language; the
//! arithmetic semantics are Solve's, not GALEC's.
use super::*;
use std::collections::BTreeSet;

/// Stable diagnostic codes. A caller matches on these, not on message text.
pub mod code {
    /// Artifact declares a version this reader does not implement.
    pub const UNSUPPORTED_VERSION: &str = "EX2-001";
    /// A `csv.*` effect names a sink the program does not declare.
    pub const UNRESOLVED_SINK: &str = "EX2-010";
    /// The target this profile expands to cannot perform file effects at all
    /// (TRP-042). Distinct from `UNRESOLVED_SINK`: that one says the program
    /// named a sink it did not declare, this one says no sink could work
    /// here whatever it was called. Currently unreachable, because every
    /// lowering profile that exists sets `file_effects`; the check is wired
    /// to the profile rather than deleted so that adding a profile without
    /// file effects is a one-line change with a diagnostic already in place.
    pub const MISSING_FILE_CAPABILITY: &str = "EX2-011";
    /// An effect appears in a lifecycle phase that cannot perform it, or
    /// names a resource that is not open in that phase.
    pub const EFFECT_LIFECYCLE: &str = "EX2-012";
    /// A written row does not have one value per declared column.
    pub const ROW_WIDTH: &str = "EX2-013";
    /// Two sinks share a key or a filename.
    pub const DUPLICATE_SINK: &str = "EX2-014";
    /// A sink filename is not one relative, non-reserved path component.
    pub const INVALID_SINK_PATH: &str = "EX2-015";
    /// A sink declares no columns, so no row it wrote could be read back.
    pub const EMPTY_SINK: &str = "EX2-016";
    /// Read of a local that is not declared, or not yet assigned.
    pub const UNDECLARED_LOCAL: &str = "EX2-020";
    /// Value of the wrong type for its position.
    pub const TYPE_MISMATCH: &str = "EX2-021";
    /// `Text` used outside a CSV column or an assert message.
    pub const TEXT_MISUSE: &str = "EX2-022";
    // EX2-023 was reserved for "assignment from outside to a local declared
    // in an inner scope". It is unused on purpose: `Local` declarations are
    // function-scoped, and `If` carries no declarations of its own, so an
    // inner declaration is unrepresentable. What a branch *can* do is assign
    // a local that only one arm assigns; a read after the join is then
    // EX2-020, which is the accurate diagnosis.
    //
    // EX2-024 was reserved for "a program expression names a model variable".
    // It is unused on purpose: `ProgramExpr` has no `VariableId` variant, so
    // the shape is unrepresentable rather than rejected.
    //
    // Both codes stay documented and unallocated so they are not reused.
    /// The call graph contains a cycle.
    pub const RECURSIVE_CALL: &str = "EX2-030";
    /// A helper's name carries no lifecycle prefix the reader knows.
    pub const UNKNOWN_LIFECYCLE: &str = "EX2-031";
    /// A call reaches a helper belonging to a different lifecycle phase.
    pub const CROSS_LIFECYCLE_CALL: &str = "EX2-032";
    /// A snapshot read appears outside the publish phase, where there is no
    /// snapshot to read.
    pub const SNAPSHOT_LIFECYCLE: &str = "EX2-033";
    /// The two arms of an `if` leave different resources open, so what is
    /// open after the join depends on the branch taken.
    pub const BRANCH_RESOURCE_MISMATCH: &str = "EX2-034";
    /// An instruction references a trace point the model does not declare.
    pub const UNKNOWN_TRACE_POINT: &str = "EX2-040";
}

fn err(code: &str, detail: impl std::fmt::Display) -> String {
    format!("{code}: {detail}")
}

pub fn validate(a: &ExecutionArtifact) -> Result<(), String> {
    if a.version != EXECUTION_VERSION {
        return Err(err(
            code::UNSUPPORTED_VERSION,
            format_args!(
                "execution version {} is not supported; this reader implements {} and carries no adapter for earlier versions",
                a.version, EXECUTION_VERSION
            ),
        ));
    }
    let mut keys = BTreeSet::new();
    let mut files = BTreeSet::new();
    for s in &a.program.sinks {
        if !keys.insert(s.key.clone()) || !files.insert(s.filename.clone()) {
            return Err(err(
                code::DUPLICATE_SINK,
                format_args!("`{}` repeats a sink key or filename", s.key),
            ));
        }
        if s.filename.is_empty()
            || s.filename.contains(['/', '\\'])
            || s.filename == "."
            || s.filename == ".."
            || s.filename == "manifest.json"
        {
            return Err(err(
                code::INVALID_SINK_PATH,
                format_args!(
                    "`{}` is not one relative, non-reserved path component",
                    s.filename
                ),
            ));
        }
        if s.columns.is_empty() {
            return Err(err(
                code::EMPTY_SINK,
                format_args!("`{}` declares no columns", s.key),
            ));
        }
    }
    for phase in ["run_start", "publish", "run_finish"] {
        let mut resources = if phase == "run_start" {
            BTreeSet::new()
        } else {
            keys.clone()
        };
        function_check(a, phase, phase, &mut resources, &mut Vec::new())?;
        let expected = if phase == "run_finish" {
            BTreeSet::new()
        } else {
            keys.clone()
        };
        if resources != expected {
            return Err(format!(
                "{phase} does not establish the required CSV resource state"
            ));
        }
    }
    // Even unused functions must be well-formed, under their declared lifecycle.
    for name in a
        .program
        .functions
        .keys()
        .filter(|n| !matches!(n.as_str(), "run_start" | "publish" | "run_finish"))
    {
        let phase = name
            .split_once(':')
            .map(|p| p.0)
            .ok_or("helper function names require a lifecycle prefix, e.g. publish:check")?;
        if !matches!(phase, "run_start" | "publish" | "run_finish") {
            return Err(err(
                code::UNKNOWN_LIFECYCLE,
                format_args!("`{name}` names no lifecycle this reader knows"),
            ));
        }
        let mut resources = if phase == "run_start" {
            BTreeSet::new()
        } else {
            keys.clone()
        };
        function_check(a, name, phase, &mut resources, &mut Vec::new())?;
    }
    Ok(())
}

fn function_check(
    a: &ExecutionArtifact,
    name: &str,
    phase: &str,
    resources: &mut BTreeSet<String>,
    stack: &mut Vec<String>,
) -> Result<(), String> {
    if stack.iter().any(|n| n == name) || stack.len() >= 64 {
        return Err(err(
            code::RECURSIVE_CALL,
            format_args!("`{name}` is reachable from itself, or the call graph is deeper than 64"),
        ));
    }
    if name != phase && !name.starts_with(&format!("{phase}:")) {
        return Err(err(
            code::CROSS_LIFECYCLE_CALL,
            format_args!("`{name}` belongs to a different lifecycle than {phase}"),
        ));
    }
    let function = a
        .program
        .functions
        .get(name)
        .ok_or_else(|| format!("undefined execution function {name}"))?;
    // Declared locals are the whole namespace: a read of anything else is a
    // name-resolution error before any type is considered.
    let mut declared: BTreeMap<String, LocalState> = BTreeMap::new();
    for local in &function.locals {
        if declared
            .insert(local.name.clone(), LocalState::declared(local.ty))
            .is_some()
        {
            return Err(err(
                code::UNDECLARED_LOCAL,
                format_args!("duplicate local `{}`", local.name),
            ));
        }
    }
    stack.push(name.into());
    body_check(
        a,
        function,
        &function.body,
        phase,
        &mut declared,
        resources,
        stack,
    )?;
    stack.pop();
    Ok(())
}

/// A declared local and whether the analysis has seen it assigned yet.
#[derive(Clone, Copy)]
struct LocalState {
    ty: ValueType,
    assigned: bool,
}

impl LocalState {
    const fn declared(ty: ValueType) -> Self {
        Self {
            ty,
            assigned: false,
        }
    }
}

fn body_check(
    a: &ExecutionArtifact,
    function: &Function,
    body: &[Instruction],
    phase: &str,
    locals: &mut BTreeMap<String, LocalState>,
    resources: &mut BTreeSet<String>,
    stack: &mut Vec<String>,
) -> Result<(), String> {
    use Instruction::*;
    for op in body {
        let produced: Option<(&String, ValueType)> = match op {
            Time { result } => snapshot(phase, result, ValueType::Real)?,
            Sequence { result } => snapshot(phase, result, ValueType::Integer)?,
            Phase { result } => snapshot(phase, result, ValueType::Text)?,
            Value { result, .. } => snapshot(phase, result, ValueType::Real)?,
            Compute { result, expr } => {
                let ty = expression_type(function, *expr, locals)?;
                Some((result, ty))
            }
            Open { sink } | Close { sink } | Write { sink, .. } => {
                effect_check(a, CsvEffect::of(op), sink, phase, locals, resources)?;
                None
            }
            If {
                condition,
                then_body,
                else_body,
            } => {
                require(locals, condition, ValueType::Boolean)?;
                // Lexical scope: a local declared outside may be assigned
                // inside; one declared inside is not visible outside. Both
                // branches are checked against a copy, and only assignments
                // to locals that already existed survive the join.
                let mut left = locals.clone();
                let mut right = locals.clone();
                let mut left_resources = resources.clone();
                let mut right_resources = resources.clone();
                body_check(
                    a,
                    function,
                    then_body,
                    phase,
                    &mut left,
                    &mut left_resources,
                    stack,
                )?;
                body_check(
                    a,
                    function,
                    else_body,
                    phase,
                    &mut right,
                    &mut right_resources,
                    stack,
                )?;
                if left_resources != right_resources {
                    return Err(err(
                        code::BRANCH_RESOURCE_MISMATCH,
                        "the two arms leave different resources open",
                    ));
                }
                *resources = left_resources;
                for (name, state) in locals.iter_mut() {
                    let in_both = left.get(name).is_some_and(|l| l.assigned)
                        && right.get(name).is_some_and(|r| r.assigned);
                    state.assigned |= in_both;
                }
                None
            }
            Call { function: callee } => {
                function_check(a, callee, phase, resources, stack)?;
                None
            }
            Assert { condition, message } => {
                require(locals, condition, ValueType::Boolean)?;
                // `Text` is admissible here and as a CSV column, nowhere else.
                match locals.get(message) {
                    Some(state) if state.assigned => Ok(()),
                    Some(_) => Err(err(
                        code::UNDECLARED_LOCAL,
                        format_args!("`{message}` is read before assignment"),
                    )),
                    None => Err(err(
                        code::UNDECLARED_LOCAL,
                        format_args!("`{message}` is not declared"),
                    )),
                }?;
                None
            }
        };
        if let Some((name, ty)) = produced {
            let Some(state) = locals.get_mut(name) else {
                return Err(err(
                    code::UNDECLARED_LOCAL,
                    format_args!("`{name}` is assigned but never declared"),
                ));
            };
            if state.ty != ty {
                return Err(err(
                    code::TYPE_MISMATCH,
                    format_args!("`{name}` is declared {:?} but assigned {:?}", state.ty, ty),
                ));
            }
            state.assigned = true;
        }
    }
    Ok(())
}

fn snapshot<'a>(
    phase: &str,
    result: &'a String,
    ty: ValueType,
) -> Result<Option<(&'a String, ValueType)>, String> {
    if phase != "publish" {
        return Err(err(
            code::SNAPSHOT_LIFECYCLE,
            format_args!("a snapshot read in {phase} has no snapshot to read"),
        ));
    }
    Ok(Some((result, ty)))
}

/// Read a local, requiring it declared, assigned, and of the wanted type.
fn require(
    locals: &BTreeMap<String, LocalState>,
    name: &str,
    want: ValueType,
) -> Result<(), String> {
    let Some(state) = locals.get(name) else {
        return Err(err(
            code::UNDECLARED_LOCAL,
            format_args!("`{name}` is not declared"),
        ));
    };
    if !state.assigned {
        return Err(err(
            code::UNDECLARED_LOCAL,
            format_args!("`{name}` is read before assignment"),
        ));
    }
    if state.ty != want {
        return Err(err(
            code::TYPE_MISMATCH,
            format_args!("`{name}` is {:?} where {want:?} is required", state.ty),
        ));
    }
    Ok(())
}

/// Type of a program-local expression, checking its leaves as it goes.
fn expression_type(
    function: &Function,
    id: ProgramExprId,
    locals: &BTreeMap<String, LocalState>,
) -> Result<ValueType, String> {
    let node = function.expressions.get(id.0 as usize).ok_or_else(|| {
        err(
            code::TYPE_MISMATCH,
            format_args!("expression {} is out of range", id.0),
        )
    })?;
    let ty = match node {
        ProgramExpr::Local { name } => {
            let Some(state) = locals.get(name) else {
                return Err(err(
                    code::UNDECLARED_LOCAL,
                    format_args!("`{name}` is not declared"),
                ));
            };
            if !state.assigned {
                return Err(err(
                    code::UNDECLARED_LOCAL,
                    format_args!("`{name}` is read before assignment"),
                ));
            }
            state.ty
        }
        ProgramExpr::Real { .. } => ValueType::Real,
        ProgramExpr::Integer { .. } => ValueType::Integer,
        ProgramExpr::Boolean { .. } => ValueType::Boolean,
        ProgramExpr::Text { .. } => ValueType::Text,
        ProgramExpr::Unary { operand, .. } => {
            let ty = expression_type(function, *operand, locals)?;
            reject_text(ty, "a unary operator")?;
            ty
        }
        ProgramExpr::Binary { lhs, rhs, .. } => {
            let (l, r) = (
                expression_type(function, *lhs, locals)?,
                expression_type(function, *rhs, locals)?,
            );
            reject_text(l, "arithmetic")?;
            reject_text(r, "arithmetic")?;
            if l != r {
                return Err(err(
                    code::TYPE_MISMATCH,
                    format_args!("binary operands are {l:?} and {r:?}"),
                ));
            }
            l
        }
        ProgramExpr::Compare { lhs, rhs, op } => {
            let (l, r) = (
                expression_type(function, *lhs, locals)?,
                expression_type(function, *rhs, locals)?,
            );
            if l != r {
                return Err(err(
                    code::TYPE_MISMATCH,
                    format_args!("comparison operands are {l:?} and {r:?}"),
                ));
            }
            // Text compares only for equality; ordering it is meaningless here.
            if l == ValueType::Text && !matches!(op, crate::CompareOp::Eq | crate::CompareOp::Ne) {
                return Err(err(
                    code::TEXT_MISUSE,
                    "Text supports only equality comparison",
                ));
            }
            ValueType::Boolean
        }
    };
    Ok(ty)
}

fn reject_text(ty: ValueType, position: &str) -> Result<(), String> {
    if ty == ValueType::Text {
        return Err(err(
            code::TEXT_MISUSE,
            format_args!(
                "Text is not admissible in {position}; it is valid only as a CSV column or an assert message"
            ),
        ));
    }
    Ok(())
}

/// The three CSV effect instructions, as a type.
///
/// The caller's match already establishes which of these an instruction is.
/// Handing `effect_check` a bare `Instruction` threw that proof away, and it
/// cost two `unreachable!()` arms to pretend to recover it -- a run-time
/// assertion standing in for a fact the compiler had one line earlier. This
/// carries the proof across the call instead, so both arms stop existing.
enum CsvEffect<'a> {
    Open,
    Close,
    Write { arguments: &'a [String] },
}

impl<'a> CsvEffect<'a> {
    /// Classify an instruction the caller has already narrowed to an effect.
    ///
    /// The sink is passed separately, from the caller's own binding, so
    /// nothing here has to take one back out of `op`.
    fn of(op: &'a Instruction) -> Self {
        match op {
            Instruction::Write { values, .. } => Self::Write { arguments: values },
            Instruction::Close { .. } => Self::Close,
            _ => Self::Open,
        }
    }
}

fn effect_check(
    a: &ExecutionArtifact,
    effect: CsvEffect<'_>,
    sink: &String,
    phase: &str,
    locals: &BTreeMap<String, LocalState>,
    resources: &mut BTreeSet<String>,
) -> Result<(), String> {
    // Two distinct failures, never merged, and they have different fixes:
    // the *target* cannot write files at all, or it can and the program
    // named a sink it never declared. "The program declares no sinks" is the
    // second question in different words, not the first, so it is not asked
    // here -- the lookup below reports it as EX2-010.
    if !a.lowering.expansion().file_effects {
        return Err(err(
            code::MISSING_FILE_CAPABILITY,
            format_args!(
                "`{sink}` is a file effect, and profile {:?} expands to a target that performs none",
                a.lowering
            ),
        ));
    }
    let decl = a
        .program
        .sinks
        .iter()
        .find(|s| &s.key == sink)
        .ok_or_else(|| {
            err(
                code::UNRESOLVED_SINK,
                format_args!("`{sink}` is not a declared CSV sink"),
            )
        })?;
    match effect {
        CsvEffect::Open => {
            if phase != "run_start" || !resources.insert(sink.clone()) {
                return Err(err(
                    code::EFFECT_LIFECYCLE,
                    format_args!(
                        "csv.open on `{sink}` must open a not-yet-open sink in run_start, not {phase}"
                    ),
                ));
            }
        }
        CsvEffect::Close => {
            if phase != "run_finish" || !resources.remove(sink) {
                return Err(err(
                    code::EFFECT_LIFECYCLE,
                    format_args!(
                        "csv.close on `{sink}` must close an open sink in run_finish, not {phase}"
                    ),
                ));
            }
        }
        CsvEffect::Write { arguments: args } => {
            // Three separate failures with three separate fixes.
            if phase != "publish" || !resources.contains(sink) {
                return Err(err(
                    code::EFFECT_LIFECYCLE,
                    format_args!(
                        "csv.write_row on `{sink}` must write an open sink in publish, not {phase}"
                    ),
                ));
            }
            if args.len() != decl.columns.len() {
                return Err(err(
                    code::ROW_WIDTH,
                    format_args!(
                        "`{sink}` declares {} columns but the row has {} values",
                        decl.columns.len(),
                        args.len()
                    ),
                ));
            }
            // Column order is owned here: the order of `values` is the order
            // of columns in the row. Nothing downstream may reorder them.
            for (arg, column) in args.iter().zip(&decl.columns) {
                require(locals, arg, column.ty)?;
            }
        }
    }

    Ok(())
}
