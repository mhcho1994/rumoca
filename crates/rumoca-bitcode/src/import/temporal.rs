//! Temporal owners -- `previous`, `terminal()`, `delay` -- and structured
//! roots, replayed in the order the DAE's own wire replay uses: `previous`
//! and `terminal()` owners before the arena (their coordinates name them),
//! each delay when the arena reaches the one coordinate that reads it (the
//! DAE creates owner and coordinate together), structured roots after the
//! conditions they sit beside.
use super::*;

/// Owners the arena's coordinates name.
pub(super) struct TemporalOwners<'dae> {
    pub(super) previous: Vec<dae::PreviousId<'dae>>,
    pub(super) terminals: Vec<dae::TerminalId<'dae>>,
}

pub(super) fn rebuild_owners<'dae>(
    construction: &mut dae::DaeConstruction<'dae>,
    ctx: &Rebuild<'_>,
    variables: &[VariableSlot<'dae>],
    clocks: &[dae::ClockId<'dae>],
) -> Result<TemporalOwners<'dae>, dae::DaeConstructionError> {
    let mut previous = Vec::with_capacity(ctx.model.previous_values.len());
    for entry in &ctx.model.previous_values {
        let at = ctx.provenance(entry.provenance)?;
        let clock = resolve(clocks, entry.clock.0, "clock", ctx)?;
        let id = match resolve(variables, entry.variable.0, "variable", ctx)? {
            VariableSlot::DiscreteReal(variable) => {
                construction.temporal(|t| t.previous_discrete_real(clock, variable, at))?
            }
            VariableSlot::DiscreteValue(variable) => {
                construction.temporal(|t| t.previous_discrete_value(clock, variable, at))?
            }
            _ => {
                return Err(ctx.unsupported(format!(
                    "previous value names variable {}, which is not discrete",
                    entry.variable.0
                )));
            }
        };
        previous.push(id);
    }
    let mut terminals = Vec::with_capacity(ctx.model.terminals.len());
    for entry in &ctx.model.terminals {
        let at = ctx.provenance(entry.provenance)?;
        terminals.push(construction.temporal(|t| t.terminal(at))?);
    }
    Ok(TemporalOwners {
        previous,
        terminals,
    })
}

/// Issue the delay owner `delay` and its coordinate, which must be next.
pub(super) fn replay_delay<'dae>(
    construction: &mut dae::DaeConstruction<'dae>,
    ctx: &Rebuild<'_>,
    built: &[dae::ExprId<'dae>],
    delay: DelayId,
    coordinate_at: dae::DaeProvenance,
) -> Result<dae::ExprId<'dae>, dae::DaeConstructionError> {
    let entry = ctx
        .model
        .delays
        .get(delay.0 as usize)
        .ok_or_else(|| ctx.unsupported(format!("delay {} is not declared", delay.0)))?;
    let owner_at = ctx.provenance(entry.provenance)?;
    let source = resolve(built, entry.source.0, "expression", ctx)?;
    let positive = |construction: &mut dae::DaeConstruction<'dae>,
                    parameter: RbcPositiveParameter| {
        let expression = resolve(built, parameter.expression.0, "expression", ctx)?;
        let at = ctx.provenance(parameter.provenance)?;
        construction.temporal(|t| t.positive_parameter(expression, parameter.value, at))
    };
    let coordinate = match entry.delay {
        RbcDelayKind::Parameter { delay_time } => {
            let delay_time = positive(construction, delay_time)?;
            construction.expressions(|e| e.at(coordinate_at).delay(source, delay_time, owner_at))?
        }
        RbcDelayKind::Bounded {
            delay_time,
            maximum,
        } => {
            let delay_time = resolve(built, delay_time.0, "expression", ctx)?;
            let maximum = positive(construction, maximum)?;
            construction.expressions(|e| {
                e.at(coordinate_at)
                    .bounded_delay(source, delay_time, maximum, owner_at)
            })?
        }
    };
    if coordinate.id().index() != delay.0 {
        return Err(ctx.unsupported(format!(
            "delay coordinate names delay {}, but the arena reaches it as delay {}",
            delay.0,
            coordinate.id().index()
        )));
    }
    Ok(coordinate.expression())
}

pub(super) fn rebuild_structured_roots<'dae>(
    construction: &mut dae::DaeConstruction<'dae>,
    ctx: &Rebuild<'_>,
    expressions: &[dae::ExprId<'dae>],
    domains: &[dae::DomainId<'dae>],
) -> Result<(), dae::DaeConstructionError> {
    for root in &ctx.model.structured_roots {
        let at = ctx.provenance(root.provenance)?;
        let domain = resolve(domains, root.domain.0, "domain", ctx)?;
        let expression = resolve(expressions, root.expression.0, "expression", ctx)?;
        construction.conditions(|c| c.structured_root(domain, expression, at))?;
    }
    Ok(())
}

/// Every span the temporal tables reference, for sizing source filler.
pub(super) fn spans(model: &RbcModel) -> Vec<RbcSpan> {
    let mut found: Vec<RbcSpan> = model
        .previous_values
        .iter()
        .map(|p| p.provenance.span)
        .chain(model.terminals.iter().map(|t| t.provenance.span))
        .chain(model.structured_roots.iter().map(|r| r.provenance.span))
        .collect();
    for delay in &model.delays {
        found.push(delay.provenance.span);
        match delay.delay {
            RbcDelayKind::Parameter { delay_time } => found.push(delay_time.provenance.span),
            RbcDelayKind::Bounded { maximum, .. } => found.push(maximum.provenance.span),
        }
    }
    found
}
