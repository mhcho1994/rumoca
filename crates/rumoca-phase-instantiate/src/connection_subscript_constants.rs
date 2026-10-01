//! Package constants in connect-statement subscripts (MLS §9.1, §10.5).
//!
//! `connect(X_in_internal[1:Medium.nXi], Xi_in_internal)` slices with an
//! extent read from the component's (replaceable) medium package. Connection
//! extraction resolves subscript names against this scope's Integer
//! parameters only, so each such name the scope's modification-aware
//! evaluator can decide is added to that table before extraction.

use crate::ast;
use rumoca_eval_ast::eval_instantiate::{InstantiateEvalCtx, try_eval_integer_expr};

pub(super) fn seed(
    equations: &[ast::Equation],
    tree: &ast::ClassTree,
    mod_env: &ast::ModificationEnvironment,
    effective_components: &rumoca_ir_ast::AstIndexMap<String, ast::Component>,
    scope: &ast::QualifiedName,
    int_params: &mut rustc_hash::FxHashMap<String, i64>,
) {
    let eval_ctx = &InstantiateEvalCtx {
        tree,
        mod_env,
        effective_components,
        resolve_class_components: crate::resolve_effective_components_for_eval,
    };
    let mut references = Vec::new();
    collect_connection_subscript_refs(equations, &mut references);
    let prefix = scope.to_component_path().to_flat_string();
    for reference in references {
        let dotted = reference
            .parts
            .iter()
            .map(|part| part.ident.text.as_ref())
            .collect::<Vec<_>>()
            .join(".");
        let key = if prefix.is_empty() {
            dotted
        } else {
            format!("{prefix}.{dotted}")
        };
        if int_params.contains_key(&key) {
            continue;
        }
        let expr = ast::Expression::ComponentReference(reference);
        if let Some(value) = try_eval_integer_expr(eval_ctx, &expr) {
            int_params.insert(key, value);
        }
    }
}

fn collect_connection_subscript_refs(
    equations: &[ast::Equation],
    out: &mut Vec<ast::ComponentReference>,
) {
    for equation in equations {
        match equation {
            ast::Equation::Connect { lhs, rhs } => {
                let subscripts = lhs
                    .parts
                    .iter()
                    .chain(&rhs.parts)
                    .flat_map(|part| part.subs.iter().flatten());
                let expressions = subscripts.filter_map(|subscript| match subscript {
                    ast::Subscript::Expression(expr) => Some(expr),
                    _ => None,
                });
                for expr in expressions {
                    collect_multi_part_refs(expr, out);
                }
            }
            ast::Equation::For { equations, .. } => {
                collect_connection_subscript_refs(equations, out);
            }
            ast::Equation::If {
                cond_blocks,
                else_block,
            } => {
                for block in cond_blocks {
                    collect_connection_subscript_refs(&block.eqs, out);
                }
                if let Some(block) = else_block {
                    collect_connection_subscript_refs(block, out);
                }
            }
            _ => {}
        }
    }
}

/// Multi-segment references (`Medium.nXi`); single names are this scope's
/// own parameters or loop indices, which extraction already knows.
fn collect_multi_part_refs(expr: &ast::Expression, out: &mut Vec<ast::ComponentReference>) {
    match expr {
        ast::Expression::ComponentReference(reference)
            if reference.parts.len() > 1 && reference.parts.iter().all(|p| p.subs.is_none()) =>
        {
            out.push(reference.clone());
        }
        ast::Expression::Range {
            start, step, end, ..
        } => {
            collect_multi_part_refs(start, out);
            if let Some(step) = step {
                collect_multi_part_refs(step, out);
            }
            collect_multi_part_refs(end, out);
        }
        ast::Expression::Binary { lhs, rhs, .. } => {
            collect_multi_part_refs(lhs, out);
            collect_multi_part_refs(rhs, out);
        }
        ast::Expression::Unary { rhs, .. } => collect_multi_part_refs(rhs, out),
        ast::Expression::Parenthesized { inner, .. } => collect_multi_part_refs(inner, out),
        _ => {}
    }
}
