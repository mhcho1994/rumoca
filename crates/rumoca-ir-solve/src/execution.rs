//! Public RBC execution v2: the *authored host program*.
//!
//! v1 serialized a derived numerical program beside the authored one. That was
//! a cache of one lowering for one backend sitting in a public artifact whose
//! own spec gives "do not bake one solver's choices into the model" as the
//! reason equation IR and execution IR are separate. v2 carries only what a
//! pass wrote; the numerical program is derived at load by
//! `rumoca_phase_solve::execution::export` from the equation IR and the
//! recorded profile, and never reaches the wire.
//!
//! This is not the private Solve wire. Certificates, caches, byte offsets and
//! differentiated programs are reconstructed by the lowering owner.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
mod derived;
mod validate;
pub use derived::*;
// Named explicitly, not only through the glob: the derived program's scalar
// checker is called across crates as `execution::checked_scalar`, and a glob
// alone leaves no declaration for that path to resolve against.
pub use derived::checked_scalar;
/// Analyses 1-4: name resolution, type, effect ordering, termination. The
/// fifth, resolving references against the model, needs the equation IR and
/// so lives in `rumoca-bitcode`, which can see both; it reports through the
/// same `code` table so a caller matches one set of codes.
pub use validate::{code, validate};

/// Current public execution contract version. v1 is rejected outright: a
/// superseded reader is not carried (SPEC_0007).
pub const EXECUTION_VERSION: u32 = 2;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionArtifact {
    pub version: u32,
    /// The profile the derived program is built with. Named, not open: a pass
    /// selects a compiler-known profile, it does not describe one.
    pub lowering: LoweringProfile,
    /// Hash over the identities this program actually depends on. Replaces v1's
    /// hash of the whole serialized model, which made an unrelated residual
    /// edit invalidate a logging program that referenced neither.
    pub dependency_digest: String,
    pub revision: u64,
    pub passes: Vec<PassRecord>,
    /// The authored part, and the only program on the wire.
    pub program: HostProgram,
}

/// Functions and the resources their effects name.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostProgram {
    /// Each function declares its locals; a read of an undeclared or
    /// unassigned local is a validation error, not a runtime one.
    pub functions: BTreeMap<String, Function>,
    pub sinks: Vec<CsvSink>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Function {
    pub locals: Vec<Local>,
    /// Program-local expression arena. Separate from `RbcModel.expressions`
    /// because an execution pass must never write into the equation IR
    /// (SPEC_RUMOCA_BITCODE §2a rule 3); one *vocabulary*, two arenas.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expressions: Vec<ProgramExpr>,
    pub body: Vec<Instruction>,
}

/// Index into one `Function::expressions`. Deliberately not `ExprId`: a model
/// expression id and a program expression id are different spaces, and a
/// newtype stops them meeting at a call site.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProgramExprId(pub u32);

/// A model trace point, as this crate can name one.
///
/// Deliberately a separate newtype from `rumoca_bitcode`'s `TracePointId`
/// rather than a re-export: `rumoca-ir-solve` sits below `rumoca-bitcode` in
/// the SPEC_0029 tier order and cannot see it. The reason for having a
/// newtype at all is the `ProgramExprId` reason — a trace point id, a
/// connector id and a program expression index are three different spaces
/// that are all `u32`, and a bare `u32` lets any two of them meet at a call
/// site without complaint. `#[serde(transparent)]`, so the wire still carries
/// a bare integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TracePointRef(pub u32);

impl std::fmt::Display for TracePointRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A model connector instance, as this crate can name one. Same reasoning as
/// [`TracePointRef`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConnectorRef(pub u32);

impl std::fmt::Display for ConnectorRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// One node of a program-local expression.
///
/// Operands name earlier nodes, so the arena is acyclic by construction, as in
/// the equation IR. Leaves are declared locals and literals only.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "node", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProgramExpr {
    /// Read a declared local. The only way a value enters an expression.
    Local {
        name: String,
    },
    Real {
        value: f64,
    },
    Integer {
        value: i64,
    },
    Boolean {
        value: bool,
    },
    Text {
        value: String,
    },
    Unary {
        op: crate::UnaryOp,
        operand: ProgramExprId,
    },
    Binary {
        op: crate::BinaryOp,
        lhs: ProgramExprId,
        rhs: ProgramExprId,
    },
    Compare {
        op: crate::CompareOp,
        lhs: ProgramExprId,
        rhs: ProgramExprId,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Local {
    pub name: String,
    pub ty: ValueType,
}

/// The value kinds a host program can hold. `Text` is deliberately restricted:
/// see the validator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    Real,
    Integer,
    Boolean,
    Text,
}

/// A compiler-known lowering profile, expanded to normalized fields.
///
/// An enum, not an open struct: a pass selects a profile the compiler knows,
/// so a profile cannot be invented on the wire. `dependency_digest` hashes the
/// *expansion*, so adding a field to it invalidates dependent programs, which
/// is the intended behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LoweringProfile {
    #[default]
    SolveScalarV1,
}

impl LoweringProfile {
    /// Normalized fields this profile expands to. The digest hashes these.
    pub const fn expansion(self) -> ProfileExpansion {
        match self {
            Self::SolveScalarV1 => ProfileExpansion {
                name: "solve-scalar-v1",
                backend_family: "rk-like",
                scalar_only: true,
                file_effects: true,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileExpansion {
    pub name: &'static str,
    pub backend_family: &'static str,
    pub scalar_only: bool,
    /// Whether a target built from this profile can perform file effects at
    /// all. `true` for every profile that exists today, which is why
    /// `EX2-011` is currently unreachable; the field is here so that the
    /// capability question has an answer other than "count the sinks", which
    /// is a different question with a different fix.
    pub file_effects: bool,
}

/// A receipt for one pass that edited this program.
///
/// Identity is a canonical path, never a `VariableId` or `TracePointId`: a
/// receipt has to stay readable across a recompilation that renumbers ids.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PassRecord {
    pub id: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, PassOption>,
}

/// A pass option. Closed: an unknown option kind is rejected at parse, not
/// carried as an opaque blob.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PassOption {
    Real(f64),
    Integer(i64),
    Boolean(bool),
    Text(String),
}

/// A CSV resource an effect may name. An opaque capability handle, not an
/// operation: the validator requires a declared file-effect capability before
/// admitting any `csv.*` effect, and reports an undeclared id separately from
/// a target that cannot write files at all.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CsvSink {
    pub key: String,
    pub filename: String,
    /// Name and type together, so a count mismatch between them is
    /// unrepresentable rather than validated.
    pub columns: Vec<SinkColumn>,
    pub metadata: SinkMetadata,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SinkColumn {
    pub name: String,
    pub ty: ValueType,
}

/// Identity only.
///
/// v1 restated `unit`, `kind` and a stringly `variable_id` here, making the
/// sink a second and weaker owner of connector identity. All three are
/// reachable from the trace point, so the wire names the trace point and the
/// runtime's manifest writer denormalizes them at `run`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SinkMetadata {
    /// Same-artifact reference. An id is admissible because a sink lives in the
    /// file its model does; the canonical-path rule binds `PassRecord`, which
    /// must survive a recompilation.
    ///
    /// One option, not two: a connector without an orientation is not a
    /// thing, and two independent `Option`s make that state representable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector: Option<ConnectorBinding>,
    /// Always present: the trace points this sink's rows are about. This is
    /// the same identity a `snapshot.value` names, so a sink's references are
    /// visible without walking its instructions.
    pub members: Vec<SinkMember>,
}

/// What a connector-instrumentation sink is attached to.
///
/// Absent from a sink that is not connector instrumentation. Naming a
/// connector such a sink does not belong to would state a fact a reader could
/// act on.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorBinding {
    pub connector: ConnectorRef,
    pub orientation: Orientation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    Inside,
    Outside,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SinkMember {
    pub trace_point: TracePointRef,
}

/// All effects are sequential. Branch regions and calls preserve that order;
/// none of these operations may be treated as an unused pure expression.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub enum Instruction {
    #[serde(rename = "snapshot.time")]
    Time { result: String },
    #[serde(rename = "snapshot.sequence")]
    Sequence { result: String },
    #[serde(rename = "snapshot.phase")]
    Phase { result: String },
    /// Observes the value a trace point requests. The reference is canonical
    /// (`TracePointId`), never a `VariableId` and never a storage index: the
    /// trace point is the single registration site (D2).
    #[serde(rename = "snapshot.value")]
    Value {
        result: String,
        trace_point: TracePointRef,
    },
    /// Evaluate a program-local expression into a declared local.
    ///
    /// v1 carried a register program from the derived form here and fed it
    /// through `arguments`. v2 carries an expression id into its own arena;
    /// the arena's leaves are declared locals, never `VariableId`, so a
    /// program cannot read a model variable except through `snapshot.value`.
    #[serde(rename = "compute")]
    Compute { result: String, expr: ProgramExprId },
    #[serde(rename = "csv.open")]
    Open { sink: String },
    #[serde(rename = "csv.write_row")]
    Write { sink: String, values: Vec<String> },
    #[serde(rename = "csv.close")]
    Close { sink: String },
    #[serde(rename = "if")]
    If {
        condition: String,
        then_body: Vec<Instruction>,
        else_body: Vec<Instruction>,
    },
    #[serde(rename = "call")]
    Call { function: String },
    #[serde(rename = "assert")]
    Assert { condition: String, message: String },
}
