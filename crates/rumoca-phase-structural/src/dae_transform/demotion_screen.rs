//! A structural lower bound on the residue a direct-state demotion leaves.
//!
//! Demoting state `x` with exact definition `x = rhs` (no retained manifold)
//! rewrites only the continuous owners that reference `x`: each `der(x)`
//! becomes the derivative of `rhs`, and each value of `x` stays, now as an
//! algebraic unknown. The unknown catalog is the dense variable order, so
//! `x`'s columns keep their positions (its derivative scalars before, its
//! value scalars after) and the column count is unchanged; the equation rows
//! are unchanged too. Every other owner keeps its exact rows, which the
//! incremental incidence already relies on.
//!
//! The bound applies only when `rhs` reads no algebraic coordinate and no
//! record field or constructor, and no state with an explicit derivative
//! definition: the differentiator reads an algebraic's
//! equality anchor, causal definition, auxiliary block, or record component
//! definitions, whose columns `rhs` does not show. Without those, the
//! derivative of `rhs` reads only coordinates `rhs` reads and the
//! derivatives of the states among them; a function body reads no model
//! coordinate. So a touched row after the demotion is contained in its prior
//! row without `x`'s columns, plus `x`'s columns, plus, when it read
//! `der(x)`, the columns of every state or algebraic `rhs` reads. A maximum
//! matching on these superset rows is at least the true one, so its residue
//! `E + U - 2M` is a lower bound on the rebuilt system's residue: when the
//! bound is not below the current residue, the demotion cannot reduce it.

use rumoca_ir_dae as dae;

use super::demotion_bounds::DemotionRowBounds;
use super::{DirectStateConstraint, StateDefinition};
use crate::incidence::ReusableIncidence;
use crate::incidence::rows::IncidenceRowsBuilder;

/// The residue bound of one round's demotion candidates.
pub(super) struct DemotionScreen<'a> {
    base: &'a ReusableIncidence,
    bounds: &'a DemotionRowBounds,
    residue: usize,
}

impl<'a> DemotionScreen<'a> {
    pub(super) const fn new(
        base: &'a ReusableIncidence,
        bounds: &'a DemotionRowBounds,
        residue: usize,
    ) -> Self {
        Self {
            base,
            bounds,
            residue,
        }
    }

    /// The residue lower bound of demoting `candidate`, when the bound
    /// applies: an exact expression definition whose coordinates are values
    /// (never a derivative or an algebraic, and no record field or constructor)
    /// and a state with unknown columns.
    pub(super) fn residue_bound(
        &self,
        view: dae::DaeView<'_>,
        facts: &super::constraints::DifferentiationFacts,
        candidate: &DirectStateConstraint,
    ) -> Option<usize> {
        let StateDefinition::Expression(rhs) = candidate.rhs else {
            return None;
        };
        let state_columns = self.bounds.columns(candidate.state)?;
        let rhs = view.expression_id(rhs as usize)?;
        let mut derivative_columns = Vec::new();
        let mut unbounded = false;
        dae::ExpressionTraversal::new().visit_pruned(view, [rhs], |_, node| {
            if matches!(
                node.operation(),
                dae::ExpressionOperation::Coordinate(
                    dae::CoordinateView::Derivative(_) | dae::CoordinateView::Algebraic(_)
                ) | dae::ExpressionOperation::Field { .. }
                    | dae::ExpressionOperation::Record(_)
            ) {
                unbounded = true;
            }
            // A state with an explicit derivative definition differentiates into
            // that definition, whose columns this read does not show.
            if let dae::ExpressionOperation::Coordinate(dae::CoordinateView::State(state)) =
                node.operation()
                && facts.derivative_definitions[state.index() as usize].is_some()
            {
                unbounded = true;
            }
            if let Some(columns) = node
                .variable_coordinate()
                .and_then(|variable| self.bounds.columns(variable.index()))
            {
                derivative_columns.extend(columns);
            }
            true
        });
        if unbounded {
            return None;
        }
        let touched = self.bounds.owners(candidate.state);
        let n_eq = self.base.rows().len();
        let mut rows = IncidenceRowsBuilder::with_row_capacity(n_eq);
        let mut owner = 0;
        let mut owner_end = self.base.owner_rows(0).map_or(n_eq, |range| range.end);
        for row in 0..n_eq {
            while row >= owner_end {
                owner += 1;
                owner_end = self.base.owner_rows(owner).map_or(n_eq, |range| range.end);
            }
            let columns = self.base.rows().row(row);
            if touched.binary_search(&owner).is_err() {
                rows.push_canonical_row(columns);
                continue;
            }
            let read_derivative = columns.iter().any(|column| state_columns.contains(column));
            rows.push_unsorted(
                columns
                    .iter()
                    .copied()
                    .filter(|column| !state_columns.contains(column))
                    .chain(state_columns.clone())
                    .chain(
                        read_derivative
                            .then(|| derivative_columns.iter().copied())
                            .into_iter()
                            .flatten(),
                    ),
            );
        }
        let rows = rows.finish();
        let n_var = self.bounds.unknown_count();
        let (matched, _) = crate::matching::maximum_matching_with_structured(
            n_eq,
            n_var,
            &rows,
            &vec![None; n_eq],
            &[],
        );
        let matched = matched.iter().filter(|column| column.is_some()).count();
        Some((n_eq - matched) + (n_var - matched))
    }

    /// Whether demoting `candidate` provably cannot reduce the residue.
    pub(super) fn cannot_reduce(
        &self,
        view: dae::DaeView<'_>,
        facts: &super::constraints::DifferentiationFacts,
        candidate: &DirectStateConstraint,
    ) -> Option<usize> {
        self.residue_bound(view, facts, candidate)
            .filter(|bound| *bound >= self.residue)
    }
}
