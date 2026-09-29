//! Prove, at translation, that no settled declaration binding indexes out of
//! bounds.
//!
//! This module used to fold pure calls with settled inputs to their values
//! as well. Folding changes the model and is an optimization, so it moved to
//! the `fold-pure-calls` bitcode pass (docs/design/minimal-frontend.md); the
//! bounds proof stays, because it rejects a model rather than rewriting it.
use crate::FlattenError;
use rumoca_core::Variability;
use rumoca_eval_flat::constant::{EvalContext, EvalError, eval_expr};
use rumoca_ir_flat as flat;

pub(crate) fn check_settled_binding_bounds(model: &flat::Model) -> Result<(), FlattenError> {
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
    for variable in model.variables.values() {
        if let Some(binding) = &variable.binding
            && let Err(EvalError::IndexOutOfBounds { index, size, span }) =
                eval_expr(binding, &context)
        {
            return Err(FlattenError::ConstantIndexOutOfBounds { index, size, span });
        }
    }
    Ok(())
}
