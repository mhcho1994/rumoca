//! MLS 3.7 §11.2.3 `while` loops with a proven iteration bound, lowered to the
//! compact `for` owner.
//!
//! `while c loop S end while` runs `S` until `c` first evaluates false. Once
//! `c` is false the loop changes no value, so `c` stays false; hence, for any
//! `B` at least the number of iterations the loop can run,
//!
//! ```text
//! for w in 1:B loop if c then S end if; end for;
//! ```
//!
//! computes the same values. Its extra evaluations of `c` all happen in the
//! state the `while` loop's final (false) test already evaluated `c` in, so
//! they raise no error the original does not.
//!
//! The bound is proven from a counter `k`:
//!
//! * a top-level conjunct of `c` is `k < N`, `k <= N`, `N > k`, or `N >= k`,
//!   with `N` settled at translation (a literal, a shape extent, an evaluable
//!   parameter) and not written by `S`;
//! * `S` has no `break` or `return`, writes `k` once at top level as
//!   `k := k + d` with a positive Integer literal `d`, writes `k` after that
//!   statement nowhere, and before it only as `k := N` (the `isEqual` early
//!   exit), which cannot lower `k` while `c` holds;
//! * `k` is at least 1 whenever `c` is evaluated: either `c` reads an array
//!   element with `k` as a subscript (MLS §10.5 makes an index below 1 an
//!   error), or `k` enters the loop holding a literal of at least 1 (its last
//!   dominating assignment, or its declaration binding when nothing wrote it).
//!
//! Then every iteration raises `k` by at least 1 from at least 1 while `k`
//! stays at most `N`, so `B = N` iterations suffice. A loop without such a
//! proof is left as written and keeps its typed rejection.

use super::*;
use rumoca_core::{ForIndex, Literal, OpBinary, StatementBlock};

/// Literal Integer values each name is proven to hold at the current point.
pub(super) type EntryValues = HashMap<VarName, i64>;

/// The literal Integer defaults of a function's locals, which hold at entry.
pub(super) fn entry_values(function: &rumoca_core::Function) -> EntryValues {
    function
        .locals
        .iter()
        .filter_map(|local| match local.default.as_ref()? {
            Expression::Literal {
                value: Literal::Integer(value),
                ..
            } => Some((VarName::new(&local.name), *value)),
            _ => None,
        })
        .collect()
}

pub(super) fn bound_while_loops(
    statements: &[rumoca_core::Statement],
    shapes: &ShapeEnvironment,
    entry: &EntryValues,
) -> Vec<rumoca_core::Statement> {
    let mut known = entry.clone();
    let mut bounded = Vec::with_capacity(statements.len());
    for statement in statements {
        bounded.push(bound_statement(statement, shapes, &known));
        advance_known(&mut known, statement);
    }
    bounded
}

/// The values still proven after `statement` runs.
fn advance_known(known: &mut EntryValues, statement: &rumoca_core::Statement) {
    if let rumoca_core::Statement::Assignment {
        comp,
        value:
            Expression::Literal {
                value: Literal::Integer(value),
                ..
            },
        ..
    } = statement
        && comp.parts().iter().all(|part| part.subs.is_empty())
    {
        known.insert(
            rumoca_core::component_ref_to_base_reference(comp)
                .var_name()
                .clone(),
            *value,
        );
        return;
    }
    for name in statements_written_names(std::slice::from_ref(statement)) {
        known.remove(&name);
    }
}

/// The values a loop body may rely on in every iteration: none it writes.
fn loop_entry(known: &EntryValues, body: &[rumoca_core::Statement]) -> EntryValues {
    let written = statements_written_names(body);
    known
        .iter()
        .filter(|(name, _)| !written.contains(*name))
        .map(|(name, value)| (name.clone(), *value))
        .collect()
}

fn bound_statement(
    statement: &rumoca_core::Statement,
    shapes: &ShapeEnvironment,
    known: &EntryValues,
) -> rumoca_core::Statement {
    match statement {
        rumoca_core::Statement::While { block, span } => {
            let body = bound_while_loops(&block.stmts, shapes, &loop_entry(known, &block.stmts));
            let block = StatementBlock {
                cond: block.cond.clone(),
                stmts: body,
            };
            iteration_bound(&block, known, shapes).map_or_else(
                || rumoca_core::Statement::While {
                    block: block.clone(),
                    span: *span,
                },
                |bound| guarded_for(block.clone(), bound, *span),
            )
        }
        rumoca_core::Statement::For {
            indices,
            equations,
            span,
        } => rumoca_core::Statement::For {
            indices: indices.clone(),
            equations: bound_while_loops(equations, shapes, &loop_entry(known, equations)),
            span: *span,
        },
        rumoca_core::Statement::If {
            cond_blocks,
            else_block,
            span,
        } => rumoca_core::Statement::If {
            cond_blocks: cond_blocks
                .iter()
                .map(|block| StatementBlock {
                    cond: block.cond.clone(),
                    stmts: bound_while_loops(&block.stmts, shapes, known),
                })
                .collect(),
            else_block: else_block
                .as_ref()
                .map(|statements| bound_while_loops(statements, shapes, known)),
            span: *span,
        },
        _ => statement.clone(),
    }
}

fn guarded_for(block: StatementBlock, bound: i64, span: Span) -> rumoca_core::Statement {
    let literal = |value| Expression::Literal {
        value: Literal::Integer(value),
        span,
    };
    rumoca_core::Statement::For {
        indices: vec![ForIndex {
            ident: format!("__rumoca_while_{}", span.start.0),
            range: Expression::Range {
                start: Box::new(literal(1)),
                step: None,
                end: Box::new(literal(bound)),
                span,
            },
        }],
        equations: vec![rumoca_core::Statement::If {
            cond_blocks: vec![block],
            else_block: None,
            span,
        }],
        span,
    }
}

/// The proven iteration bound of one `while` loop (see the module note).
fn iteration_bound(
    block: &StatementBlock,
    known: &EntryValues,
    shapes: &ShapeEnvironment,
) -> Option<i64> {
    if statements_exit_early(&block.stmts) {
        return None;
    }
    let mut conjuncts = Vec::new();
    collect_conjuncts(&block.cond, &mut conjuncts);
    conjuncts.iter().find_map(|conjunct| {
        let (counter, limit) = counter_limit(conjunct)?;
        let bound = shapes.proven_extent(limit)?;
        let written = statements_written_names(&block.stmts);
        let limit_invariant = {
            let mut reads = Vec::new();
            limit.collect_var_refs(&mut reads);
            reads.iter().all(|name| !written.contains(name))
        };
        let starts_positive = known.get(counter).is_some_and(|start| *start >= 1);
        (bound >= 0
            && limit_invariant
            && advances_each_iteration(&block.stmts, counter, bound, shapes)
            && (indexes_with(&block.cond, counter) || starts_positive))
            .then_some(bound)
    })
}

fn collect_conjuncts<'a>(expression: &'a Expression, conjuncts: &mut Vec<&'a Expression>) {
    if let Expression::Binary {
        op: OpBinary::And,
        lhs,
        rhs,
        ..
    } = expression
    {
        collect_conjuncts(lhs, conjuncts);
        collect_conjuncts(rhs, conjuncts);
    } else {
        conjuncts.push(expression);
    }
}

/// `k < N`, `k <= N`, `N > k`, or `N >= k` for a plain reference `k`.
fn counter_limit(expression: &Expression) -> Option<(&VarName, &Expression)> {
    let Expression::Binary { op, lhs, rhs, .. } = expression else {
        return None;
    };
    let (counter, limit) = match op {
        OpBinary::Lt | OpBinary::Le => (lhs, rhs),
        OpBinary::Gt | OpBinary::Ge => (rhs, lhs),
        _ => return None,
    };
    plain_reference(counter).map(|name| (name, limit.as_ref()))
}

fn plain_reference(expression: &Expression) -> Option<&VarName> {
    match expression {
        Expression::VarRef {
            name, subscripts, ..
        } if subscripts.is_empty() => Some(name.var_name()),
        _ => None,
    }
}

/// The body raises `counter` by a positive literal once per iteration: one
/// top-level `k := k + d`, no write of `k` after it, and before it only
/// `k := N` (the limit itself), anywhere.
fn advances_each_iteration(
    statements: &[rumoca_core::Statement],
    counter: &VarName,
    limit: i64,
    shapes: &ShapeEnvironment,
) -> bool {
    let Some(increment) = statements.iter().position(|statement| {
        matches!(statement, rumoca_core::Statement::Assignment { comp, value, .. }
            if comp.parts().iter().all(|part| part.subs.is_empty())
                && rumoca_core::component_ref_to_base_reference(comp).var_name() == counter
                && is_positive_increment(value, counter))
    }) else {
        return false;
    };
    !statements_written_names(&statements[increment + 1..]).contains(counter)
        && writes_only_limit(&statements[..increment], counter, limit, shapes)
}

/// Every write of `counter` in `statements` assigns exactly `limit`.
fn writes_only_limit(
    statements: &[rumoca_core::Statement],
    counter: &VarName,
    limit: i64,
    shapes: &ShapeEnvironment,
) -> bool {
    statements.iter().all(|statement| match statement {
        rumoca_core::Statement::Assignment { comp, value, .. }
            if rumoca_core::component_ref_to_base_reference(comp).var_name() == counter =>
        {
            comp.parts().iter().all(|part| part.subs.is_empty())
                && shapes.proven_extent(value) == Some(limit)
        }
        rumoca_core::Statement::For { equations, .. } => {
            writes_only_limit(equations, counter, limit, shapes)
        }
        rumoca_core::Statement::While { block, .. } => {
            writes_only_limit(&block.stmts, counter, limit, shapes)
        }
        rumoca_core::Statement::If {
            cond_blocks,
            else_block,
            ..
        } => {
            cond_blocks
                .iter()
                .all(|block| writes_only_limit(&block.stmts, counter, limit, shapes))
                && else_block
                    .as_ref()
                    .is_none_or(|statements| writes_only_limit(statements, counter, limit, shapes))
        }
        other => !statements_written_names(std::slice::from_ref(other)).contains(counter),
    })
}

fn is_positive_increment(value: &Expression, counter: &VarName) -> bool {
    let Expression::Binary {
        op: OpBinary::Add,
        lhs,
        rhs,
        ..
    } = value
    else {
        return false;
    };
    let positive = |expression: &Expression| {
        matches!(
            expression,
            Expression::Literal {
                value: Literal::Integer(step),
                ..
            } if *step > 0
        )
    };
    (plain_reference(lhs) == Some(counter) && positive(rhs))
        || (plain_reference(rhs) == Some(counter) && positive(lhs))
}

/// Whether `expression` subscripts an array with exactly `counter` (MLS §10.5:
/// an index below 1 is an error, so every error-free evaluation has `k >= 1`).
fn indexes_with(expression: &Expression, counter: &VarName) -> bool {
    struct Search<'a> {
        counter: &'a VarName,
        found: bool,
    }
    impl rumoca_core::ExpressionVisitor for Search<'_> {
        fn visit_expression(&mut self, expression: &Expression) {
            let subscripts = match expression {
                Expression::VarRef { subscripts, .. } | Expression::Index { subscripts, .. } => {
                    subscripts.as_slice()
                }
                _ => &[],
            };
            if subscripts.iter().any(|subscript| {
                matches!(subscript, Subscript::Expr { expr, .. }
                    if plain_reference(expr) == Some(self.counter))
            }) {
                self.found = true;
            }
            self.walk_expression(expression);
        }
    }
    let mut search = Search {
        counter,
        found: false,
    };
    rumoca_core::ExpressionVisitor::visit_expression(&mut search, expression);
    search.found
}

fn statements_exit_early(statements: &[rumoca_core::Statement]) -> bool {
    statements.iter().any(|statement| match statement {
        rumoca_core::Statement::Break { .. } | rumoca_core::Statement::Return { .. } => true,
        rumoca_core::Statement::For { equations, .. } => statements_exit_early(equations),
        rumoca_core::Statement::While { block, .. } => statements_exit_early(&block.stmts),
        rumoca_core::Statement::If {
            cond_blocks,
            else_block,
            ..
        } => {
            cond_blocks
                .iter()
                .any(|block| statements_exit_early(&block.stmts))
                || else_block
                    .as_ref()
                    .is_some_and(|statements| statements_exit_early(statements))
        }
        _ => false,
    })
}

/// Every name an assignment, call output, or loop index in `statements`
/// writes.
fn statements_written_names(statements: &[rumoca_core::Statement]) -> HashSet<VarName> {
    let mut written = HashSet::new();
    collect_written_names(statements, &mut written);
    written
}

fn collect_written_names(statements: &[rumoca_core::Statement], written: &mut HashSet<VarName>) {
    for statement in statements {
        match statement {
            rumoca_core::Statement::Assignment { comp, .. } => {
                written.insert(
                    rumoca_core::component_ref_to_base_reference(comp)
                        .var_name()
                        .clone(),
                );
            }
            rumoca_core::Statement::FunctionCall { outputs, .. } => {
                written.extend(outputs.iter().flatten().map(|output| {
                    rumoca_core::component_ref_to_base_reference(output)
                        .var_name()
                        .clone()
                }));
            }
            rumoca_core::Statement::For {
                indices, equations, ..
            } => {
                written.extend(indices.iter().map(|index| VarName::new(&index.ident)));
                collect_written_names(equations, written);
            }
            rumoca_core::Statement::While { block, .. } => {
                collect_written_names(&block.stmts, written);
            }
            rumoca_core::Statement::If {
                cond_blocks,
                else_block,
                ..
            } => {
                for block in cond_blocks {
                    collect_written_names(&block.stmts, written);
                }
                if let Some(statements) = else_block {
                    collect_written_names(statements, written);
                }
            }
            _ => {}
        }
    }
}
