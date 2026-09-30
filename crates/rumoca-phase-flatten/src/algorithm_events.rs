//! `terminate` and `reinit` in an algorithm `when` statement.
//!
//! MLS §11.2.7 allows both in the body of a when-statement, exactly as in a
//! when-equation, but the model-algorithm owner downstream only schedules
//! assignments and assertions. Both are event actions, not assignments: they
//! read the algorithm's values at the event and take effect when it
//! completes, so where they sit among the block's assignments does not
//! matter. Each is moved to a when-equation with the same `when`/`elsewhen`
//! chain -- a branch keeps its priority because the chain keeps its order --
//! and an `if` around one becomes the when-equation's conditional
//! (TOOLBUG-067).

use rumoca_core::{Expression, Span, Statement, StatementBlock};
use rumoca_ir_flat as flat;

/// Move every `terminate`/`reinit` out of algorithm when-statements into
/// equivalent when-equation chains.
pub(crate) fn hoist_algorithm_event_actions(flat: &mut flat::Model) {
    let mut chains = Vec::new();
    for algorithm in &mut flat.algorithms {
        for statement in &mut algorithm.statements {
            if let Statement::When { blocks, span } = statement
                && let Some(chain) = hoist_from_when(blocks, *span)
            {
                chains.push(chain);
            }
        }
        algorithm
            .statements
            .retain(|statement| !is_empty_when(statement));
    }
    flat.algorithms
        .retain(|algorithm| !algorithm.statements.is_empty());
    flat.when_chains.extend(chains);
}

/// Take the event actions out of one when-statement; the chain that performs
/// them, or `None` when it has none.
fn hoist_from_when(blocks: &mut [StatementBlock], span: Span) -> Option<flat::WhenChain> {
    let mut branches = Vec::with_capacity(blocks.len());
    let mut found = false;
    for block in blocks.iter_mut() {
        let actions = take_actions(&mut block.stmts);
        found |= !actions.is_empty();
        let mut branch = flat::WhenBranch::new(block.cond.clone(), span);
        branch.equations = actions;
        branches.push(branch);
    }
    if !found {
        return None;
    }
    let mut branches = branches.into_iter();
    let mut chain = flat::WhenChain::new(branches.next()?, span);
    for branch in branches {
        chain.push_else_when(branch);
    }
    Some(chain)
}

/// Remove the event actions from a statement list, returning them as
/// when-equations in source order.
fn take_actions(statements: &mut Vec<Statement>) -> Vec<flat::WhenEquation> {
    let mut actions = Vec::new();
    statements.retain_mut(|statement| match action(statement) {
        Some(equation) => {
            actions.push(equation);
            false
        }
        None => {
            if let Some(conditional) = take_conditional(statement) {
                actions.push(conditional);
            }
            true
        }
    });
    actions
}

fn action(statement: &Statement) -> Option<flat::WhenEquation> {
    match statement {
        Statement::FunctionCall {
            comp,
            args,
            outputs,
            span,
        } if comp.as_str() == "terminate" && outputs.is_empty() && args.len() == 1 => Some(
            flat::WhenEquation::terminate(args[0].clone(), *span, "terminate in when-statement"),
        ),
        Statement::FunctionCall {
            comp,
            args,
            outputs,
            span,
        } if comp.as_str() == "reinit" && outputs.is_empty() && args.len() == 2 => {
            let Expression::VarRef {
                name, subscripts, ..
            } = &args[0]
            else {
                return None;
            };
            if !subscripts.is_empty() {
                return None;
            }
            let state = name.var_name().clone();
            let origin = format!("reinit({state})");
            Some(flat::WhenEquation::reinit(
                state,
                args[1].clone(),
                *span,
                origin,
            ))
        }
        Statement::Reinit {
            variable,
            value,
            span,
        } => {
            let state = variable.to_var_name();
            let origin = format!("reinit({state})");
            Some(flat::WhenEquation::reinit(
                state,
                value.clone(),
                *span,
                origin,
            ))
        }
        _ => None,
    }
}

/// For an `if` statement, move the event actions out of its branches into a
/// conditional when-equation; the statement keeps everything else.
fn take_conditional(statement: &mut Statement) -> Option<flat::WhenEquation> {
    let Statement::If {
        cond_blocks,
        else_block,
        span,
    } = statement
    else {
        return None;
    };
    let branches: Vec<(Expression, Vec<flat::WhenEquation>)> = cond_blocks
        .iter_mut()
        .map(|block| (block.cond.clone(), take_actions(&mut block.stmts)))
        .collect();
    let else_branch = else_block.as_mut().map(take_actions);
    let any = branches.iter().any(|(_, actions)| !actions.is_empty())
        || else_branch
            .as_ref()
            .is_some_and(|actions| !actions.is_empty());
    any.then(|| {
        flat::WhenEquation::conditional(branches, else_branch, *span, "if in when-statement")
    })
}

fn is_empty_when(statement: &Statement) -> bool {
    matches!(statement, Statement::When { blocks, .. }
        if blocks.iter().all(|block| block.stmts.is_empty()))
}
