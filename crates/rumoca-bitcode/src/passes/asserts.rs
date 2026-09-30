//! `fold-asserts`: drop an assertion that can never fail.
//!
//! `assert(c > 1, ...)` is an event whose guard is `not(c > 1)`. Once the
//! constants it reads are inlined and folded, the relation is the literal
//! `true` and the guard can never hold, so the check is removed. The
//! frontend no longer does this: it only reports an assertion proven false
//! at translation (EF030), because dropping a proven-true check is an
//! optimization (docs/design/minimal-frontend.md).
//!
//! Only `assert` events are removed, and only when the guard evaluates to
//! false from literals alone. `initial()` and anything read at run time make
//! the guard unknown and the event stays.

use super::*;

pub(super) struct FoldAsserts;

impl Pass for FoldAsserts {
    fn name(&self) -> &'static str {
        "fold-asserts"
    }
    fn description(&self) -> &'static str {
        "remove assertions whose condition folds to true"
    }
    fn run(&self, model: &mut RbcModel) -> Result<usize, PassError> {
        let before = model.events.len();
        let never = model
            .events
            .iter()
            .map(|event| {
                matches!(event.action, RbcAction::Assert { .. })
                    && condition_value(model, event.guard, 0) == Some(false)
            })
            .collect::<Vec<_>>();
        let mut index = 0;
        model.events.retain(|_| {
            let keep = !never[index];
            index += 1;
            keep
        });
        for (ordinal, event) in model.events.iter_mut().enumerate() {
            event.id = EventId(ordinal as u32);
        }
        Ok(before - model.events.len())
    }
}

/// The condition's value when literals alone decide it.
fn condition_value(model: &RbcModel, condition: ConditionId, depth: usize) -> Option<bool> {
    // Conditions reference earlier ones; the bound only guards a malformed
    // artifact, which validation rejects anyway.
    if depth > model.conditions.len() {
        return None;
    }
    match &model.conditions.get(condition.0 as usize)?.node {
        RbcConditionNode::Always => Some(true),
        RbcConditionNode::Not { operand } => {
            condition_value(model, *operand, depth + 1).map(|value| !value)
        }
        RbcConditionNode::And { lhs, rhs } => {
            let lhs = condition_value(model, *lhs, depth + 1);
            let rhs = condition_value(model, *rhs, depth + 1);
            match (lhs, rhs) {
                (Some(false), _) | (_, Some(false)) => Some(false),
                (Some(true), Some(true)) => Some(true),
                _ => None,
            }
        }
        RbcConditionNode::Or { lhs, rhs } => {
            let lhs = condition_value(model, *lhs, depth + 1);
            let rhs = condition_value(model, *rhs, depth + 1);
            match (lhs, rhs) {
                (Some(true), _) | (_, Some(true)) => Some(true),
                (Some(false), Some(false)) => Some(false),
                _ => None,
            }
        }
        RbcConditionNode::Relation { relation } => {
            let relation = model.relations.get(relation.0 as usize)?;
            boolean_literal(model, relation.expression)
        }
        RbcConditionNode::Discrete { expression } => boolean_literal(model, *expression),
        _ => None,
    }
}

/// A Boolean literal, or a comparison of two known operands evaluated with
/// `fold-constants`' rules (which leaves relation roots unfolded).
fn boolean_literal(model: &RbcModel, expression: ExprId) -> Option<bool> {
    let value = match &model.expressions.get(expression.0 as usize)?.node {
        RbcExprNode::Binary { op, lhs, rhs } => {
            super::constants::binary(*op, known(model, *lhs)?, known(model, *rhs)?)?
        }
        _ => known(model, expression)?,
    };
    match value {
        super::constants::Scalar::Boolean(value) => Some(value),
        _ => None,
    }
}

/// A literal, or a translation-frozen parameter's literal binding: a
/// constant, or a structural (`Evaluate`/`final`) parameter, which cannot
/// change without retranslation (MLS §18.3). Read here without rewriting the
/// model, because such a parameter is still a parameter to every other pass
/// and backend.
fn known(model: &RbcModel, expression: ExprId) -> Option<super::constants::Scalar> {
    if let Some(value) = super::constants::literal(model, expression) {
        return Some(value);
    }
    let RbcExprNode::Coordinate {
        coordinate: RbcCoordinate::Parameter { variable },
    } = &model.expressions.get(expression.0 as usize)?.node
    else {
        return None;
    };
    let variable = model.variables.get(variable.0 as usize)?;
    let contract = variable.contract.as_ref()?;
    if variable.scalar_count != 1
        || !(contract.variability == RbcVariability::Constant || contract.structural)
    {
        return None;
    }
    super::constants::literal(model, variable.binding?)
}
