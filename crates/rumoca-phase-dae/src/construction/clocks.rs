use std::collections::HashMap;

use rumoca_core::{InstanceId, PeriodicClockSchedule, Span, VarName};
use rumoca_ir_dae as dae;
use rumoca_ir_flat as flat;

use super::Coordinate;
use super::analysis::{ClockPlan, ClockedValuePlan};

pub(super) struct LoweredClocks<'dae> {
    pub(super) by_plan: HashMap<ClockPlan, dae::PeriodicClockId<'dae>>,
    /// MLS §3.7.5 event clocks are identified entirely by their exact
    /// periodic schedule. Every occurrence and exact Boolean alias reuses this
    /// one owner rather than allocating parallel activation lanes.
    by_sample_schedule: HashMap<PeriodicClockSchedule, dae::PeriodicClockId<'dae>>,
    /// Clock coordinates keyed by their Flat catalog name, the identity every
    /// Flat expression occurrence names. `Reference::instance_id` carries the
    /// enclosing class occurrence, so it cannot select a referenced coordinate.
    pub(super) by_coordinate: HashMap<VarName, dae::PeriodicClockId<'dae>>,
    /// MLS §16.9 `firstTick()`: per clock, the `previous` of a generated
    /// clocked indicator that starts at one and is zero after every tick.
    first_ticks: HashMap<dae::PeriodicClockId<'dae>, dae::PreviousId<'dae>>,
}

impl<'dae> LoweredClocks<'dae> {
    pub(super) fn id(
        &self,
        plan: &ClockPlan,
        span: rumoca_core::Span,
    ) -> Result<dae::PeriodicClockId<'dae>, dae::DaeConstructionError> {
        self.by_plan
            .get(plan)
            .copied()
            .ok_or(dae::DaeConstructionError::MissingClockDomainOwner { span })
    }

    /// The exact lattice of the lowered periodic clock `id`.
    pub(super) fn lattice(
        &self,
        id: dae::PeriodicClockId<'dae>,
    ) -> Option<rumoca_core::ClockLattice> {
        self.by_plan
            .iter()
            .find_map(|(plan, owned)| (*owned == id).then_some(plan.lattice))
    }

    /// The `previous` coordinate whose value is `firstTick()` of `clock`.
    pub(super) fn first_tick(
        &self,
        clock: dae::PeriodicClockId<'dae>,
        span: Span,
    ) -> Result<dae::PreviousId<'dae>, dae::DaeConstructionError> {
        self.first_ticks
            .get(&clock)
            .copied()
            .ok_or(dae::DaeConstructionError::MissingClockDomainOwner { span })
    }

    pub(super) fn sample_id(
        &self,
        schedule: PeriodicClockSchedule,
        span: Span,
    ) -> Result<dae::PeriodicClockId<'dae>, dae::DaeConstructionError> {
        self.by_sample_schedule
            .get(&schedule)
            .copied()
            .ok_or(dae::DaeConstructionError::MissingClockDomainOwner { span })
    }
}

pub(super) fn lower_clocks<'dae>(
    construction: &mut dae::DaeConstruction<'dae>,
    flat: &flat::Model,
    plans: &HashMap<InstanceId, ClockPlan>,
    clocked_values: &HashMap<InstanceId, ClockedValuePlan>,
    sample_schedules: impl Iterator<Item = (PeriodicClockSchedule, Span)>,
) -> Result<LoweredClocks<'dae>, dae::DaeConstructionError> {
    let mut plan_ids = HashMap::new();
    let mut coordinate_ids = HashMap::new();
    for (name, variable) in &flat.variables {
        let Some(plan) = plans.get(&variable.instance_id).copied() else {
            continue;
        };
        let clock = if let Some(clock) = plan_ids.get(&plan).copied() {
            clock
        } else {
            let provenance = dae::DaeProvenance::source(plan.constructor_span)?;
            let clock = construction.clocks(|clocks| clocks.periodic(plan.lattice, provenance))?;
            plan_ids.insert(plan, clock);
            clock
        };
        coordinate_ids.insert(name.clone(), clock);
    }
    for (_, value) in clocked_values_in_instance_order(clocked_values) {
        if plan_ids.contains_key(&value.clock) {
            continue;
        }
        let provenance = dae::DaeProvenance::source(value.clock.constructor_span)?;
        let clock =
            construction.clocks(|clocks| clocks.periodic(value.clock.lattice, provenance))?;
        plan_ids.insert(value.clock, clock);
    }
    let mut sample_ids = HashMap::new();
    for (schedule, span) in sample_schedules {
        if sample_ids.contains_key(&schedule) {
            continue;
        }
        let provenance = dae::DaeProvenance::source(span)?;
        let clock = construction.clocks(|clocks| clocks.scheduled(schedule, provenance))?;
        sample_ids.insert(schedule, clock);
    }
    Ok(LoweredClocks {
        by_plan: plan_ids,
        by_sample_schedule: sample_ids,
        by_coordinate: coordinate_ids,
        first_ticks: HashMap::new(),
    })
}

impl<'dae> LoweredClocks<'dae> {
    /// Issue the `firstTick()` indicator of every clock whose partition reads
    /// it (MLS §16.9: true at the first tick of the clock, false afterwards).
    ///
    /// The indicator is a generated clocked discrete Real `f` with `start =
    /// 1` and the partition equation `f = 0`, so `previous(f)` is one exactly
    /// at the first tick; `firstTick()` lowers to `previous(f) > 0.5`.
    pub(super) fn issue_first_ticks(
        &mut self,
        construction: &mut dae::DaeConstruction<'dae>,
        hosts: impl IntoIterator<Item = (ClockPlan, Span)>,
    ) -> Result<(), dae::DaeConstructionError> {
        let mut clocks = hosts
            .into_iter()
            .map(|(plan, span)| self.id(&plan, span).map(|clock| (clock, span)))
            .collect::<Result<Vec<_>, _>>()?;
        clocks.sort_by_key(|(clock, _)| clock.index());
        clocks.dedup_by_key(|(clock, _)| *clock);
        for (clock, span) in clocks {
            let previous = issue_first_tick(construction, clock, span)?;
            self.first_ticks.insert(clock, previous);
        }
        Ok(())
    }
}

fn issue_first_tick<'dae>(
    construction: &mut dae::DaeConstruction<'dae>,
    clock: dae::PeriodicClockId<'dae>,
    span: Span,
) -> Result<dae::PreviousId<'dae>, dae::DaeConstructionError> {
    let provenance = dae::DaeProvenance::generated(dae::DaeGeneration::ClockLowering, span)?;
    let value_type = construction.types(|types| {
        types.derived(dae::ValueType::scalar(dae::ScalarType::Real), provenance)
    })?;
    let (one, zero) = construction.expressions(|expressions| {
        Ok((
            expressions
                .at(provenance)
                .literal(dae::DaeLiteral::Real(1.0))?,
            expressions
                .at(provenance)
                .literal(dae::DaeLiteral::Real(0.0))?,
        ))
    })?;
    let variable = construction.variables(|variables| {
        variables.discrete_real(
            VarName::new(format!("firstTick(clock {})", clock.index())),
            value_type,
            provenance,
            dae::VariableAttributes {
                start: Some(one),
                fixed: Some(vec![true]),
                origin: dae::VariableOrigin::Generated,
                ..Default::default()
            },
        )
    })?;
    construction.clocks(|clocks| {
        clocks.own_discrete_real(clock.into(), variable, provenance)?;
        Ok(())
    })?;
    let residual = construction.expressions(|expressions| {
        let current = expressions
            .at(provenance)
            .coordinate(dae::CoordinateInput::DiscreteReal(variable))?;
        expressions
            .at(provenance)
            .binary(dae::BinaryOperator::Subtract, current, zero)
    })?;
    construction.discrete(|system| {
        system.real_equation(provenance, |equation| equation.residual(residual))
    })?;
    construction.temporal(|temporal| {
        temporal.previous_discrete_real(clock.into(), variable, provenance)
    })
}

fn clocked_values_in_instance_order(
    values: &HashMap<InstanceId, ClockedValuePlan>,
) -> Vec<(InstanceId, &ClockedValuePlan)> {
    let mut ordered: Vec<_> = values.iter().collect();
    ordered.sort_unstable_by_key(|(instance, _)| instance.index());
    ordered
        .into_iter()
        .map(|(instance, plan)| (*instance, plan))
        .collect()
}

pub(super) fn lower_clocked_value_owners<'dae>(
    construction: &mut dae::DaeConstruction<'dae>,
    flat: &flat::Model,
    coordinates: &HashMap<VarName, Coordinate<'dae>>,
    clocked_values: &HashMap<InstanceId, ClockedValuePlan>,
    clocks: &LoweredClocks<'dae>,
) -> Result<(), dae::DaeConstructionError> {
    for (name, variable) in &flat.variables {
        let Some(plan) = clocked_values.get(&variable.instance_id).copied() else {
            continue;
        };
        let clock = clocks.id(&plan.clock, plan.ownership_span)?;
        let ownership = dae::DaeProvenance::source(plan.ownership_span)?;
        let coordinate = coordinates.get(name).copied().ok_or_else(|| {
            dae::DaeConstructionError::InvalidVariableRole {
                name: name.clone(),
                span: plan.ownership_span,
            }
        })?;
        construction.clocks(|clocks| match coordinate {
            Coordinate::DiscreteReal(variable) if plan.sampled => {
                clocks.own_sampled_discrete_real(clock.into(), variable, ownership)?;
                Ok(())
            }
            Coordinate::DiscreteReal(variable) => {
                clocks.own_discrete_real(clock.into(), variable, ownership)?;
                Ok(())
            }
            Coordinate::DiscreteValue(variable) if plan.sampled => {
                clocks.own_sampled_discrete_value(clock.into(), variable, ownership)?;
                Ok(())
            }
            Coordinate::DiscreteValue(variable) => {
                clocks.own_discrete_value(clock.into(), variable, ownership)?;
                Ok(())
            }
            _ => Err(dae::DaeConstructionError::InvalidVariableRole {
                name: name.clone(),
                span: plan.ownership_span,
            }),
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rumoca_core::{ClockLattice, ClockRational};

    fn plan() -> ClockedValuePlan {
        ClockedValuePlan {
            clock: ClockPlan {
                lattice: ClockLattice::new(ClockRational::ONE, ClockRational::ZERO).unwrap(),
                constructor_span: Span::DUMMY,
            },
            ownership_span: Span::DUMMY,
            sampled: false,
        }
    }

    #[test]
    fn clocked_value_allocation_order_uses_instance_identity() {
        let mut values = HashMap::new();
        values.insert(InstanceId::new(9), plan());
        values.insert(InstanceId::new(2), plan());
        values.insert(InstanceId::new(5), plan());

        let ids: Vec<_> = clocked_values_in_instance_order(&values)
            .into_iter()
            .map(|(instance, _)| instance.index())
            .collect();

        assert_eq!(ids, [2, 5, 9]);
    }
}
