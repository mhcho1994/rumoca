//! Model event transactions: discrete variables owned and defined together.
use super::*;

pub(super) fn export_model_event_transactions(
    view: dae::DaeView<'_>,
    ctx: &mut Ctx<'_>,
) -> Vec<RbcModelEventTransaction> {
    let target = |target: dae::ModelEventTarget<'_>| VariableId(target.variable());
    view.model_event_transactions()
        .map(|(_, transaction)| RbcModelEventTransaction {
            targets: transaction.targets().map(target).collect(),
            steps: transaction
                .steps()
                .map(|step| RbcModelEventStep {
                    trigger: ConditionId(step.trigger().index()),
                    guard: ConditionId(step.guard().index()),
                    clock: step.clock().map(|clock| ClockId(clock.index())),
                    definitions: step
                        .definitions()
                        .map(|definition| RbcModelEventDefinition {
                            target: target(definition.target()),
                            value: ExprId(definition.value().index()),
                            provenance: ctx.provenance(definition.provenance()),
                        })
                        .collect(),
                    provenance: ctx.provenance(step.provenance()),
                })
                .collect(),
            provenance: ctx.provenance(transaction.provenance()),
        })
        .collect()
}
