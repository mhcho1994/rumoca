//! Fold `der(...)` of time-invariant operands to zero (MLS §3.7.4.2).
//!
//! MLS §3.7.4.2 `der(expr)`: "For Real parameters and constants the result is
//! a zero scalar or array of the same size as the variable." A parameter or
//! constant is not a state, so the canonical DAE has no derivative coordinate
//! for it; the operator is folded here, once, where the variability of every
//! flat variable is still known. Package constants that were substituted by
//! value leave `der(<literal>)`, which folds the same way.
//!
//! Only operands whose result shape is known exactly are folded: a whole
//! variable, or a variable selected by scalar subscripts. A slice keeps the
//! call, and DAE validation reports it with a source diagnostic.

use super::*;
use rumoca_core::ExpressionRewriter;

pub(crate) fn fold_time_invariant_derivatives(flat: &mut flat::Model) {
    let invariant = TimeInvariantVars::build(flat);
    let mut folder = DerivativeFolder {
        invariant: &invariant,
    };
    for equation in flat
        .equations
        .iter_mut()
        .chain(flat.initial_equations.iter_mut())
    {
        equation.residual = folder.rewrite_expression(&equation.residual);
    }
    for family in flat
        .structured_equations
        .iter_mut()
        .chain(flat.initial_structured_equations.iter_mut())
    {
        let Some(template) = family.template.as_mut() else {
            continue;
        };
        for body in &mut template.body {
            *body = folder.rewrite_expression(body);
        }
    }
    for variable in flat.variables.values_mut() {
        if let Some(binding) = &variable.binding {
            variable.binding = Some(folder.rewrite_expression(binding));
        }
    }
}

/// Declared extents of every parameter or constant flat variable.
struct TimeInvariantVars {
    dims: rustc_hash::FxHashMap<String, Vec<i64>>,
}

impl TimeInvariantVars {
    fn build(flat: &flat::Model) -> Self {
        let dims = flat
            .variables
            .iter()
            .filter(|(_, variable)| {
                matches!(
                    variable.variability,
                    rumoca_core::Variability::Parameter(_) | rumoca_core::Variability::Constant(_)
                )
            })
            .map(|(name, variable)| (name.as_str().to_string(), variable.dims.clone()))
            .collect();
        Self { dims }
    }
}

struct DerivativeFolder<'a> {
    invariant: &'a TimeInvariantVars,
}

impl DerivativeFolder<'_> {
    /// The exact result extents of `der(argument)` when the operand is time
    /// invariant, or `None` when the operator must stay.
    fn invariant_extents(&self, argument: &Expression) -> Option<Vec<i64>> {
        match argument {
            Expression::Literal {
                value: rumoca_core::Literal::Real(_) | rumoca_core::Literal::Integer(_),
                ..
            } => Some(Vec::new()),
            Expression::VarRef {
                name, subscripts, ..
            } => {
                let dims = self.invariant.dims.get(name.as_str())?;
                if subscripts.is_empty() {
                    return Some(dims.clone());
                }
                let scalar =
                    subscripts.len() == dims.len() && subscripts.iter().all(is_scalar_subscript);
                scalar.then(Vec::new)
            }
            _ => None,
        }
    }
}

fn is_scalar_subscript(subscript: &rumoca_core::Subscript) -> bool {
    match subscript {
        rumoca_core::Subscript::Index { .. } => true,
        rumoca_core::Subscript::Colon { .. } => false,
        rumoca_core::Subscript::Expr { expr, .. } => !matches!(
            expr.as_ref(),
            Expression::Range { .. } | Expression::Array { .. }
        ),
    }
}

/// A Real zero of the given extents, as an explicit array literal.
fn zero_of_extents(extents: &[i64], span: rumoca_core::Span) -> Option<Expression> {
    let Some((&first, rest)) = extents.split_first() else {
        return Some(Expression::Literal {
            value: rumoca_core::Literal::Real(0.0),
            span,
        });
    };
    let count = usize::try_from(first).ok().filter(|count| *count > 0)?;
    let element = zero_of_extents(rest, span)?;
    Some(Expression::Array {
        elements: vec![element; count],
        kind: rumoca_core::ArrayConstructor::Array,
        span,
    })
}

impl ExpressionRewriter for DerivativeFolder<'_> {
    fn rewrite_expression(&mut self, expr: &Expression) -> Expression {
        if let Expression::BuiltinCall {
            function: rumoca_core::BuiltinFunction::Der,
            args,
            span,
        } = expr
            && let [argument] = args.as_slice()
            && let Some(zero) = self
                .invariant_extents(argument)
                .and_then(|extents| zero_of_extents(&extents, *span))
        {
            return zero;
        }
        self.walk_expression(expr)
    }
}
