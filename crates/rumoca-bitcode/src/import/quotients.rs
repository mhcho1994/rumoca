//! Dynamic quotients: `div`, `mod` and `rem` whose operands vary.
//!
//! The DAE does not admit such a quotient as a bare builtin. In an equation
//! it needs an event owner -- MLS §3.7.2 makes it discontinuous wherever the
//! ratio crosses an integer -- which the compiler builds as one batch: the
//! quotient, six generated nodes of the indicator `sin(pi * lhs / rhs) >= 0`,
//! a relation on that indicator, an `Always` activation and a root. In a
//! function body it is event-free but still owned, by the function.
//!
//! The artifact carries all of that as ordinary nodes, relations, conditions
//! and roots. Rebuilding them one by one fails at the quotient, so import
//! re-issues the batch through the DAE's replay protocol instead, and later
//! routes the owner's relation, activation and root through the same token.

use super::*;

/// A model-owned quotient heads its batch: the next node is the first
/// generated indicator node, stamped `RuntimeDiscontinuity`.
pub(super) fn model_owner(model: &RbcModel, index: usize) -> Option<dae::PureBuiltin> {
    let (builtin, _) = operands(&model.expressions.get(index)?.node)?;
    let next = model.expressions.get(index + 1)?;
    let generated = matches!(
        next.provenance.origin,
        RbcOrigin::Generated {
            generation: RbcGeneration::RuntimeDiscontinuity
        }
    );
    (generated && model.expressions.len() > index + 6).then_some(builtin)
}

/// The builtin and its two operands, when a node is a quotient.
pub(super) fn operands(node: &RbcExprNode) -> Option<(dae::PureBuiltin, [ExprId; 2])> {
    let RbcExprNode::Builtin { name, arguments } = node else {
        return None;
    };
    let builtin = match name.as_str() {
        "div" => dae::PureBuiltin::Div,
        "mod" => dae::PureBuiltin::Mod,
        "rem" => dae::PureBuiltin::Rem,
        _ => return None,
    };
    match arguments.as_slice() {
        [lhs, rhs] => Some((builtin, [*lhs, *rhs])),
        _ => None,
    }
}

/// Define relations, conditions and roots, routing each quotient owner's
/// three through its replay token and finishing every token.
pub(super) fn define_conditions<'dae>(
    construction: &mut dae::DaeConstruction<'dae>,
    ctx: &Rebuild<'_>,
    expressions: &[dae::ExprId<'dae>],
    conditions: &[dae::ConditionId<'dae>],
    clocks: &[dae::ClockId<'dae>],
    mut owners: Vec<dae::QuotientReplayToken<'dae>>,
) -> Result<(), dae::DaeConstructionError> {
    let model = ctx.model;
    // Which owner, if any, each relation's indicator belongs to.
    let relation_owner: Vec<Option<usize>> = model
        .relations
        .iter()
        .map(|relation| {
            let expression = expressions.get(relation.expression.0 as usize)?;
            owners
                .iter()
                .position(|token| token.generated()[5].index() == expression.index())
        })
        .collect();
    let mut relations = Vec::with_capacity(model.relations.len());
    for (relation, owner) in model.relations.iter().zip(&relation_owner) {
        relations.push(match owner {
            Some(owner) => construction.replay_quotient_relation(&mut owners[*owner])?,
            None => {
                let at = ctx.provenance(relation.provenance)?;
                let expression = resolve(expressions, relation.expression.0, "expression", ctx)?;
                construction.conditions(|c| c.relation(expression, at))?
            }
        });
    }
    let root_owner = |root: &RbcRoot| {
        relation_owner
            .get(root.relation.0 as usize)
            .copied()
            .flatten()
    };
    let activation_owner: Vec<(u32, usize)> = model
        .roots
        .iter()
        .filter_map(|root| root_owner(root).map(|owner| (root.activation.0, owner)))
        .collect();
    for (index, (condition, id)) in model.conditions.iter().zip(conditions).enumerate() {
        let owned = activation_owner
            .iter()
            .find(|(activation, _)| *activation as usize == index);
        if let Some((_, owner)) = owned {
            construction.replay_quotient_activation(&mut owners[*owner], *id)?;
            continue;
        }
        let at = ctx.provenance(condition.provenance)?;
        let input = condition_input(ctx, condition, &relations, conditions, expressions, clocks)?;
        construction.conditions(|c| c.define(*id, input, at))?;
    }
    for root in &model.roots {
        if let Some(owner) = root_owner(root) {
            construction.replay_quotient_root(&mut owners[owner])?;
            continue;
        }
        let at = ctx.provenance(root.provenance)?;
        let relation = resolve(&relations, root.relation.0, "relation", ctx)?;
        let activation = resolve(conditions, root.activation.0, "condition", ctx)?;
        construction.conditions(|c| c.root(relation, activation, at))?;
    }
    for token in owners {
        construction.finish_quotient_replay(token)?;
    }
    Ok(())
}
