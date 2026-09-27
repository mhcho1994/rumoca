//! Loop-guarded relations inside `smooth(0, ..)` own events (SPEC_0044
//! ME-EVENT-008).
//!
//! MLS §3.7.5 lets a tool generate no events for expressions inside
//! `smooth`, and rumoca takes that freedom by default. The freedom cannot
//! hold for a relation inside a merely continuous `smooth(0, ..)` whose
//! operands are unknowns of the algebraic loop its equation belongs to: the
//! loop residual is then piecewise in its own unknowns with kinks, so the
//! projection's Newton iteration depends on which branch each step lands on
//! and cannot be proven to converge. Such a relation is frozen
//! like any MLS §8.5 event relation: it owns a root and relation memory, the
//! projection solves with its branch held, and event iteration switches it.
//!
//! The decision needs block membership, so it is made here, after BLT, and is
//! carried as an ordinary relation owner: every later stage and executor
//! handles it without special cases. Relations under `noEvent`, relations of a
//! `semiLinear` expansion (MLS §3.7.4.5, whose operand relation OMC also gives
//! no zero crossing), relations inside a comprehension, and relations that
//! read no unknown of their own block keep the MLS §3.7.5 default. A scalar
//! block counts as a block of one: a relation reading its own unknown makes
//! the equation implicit in it, exactly as in a loop.

use std::collections::BTreeSet;

use rumoca_ir_dae as dae;

use super::{ManifoldEntry, PreparedDae, PreparedSystem, structural_analysis, transformed};
use crate::{BltBlock, StructuralError, UnknownId};

/// Give every loop-guarded `smooth` relation of `prepared` an event owner.
///
/// A system without one is returned unchanged. The rebuild keeps every
/// declaration ordinal, so reduced-chart coordinates survive, and the
/// structural analysis is recomputed on the rebuilt root.
pub fn own_loop_guarded_relations(
    prepared: PreparedDae<'_>,
) -> Result<PreparedDae<'_>, StructuralError> {
    let guards = prepared.inspect(loop_guarded_relations);
    if guards.is_empty() {
        return Ok(prepared);
    }
    let (ids, redundant, charts) = match &prepared {
        PreparedDae::Borrowed { .. } => (Vec::new(), Vec::new(), Box::default()),
        PreparedDae::Transformed {
            manifold,
            manifold_redundant,
            charts,
            ..
        } => (
            manifold.to_vec(),
            manifold_redundant.to_vec(),
            charts.clone(),
        ),
    };
    let (model, ids) =
        super::reconstruction::rebuild_loop_guards(prepared.as_dae(), &guards, &ids)?;
    let manifold = ManifoldEntry::replayed(ids, &redundant);
    let structural = structural_analysis(&model)?;
    transformed(model, manifold, structural, charts)
}

/// Source expression ordinals of the relations that must own events.
fn loop_guarded_relations(system: PreparedSystem<'_, '_>) -> Vec<u32> {
    let Some(sorted) = system.structural else {
        return Vec::new();
    };
    let view = system.view;
    let owned = owned_relations(view);
    let mut guards = BTreeSet::new();
    for block in &sorted.blocks {
        // A scalar block whose relation reads its own unknown is implicit in
        // it just as a loop is: its equation needs the same Newton solve.
        let (equations, unknowns) = match block {
            BltBlock::AlgebraicLoop {
                equations,
                unknowns,
                ..
            } => (equations.as_slice(), unknowns.as_slice()),
            BltBlock::Scalar { equation, unknown } => (
                std::slice::from_ref(equation),
                std::slice::from_ref(unknown),
            ),
            BltBlock::StructuredScalar(_) => continue,
        };
        let loop_unknowns = unknowns
            .iter()
            .filter_map(|unknown| match unknown {
                UnknownId::Algebraic { variable, .. } => Some(variable.index()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        if loop_unknowns.is_empty() {
            continue;
        }
        let residuals = equations
            .iter()
            .filter_map(
                |equation| match view.continuous_owner_for_scalar_row(equation.0) {
                    Some(dae::ContinuousOwnerView::Residual { equation, .. }) => {
                        Some(equation.residual())
                    }
                    _ => None,
                },
            )
            .collect::<BTreeSet<_>>();
        guards.extend(
            residuals
                .into_iter()
                .flat_map(|residual| smooth_relations(view, residual))
                .filter(|relation| {
                    !owned.contains(&relation.index()) && reads_any(view, *relation, &loop_unknowns)
                })
                .map(|relation| relation.index()),
        );
    }
    guards.into_iter().collect()
}

/// Expression ordinals of the relations that already own a root.
fn owned_relations(view: dae::DaeView<'_>) -> BTreeSet<u32> {
    (0..view.root_count())
        .filter_map(|index| {
            let root = view.root(view.root_id(index)?)?;
            Some(view.relation(root.relation())?.expression().index())
        })
        .collect()
}

/// The primitive scalar relations written inside `smooth` (not a `semiLinear`
/// expansion) and not under `noEvent` or a comprehension, in `root`.
fn smooth_relations<'dae>(
    view: dae::DaeView<'dae>,
    root: dae::ExprId<'dae>,
) -> Vec<dae::ExprId<'dae>> {
    let mut smooth_bodies = Vec::new();
    dae::for_each_expression_pruned(view, root, |_, node| match node.operation() {
        dae::ExpressionOperation::Builtin {
            builtin: dae::PureBuiltin::NoEvent,
            ..
        }
        | dae::ExpressionOperation::Comprehension { .. } => false,
        dae::ExpressionOperation::Builtin {
            builtin: dae::PureBuiltin::Smooth,
            arguments,
        } => {
            let semi_linear = node.provenance().origin()
                == dae::DaeProvenanceOrigin::Generated(dae::DaeGeneration::SemiLinearLowering);
            if !semi_linear && continuous_only(view, arguments.get(0)) {
                smooth_bodies.extend(arguments.get(1));
            }
            false
        }
        _ => true,
    });
    let mut relations = Vec::new();
    for body in smooth_bodies {
        dae::for_each_expression_pruned(view, body, |id, node| match node.operation() {
            dae::ExpressionOperation::Builtin {
                builtin: dae::PureBuiltin::NoEvent,
                ..
            }
            | dae::ExpressionOperation::Comprehension { .. } => false,
            dae::ExpressionOperation::Binary { operator, .. } => {
                if is_primitive_relation(operator, node) {
                    relations.push(id);
                }
                true
            }
            _ => true,
        });
    }
    relations
}

fn is_primitive_relation(operator: dae::BinaryOperator, node: dae::ExpressionView<'_>) -> bool {
    matches!(
        operator,
        dae::BinaryOperator::Less
            | dae::BinaryOperator::LessEqual
            | dae::BinaryOperator::Greater
            | dae::BinaryOperator::GreaterEqual
    ) && node.value_type().is_scalar()
        && node.binder_domain().is_none()
        && node.function_scope().is_none()
}

/// Whether `relation` reads an algebraic variable in `unknowns`.
fn reads_any<'dae>(
    view: dae::DaeView<'dae>,
    relation: dae::ExprId<'dae>,
    unknowns: &BTreeSet<u32>,
) -> bool {
    let mut reads = false;
    dae::for_each_expression(view, relation, |_, node| {
        if let dae::ExpressionOperation::Coordinate(dae::CoordinateView::Algebraic(variable)) =
            node.operation()
        {
            reads |= unknowns.contains(&variable.index());
        }
    });
    reads
}

/// Whether a `smooth` order is the literal 0. An expression that is at least
/// once continuously differentiable keeps a residual whose Newton step is
/// well defined across its branches, so only a merely continuous expression
/// needs its loop-guarded relations frozen; freezing a differentiable one
/// would hold a branch past the domain its guard protects (an exponential
/// past its linear continuation, for example).
fn continuous_only<'dae>(view: dae::DaeView<'dae>, order: Option<dae::ExprId<'dae>>) -> bool {
    order
        .and_then(|order| view.expression(order))
        .is_some_and(|order| {
            matches!(
                order.operation(),
                dae::ExpressionOperation::Literal(dae::DaeLiteral::Integer(0))
            )
        })
}
