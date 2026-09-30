//! Real-valued predefined functions inside structural parameter expressions.
//!
//! A conditional-component condition may reduce a parameter array, as in
//! `RC.OneElement`'s `solRad[nOrientations] if sum(ATransparent) > 0`
//! (MLS §4.4.5, §10.3.4). The reduction is decided only when every element
//! of the array is known: a modifier array must consist of literals, because
//! its elements were written in another scope (MLS §7.2.4), while a
//! declaration binding is folded in the declaring scope itself.

use super::{
    InstantiateScalarAdapter, ast, component_expr_for_structural_eval, is_predefined_call,
};
use crate::ast_scalar;

/// Evaluate a predefined Real function call, or `None` when not decidable.
pub(super) fn eval_real_builtin_call(
    adapter: &InstantiateScalarAdapter<'_>,
    function: &ast::ComponentReference,
    args: &[ast::Expression],
    depth: usize,
) -> Option<f64> {
    if !is_predefined_call(function, adapter.env.tree) {
        return None;
    }
    let real = |expr| ast_scalar::eval_real(expr, adapter, "", depth + 1);
    match (function.parts[0].ident.text.as_ref(), args) {
        ("sum", [array]) => Some(real_array(adapter, array, depth)?.iter().sum()),
        ("product", [array]) => Some(real_array(adapter, array, depth)?.iter().product()),
        ("min", [array]) => real_array(adapter, array, depth)?
            .into_iter()
            .reduce(f64::min),
        ("max", [array]) => real_array(adapter, array, depth)?
            .into_iter()
            .reduce(f64::max),
        ("min", [lhs, rhs]) => Some(real(lhs)?.min(real(rhs)?)),
        ("max", [lhs, rhs]) => Some(real(lhs)?.max(real(rhs)?)),
        ("abs", [value]) => Some(real(value)?.abs()),
        ("sqrt", [value]) => Some(real(value)?).filter(|v| *v >= 0.0).map(f64::sqrt),
        _ => None,
    }
}

/// Every element of a Real array expression, in row-major order.
fn real_array(
    adapter: &InstantiateScalarAdapter<'_>,
    expr: &ast::Expression,
    depth: usize,
) -> Option<Vec<f64>> {
    match expr {
        ast::Expression::Parenthesized { inner, .. } => real_array(adapter, inner, depth),
        ast::Expression::Array { elements, .. } => {
            let mut values = Vec::with_capacity(elements.len());
            for element in elements {
                if matches!(element, ast::Expression::Array { .. }) {
                    values.extend(real_array(adapter, element, depth)?);
                } else {
                    values.push(ast_scalar::eval_real(element, adapter, "", depth + 1)?);
                }
            }
            Some(values)
        }
        ast::Expression::ComponentReference(reference) => {
            let [part] = reference.parts.as_slice() else {
                return None;
            };
            if part.subs.is_some() {
                return None;
            }
            let name = part.ident.text.as_ref();
            if let Some(modification) = adapter
                .env
                .mod_env
                .get(&ast::QualifiedName::from_ident(name))
            {
                return literal_real_array(&modification.value);
            }
            let component = adapter.env.effective_components.get(name)?;
            real_array(
                adapter,
                component_expr_for_structural_eval(component)?,
                depth + 1,
            )
        }
        _ => None,
    }
}

/// Elements of an array constructor made only of numeric literals.
fn literal_real_array(expr: &ast::Expression) -> Option<Vec<f64>> {
    match expr {
        ast::Expression::Parenthesized { inner, .. } => literal_real_array(inner),
        ast::Expression::Array { elements, .. } => {
            let mut values = Vec::with_capacity(elements.len());
            for element in elements {
                match element {
                    ast::Expression::Array { .. } => values.extend(literal_real_array(element)?),
                    _ => values.push(literal_real(element)?),
                }
            }
            Some(values)
        }
        _ => None,
    }
}

fn literal_real(expr: &ast::Expression) -> Option<f64> {
    match expr {
        ast::Expression::Terminal {
            terminal_type: ast::TerminalType::UnsignedInteger | ast::TerminalType::UnsignedReal,
            token,
            ..
        } => token.text.parse().ok(),
        ast::Expression::Unary {
            op: rumoca_core::OpUnary::Minus,
            rhs,
            ..
        } => literal_real(rhs).map(|value| -value),
        ast::Expression::Parenthesized { inner, .. } => literal_real(inner),
        _ => None,
    }
}
