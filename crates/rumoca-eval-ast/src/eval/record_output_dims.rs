//! Dimensions of a record field read from a function's record output.
//!
//! A record parameter bound to a function call (`parameter R r = f(p)`) has its
//! fields projected as `f(p).y`. When the record declares `y[:]`, MLS §10.1
//! takes the extent from the binding, i.e. from the function's output. The
//! output declaration fixes it through its own modifier
//! (`output R r(y = {0, 0.5, 1})`), so the extent is read from that modifier,
//! evaluated in the function's scope (inputs and protected constants).

use super::{
    TypeCheckEvalContext, build_func_eval_context, eval_integer_with_scope,
    infer_dimensions_from_binding_with_scope, lookup_function,
};
use rumoca_core::{Causality, ClassType, Variability};
use rumoca_ir_ast::{ClassDef, Expression};

/// A record field read from a function output: `function(arguments).output.field`.
pub(super) struct OutputFieldQuery<'a> {
    pub(super) function: &'a str,
    pub(super) arguments: &'a [Expression],
    /// The selected output; `None` means the first output.
    pub(super) output: Option<&'a str>,
    pub(super) field: &'a str,
}

/// Dimensions of the queried field of the record returned by the call.
pub(super) fn infer_function_output_field_dims(
    call: &OutputFieldQuery<'_>,
    ctx: &TypeCheckEvalContext,
    scope: &str,
) -> Option<Vec<usize>> {
    let func_def = lookup_function(call.function, ctx)?;
    if func_def.class_type != ClassType::Function {
        return None;
    }
    let (_, output) = func_def.components.iter().find(|(name, comp)| {
        matches!(comp.causality, Causality::Output(_))
            && call.output.is_none_or(|out| out == name.as_str())
    })?;
    let field_binding = output.modifications.get(call.field)?;
    let mut local = build_func_eval_context(func_def, call.arguments, ctx, scope)?;
    bind_function_constants(func_def, &mut local);
    infer_dimensions_from_binding_with_scope(field_binding, &local, "")
}

/// Bind the function's Integer constants (typically protected sizes such as
/// `constant Integer n = 9`) so output modifiers referring to them evaluate.
fn bind_function_constants(func_def: &ClassDef, local: &mut TypeCheckEvalContext) {
    let constants: Vec<_> = func_def
        .components
        .iter()
        .filter(|(_, comp)| {
            matches!(comp.variability, Variability::Constant(_))
                && comp.shape_expr.is_empty()
                && comp.shape.is_empty()
        })
        .filter_map(|(name, comp)| comp.binding.as_ref().map(|b| (name, b)))
        .collect();
    // Constants may refer to each other in any order; settle by fixpoint.
    for _ in 0..constants.len() {
        let mut progress = false;
        for (name, binding) in &constants {
            if super::local_has_scalar(local, name) {
                continue;
            }
            let value = eval_integer_with_scope(binding, local, "");
            if let Some(v) = value {
                local.integers.insert(name.to_string(), v);
                local.reals.insert(name.to_string(), v as f64);
                local.remember_scalar_span(name, binding.span());
                progress = true;
            }
        }
        if !progress {
            break;
        }
    }
}
