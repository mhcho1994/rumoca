//! MLS §11.5 / §11.2.8.1: a branch assertion observes its statement's
//! reaching definitions and is vacuously satisfied on every unselected arm.
//! The existing checked assertion owner retains source provenance and lazily
//! renders the original message only when its guarded predicate fails.

use super::*;

#[derive(Default)]
pub(super) struct BranchValues<'dae> {
    pub(super) values: HashMap<VarName, dae::ExprId<'dae>>,
    pub(super) assertions: Vec<ConditionalAssertion<'dae>>,
}

impl<'dae> BranchValues<'dae> {
    pub(super) fn new(values: HashMap<VarName, dae::ExprId<'dae>>) -> Self {
        Self {
            values,
            assertions: Vec::new(),
        }
    }
}

pub(super) struct ConditionalAssertion<'dae> {
    pub(super) condition: dae::ExprId<'dae>,
    pub(super) message: dae::ExprId<'dae>,
    pub(super) provenance: dae::DaeProvenance,
}

pub(super) fn lower_branch_assertion<'dae>(
    construction: &mut dae::DaeConstruction<'dae>,
    body: &dae::FunctionBody<'dae>,
    symbols: FunctionSymbols<'_, 'dae>,
    binders: &HashMap<VarName, dae::DomainBinderId<'dae>>,
    statement: &rumoca_core::Statement,
    values: &mut BranchValues<'dae>,
) -> Result<(), dae::DaeConstructionError> {
    let assertion = function_assertion(statement, symbols.functions.flat)
        .expect("analysis validates the branch assertion")
        .expect("a runtime assertion plan owns an assertion statement");
    let mut lower = |expression| {
        lower_expression_scoped(
            construction,
            LoweringSymbols {
                coordinates: symbols.coordinates,
                functions: symbols.functions,
                shapes: symbols.shapes,
                function_body: Some(body),
                values: Some(&values.values),
                owner_clock: None,
            },
            binders,
            expression,
            None,
        )
    };
    let condition = lower(assertion.condition)?;
    let message = lower(assertion.message)?;
    values.assertions.push(ConditionalAssertion {
        condition,
        message,
        provenance: dae::DaeProvenance::source(assertion.span)?,
    });
    Ok(())
}

/// Use ordered lazy conditionals, not Boolean conjunction: a later branch's
/// predicate must never be evaluated after an earlier branch was selected.
fn guard_assertions<'dae>(
    construction: &mut dae::DaeConstruction<'dae>,
    conditions: &[dae::ExprId<'dae>],
    selected: Option<usize>,
    assertions: Vec<ConditionalAssertion<'dae>>,
) -> Result<Vec<ConditionalAssertion<'dae>>, dae::DaeConstructionError> {
    assertions
        .into_iter()
        .map(|mut assertion| {
            assertion.condition = guard_condition(construction, conditions, selected, &assertion)?;
            Ok(assertion)
        })
        .collect()
}

fn guard_condition<'dae>(
    construction: &mut dae::DaeConstruction<'dae>,
    conditions: &[dae::ExprId<'dae>],
    selected: Option<usize>,
    assertion: &ConditionalAssertion<'dae>,
) -> Result<dae::ExprId<'dae>, dae::DaeConstructionError> {
    let provenance = dae::DaeProvenance::generated(
        dae::DaeGeneration::FunctionConditionLowering,
        assertion.provenance.span(),
    )?;
    construction.expressions(|expressions| {
        let satisfied = expressions
            .at(provenance)
            .literal(dae::DaeLiteral::Boolean(true))?;
        let count = selected.map_or(conditions.len(), |ordinal| ordinal + 1);
        let arms = conditions[..count]
            .iter()
            .enumerate()
            .map(|(ordinal, condition)| {
                let predicate = if selected == Some(ordinal) {
                    assertion.condition
                } else {
                    satisfied
                };
                (*condition, predicate)
            });
        let fallback = if selected.is_some() {
            satisfied
        } else {
            assertion.condition
        };
        expressions.at(provenance).conditional(arms, fallback)
    })
}

pub(super) fn collect_guarded_assertions<'dae>(
    construction: &mut dae::DaeConstruction<'dae>,
    conditions: &[dae::ExprId<'dae>],
    branches: Vec<BranchValues<'dae>>,
    fallback: Option<BranchValues<'dae>>,
) -> Result<Vec<ConditionalAssertion<'dae>>, dae::DaeConstructionError> {
    let mut assertions = Vec::new();
    for (ordinal, branch) in branches.into_iter().enumerate() {
        assertions.extend(guard_assertions(
            construction,
            conditions,
            Some(ordinal),
            branch.assertions,
        )?);
    }
    if let Some(branch) = fallback {
        assertions.extend(guard_assertions(
            construction,
            conditions,
            None,
            branch.assertions,
        )?);
    }
    Ok(assertions)
}
