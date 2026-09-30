use rumoca_ir_ast as ast;

/// Check if a component has annotation(Evaluate=true).
///
/// MLS §18.3: The Evaluate annotation indicates that a parameter should be
/// evaluated at compile time. This is used for structural parameters that
/// affect equation structure (e.g., if-equation branch selection).
///
/// Returns true if:
/// - The component has `annotation(Evaluate=true)`, or
/// - The component is declared `final` (implies compile-time evaluation)
///
/// MLS §18.3: `Evaluate` "only has effect for a component declared with the
/// prefix parameter" (or constant, which is always evaluable). Resolve accepts
/// the annotation on other components with the WR006 advisory; here it is
/// ignored, so a `Real Dzero annotation(Evaluate=true)` stays an ordinary
/// variable instead of being treated as a structural value.
pub(crate) fn has_evaluate_annotation(
    comp: &ast::Component,
    effective_variability: &rumoca_core::Variability,
) -> bool {
    if comp.is_final {
        return true;
    }
    if !matches!(
        effective_variability,
        rumoca_core::Variability::Parameter(_) | rumoca_core::Variability::Constant(_)
    ) {
        return false;
    }

    comp.annotation.iter().any(is_evaluate_true_annotation)
}

fn is_evaluate_true_annotation(anno_expr: &ast::Expression) -> bool {
    let (name_text, value) = match anno_expr {
        ast::Expression::NamedArgument { name, value, .. } => (name.text.as_ref(), value.as_ref()),
        ast::Expression::Modification { target, value, .. } => {
            let Some(first_part) = target.parts.first() else {
                return false;
            };
            (first_part.ident.text.as_ref(), value.as_ref())
        }
        _ => return false,
    };

    if name_text != "Evaluate" {
        return false;
    }
    matches!(
        value,
        ast::Expression::Terminal {
            terminal_type: ast::TerminalType::Bool,
            token,
            ..
        } if token.text.as_ref() == "true"
    )
}
