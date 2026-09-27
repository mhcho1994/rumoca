//! Array comprehensions `{expr for i in range, ...}` (MLS §10.4.2.1) in
//! model-scope constant evaluation.
//!
//! Each iterator binds its name for the iterations of its own range only, so a
//! binder shadows any outer name of the same spelling for exactly the
//! sub-expressions it scopes. The first iterator is the outermost array
//! dimension, the same element order the function-body evaluator produces.

use std::borrow::Cow;

use rumoca_core::{ComprehensionIndex, Span};

use super::errors::EvalError;
use super::expr_eval::eval_expr_with_span;
use super::value::Value;
use super::{DeferredParameterSource, EvalEnvironment, Expression, Function};

/// One comprehension iterator bound to its current value over an outer scope.
struct BinderScope<'scope> {
    outer: &'scope dyn EvalEnvironment,
    name: &'scope str,
    value: Value,
}

impl EvalEnvironment for BinderScope<'_> {
    fn get_value(&self, name: &str) -> Option<Cow<'_, Value>> {
        if name == self.name {
            return Some(Cow::Borrowed(&self.value));
        }
        self.outer.get_value(name)
    }

    fn get_enum(&self, name: &str) -> Option<&(String, String)> {
        (name != self.name)
            .then(|| self.outer.get_enum(name))
            .flatten()
    }

    fn get_function(&self, name: &str) -> Option<&Function> {
        self.outer.get_function(name)
    }

    fn get_array_dimensions(&self, name: &str) -> Option<&[i64]> {
        (name != self.name)
            .then(|| self.outer.get_array_dimensions(name))
            .flatten()
    }

    fn deferred_parameter(&self, name: &str) -> Option<DeferredParameterSource> {
        (name != self.name)
            .then(|| self.outer.deferred_parameter(name))
            .flatten()
    }
}

/// Evaluate `{expr for indices if filter}` to a nested constant array.
pub(super) fn eval_comprehension(
    expr: &Expression,
    indices: &[ComprehensionIndex],
    filter: Option<&Expression>,
    ctx: &dyn EvalEnvironment,
    span: Span,
) -> Result<Value, EvalError> {
    let Some((index, remaining)) = indices.split_first() else {
        return eval_expr_with_span(expr, ctx, span);
    };
    let range = match eval_expr_with_span(&index.range, ctx, span)? {
        Value::Array(values) => values,
        other => return Err(EvalError::type_mismatch("Array", other.type_name(), span)),
    };
    let mut elements = Vec::with_capacity(range.len());
    for value in range {
        let scope = BinderScope {
            outer: ctx,
            name: &index.name,
            value,
        };
        if !remaining.is_empty() {
            elements.push(eval_comprehension(expr, remaining, filter, &scope, span)?);
            continue;
        }
        if let Some(filter) = filter {
            let keep = eval_expr_with_span(filter, &scope, span)?;
            let keep = keep
                .as_bool()
                .ok_or_else(|| EvalError::type_mismatch("Boolean", keep.type_name(), span))?;
            if !keep {
                continue;
            }
        }
        elements.push(eval_expr_with_span(expr, &scope, span)?);
    }
    Ok(Value::Array(elements))
}
