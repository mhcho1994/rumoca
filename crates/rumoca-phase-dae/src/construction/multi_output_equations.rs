use super::*;

/// The discrete owners a model multi-output equation may define.
pub(super) struct MultiOutputDiscreteOwners<'scope, 'dae> {
    pub(super) discrete_values: &'scope mut DiscreteValueStaging<'dae>,
    pub(super) topology: &'scope DiscreteValueTopologyPlan,
    /// The clock of the partition the equation belongs to, when it is clocked.
    pub(super) owner_clock: Option<dae::PeriodicClockId<'dae>>,
}

pub(super) fn lower_multi_output_equation<'dae>(
    construction: &mut dae::DaeConstruction<'dae>,
    coordinates: &HashMap<VarName, Coordinate<'dae>>,
    functions: &FunctionRegistry<'_, 'dae>,
    equation: &flat::Equation,
    plan: &MultiOutputEquationPlan,
    owner: dae::DaeProvenance,
    discrete: Option<MultiOutputDiscreteOwners<'_, 'dae>>,
) -> Result<(), dae::DaeConstructionError> {
    let Expression::Binary {
        op: OpBinary::Sub,
        lhs,
        rhs,
        ..
    } = &equation.residual
    else {
        unreachable!("a multi-output equation plan owns a subtraction residual")
    };
    let Expression::Tuple { elements, .. } = lhs.as_ref() else {
        unreachable!("a multi-output equation plan owns a receiving tuple")
    };
    let Expression::FunctionCall {
        name,
        args,
        is_constructor: false,
        span,
    } = rhs.as_ref()
    else {
        unreachable!("a multi-output equation plan owns a function call")
    };
    let provenance = dae::DaeProvenance::source(*span)?;
    let symbols = LoweringSymbols {
        coordinates,
        functions,
        shapes: functions.shapes.model_values(),
        function_body: None,
        values: None,
        owner_clock: discrete.as_ref().and_then(|discrete| discrete.owner_clock),
    };
    let selected = plan
        .outputs
        .iter()
        .enumerate()
        .filter_map(|(ordinal, target)| target.as_ref().map(|target| (ordinal, target)))
        .collect::<Vec<_>>();
    // The continuous receivers read one shared call. Every discrete receiver
    // belongs to its own owner and reads its result from its own call of the
    // same pure function, which the analysis proved yields the same value.
    let is_discrete = |target: &VarName| {
        matches!(
            coordinates[target],
            Coordinate::DiscreteReal(_) | Coordinate::DiscreteValue(_)
        )
    };
    let shared = lower_call_operands(
        construction,
        symbols,
        &HashMap::new(),
        name,
        args,
        provenance,
    )?
    .results(
        construction,
        selected
            .iter()
            .filter(|(_, target)| !is_discrete(target))
            .map(|(ordinal, _)| *ordinal),
        provenance,
    )?;
    let mut shared = shared.into_iter();
    let mut results = Vec::with_capacity(selected.len());
    for (ordinal, target) in &selected {
        if is_discrete(target) {
            let own = lower_call_operands(
                construction,
                symbols,
                &HashMap::new(),
                name,
                args,
                provenance,
            )?
            .results(construction, [*ordinal], provenance)?;
            results.extend(own);
        } else {
            results.push(
                shared
                    .next()
                    .expect("one shared result per continuous receiver"),
            );
        }
    }
    let mut discrete = discrete;
    let discrete_value_targets = selected
        .iter()
        .filter(|(_, target)| matches!(coordinates[*target], Coordinate::DiscreteValue(_)))
        .map(|(_, target)| (*target).clone())
        .collect::<Vec<_>>();
    let discrete_value_owner = match (&mut discrete, discrete_value_targets.is_empty()) {
        (_, true) => None,
        (Some(discrete), false) => discrete.discrete_values.owner(
            owner,
            discrete_value_targets,
            coordinates,
            discrete.topology,
        )?,
        (None, false) => unreachable!("analysis admits discrete receivers only in model equations"),
    };
    for ((ordinal, target), value) in selected.into_iter().zip(results) {
        match coordinates[target] {
            Coordinate::DiscreteValue(target) => {
                let discrete = discrete
                    .as_mut()
                    .expect("a discrete-valued receiver has a discrete owner");
                discrete.discrete_values.always(
                    discrete_value_owner.expect("a discrete-valued receiver has a planned owner"),
                    target,
                    value,
                    owner,
                    provenance,
                )?;
            }
            coordinate => {
                let lhs = lower_expression(
                    construction,
                    coordinates,
                    functions,
                    &elements[ordinal],
                    None,
                )?;
                let residual = generated_residual(construction, owner, lhs, value)?;
                match (coordinate, &discrete) {
                    (Coordinate::DiscreteReal(_), _) => {
                        construction.discrete(|system| {
                            system.real_equation(owner, |equation| equation.residual(residual))
                        })?;
                    }
                    (_, None) => {
                        construction.initialization(|system| system.value_equation(owner, residual))?
                    }
                    (_, Some(_)) => {
                        construction.continuous(|system| system.value_equation(owner, residual))?
                    }
                }
            }
        }
    }
    Ok(())
}
