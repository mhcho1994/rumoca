//! Translation-time evaluation of array comprehensions (MLS §10.4.1).
//!
//! `{e for i in r}` is the array of `e` evaluated for each value of `r`; with
//! several iterators the first iterator is the outermost dimension. Each value
//! is substituted into the body as a literal, so evaluation needs no mutable
//! scope.

use super::expr_eval::eval_expr_with_span;
use super::{EvalContext, EvalError, Value};
use rumoca_core::{ComprehensionIndex, Expression, ExpressionRewriter, Literal, Span, Subscript};

/// Upper bound on comprehension elements evaluated at translation time.
const MAX_ELEMENTS: usize = 100_000;

pub(super) fn eval_comprehension(
    body: &Expression,
    indices: &[ComprehensionIndex],
    filter: Option<&Expression>,
    ctx: &EvalContext,
    span: Span,
) -> Result<Value, EvalError> {
    let unsupported = || EvalError::UnsupportedExpression {
        kind: "ArrayComprehension".to_string(),
        span,
    };
    if filter.is_some() {
        return Err(unsupported());
    }
    let Some((index, rest)) = indices.split_first() else {
        return Err(unsupported());
    };
    let Value::Array(values) = eval_expr_with_span(&index.range, ctx, span)? else {
        return Err(unsupported());
    };
    if values.len() > MAX_ELEMENTS {
        return Err(unsupported());
    }
    let mut elements = Vec::with_capacity(values.len());
    for value in values {
        let literal = value_literal(&value, span).ok_or_else(unsupported)?;
        let mut substitution = Substitute {
            name: &index.name,
            value: &literal,
        };
        let body = substitution.rewrite_expression(body);
        let rest = rest
            .iter()
            .map(|later| ComprehensionIndex {
                name: later.name.clone(),
                range: substitution.rewrite_expression(&later.range),
            })
            .collect::<Vec<_>>();
        elements.push(if rest.is_empty() {
            eval_expr_with_span(&body, ctx, span)?
        } else {
            eval_comprehension(&body, &rest, None, ctx, span)?
        });
    }
    Ok(Value::Array(elements))
}

fn value_literal(value: &Value, span: Span) -> Option<Expression> {
    let literal = match value {
        Value::Integer(v) => Literal::Integer(*v),
        Value::Real(v) => Literal::Real(*v),
        Value::Bool(v) => Literal::Boolean(*v),
        Value::String(v) => Literal::String(v.clone()),
        _ => return None,
    };
    Some(Expression::Literal {
        value: literal,
        span,
    })
}

struct Substitute<'a> {
    name: &'a str,
    value: &'a Expression,
}

impl ExpressionRewriter for Substitute<'_> {
    fn rewrite_var_ref_expression(
        &mut self,
        name: &rumoca_core::Reference,
        subscripts: &[Subscript],
        span: Span,
    ) -> Expression {
        if name.as_str() == self.name && subscripts.is_empty() {
            return self.value.clone();
        }
        self.walk_var_ref_expression(name, subscripts, span)
    }
}
