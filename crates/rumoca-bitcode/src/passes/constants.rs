//! `fold-constants`: evaluate operators whose operands are literals.

use super::*;

/// Replace an operator node whose operands are all literals with the literal
/// it evaluates to.
///
/// Deliberately narrow: arithmetic, comparison and logic on scalars, with
/// Modelica's promotion rule (an Integer meets a Real as a Real) and no fold
/// where the runtime would report something -- integer overflow, division
/// by zero, a non-finite result. Built-in functions are not evaluated here:
/// whether `sin(1.0)` folds to the bits the runtime would compute depends on
/// which math library the backend links, and a pass must not change a result.
pub(super) struct FoldConstants;

impl Pass for FoldConstants {
    fn name(&self) -> &'static str {
        "fold-constants"
    }
    fn description(&self) -> &'static str {
        "evaluate scalar operators whose operands are literals"
    }
    fn run(&self, model: &mut RbcModel) -> Result<usize, PassError> {
        let mut folded = 0;
        // A relation's root stays relational: the DAE owns it as a zero
        // crossing, and a literal is not one. `fold-asserts` evaluates such a
        // comparison itself when it needs the value.
        let relation_roots = model
            .relations
            .iter()
            .map(|relation| relation.expression.0 as usize)
            .collect::<std::collections::HashSet<_>>();
        // Operands precede their users, so one forward sweep folds chains.
        for index in 0..model.expressions.len() {
            if relation_roots.contains(&index) {
                continue;
            }
            let value = match &model.expressions[index].node {
                RbcExprNode::Unary { op, operand } => {
                    literal(model, *operand).and_then(|value| unary(*op, value))
                }
                RbcExprNode::Binary { op, lhs, rhs } => literal(model, *lhs)
                    .zip(literal(model, *rhs))
                    .and_then(|(lhs, rhs)| binary(*op, lhs, rhs)),
                _ => None,
            };
            if let Some(value) = value {
                model.expressions[index].node = RbcExprNode::Literal {
                    value: value.into(),
                };
                folded += 1;
            }
        }
        Ok(folded)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Scalar {
    Real(f64),
    Integer(i64),
    Boolean(bool),
}

impl From<Scalar> for RbcLiteral {
    fn from(value: Scalar) -> Self {
        match value {
            Scalar::Real(value) => RbcLiteral::Real { value },
            Scalar::Integer(value) => RbcLiteral::Integer { value },
            Scalar::Boolean(value) => RbcLiteral::Boolean { value },
        }
    }
}

pub(super) fn literal(model: &RbcModel, id: ExprId) -> Option<Scalar> {
    match &model.expressions.get(id.0 as usize)?.node {
        RbcExprNode::Literal { value } => match value {
            RbcLiteral::Real { value } => Some(Scalar::Real(*value)),
            RbcLiteral::Integer { value } => Some(Scalar::Integer(*value)),
            RbcLiteral::Boolean { value } => Some(Scalar::Boolean(*value)),
            _ => None,
        },
        _ => None,
    }
}

fn real(value: f64) -> Option<Scalar> {
    value.is_finite().then_some(Scalar::Real(value))
}

fn unary(op: RbcUnaryOp, value: Scalar) -> Option<Scalar> {
    match (op, value) {
        (RbcUnaryOp::Negate, Scalar::Real(v)) => real(-v),
        (RbcUnaryOp::Negate, Scalar::Integer(v)) => v.checked_neg().map(Scalar::Integer),
        (RbcUnaryOp::Not, Scalar::Boolean(v)) => Some(Scalar::Boolean(!v)),
        (RbcUnaryOp::Plus, value @ (Scalar::Real(_) | Scalar::Integer(_))) => Some(value),
        _ => None,
    }
}

pub(super) fn binary(op: RbcBinaryOp, lhs: Scalar, rhs: Scalar) -> Option<Scalar> {
    use RbcBinaryOp as Op;
    match (lhs, rhs) {
        (Scalar::Integer(a), Scalar::Integer(b)) => match op {
            Op::Add => a.checked_add(b).map(Scalar::Integer),
            Op::Subtract => a.checked_sub(b).map(Scalar::Integer),
            Op::Multiply => a.checked_mul(b).map(Scalar::Integer),
            // MLS §3.4: `/` on two Integers is Real division.
            Op::Divide if b != 0 => real(a as f64 / b as f64),
            _ => compare(op, a.cmp(&b)),
        },
        (Scalar::Boolean(a), Scalar::Boolean(b)) => match op {
            Op::And => Some(Scalar::Boolean(a && b)),
            Op::Or => Some(Scalar::Boolean(a || b)),
            Op::Equal => Some(Scalar::Boolean(a == b)),
            Op::NotEqual => Some(Scalar::Boolean(a != b)),
            _ => None,
        },
        (Scalar::Boolean(_), _) | (_, Scalar::Boolean(_)) => None,
        (a, b) => {
            let (a, b) = (as_real(a)?, as_real(b)?);
            match op {
                Op::Add => real(a + b),
                Op::Subtract => real(a - b),
                Op::Multiply => real(a * b),
                Op::Divide if b != 0.0 => real(a / b),
                _ => compare(op, a.partial_cmp(&b)?),
            }
        }
    }
}

fn as_real(value: Scalar) -> Option<f64> {
    match value {
        Scalar::Real(v) => Some(v),
        Scalar::Integer(v) => Some(v as f64),
        Scalar::Boolean(_) => None,
    }
}

fn compare(op: RbcBinaryOp, ordering: std::cmp::Ordering) -> Option<Scalar> {
    use RbcBinaryOp as Op;
    use std::cmp::Ordering::{Equal, Greater, Less};
    let result = match op {
        Op::Equal => ordering == Equal,
        Op::NotEqual => ordering != Equal,
        Op::Less => ordering == Less,
        Op::LessEqual => ordering != Greater,
        Op::Greater => ordering == Greater,
        Op::GreaterEqual => ordering != Less,
        _ => return None,
    };
    Some(Scalar::Boolean(result))
}
