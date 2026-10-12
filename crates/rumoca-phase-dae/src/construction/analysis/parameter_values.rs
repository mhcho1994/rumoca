use super::*;

/// Fold every `constant`/`parameter` binding the Flat model settles at
/// translation time.
///
/// The fixed point is keyed on the exact occurrence identity of each
/// declaration — `validate_flat_shape` has already proven every Flat variable
/// carries a distinct allocated [`InstanceId`], so "has this declaration been
/// settled" is a question about the identity and never about a rendered name
/// two occurrences could share.
///
/// A binding that does not fold is only skipped when the evaluator says *why*
/// in typed terms: MLS §4.4 permits a parameter value to be established during
/// initialization instead, and those failures carry a
/// `RuntimeDependentReason`. Any other failure proves the model or the
/// evaluator wrong and is reported at the binding.
pub(super) fn constant_context(flat: &flat::Model) -> Result<EvalContext, ToDaeError> {
    let mut context = seeded_eval_context(flat);
    for _ in 0..flat.variables.len() {
        let mut progress = false;
        for (name, variable) in &flat.variables {
            if context.instance_value(variable.instance_id).is_some()
                || !matches!(
                    variable.variability,
                    Variability::Constant(_) | Variability::Parameter(_)
                )
                // Parameter `fixed` is uniform here (flatten refuses non-uniform
                // parameter arrays, EF033), so this reduction is exact.
                || matches!(variable.variability, Variability::Parameter(_))
                    && variable.fixed_uniform() == Some(false)
            {
                continue;
            }
            let evaluated = match &variable.binding {
                Some(binding) => eval_expr(binding, &context),
                None => match unbound_parameter_value(flat, variable, &context) {
                    Some(value) => Ok(value),
                    None => continue,
                },
            };
            match evaluated {
                Ok(value) => {
                    context.add_instance_parameter(variable.instance_id, name.to_string(), value);
                    progress = true;
                }
                Err(error) if error.runtime_dependent_reason().is_some() => {}
                // A division by zero over parameter values (typically a
                // parameter left at its default `start = 0` that the user is
                // expected to set) is not a translation-time defect: the
                // binding stays a runtime parameter equation, and a consumer
                // that needs its value structurally reports its own error.
                Err(rumoca_eval_flat::constant::EvalError::DivisionByZero { .. }) => {}
                Err(error) => {
                    return Err(ToDaeError::unsupported_flat(
                        "parameter binding",
                        format!("`{name}` cannot be evaluated: {error}"),
                        error.span().unwrap_or(variable.source_span),
                    ));
                }
            }
        }
        if !progress {
            break;
        }
    }
    parameter_cycles::reject_cyclic_parameter_bindings(flat, &context)?;
    register_deferred_parameters(flat, &mut context);
    Ok(context)
}

/// The value of a constant or fixed parameter that has no binding equation.
///
/// MLS §8.6: such a parameter takes its `start` value, and a type's default
/// `start` (`0`, `0.0`, `false`) when none is written; tools accept it with a
/// warning. `Modelica.Blocks.Interfaces.DiscreteBlock.samplePeriod` (declared
/// with `start = 0.1` only) and OpenIPSL `DEGOV.TD` (no value at all) rely on
/// it. A `start` that does not evaluate leaves the parameter without a value.
fn unbound_parameter_value(
    flat: &flat::Model,
    variable: &flat::Variable,
    context: &EvalContext,
) -> Option<EvalValue> {
    if let Some(start) = &variable.start {
        return eval_expr(start, context).ok();
    }
    if !variable.dims.is_empty() {
        return None;
    }
    match effective_variable_scalar_type(flat, variable)? {
        dae::ScalarType::Real => Some(EvalValue::Real(0.0)),
        dae::ScalarType::Integer => Some(EvalValue::Integer(0)),
        dae::ScalarType::Boolean => Some(EvalValue::Bool(false)),
        _ => None,
    }
}

/// An evaluation context holding the model's functions, array dimensions, and
/// enumeration ordinals, with no variable values yet.
fn seeded_eval_context(flat: &flat::Model) -> EvalContext {
    let mut context = EvalContext::with_capacity(flat.variables.len(), 0, flat.functions.len() * 2);
    for function in flat.functions.values() {
        context.add_function(function.clone());
    }
    for (name, variable) in &flat.variables {
        context.add_array_dimensions(name.to_string(), variable.dims.clone());
    }
    // MLS §4.8.5.2: an enumeration literal's semantic identity is its ordinal,
    // and both `Integer(...)` and the relational operators are defined on that
    // ordinal. Seeding the constant table with the model's exact ordinals is
    // what lets a parameter expression over an enumeration (such as the
    // `resolution < Resolution.s` guard of a periodic clock) evaluate.
    for (literal, ordinal) in &flat.enum_literal_ordinals {
        context.add_parameter(literal.clone(), EvalValue::Integer(*ordinal));
    }
    context
}

/// Parameters STRUCT-T10(a) may fold (MLS §4.5, §18.6).
///
/// A parameter qualifies when it keeps the default `fixed=true`, does not
/// write `Evaluate=false`, and its binding evaluates from constants and other
/// qualifying parameters alone, and either it is `final` or `Evaluate=true`,
/// or its binding reads a qualifying parameter. The second kind is a
/// dependent parameter determined solely by translation-time values. An
/// ordinary independent parameter is never admitted to the context, so a
/// binding that reads one stays unevaluated here and its parameter keeps its
/// runtime meaning.
pub(super) fn evaluable_parameters(flat: &flat::Model) -> HashSet<VarName> {
    let mut context = seeded_eval_context(flat);
    let mut evaluable = HashSet::new();
    for _ in 0..flat.variables.len() {
        let mut progress = false;
        for (name, variable) in &flat.variables {
            let admitted = match variable.variability {
                Variability::Constant(_) => true,
                Variability::Parameter(_) => {
                    !variable.evaluate_refused && variable.fixed_uniform() != Some(false)
                }
                _ => false,
            };
            if !admitted || context.instance_value(variable.instance_id).is_some() {
                continue;
            }
            let Some(binding) = &variable.binding else {
                continue;
            };
            if matches!(variable.variability, Variability::Parameter(_))
                && !reads_only_evaluable(flat, binding, &evaluable, variable.evaluate)
            {
                continue;
            }
            let Ok(value) = eval_expr(binding, &context) else {
                continue;
            };
            context.add_instance_parameter(variable.instance_id, name.to_string(), value);
            if matches!(variable.variability, Variability::Parameter(_)) {
                evaluable.insert(name.clone());
            }
            progress = true;
        }
        if !progress {
            break;
        }
    }
    evaluable
}

/// Whether every variable `binding` reads by value is a constant or an
/// evaluable parameter, and, unless `marked` (`final` or `Evaluate=true`), at
/// least one is an evaluable parameter. A `size(a, k)` argument reads only the
/// shape of `a`. This mirrors the reads DAE construction checks for the
/// `evaluable` attribute, so a branch of a conditional binding counts even when
/// translation would never select it.
fn reads_only_evaluable(
    flat: &flat::Model,
    binding: &Expression,
    evaluable: &HashSet<VarName>,
    marked: bool,
) -> bool {
    let mut reads = ValueReads::default();
    rumoca_core::ExpressionVisitor::visit_expression(&mut reads, binding);
    let mut dependent = marked;
    for read in &reads.names {
        match flat.variables.get(read) {
            Some(variable) if matches!(variable.variability, Variability::Constant(_)) => {}
            Some(_) if evaluable.contains(read) => dependent = true,
            Some(_) => return false,
            None => {}
        }
    }
    dependent
}

/// Name every `fixed = false` parameter the initialization system settles, and
/// what settles it.
///
/// MLS §4.4 lets a parameter declaration defer its value and MLS §8.6 gives the
/// initial section the job of establishing it. Such a parameter is absent from
/// the fold above for a *reason*, not by accident: it has no binding to fold.
/// A later consumer that asks the fold for its value — a `sample(start,
/// interval)` phase, a `delay` bound, a `Clock` argument — otherwise learns
/// only that the name has no value and reports it as unknown, which reads as a
/// name-resolution defect and hides the construct that was actually hit.
///
/// The distinction recorded here is the one that matters to those consumers:
/// a determining expression that reads `time` is settled by the *simulation
/// start instant*, because MLS §8.6 evaluates the initial section at the
/// initialization instant. That instant is chosen when the model is simulated,
/// not when it is translated, so no translation-time value exists for it at
/// all — as opposed to a value that merely waits for the initialization system
/// to run.
///
/// The two tiers are ordered, not symmetric: [`DeferredParameterSource::
/// StartInstant`] is a *proof* about the shape it recognizes, while
/// [`DeferredParameterSource::InitializationSystem`] is the conservative floor
/// every other deferred parameter falls to and asserts nothing about the start
/// instant either way. See [`deferred_parameter_source`] for exactly what is
/// proven and which start-instant-dependent shapes land in the weaker tier.
///
/// # Known remaining
///
/// A parameter that *has* a binding reading a deferred parameter is not
/// registered here — the guard below skips binding-holders, and the fold at
/// [`constant_context`] discards its failure through the
/// `runtime_dependent_reason` arm — so such a parameter is still reported as an
/// unknown name. This is real MSL, not a hypothetical:
/// `Modelica.Blocks.Math.ContinuousMean` (`Blocks/Math.mo:2349-2352`) declares
/// `parameter Real t_0(fixed = false)` with `initial equation t_0 = time` and
/// then `parameter Real actualStartTime = max(t_0, startTime)`, so
/// `actualStartTime` is start-instant-dependent through its binding. Closing it
/// needs one of: propagating the deferred source through the binding fold, or
/// registering a failed-fold binding over deferred free names as deferred
/// itself. Both belong with the start-relative-schedule work that would let
/// these sample starts construct at all, rather than with this diagnostic.
fn register_deferred_parameters(flat: &flat::Model, context: &mut EvalContext) {
    for (name, variable) in &flat.variables {
        // Parameter `fixed` is uniform (flatten refuses non-uniform parameter
        // arrays, EF033), so this whole-declaration reduction is exact.
        if !matches!(variable.variability, Variability::Parameter(_))
            || variable.fixed_uniform() != Some(false)
            || variable.binding.is_some()
            || context.instance_value(variable.instance_id).is_some()
        {
            continue;
        }
        context.add_deferred_parameter(name.to_string(), deferred_parameter_source(flat, name));
    }
}

/// What settles `target`: the start instant, or the initialization system.
///
/// This proves exactly one shape and claims nothing beyond it. `StartInstant`
/// is returned when some initial equation's residual is a *top-level*
/// subtraction with `target` itself as one direct operand, and the other
/// operand *syntactically* reads `time`. That is the shape flat lowering gives
/// `t0 = time`, and it is the shape both MSL sample-start sites are written in.
///
/// Everything else falls to `InitializationSystem`, which is therefore a floor
/// — "this parameter has no translation-time value" — and **not** a claim that
/// the value is independent of the start instant. Reachable shapes that do
/// depend on the start instant and still land in the weaker tier:
///
/// - indirectly, through another deferred parameter (`a = time; t0 = a`);
/// - through an initial *algorithm* target (`t0 := time`), which is owned by
///   [`initial_algorithms`] and never appears in `flat.initial_equations`;
/// - through a residual that is not a top-level subtraction on `target`
///   (`t0*t0 = time + 1.0`).
///
/// Each is refused either way, and the weaker label is true of all of them, so
/// the floor costs precision in the message and never correctness. Widening the
/// rule is deliberately left to the start-relative-schedule work, where a
/// start-instant dependency has to be *represented* rather than only named.
fn deferred_parameter_source(
    flat: &flat::Model,
    target: &VarName,
) -> rumoca_eval_flat::constant::DeferredParameterSource {
    use rumoca_eval_flat::constant::DeferredParameterSource;
    let determined_by_time = flat
        .initial_equations
        .iter()
        .filter_map(|equation| determining_value(&equation.residual, target))
        .any(reads_time);
    if determined_by_time {
        DeferredParameterSource::StartInstant
    } else {
        DeferredParameterSource::InitializationSystem
    }
}

/// The side of a `target - value` residual that is not `target`.
///
/// Flat lowering writes `t0 = time` as the residual `t0 - time`, so this
/// recognizes a residual that is a top-level subtraction with `target` as one
/// *direct* operand, and returns the other operand. It is a syntactic match on
/// that one shape, not a solve: a residual that merely constrains `target`
/// (`t0*t0 - (time + 1.0)`) yields `None`, and its caller then labels the
/// parameter by the weaker tier rather than inspecting it further.
fn determining_value<'a>(residual: &'a Expression, target: &VarName) -> Option<&'a Expression> {
    let Expression::Binary {
        op: OpBinary::Sub,
        lhs,
        rhs,
        ..
    } = residual
    else {
        return None;
    };
    if names_variable(lhs, target) {
        return Some(rhs);
    }
    if names_variable(rhs, target) {
        return Some(lhs);
    }
    None
}

fn names_variable(expression: &Expression, target: &VarName) -> bool {
    matches!(
        expression,
        Expression::VarRef { name, subscripts, .. }
            if subscripts.is_empty() && name.var_name() == target
    )
}

/// True when the expression reads `time`, whose value MLS §8.6 fixes at the
/// initialization instant.
fn reads_time(expression: &Expression) -> bool {
    if let Expression::VarRef { name, .. } = expression
        && name.as_str() == "time"
    {
        return true;
    }
    expression_children(expression).into_iter().any(reads_time)
}
