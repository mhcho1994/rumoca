use super::*;

pub(super) struct OrdinaryEquationRow<'input, 'scope, 'dae> {
    pub(super) input: &'input EquationRows<'scope, 'dae>,
    pub(super) index: usize,
    pub(super) equation: &'scope flat::Equation,
    pub(super) owner: dae::DaeProvenance,
    pub(super) generation: Option<dae::DaeGeneration>,
    pub(super) owner_clock: Option<dae::PeriodicClockId<'dae>>,
}

pub(super) fn lower_ordinary_equation<'dae>(
    construction: &mut dae::DaeConstruction<'dae>,
    discrete_values: &mut DiscreteValueStaging<'dae>,
    coordinates: &HashMap<VarName, Coordinate<'dae>>,
    functions: &FunctionRegistry<'_, 'dae>,
    row: OrdinaryEquationRow<'_, '_, 'dae>,
) -> Result<(), dae::DaeConstructionError> {
    let OrdinaryEquationRow {
        input,
        index,
        equation,
        owner,
        generation,
        owner_clock,
    } = row;
    if input.initialization {
        let residual = lower_expression(
            construction,
            coordinates,
            functions,
            &equation.residual,
            generation,
        )?;
        construction.initialization(|system| system.value_equation(owner, residual))?;
        return Ok(());
    }
    match input.partition(index, equation) {
        EquationPartition::Continuous => {
            let (source, generation) = match input.semi_linear.residual(index) {
                Some(replacement) => (replacement, Some(dae::DaeGeneration::SemiLinearLowering)),
                None => (&equation.residual, generation),
            };
            let residual = lower_equation_expression(
                construction,
                coordinates,
                functions,
                owner_clock,
                source,
                generation,
            )?;
            construction.continuous(|system| system.value_equation(owner, residual))?;
        }
        EquationPartition::DiscreteReal { .. } => {
            let residual = lower_equation_expression(
                construction,
                coordinates,
                functions,
                owner_clock,
                &equation.residual,
                generation,
            )?;
            construction.discrete(|system| {
                system.real_equation(owner, |equation| equation.residual(residual))
            })?;
        }
        EquationPartition::DiscreteValue(plan) => {
            let generation = if plan.generated {
                Some(dae::DaeGeneration::DiscreteUpdate)
            } else {
                generation
            };
            let value = lower_equation_expression(
                construction,
                coordinates,
                functions,
                owner_clock,
                plan.value.as_ref(),
                generation,
            )?;
            let Coordinate::DiscreteValue(target) = coordinates[plan.target] else {
                unreachable!("analysis classifies the equation target as discrete-valued")
            };
            let semantic_owner = discrete_values
                .owner(owner, [plan.target.clone()], coordinates, input.topology)?
                .expect("a discrete equation has one planned B.1c owner");
            discrete_values.always(
                semantic_owner,
                target,
                value,
                owner,
                dae::DaeProvenance::source(equation.span)?,
            )?;
        }
        EquationPartition::ConsumedDiscreteValue => {}
        // A multi-result row is lowered only by its multi-output plan.
        EquationPartition::MultiOutput { .. } => {
            return Err(dae::DaeConstructionError::InvalidExpressionForm {
                span: equation.span,
            });
        }
    }
    Ok(())
}
