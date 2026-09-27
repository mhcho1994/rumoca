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
//! The derivative of `rhs` reads a closure of columns that `DerivativeClosure`
//! collects by following every path the differentiator can take: the values
//! of the coordinates read, a state's derivative (or the values its explicit
//! derivative definition reads), and for an algebraic the state anchor of its
//! equality class under each anchor mode the reconstruction may select, else
//! the closure of its causal definition. A function body reads no model
//! coordinate. Paths the closure does not follow (a derivative or record read,
//! an auxiliary block, a record component definition, or a derivative
//! definition reading a derivative, algebraic, or record) leave the candidate
//! unbounded. So a touched row after the demotion is contained in its prior
//! row without `x`'s columns, plus `x`'s columns, plus, when it read
//! `der(x)`, the closure. A maximum matching on these superset rows is at
//! least the true one, so its residue `E + U - 2M` is a lower bound on the
//! rebuilt system's residue: when the bound is not below the current residue,
//! the demotion cannot reduce it.

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
    /// applies: an exact expression definition whose derivative closure is
    /// followed completely, and a state with unknown columns.
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
        let derivative_columns = DerivativeClosure {
            view,
            facts,
            bounds: self.bounds,
            demoted: candidate.state,
            columns: Vec::new(),
            algebraics: std::collections::BTreeSet::new(),
            pending: vec![rhs],
            rates: Vec::new(),
        }
        .collect()?;
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

/// Whether differentiating or materializing `node` can read columns its own
/// coordinates do not show: a derivative, an algebraic (whose equality anchor,
/// causal definition, or auxiliary block is followed), or a record field or
/// constructor (whose component definitions are followed).
fn reads_hidden_columns(node: dae::ExpressionView<'_>) -> bool {
    matches!(
        node.operation(),
        dae::ExpressionOperation::Coordinate(
            dae::CoordinateView::Derivative(_) | dae::CoordinateView::Algebraic(_)
        ) | dae::ExpressionOperation::Field { .. }
            | dae::ExpressionOperation::Record(_)
    )
}

/// The unknown columns the derivative of a demotion's definition can read,
/// following every path the differentiator can take, or `None` when a path
/// reads columns this closure does not follow.
///
/// Differentiating an expression reads the values of the coordinates it reads
/// and, per coordinate: nothing for a parameter or time; for a state, its
/// derivative, or the values its explicit derivative definition reads; for an
/// algebraic, its auxiliary block or record component definition (not
/// followed), else the state anchor of its equality class (whichever of the
/// exact, affine, or demotion anchor the reconstruction selects, all taken
/// here), else the derivative of its causal definition. A derivative read, a
/// record field, or a record constructor is not followed.
struct DerivativeClosure<'a, 'dae> {
    view: dae::DaeView<'dae>,
    facts: &'a super::constraints::DifferentiationFacts,
    bounds: &'a DemotionRowBounds,
    demoted: u32,
    columns: Vec<usize>,
    /// Algebraics already followed.
    algebraics: std::collections::BTreeSet<u32>,
    /// Expressions whose derivative is still to be followed.
    pending: Vec<dae::ExprId<'dae>>,
    /// Explicit derivative definitions, read as values.
    rates: Vec<dae::ExprId<'dae>>,
}

impl<'dae> DerivativeClosure<'_, 'dae> {
    fn collect(mut self) -> Option<Vec<usize>> {
        while let Some(expression) = self.pending.pop() {
            self.differentiate(expression)?;
        }
        let mut hidden = false;
        dae::ExpressionTraversal::new().visit_pruned(self.view, self.rates.clone(), |_, node| {
            hidden |= reads_hidden_columns(node);
            true
        });
        (!hidden).then_some(self.columns)
    }

    fn differentiate(&mut self, expression: dae::ExprId<'dae>) -> Option<()> {
        let mut nodes = Vec::new();
        dae::ExpressionTraversal::new().visit_pruned(self.view, [expression], |_, node| {
            nodes.push(node);
            true
        });
        for node in nodes {
            if matches!(
                node.operation(),
                dae::ExpressionOperation::Coordinate(dae::CoordinateView::Derivative(_))
                    | dae::ExpressionOperation::Field { .. }
                    | dae::ExpressionOperation::Record(_)
            ) {
                return None;
            }
            if let Some(columns) = node
                .variable_coordinate()
                .and_then(|variable| self.bounds.columns(variable.index()))
            {
                self.columns.extend(columns);
            }
            match node.operation() {
                dae::ExpressionOperation::Coordinate(dae::CoordinateView::State(state)) => {
                    self.state(state.index());
                }
                dae::ExpressionOperation::Coordinate(dae::CoordinateView::Algebraic(algebraic)) => {
                    self.algebraic(algebraic)?;
                }
                _ => {}
            }
        }
        Some(())
    }

    /// A state's derivative: its column, or the values its definition reads.
    fn state(&mut self, state: u32) {
        self.columns
            .extend(self.bounds.columns(state).into_iter().flatten());
        if let Some(definition) = self.facts.derivative_definitions[state as usize] {
            self.rates
                .extend(self.view.expression_id(definition.expression as usize));
        }
    }

    fn algebraic(&mut self, algebraic: dae::AlgebraicId<'dae>) -> Option<()> {
        let index = algebraic.index();
        if !self.algebraics.insert(index) {
            return Some(());
        }
        if self.facts.auxiliary_blocks[index as usize].is_some()
            || self.facts.component_definitions[index as usize].is_some()
        {
            return None;
        }
        let equalities = &self.facts.equalities;
        let anchors = [
            equalities.anchor_of(index),
            equalities.value_anchor_of(index),
            equalities.anchor_for_demotion(index, self.demoted),
        ];
        for (anchor, _) in anchors.iter().flatten() {
            if let super::equalities::EqualityAnchor::State(state) = anchor {
                self.state(*state);
            }
        }
        // Some anchor mode finds none, so the causal definition is followed.
        if anchors.iter().any(Option::is_none) {
            let definition = self.facts.algebraic_definition(self.view, algebraic)?;
            self.pending.push(definition);
        }
        Some(())
    }
}
