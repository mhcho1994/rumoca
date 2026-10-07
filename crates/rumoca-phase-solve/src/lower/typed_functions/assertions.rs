//! Function-assertion discovery over checked DAE statement owners.

use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

use rumoca_ir_dae as dae;
use rumoca_ir_solve as solve;

/// MLS §11.2.2: a zero extent anywhere in a rectangular loop nest means
/// that its body, including every assertion predicate, is never evaluated.
pub(super) fn assertion_domain_is_empty<'dae>(
    view: dae::DaeView<'dae>,
    domains: &[dae::DomainId<'dae>],
    provenance: rumoca_core::Span,
) -> Result<bool, solve::SolveProgramConstructionError> {
    for domain in domains {
        let extents = view
            .domain(*domain)
            .ok_or(solve::SolveProgramConstructionError::WireMismatch)?
            .structured()
            .extents()
            .map_err(|_| solve::SolveProgramConstructionError::InvalidMap { provenance })?;
        if extents.contains(&0) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[derive(Clone)]
pub(super) struct FunctionAssertion<'dae> {
    pub(super) condition: dae::ExprId<'dae>,
    pub(super) message: dae::ExprId<'dae>,
    pub(super) provenance: dae::DaeProvenance,
}

/// The source loop body and its interval in the call's predicate interface.
/// Indexing before demand lowering keeps an early fold projection from
/// assigning its assertions to an unrelated statement's output slots.
#[derive(Clone)]
pub(super) struct FoldBody<'dae> {
    pub(super) statements: dae::FunctionStatements<'dae>,
    pub(super) assertions: Range<usize>,
    pub(super) conditions: Vec<dae::ExprId<'dae>>,
}

#[derive(Default)]
pub(super) struct FunctionAssertions<'dae> {
    pub(super) conditions: Vec<FunctionAssertion<'dae>>,
    pub(super) loops: HashMap<dae::FunctionFoldId<'dae>, FoldBody<'dae>>,
}

pub(super) fn assertion_conditions<'dae>(
    view: dae::DaeView<'dae>,
    function: dae::FunctionView<'dae>,
) -> Result<FunctionAssertions<'dae>, solve::SolveProgramConstructionError> {
    let mut assertions = FunctionAssertions::default();
    collect_assertion_conditions(view, function.statements(), &mut assertions)?;
    Ok(assertions)
}

fn collect_assertion_conditions<'dae>(
    view: dae::DaeView<'dae>,
    statements: dae::FunctionStatements<'dae>,
    assertions: &mut FunctionAssertions<'dae>,
) -> Result<(), solve::SolveProgramConstructionError> {
    for statement in statements {
        match statement {
            dae::FunctionStatementView::Assignment { .. }
            | dae::FunctionStatementView::AssignmentGroup { .. } => {}
            dae::FunctionStatementView::Assertion {
                condition,
                message,
                provenance,
            } => assertions.conditions.push(FunctionAssertion {
                condition,
                message,
                provenance,
            }),
            dae::FunctionStatementView::For {
                fold, statements, ..
            } => {
                view.function_fold(fold)
                    .ok_or(solve::SolveProgramConstructionError::WireMismatch)?;
                let start = assertions.conditions.len();
                collect_assertion_conditions(view, statements.clone(), assertions)?;
                let range = start..assertions.conditions.len();
                let conditions = assertions.conditions[range.clone()]
                    .iter()
                    .map(|assertion| assertion.condition)
                    .collect();
                assertions.loops.insert(
                    fold,
                    FoldBody {
                        statements,
                        assertions: range,
                        conditions,
                    },
                );
            }
        }
    }
    Ok(())
}

pub(super) fn nested_calls<'dae>(
    view: dae::DaeView<'dae>,
    function: dae::FunctionView<'dae>,
    assertions: &[FunctionAssertion<'dae>],
) -> Vec<dae::ExprId<'dae>> {
    let mut calls = Vec::new();
    let mut seen = HashSet::new();
    for root in function
        .result_values()
        .rhs_iter()
        .chain(assertions.iter().map(|assertion| assertion.condition))
    {
        dae::for_each_expression(view, root, |_, node| {
            if let dae::ExpressionOperation::Call { owner, .. } = node.operation()
                && seen.insert(owner)
            {
                calls.push(owner);
            }
        });
    }
    calls
}

pub(super) fn assertion_is_map_independent<'dae>(
    view: dae::DaeView<'dae>,
    condition: dae::ExprId<'dae>,
) -> bool {
    let mut independent = true;
    dae::for_each_expression(view, condition, |_, node| {
        if matches!(
            node.operation(),
            dae::ExpressionOperation::FunctionValue { .. }
                | dae::ExpressionOperation::FunctionFoldParameter { .. }
        ) {
            independent = false;
        }
    });
    independent
}

impl<'dae> super::ExpressionLowerer<'_, '_, 'dae> {
    /// Demand assertions in the same iteration environment as numeric updates.
    /// Assignments are resolved by exact SSA identity, so a shared local is
    /// computed once and reads before/after a redefinition stay distinct.
    pub(super) fn iteration_assertions(
        &mut self,
        statements: dae::FunctionStatements<'dae>,
    ) -> Result<(), solve::SolveProgramConstructionError> {
        for statement in statements {
            match statement {
                dae::FunctionStatementView::Assertion {
                    condition,
                    provenance,
                    ..
                } => {
                    let predicate = self
                        .expression(condition)?
                        .only_register(provenance.span())?;
                    self.record_assertion_predicate(predicate, provenance.span())?;
                }
                dae::FunctionStatementView::For {
                    fold, provenance, ..
                } => {
                    self.iteration_nested_assertions(fold, provenance.span())?;
                }
                dae::FunctionStatementView::Assignment { .. }
                | dae::FunctionStatementView::AssignmentGroup { .. } => {}
            }
        }
        Ok(())
    }
    fn iteration_nested_assertions(
        &mut self,
        fold: dae::FunctionFoldId<'dae>,
        provenance: rumoca_core::Span,
    ) -> Result<(), solve::SolveProgramConstructionError> {
        let range = self
            .fold_bodies
            .get(&fold)
            .ok_or(solve::SolveProgramConstructionError::WireMismatch)?
            .assertions
            .clone();
        if !range.is_empty() {
            self.function_fold(fold, provenance)?;
        }
        self.next_direct_assertion = range.end;
        Ok(())
    }
}
