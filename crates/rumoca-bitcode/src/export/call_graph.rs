//! Which functions each function calls, recovered from the checked DAE.

use super::*;

/// Callee edges for every function, indexed by function ordinal.
///
/// Two sweeps: every expression that depends on a body knows the function
/// scope it belongs to (`function_scope`), and each body is also walked from
/// its definitions and loop values, which reaches the calls that depend on
/// nothing in it.
pub(super) fn call_graph(view: dae::DaeView<'_>) -> Vec<Vec<FunctionId>> {
    let mut edges: Vec<Vec<FunctionId>> = vec![Vec::new(); view.function_count()];
    for index in 0..view.expression_count() {
        let Some(id) = view.expression_id(index) else {
            continue;
        };
        let Some(expression) = view.expression(id) else {
            continue;
        };
        let Some(owner) = expression.function_scope() else {
            continue;
        };
        let dae::ExpressionOperation::Call { function, .. } = expression.operation() else {
            continue;
        };
        if let Some(slot) = edges.get_mut(owner.index() as usize) {
            slot.push(FunctionId(function.index()));
        }
    }
    // A call whose arguments are all literals depends on nothing in the body
    // (`candidate(2.0)`), so it carries no function scope and the sweep above
    // misses it; it is still reached only through the body. Walk each body
    // from its definitions and loop values as well.
    let mut traversal = dae::ExpressionTraversal::new();
    for (index, slot) in edges.iter_mut().enumerate() {
        let Some(function) = view.function_id(index).and_then(|id| view.function(id)) else {
            continue;
        };
        let definitions = (0..function.definition_count())
            .filter_map(|ordinal| function.definition_id(ordinal))
            .filter_map(|definition| view.function_definition(definition))
            .map(|definition| definition.rhs());
        let folds = (0..function.fold_count())
            .filter_map(|ordinal| function.fold_id(ordinal))
            .filter_map(|fold| view.function_fold(fold))
            .flat_map(|fold| {
                fold.initial_values()
                    .rhs_iter()
                    .chain(fold.update_values().rhs_iter())
                    .collect::<Vec<_>>()
            });
        let roots = definitions.chain(folds).collect::<Vec<_>>();
        traversal.visit_pruned(view, roots, |_, expression| {
            if let dae::ExpressionOperation::Call { function, .. } = expression.operation() {
                slot.push(FunctionId(function.index()));
            }
            true
        });
    }
    for callees in &mut edges {
        callees.sort_unstable_by_key(|id| id.0);
        callees.dedup();
    }
    edges
}
