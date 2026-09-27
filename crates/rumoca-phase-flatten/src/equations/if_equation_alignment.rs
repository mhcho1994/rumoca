//! Row pairing for run-time if-equations (MLS §8.3.4).

use rumoca_ir_ast as ast;

use super::SimpleEquation;

/// The declaration identity of an unsubscripted component reference `v` that
/// forms the left-hand side of an explicit `v = expr` equation.
fn assigned_target_identity(equation: &SimpleEquation) -> Option<Vec<rumoca_core::DefId>> {
    let ast::Expression::ComponentReference(reference) = &equation.lhs else {
        return None;
    };
    reference
        .parts
        .iter()
        .map(|part| part.subs.is_none().then_some(part.def_id).flatten())
        .collect()
}

/// Pair the rows of a run-time if-equation by the variable each row assigns.
///
/// MLS §8.3.4 makes each branch a set of equations; their textual order carries
/// no meaning. When every row of every branch is an explicit `v = expr`
/// assignment of a distinct unsubscripted variable and all branches assign the
/// same variables, each branch is reordered to the first branch's target
/// order, so the conditional row built for one position assigns one variable in
/// every branch. Any other shape keeps its written order.
pub(super) fn align_branches_by_assigned_target(
    branches: &mut [(ast::Expression, Vec<SimpleEquation>)],
    else_equations: &mut Vec<SimpleEquation>,
) {
    let Some((_, first)) = branches.first() else {
        return;
    };
    let Some(order) = first
        .iter()
        .map(assigned_target_identity)
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    let distinct = order.iter().collect::<std::collections::HashSet<_>>();
    if distinct.len() != order.len() {
        return;
    }
    let aligned = |equations: &[SimpleEquation]| -> Option<Vec<usize>> {
        let targets = equations
            .iter()
            .map(assigned_target_identity)
            .collect::<Option<Vec<_>>>()?;
        order
            .iter()
            .map(|target| targets.iter().position(|candidate| candidate == target))
            .collect::<Option<Vec<_>>>()
            .filter(|positions| {
                positions.iter().collect::<std::collections::HashSet<_>>().len() == positions.len()
            })
    };
    let Some(permutations) = branches
        .iter()
        .map(|(_, equations)| aligned(equations))
        .chain(std::iter::once(aligned(else_equations)))
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    let reorder = |equations: &mut Vec<SimpleEquation>, positions: &[usize]| {
        let mut taken = std::mem::take(equations)
            .into_iter()
            .map(Some)
            .collect::<Vec<_>>();
        *equations = positions
            .iter()
            .map(|&position| taken[position].take().expect("positions are distinct"))
            .collect();
    };
    let (else_positions, branch_positions) =
        permutations.split_last().expect("the else permutation is present");
    for ((_, equations), positions) in branches.iter_mut().zip(branch_positions) {
        reorder(equations, positions);
    }
    reorder(else_equations, else_positions);
}

