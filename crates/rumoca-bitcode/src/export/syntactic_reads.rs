//! Read sets for residuals the scalar projection cannot see through.
use super::*;

/// Every variable coordinate reachable from `root`, by syntax.
///
/// Used only when `rumoca_eval_dae`'s scalar projection refuses a residual
/// because it calls an external body (MLS §12.9: no definition to follow) or
/// reads a record field it does not project. For an opaque call the honest
/// incidence is "everything its arguments read", which is exactly what a
/// syntactic walk gives: a superset of the scalar incidence, never a subset.
/// Refusing instead made such models impossible to export at all, although
/// the compiler accepts them.
pub(super) fn syntactic(view: dae::DaeView<'_>, root: dae::ExprId<'_>) -> Option<Reads> {
    let options = ExportOptions::default();
    let mut reads = Reads::default();
    let mut seen = BTreeSet::new();
    let mut pending = vec![root.index()];
    while let Some(index) = pending.pop() {
        if !seen.insert(index) {
            continue;
        }
        let id = view.expression_id(index as usize)?;
        let expression = view.expression(id)?;
        if let dae::ExpressionOperation::Coordinate(coordinate) = expression.operation() {
            reads.visit(coordinate);
        }
        let node = expression_node(expression, index, &options).ok()?;
        pending.extend(
            crate::build::references(ExprId(index), &node)
                .into_iter()
                .map(|operand| operand.0),
        );
    }
    Some(reads)
}
