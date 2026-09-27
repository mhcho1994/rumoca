use super::*;

/// The discrete owners a model multi-output equation may define.
pub(super) struct MultiOutputDiscreteOwners<'scope, 'dae> {
    pub(super) discrete_values: &'scope mut DiscreteValueStaging<'dae>,
    pub(super) topology: &'scope DiscreteValueTopologyPlan,
    /// The clock of the partition the equation belongs to, when it is clocked.
    pub(super) owner_clock: Option<dae::PeriodicClockId<'dae>>,
}

/// The receiving tuple and the called function of one multi-result equation.
struct MultiOutputSource<'flat> {
    receivers: &'flat [Expression],
    name: &'flat rumoca_core::Reference,
    arguments: &'flat [Expression],
    provenance: dae::DaeProvenance,
}

fn multi_output_source(
    equation: &flat::Equation,
) -> Result<MultiOutputSource<'_>, dae::DaeConstructionError> {
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
    Ok(MultiOutputSource {
        receivers: elements,
        name,
        arguments: args,
        provenance: dae::DaeProvenance::source(*span)?,
    })
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
    let source = multi_output_source(equation)?;
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
    let results = receiver_results(construction, symbols, &source, &selected)?;
    let mut discrete = discrete;
    let discrete_value_owner =
        discrete_value_owner(coordinates, &selected, discrete.as_mut(), owner)?;
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
                    source.provenance,
                )?;
            }
            coordinate => define_residual_receiver(
                construction,
                (coordinates, functions),
                (&source.receivers[ordinal], value),
                coordinate,
                (owner, discrete.is_none()),
            )?,
        }
    }
    Ok(())
}

/// The call result of every retained receiver, in receiver order.
///
/// The continuous receivers read one shared call. Every discrete receiver
/// belongs to its own owner and reads its result from its own call of the same
/// pure function, which the analysis proved yields the same value.
fn receiver_results<'dae>(
    construction: &mut dae::DaeConstruction<'dae>,
    symbols: LoweringSymbols<'_, 'dae>,
    source: &MultiOutputSource<'_>,
    selected: &[(usize, &VarName)],
) -> Result<Vec<dae::ExprId<'dae>>, dae::DaeConstructionError> {
    let is_discrete = |target: &VarName| {
        matches!(
            symbols.coordinates[target],
            Coordinate::DiscreteReal(_) | Coordinate::DiscreteValue(_)
        )
    };
    let call = |construction: &mut dae::DaeConstruction<'dae>| {
        lower_call_operands(
            construction,
            symbols,
            &HashMap::new(),
            source.name,
            source.arguments,
            source.provenance,
        )
    };
    let shared = call(construction)?.results(
        construction,
        selected
            .iter()
            .filter(|(_, target)| !is_discrete(target))
            .map(|(ordinal, _)| *ordinal),
        source.provenance,
    )?;
    let mut shared = shared.into_iter();
    let mut results = Vec::with_capacity(selected.len());
    for (ordinal, target) in selected {
        if is_discrete(target) {
            let own = call(construction)?.results(construction, [*ordinal], source.provenance)?;
            results.extend(own);
        } else {
            results.push(
                shared
                    .next()
                    .expect("one shared result per continuous receiver"),
            );
        }
    }
    Ok(results)
}

/// The planned B.1c owner of the discrete-valued receivers, if any.
fn discrete_value_owner<'dae>(
    coordinates: &HashMap<VarName, Coordinate<'dae>>,
    selected: &[(usize, &VarName)],
    discrete: Option<&mut MultiOutputDiscreteOwners<'_, 'dae>>,
    owner: dae::DaeProvenance,
) -> Result<Option<DiscreteValueOwnerHandle>, dae::DaeConstructionError> {
    let targets = selected
        .iter()
        .filter(|(_, target)| matches!(coordinates[*target], Coordinate::DiscreteValue(_)))
        .map(|(_, target)| (*target).clone())
        .collect::<Vec<_>>();
    if targets.is_empty() {
        return Ok(None);
    }
    let discrete = discrete.expect("analysis admits discrete receivers only in model equations");
    discrete
        .discrete_values
        .owner(owner, targets, coordinates, discrete.topology)
}

/// A continuous, initialization, or discrete Real receiver is defined by the
/// residual `receiver - result`.
fn define_residual_receiver<'dae>(
    construction: &mut dae::DaeConstruction<'dae>,
    (coordinates, functions): (
        &HashMap<VarName, Coordinate<'dae>>,
        &FunctionRegistry<'_, 'dae>,
    ),
    (receiver, value): (&Expression, dae::ExprId<'dae>),
    coordinate: Coordinate<'dae>,
    (owner, initialization): (dae::DaeProvenance, bool),
) -> Result<(), dae::DaeConstructionError> {
    let lhs = lower_expression(construction, coordinates, functions, receiver, None)?;
    let residual = generated_residual(construction, owner, lhs, value)?;
    match coordinate {
        Coordinate::DiscreteReal(_) => {
            construction.discrete(|system| {
                system.real_equation(owner, |equation| equation.residual(residual))
            })?;
        }
        _ if initialization => {
            construction.initialization(|system| system.value_equation(owner, residual))?;
        }
        _ => {
            construction.continuous(|system| system.value_equation(owner, residual))?;
        }
    }
    Ok(())
}
