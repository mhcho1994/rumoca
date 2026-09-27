//! The **derived** numerical program.
//!
//! Not part of the wire. v1 serialized this beside the authored program,
//! which put one lowering's output for one backend into a public artifact;
//! v2 rebuilds it at load from the equation IR and the recorded profile
//! (`rumoca_phase_solve::execution::export`). It lives in its own module so
//! that `execution.rs` contains wire types only, and so the §9 grep for
//! `observations` in the wire module finds nothing.
//!
//! `ScalarOp` remains the internal form here. SPEC_0045 SEV-006 calls it a
//! superseded adapter awaiting deletion; what that forbids is a *stored*
//! scalar projection, and this is no longer stored.
// Deliberately no `Serialize`/`Deserialize`: D1 says this program is rebuilt
// at load, and a type that cannot be serialized cannot drift back onto the
// wire by someone adding a field to an artifact struct.

#[derive(Clone, Debug)]
pub struct NumericalProgram {
    /// Virtual source owner for generated executable instructions.
    pub source_name: String,
    pub storage: Vec<Storage>,
    pub residual: Vec<Row>,
    pub derivatives: Vec<Row>,
    pub initialization: Vec<Row>,
    pub algebraic_blocks: Vec<Projection>,
    pub initial_blocks: Vec<Projection>,
    pub observations: Vec<Observation>,
}

#[derive(Clone, Debug)]
pub struct Storage {
    /// The model variable this slot holds, when the deriving caller knows it.
    ///
    /// Populated at derivation by the layer that has the equation IR. It
    /// exists so a run-local override resolves `VariableId -> slot` instead of
    /// matching storage by name, which was the v1 behaviour and broke the
    /// post-resolution identity rule.
    pub variable_id: Option<u32>,
    pub name: String,
    pub role: String,
    pub causality: String,
    pub index: usize,
    pub start: f64,
    pub nominal: f64,
    pub unit: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub output: usize,
    pub target: Option<usize>,
    pub instructions: Vec<ScalarOp>,
}

#[derive(Clone, Debug)]
pub struct Projection {
    pub rows: Vec<usize>,
    pub unknowns: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct Observation {
    /// The trace point this observation answers. Keyed by the request, not by
    /// the variable: the trace point is the single registration site (D2).
    pub trace_point: super::TracePointRef,
    pub variable_id: u32,
    pub name: String,
    /// Physical role, borrowed from the trace point at derivation. Present so
    /// an emitted manifest can say `Q_flow` is a flow without the reader
    /// inferring it from the name.
    pub quantity: Option<String>,
    /// Connector instance path and member name, when this observation is a
    /// connector member. Taken from `RbcConnectorInstance`, never by
    /// splitting `name` on `.`: a component called `a.b` and a component `a`
    /// owning `b` produce the same flattened text.
    pub owner: Option<String>,
    pub member: Option<String>,
    pub instructions: Vec<ScalarOp>,
}

/// Operand/register semantics are exactly those of canonical Solve LinearOp.
#[derive(Clone, Debug)]
pub enum ScalarOp {
    Const {
        dst: u32,
        value: f64,
    },
    LoadTime {
        dst: u32,
    },
    LoadY {
        dst: u32,
        index: usize,
    },
    LoadP {
        dst: u32,
        index: usize,
    },
    Unary {
        dst: u32,
        operator: String,
        src: u32,
    },
    Binary {
        dst: u32,
        operator: String,
        lhs: u32,
        rhs: u32,
    },
    Compare {
        dst: u32,
        operator: String,
        lhs: u32,
        rhs: u32,
    },
    Select {
        dst: u32,
        cond: u32,
        if_true: u32,
        if_false: u32,
    },
    StoreOutput {
        src: u32,
    },
}

impl ScalarOp {
    pub fn from_solve(op: &crate::LinearOp) -> Result<Self, String> {
        use crate::LinearOp as L;
        Ok(match op {
            L::Const { dst, value } => Self::Const {
                dst: *dst,
                value: *value,
            },
            L::LoadTime { dst } => Self::LoadTime { dst: *dst },
            L::LoadY { dst, index } => Self::LoadY {
                dst: *dst,
                index: *index,
            },
            L::LoadP { dst, index } => Self::LoadP {
                dst: *dst,
                index: *index,
            },
            L::Unary { dst, op, arg } => Self::Unary {
                dst: *dst,
                operator: op.kind_name().into(),
                src: *arg,
            },
            L::Binary { dst, op, lhs, rhs } => Self::Binary {
                dst: *dst,
                operator: op.kind_name().into(),
                lhs: *lhs,
                rhs: *rhs,
            },
            L::Compare { dst, op, lhs, rhs } => Self::Compare {
                dst: *dst,
                operator: op.kind_name().into(),
                lhs: *lhs,
                rhs: *rhs,
            },
            L::Select {
                dst,
                cond,
                if_true,
                if_false,
            } => Self::Select {
                dst: *dst,
                cond: *cond,
                if_true: *if_true,
                if_false: *if_false,
            },
            L::StoreOutput { src } => Self::StoreOutput { src: *src },
            other => {
                return Err(format!(
                    "execution v1 unsupported Solve operation: {}",
                    other.kind_name()
                ));
            }
        })
    }

    pub fn to_solve(&self) -> Result<crate::LinearOp, String> {
        use crate::LinearOp as L;
        fn operator<T: serde::de::DeserializeOwned>(name: &str) -> Result<T, String> {
            serde_json::from_value(serde_json::Value::String(name.into()))
                .map_err(|e| e.to_string())
        }
        Ok(match self {
            Self::Const { dst, value } => L::Const {
                dst: *dst,
                value: *value,
            },
            Self::LoadTime { dst } => L::LoadTime { dst: *dst },
            Self::LoadY { dst, index } => L::LoadY {
                dst: *dst,
                index: *index,
            },
            Self::LoadP { dst, index } => L::LoadP {
                dst: *dst,
                index: *index,
            },
            Self::Unary {
                dst,
                operator: op,
                src,
            } => L::Unary {
                dst: *dst,
                op: operator(op)?,
                arg: *src,
            },
            Self::Binary {
                dst,
                operator: op,
                lhs,
                rhs,
            } => L::Binary {
                dst: *dst,
                op: operator(op)?,
                lhs: *lhs,
                rhs: *rhs,
            },
            Self::Compare {
                dst,
                operator: op,
                lhs,
                rhs,
            } => L::Compare {
                dst: *dst,
                op: operator(op)?,
                lhs: *lhs,
                rhs: *rhs,
            },
            Self::Select {
                dst,
                cond,
                if_true,
                if_false,
            } => L::Select {
                dst: *dst,
                cond: *cond,
                if_true: *if_true,
                if_false: *if_false,
            },
            Self::StoreOutput { src } => L::StoreOutput { src: *src },
        })
    }
}

pub fn checked_scalar(
    ops: &[ScalarOp],
    y: usize,
    p: usize,
) -> Result<Vec<crate::LinearOp>, String> {
    if ops
        .iter()
        .filter(|o| matches!(o, ScalarOp::StoreOutput { .. }))
        .count()
        != 1
    {
        return Err("scalar program must store exactly one output".into());
    }
    if !matches!(ops.last(), Some(ScalarOp::StoreOutput { .. })) {
        return Err("scalar program must end with its output store".into());
    }
    for op in ops {
        let dst = match op {
            ScalarOp::Const { dst, .. }
            | ScalarOp::LoadTime { dst }
            | ScalarOp::LoadY { dst, .. }
            | ScalarOp::LoadP { dst, .. }
            | ScalarOp::Unary { dst, .. }
            | ScalarOp::Binary { dst, .. }
            | ScalarOp::Compare { dst, .. }
            | ScalarOp::Select { dst, .. } => Some(*dst),
            ScalarOp::StoreOutput { .. } => None,
        };
        if dst.is_some_and(|d| d as usize >= ops.len()) {
            return Err("register exceeds scalar program size".into());
        }
        match op {
            ScalarOp::LoadY { index, .. } if *index >= y => {
                return Err("LoadY outside storage".into());
            }
            ScalarOp::LoadP { index, .. } if *index >= p => {
                return Err("LoadP outside storage".into());
            }
            ScalarOp::Const { value, .. } if !value.is_finite() => {
                return Err("non-finite constant".into());
            }
            _ => {}
        }
    }
    let row = ops
        .iter()
        .map(ScalarOp::to_solve)
        .collect::<Result<Vec<_>, _>>()?;
    let span = rumoca_core::Span::from_offsets(
        rumoca_core::SourceId::from_source_name("rbc:execution/scalar-check"),
        0,
        0,
    );
    crate::ScalarProgramBlock::with_program_spans(vec![row.clone()], vec![span])
        .map_err(|e| e.to_string())?;
    Ok(row)
}
