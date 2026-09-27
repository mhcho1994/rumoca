//! Evaluate pure calls whose actual inputs are settled at translation time.
//! MLS §12.3 preserves evaluation of the source function body, including its
//! branches and while loops, in the existing bounded constant evaluator.
use crate::FlattenError;
use rumoca_core::{
    Expression, FallibleExpressionRewriter, FallibleStatementRewriter, Literal, Span, Variability,
};
use rumoca_eval_flat::constant::{EvalContext, EvalError, Value, eval_expr};
use rumoca_ir_flat as flat;

pub(crate) fn fold_pure_constant_calls(model: &mut flat::Model) -> Result<(), FlattenError> {
    let mut context = EvalContext::new();
    for function in model.functions.values() {
        context.add_function(function.clone());
    }
    // Freeze is a whole fixed-parameter profile: a final/evaluated parameter
    // can still depend on a tunable parent, so no individual flag proves its
    // value immutable. Every fixed parameter must be non-tunable first.
    let fixed_parameters_frozen = model.variables.values().all(|variable| {
        !matches!(variable.variability, Variability::Parameter(_))
            || variable.fixed == Some(false)
            || variable.evaluate
    });
    // A frozen parameter can depend on a later frozen parameter. Only insert
    // newly established values; the finite declaration set bounds this loop.
    loop {
        let mut changed = false;
        for (name, variable) in &model.variables {
            if !(matches!(variable.variability, Variability::Constant(_))
                || (matches!(variable.variability, Variability::Parameter(_))
                    && fixed_parameters_frozen
                    && variable.evaluate))
                || variable.fixed == Some(false)
                || context.parameters.contains_key(name.as_str())
            {
                continue;
            }
            if let Some(binding) = &variable.binding
                && let Ok(value) = eval_expr(binding, &context)
            {
                context.parameters.insert(name.as_str().to_owned(), value);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // A declaration binding is evaluated unconditionally. Refuse only a typed
    // bounds proof here, never an unsupported body form or an unknown input.
    // Conditional/event branches elsewhere are folded opportunistically below.
    for variable in model.variables.values() {
        if let Some(binding) = &variable.binding
            && let Err(EvalError::IndexOutOfBounds { index, size, span }) =
                eval_expr(binding, &context)
        {
            return Err(FlattenError::ConstantIndexOutOfBounds { index, size, span });
        }
    }
    // Fold only where a value is *settled at translation time*: the bindings
    // of parameters and constants, and the attributes that must be literal by
    // the time a checked DAE is built.
    //
    // Applying this to every model expression instead folds a pure call in an
    // ordinary equation down to its result whenever the arguments happen to be
    // literal -- `scaled = elementLoop({{1.0, ...}}, {0.5, ...})` -- and the
    // callee then does not reach the DAE at all. That deletes a declared
    // function from the compiler's own output artifact because of how its one
    // call site was written, which is not an optimisation a consumer can
    // recover from: 25 tests across `function_slice_compaction_rank_position`,
    // `function_inner_index_slice_compaction`, `function_conditional_sequence`
    // and others pin exactly the function bodies and loop nesting it removed.
    let mut folder = PureCallFolder { context };
    for variable in model.variables.values_mut() {
        if !matches!(
            variable.variability,
            Variability::Parameter(_) | Variability::Constant(_)
        ) {
            continue;
        }
        for expression in [
            &mut variable.binding,
            &mut variable.start,
            &mut variable.min,
            &mut variable.max,
            &mut variable.nominal,
        ]
        .into_iter()
        .flatten()
        {
            *expression = folder.rewrite_expression(expression)?;
        }
    }
    Ok(())
}

struct PureCallFolder {
    context: EvalContext,
}
impl FallibleExpressionRewriter for PureCallFolder {
    type Error = FlattenError;
    fn rewrite_expression(&mut self, expression: &Expression) -> Result<Expression, FlattenError> {
        if let Expression::FunctionCall {
            name,
            is_constructor: false,
            span,
            ..
        } = expression
            && name.resolved_function().is_some_and(|reference| {
                self.context.functions.values().any(|function| {
                    function.instance_id == Some(reference.instance_id) && function.pure
                })
            })
            && let Ok(value) = eval_expr(expression, &self.context)
            && let Some(expression) = literal_value(value, *span)
        {
            return Ok(expression);
        }
        self.walk_expression(expression)
    }
}
impl FallibleStatementRewriter for PureCallFolder {}

fn literal_value(value: Value, span: Span) -> Option<Expression> {
    let value = match value {
        Value::Real(value) if value.is_finite() => Literal::Real(value),
        Value::Integer(value) => Literal::Integer(value),
        Value::Bool(value) => Literal::Boolean(value),
        Value::String(value) => Literal::String(value),
        Value::Array(values) => {
            return Some(Expression::Array {
                elements: values
                    .into_iter()
                    .map(|value| literal_value(value, span))
                    .collect::<Option<_>>()?,
                is_matrix: false,
                span,
            });
        }
        _ => return None,
    };
    Some(Expression::Literal { value, span })
}
