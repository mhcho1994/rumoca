//! A state derivative isolated from a sum of terms.
//!
//! The continuous rows name the derivative they determine, and the Solve
//! lowering needs it isolated. `der(x) = f` and `c*der(x) = f` are read
//! directly; this handles the derivative as one term of a sum on either side,
//! as in `umin + Tfilter*der(umin) = if ... then u else umin` -- affine in
//! `der(umin)`, but not with the derivative term at the top of the residual.
//!
//! The side holding the derivative is flattened through `+`, `-` and unary
//! `-`/`+` into signed terms. Exactly one term may contain a derivative, and
//! it must peel down to `der(x)` through products and quotients whose other
//! operand is derivative-free (`J*der(w)*w`, `der(x)/T`, `-(k*der(x))`).
//! Neither the other terms nor the other side may contain a derivative the
//! caller cannot substitute (`DerivativeReads`):
//!
//! ```text
//! s_d * (Π a_j / Π b_k) * der(x) + Σ_i s_i * t_i = other   =>
//! der(x) = (other - Σ_i s_i * t_i) / Π a_j * Π b_k * s_d
//! ```
//!
//! Anything else is left to the forms that refuse it with their own reason.

use super::*;

/// One term of a flattened sum, with the sign it carries.
#[derive(Clone, Copy)]
pub(super) struct SignedTerm<'dae> {
    pub(super) expression: dae::ExprId<'dae>,
    pub(super) scalar: usize,
    pub(super) negated: bool,
}

/// One derivative-free factor between the derivative term and `der(x)`.
#[derive(Clone, Copy)]
pub(super) struct Factor<'dae> {
    pub(super) expression: dae::ExprId<'dae>,
    pub(super) scalar: usize,
    /// The factor divides the derivative (`der(x)/b`) rather than scaling it.
    pub(super) divides: bool,
}

/// The pieces `der(x) = (other - Σ offsets) / coefficient` is assembled from.
pub(super) struct SummedDerivative<'dae> {
    /// The factors the derivative is scaled by, outermost first; empty for a
    /// bare `der(x)` term.
    pub(super) factors: Vec<Factor<'dae>>,
    /// Whether the derivative term enters the sum negated.
    pub(super) derivative_negated: bool,
    /// The other terms on the derivative's side.
    pub(super) offsets: Vec<SignedTerm<'dae>>,
}

fn operand_scalar<'dae>(
    view: dae::DaeView<'dae>,
    operand: dae::ExprId<'dae>,
    scalar: usize,
) -> usize {
    if scalar_count(view, operand) == 1 {
        0
    } else {
        scalar
    }
}

fn flatten<'dae>(
    selector: &ScalarSelector<'dae>,
    term: SignedTerm<'dae>,
    out: &mut Vec<SignedTerm<'dae>>,
) -> Result<(), LowerError> {
    let view = selector.view();
    let expression = selector.structural_branch(term.expression, term.scalar)?;
    let node = selector.node(expression);
    let child = |operand, negated| SignedTerm {
        expression: operand,
        scalar: operand_scalar(view, operand, term.scalar),
        negated,
    };
    match node.operation() {
        dae::ExpressionOperation::Binary {
            operator: dae::BinaryOperator::Add | dae::BinaryOperator::ElementwiseAdd,
            lhs,
            rhs,
        } => {
            flatten(selector, child(lhs, term.negated), out)?;
            flatten(selector, child(rhs, term.negated), out)
        }
        dae::ExpressionOperation::Binary {
            operator: dae::BinaryOperator::Subtract | dae::BinaryOperator::ElementwiseSubtract,
            lhs,
            rhs,
        } => {
            flatten(selector, child(lhs, term.negated), out)?;
            flatten(selector, child(rhs, !term.negated), out)
        }
        dae::ExpressionOperation::Unary {
            operator: dae::UnaryOperator::Negate,
            operand,
        } => flatten(selector, child(operand, !term.negated), out),
        dae::ExpressionOperation::Unary {
            operator: dae::UnaryOperator::Plus,
            operand,
        } => flatten(selector, child(operand, term.negated), out),
        _ => {
            out.push(SignedTerm { expression, ..term });
            Ok(())
        }
    }
}

/// Isolate `der(state[state_scalar])` from `side`, or `None` if it is not a
/// single affine term of a sum.
pub(super) fn summed_derivative<'dae>(
    selector: &ScalarSelector<'dae>,
    side: dae::ExprId<'dae>,
    scalar: usize,
    (state, state_scalar): (dae::StateId<'dae>, usize),
    reads: DerivativeReads<'dae>,
) -> Result<Option<SummedDerivative<'dae>>, LowerError> {
    let view = selector.view();
    let mut terms = Vec::new();
    flatten(
        selector,
        SignedTerm {
            expression: side,
            scalar,
            negated: false,
        },
        &mut terms,
    )?;
    let mut found = None;
    let mut offsets = Vec::with_capacity(terms.len());
    for term in terms {
        if !reads.blocked(view, term.expression) {
            offsets.push(term);
            continue;
        }
        if found.is_some() {
            return Ok(None);
        }
        let mut factors = Vec::new();
        let mut negated = term.negated;
        if !peel(
            selector,
            term,
            (state, state_scalar),
            reads,
            &mut factors,
            &mut negated,
        )? {
            return Ok(None);
        }
        found = Some((factors, negated));
    }
    Ok(found.map(|(factors, derivative_negated)| SummedDerivative {
        factors,
        derivative_negated,
        offsets,
    }))
}

/// Peel `term` down to `der(state)` through derivative-free products and
/// quotients, collecting the factors; `false` if it does not reduce to it.
fn peel<'dae>(
    selector: &ScalarSelector<'dae>,
    term: SignedTerm<'dae>,
    target: (dae::StateId<'dae>, usize),
    reads: DerivativeReads<'dae>,
    factors: &mut Vec<Factor<'dae>>,
    negated: &mut bool,
) -> Result<bool, LowerError> {
    let view = selector.view();
    let (state, state_scalar) = target;
    let mut expression = selector.structural_branch(term.expression, term.scalar)?;
    let mut scalar = term.scalar;
    loop {
        if is_target_derivative(selector, expression, scalar, state, state_scalar)? {
            return Ok(true);
        }
        let node = selector.node(expression);
        let (next, factor) = match node.operation() {
            dae::ExpressionOperation::Unary {
                operator: dae::UnaryOperator::Negate,
                operand,
            } => {
                *negated = !*negated;
                (operand, None)
            }
            dae::ExpressionOperation::Unary {
                operator: dae::UnaryOperator::Plus,
                operand,
            } => (operand, None),
            dae::ExpressionOperation::Binary {
                operator: dae::BinaryOperator::Multiply | dae::BinaryOperator::ElementwiseMultiply,
                lhs,
                rhs,
            } => match (reads.blocked(view, lhs), reads.blocked(view, rhs)) {
                (true, false) => (lhs, Some((rhs, false))),
                (false, true) => (rhs, Some((lhs, false))),
                _ => return Ok(false),
            },
            dae::ExpressionOperation::Binary {
                operator: dae::BinaryOperator::Divide | dae::BinaryOperator::ElementwiseDivide,
                lhs,
                rhs,
            } if !reads.blocked(view, rhs) => (lhs, Some((rhs, true))),
            _ => return Ok(false),
        };
        if let Some((operand, divides)) = factor {
            let operand_scalar = operand_scalar(view, operand, scalar);
            let operand = selector.structural_branch(operand, operand_scalar)?;
            if !divides {
                reject_zero_coefficient(
                    selector,
                    (operand, operand_scalar),
                    node.provenance().span(),
                )?;
            }
            factors.push(Factor {
                expression: operand,
                scalar: operand_scalar,
                divides,
            });
        }
        scalar = operand_scalar(view, next, scalar);
        expression = selector.structural_branch(next, scalar)?;
    }
}
