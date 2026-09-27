//! MLS §10.5: scalar indices remove an axis; vector indices and `:` retain it.
use super::{EvalError, Expression, Subscript, Value};
use rumoca_core::Span;

pub(super) fn apply_subscripts(
    value: &Value,
    subscripts: &[Subscript],
    span: Span,
    evaluate: &mut impl FnMut(&Expression) -> Result<Value, EvalError>,
) -> Result<Value, EvalError> {
    let Some((first, remaining)) = subscripts.split_first() else {
        return Ok(value.clone());
    };
    let array = value
        .as_array()
        .ok_or_else(|| EvalError::type_mismatch("Array", value.type_name(), span))?;
    match first {
        Subscript::Index { value, span } => {
            apply_subscripts(element(array, *value, *span)?, remaining, *span, evaluate)
        }
        Subscript::Colon { span } => array
            .iter()
            .map(|value| apply_subscripts(value, remaining, *span, evaluate))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Subscript::Expr { expr, span } => match evaluate(expr)? {
            Value::Integer(index) => {
                apply_subscripts(element(array, index, *span)?, remaining, *span, evaluate)
            }
            Value::Array(indices) => indices
                .iter()
                .map(|index| {
                    let index = index.as_integer().ok_or_else(|| {
                        EvalError::type_mismatch("Integer", index.type_name(), *span)
                    })?;
                    apply_subscripts(element(array, index, *span)?, remaining, *span, evaluate)
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array),
            other => Err(EvalError::type_mismatch(
                "Integer or Integer vector",
                other.type_name(),
                *span,
            )),
        },
    }
}

fn element(array: &[Value], index: i64, span: Span) -> Result<&Value, EvalError> {
    usize::try_from(index)
        .ok()
        .and_then(|index| index.checked_sub(1))
        .and_then(|index| array.get(index))
        .ok_or(EvalError::IndexOutOfBounds {
            index,
            size: array.len(),
            span,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn integer(value: i64) -> Value {
        Value::Integer(value)
    }
    fn vector(values: &[i64]) -> Value {
        Value::Array(values.iter().copied().map(integer).collect())
    }
    fn selector() -> Subscript {
        Subscript::Expr {
            expr: Box::new(Expression::Literal {
                value: rumoca_core::Literal::Integer(0),
                span: Span::DUMMY,
            }),
            span: Span::DUMMY,
        }
    }
    #[test]
    fn vector_selection_preserves_order_and_duplicates() {
        let result = apply_subscripts(
            &vector(&[10, 20, 30]),
            &[selector()],
            Span::DUMMY,
            &mut |_| Ok(vector(&[3, 1, 3])),
        )
        .unwrap();
        assert_eq!(result, vector(&[30, 10, 30]));
    }
    #[test]
    fn colon_retains_axis_before_scalar_index() {
        let matrix = Value::Array(vec![vector(&[1, 2]), vector(&[3, 4])]);
        let result = apply_subscripts(
            &matrix,
            &[
                Subscript::colon(Span::DUMMY),
                Subscript::index(2, Span::DUMMY),
            ],
            Span::DUMMY,
            &mut |_| unreachable!(),
        )
        .unwrap();
        assert_eq!(result, vector(&[2, 4]));
    }
    #[test]
    fn vector_selection_reports_exact_out_of_bounds_coordinate() {
        let error = apply_subscripts(&vector(&[10]), &[selector()], Span::DUMMY, &mut |_| {
            Ok(vector(&[1, 2]))
        })
        .unwrap_err();
        assert!(matches!(
            error,
            EvalError::IndexOutOfBounds {
                index: 2,
                size: 1,
                ..
            }
        ));
    }
    #[test]
    fn negative_indices_keep_the_signed_diagnostic() {
        let error = apply_subscripts(
            &vector(&[10]),
            &[Subscript::index(-1, Span::DUMMY)],
            Span::DUMMY,
            &mut |_| unreachable!(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            EvalError::IndexOutOfBounds {
                index: -1,
                size: 1,
                ..
            }
        ));
    }
}
