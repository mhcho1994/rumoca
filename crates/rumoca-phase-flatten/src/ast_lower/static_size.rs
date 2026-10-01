//! `size(e, k)` of an expression whose shape is fixed by its syntax.
//!
//! MLS §10.3.1: `size(A, i)` is a constant Integer when `A`'s extents are. For
//! an array literal (`{"a", "b"}`) or a `fill`/`zeros`/`ones` call with literal
//! extents the extent is known at lowering, and the checked DAE requires it as
//! a literal wherever it is an array extent (e.g. MSL `PartialMedium`:
//! `nX = size(substanceNames, 1)`, `X_default = fill(1/nX, nX)`).

use rumoca_core::{BuiltinFunction, Expression, Literal};

/// The literal `size(base, axis)` when `base`'s shape is syntactic.
pub(crate) fn fold_static_size(args: &[Expression]) -> Option<Expression> {
    let [
        base,
        Expression::Literal {
            value: Literal::Integer(axis),
            span,
        },
    ] = args
    else {
        return None;
    };
    let shape = static_shape(base)?;
    let index = usize::try_from(*axis).ok()?.checked_sub(1)?;
    let extent = *shape.get(index)?;
    Some(Expression::Literal {
        value: Literal::Integer(extent),
        span: *span,
    })
}

fn static_shape(expr: &Expression) -> Option<Vec<i64>> {
    match expr {
        Expression::Literal { .. } => Some(Vec::new()),
        Expression::Array { elements, .. } => {
            let mut shape = vec![i64::try_from(elements.len()).ok()?];
            let Some(first) = elements.first() else {
                return Some(shape);
            };
            let inner = static_shape(first)?;
            if elements
                .iter()
                .skip(1)
                .any(|element| static_shape(element).as_ref() != Some(&inner))
            {
                return None;
            }
            shape.extend(inner);
            Some(shape)
        }
        Expression::BuiltinCall { function, args, .. } => match function {
            BuiltinFunction::Fill => {
                let (value, extents) = args.split_first()?;
                let mut shape = literal_extents(extents)?;
                shape.extend(static_shape(value)?);
                Some(shape)
            }
            BuiltinFunction::Zeros | BuiltinFunction::Ones => literal_extents(args),
            _ => None,
        },
        _ => None,
    }
}

fn literal_extents(args: &[Expression]) -> Option<Vec<i64>> {
    args.iter()
        .map(|arg| match arg {
            Expression::Literal {
                value: Literal::Integer(extent),
                ..
            } if *extent >= 0 => Some(*extent),
            _ => None,
        })
        .collect()
}
