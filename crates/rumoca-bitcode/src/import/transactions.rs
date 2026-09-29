//! Model event transactions, replayed after every other owner as the DAE's
//! own wire replay does: a transaction names conditions, clocks and
//! expressions, and owns discrete targets no other owner may claim.
use super::*;

pub(super) fn rebuild<'dae>(
    construction: &mut dae::DaeConstruction<'dae>,
    ctx: &Rebuild<'_>,
    expressions: &[dae::ExprId<'dae>],
    variables: &[VariableSlot<'dae>],
    conditions: &[dae::ConditionId<'dae>],
    clocks: &[dae::ClockId<'dae>],
) -> Result<(), dae::DaeConstructionError> {
    let target = |id: VariableId| match variables.get(id.0 as usize) {
        Some(VariableSlot::DiscreteReal(variable)) => {
            Ok(dae::ModelEventTarget::DiscreteReal(*variable))
        }
        Some(VariableSlot::DiscreteValue(variable)) => {
            Ok(dae::ModelEventTarget::DiscreteValue(*variable))
        }
        _ => Err(ctx.unsupported(format!(
            "model event transaction target {} is not a discrete variable",
            id.0
        ))),
    };
    for transaction in &ctx.model.model_event_transactions {
        let at = ctx.provenance(transaction.provenance)?;
        let targets = transaction
            .targets
            .iter()
            .map(|id| target(*id))
            .collect::<Result<Vec<_>, _>>()?;
        let mut steps = Vec::with_capacity(transaction.steps.len());
        for step in &transaction.steps {
            let definitions = step
                .definitions
                .iter()
                .map(|definition| {
                    Ok(dae::ModelEventDefinition::new(
                        target(definition.target)?,
                        resolve(expressions, definition.value.0, "expression", ctx)?,
                        ctx.provenance(definition.provenance)?,
                    ))
                })
                .collect::<Result<Vec<_>, dae::DaeConstructionError>>()?;
            steps.push(dae::ModelEventStep::new(
                resolve(conditions, step.trigger.0, "condition", ctx)?,
                resolve(conditions, step.guard.0, "condition", ctx)?,
                step.clock
                    .map(|clock| resolve(clocks, clock.0, "clock", ctx))
                    .transpose()?,
                definitions,
                ctx.provenance(step.provenance)?,
            ));
        }
        construction.model_events(|events| events.transaction(targets, steps, at))?;
    }
    Ok(())
}

/// Every span a transaction references, for sizing source filler.
pub(super) fn spans(model: &RbcModel) -> impl Iterator<Item = RbcSpan> + '_ {
    model
        .model_event_transactions
        .iter()
        .flat_map(|transaction| {
            std::iter::once(transaction.provenance.span).chain(transaction.steps.iter().flat_map(
                |step| {
                    std::iter::once(step.provenance.span)
                        .chain(step.definitions.iter().map(|d| d.provenance.span))
                },
            ))
        })
}
