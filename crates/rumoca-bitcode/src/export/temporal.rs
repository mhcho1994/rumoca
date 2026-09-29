//! Owner tables for temporal coordinates and structured roots: `previous`,
//! `terminal()`, `delay`, and tensor-native root families.
use super::*;

pub(super) struct Temporal {
    pub(super) previous_values: Vec<RbcPreviousValue>,
    pub(super) terminals: Vec<RbcTerminal>,
    pub(super) structured_roots: Vec<RbcStructuredRoot>,
    pub(super) delays: Vec<RbcDelay>,
}

pub(super) fn export(view: dae::DaeView<'_>, ctx: &mut Ctx<'_>) -> Temporal {
    let previous_values = view
        .previous_values()
        .map(|(_, previous)| RbcPreviousValue {
            variable: VariableId(previous.variable().index()),
            clock: ClockId(previous.clock().index()),
            provenance: ctx.provenance(previous.provenance()),
        })
        .collect();
    let terminals = view
        .terminals()
        .map(|(_, terminal)| RbcTerminal {
            provenance: ctx.provenance(terminal.provenance()),
        })
        .collect();
    let structured_roots = view
        .structured_roots()
        .map(|(_, root)| RbcStructuredRoot {
            domain: DomainId(root.domain().index()),
            expression: ExprId(root.expression().index()),
            provenance: ctx.provenance(root.provenance()),
        })
        .collect();
    let delays = (0..view.delay_count())
        .filter_map(|index| view.delay(view.delay_id(index)?))
        .map(|delay| RbcDelay {
            source: ExprId(delay.source().index()),
            delay: match delay.operation() {
                dae::DelayOperation::ParameterDelay { delay_time } => RbcDelayKind::Parameter {
                    delay_time: positive(delay_time, ctx),
                },
                dae::DelayOperation::BoundedDelay {
                    delay_time,
                    delay_max,
                } => RbcDelayKind::Bounded {
                    delay_time: ExprId(delay_time.index()),
                    maximum: positive(delay_max, ctx),
                },
            },
            provenance: ctx.provenance(delay.provenance()),
        })
        .collect();
    Temporal {
        previous_values,
        terminals,
        structured_roots,
        delays,
    }
}

fn positive(parameter: dae::PositiveParameterView<'_>, ctx: &mut Ctx<'_>) -> RbcPositiveParameter {
    RbcPositiveParameter {
        expression: ExprId(parameter.expression().index()),
        value: parameter.value(),
        provenance: ctx.provenance(parameter.provenance()),
    }
}
