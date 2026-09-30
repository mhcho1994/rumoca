//! Resolve-time annotation semantic checks.
//!
//! These checks cover annotation semantics that depend on resolved declaration
//! context, such as `Evaluate` only being legal on parameter/constant components.

use super::*;

pub(super) const WR006_EVALUATE_WITHOUT_EFFECT: &str = "WR006";

pub(super) fn check_annotation_restrictions(class: &ClassDef, diags: &mut Vec<Diagnostic>) {
    check_non_component_evaluate_annotations(
        &class.annotation,
        "class",
        class.name.text.as_ref(),
        diags,
    );

    for ext in &class.extends {
        check_non_component_evaluate_annotations(
            &ext.annotation,
            "extends clause",
            &ext.base_name.to_string(),
            diags,
        );
    }

    let in_function = class.class_type == ClassType::Function;
    for comp in class.components.values() {
        check_component_evaluate_annotations(comp, in_function, diags);
    }
}

fn check_non_component_evaluate_annotations(
    annotations: &[Expression],
    owner_kind: &str,
    owner_name: &str,
    diags: &mut Vec<Diagnostic>,
) {
    for expr in annotations {
        if !is_evaluate_annotation(expr) {
            continue;
        }
        let label = label_from_expression(
            expr,
            "check_annotation_restrictions/non_component_evaluate",
            format!("Evaluate has no effect on {} '{}'", owner_kind, owner_name),
        )
        .expect("annotation expression must carry a span");
        diags.push(Diagnostic::warning(
            WR006_EVALUATE_WITHOUT_EFFECT,
            format!(
                "annotation Evaluate on {} '{}' has no effect: only components declared with the parameter prefix are evaluated (MLS §18.3)",
                owner_kind, owner_name
            ),
            label,
        ));
    }
}

/// ANN-008 (MLS §18.3): `Evaluate` "only has effect for a component declared
/// with the prefix parameter". The MLS does not make the annotation illegal
/// elsewhere, and OpenModelica ignores it on other components (CDL's
/// `Real Dzero annotation(Evaluate=true)`, TRANSFORM/IBPSA
/// `y_reset_internal`, MSL's function-local `Integer m = size(x, 1)`).
/// Every non-parameter/non-constant component therefore warns (WR006) and the
/// annotation is dropped: instantiation only honours `Evaluate` on
/// parameter/constant components.
fn check_component_evaluate_annotations(
    comp: &ast::Component,
    in_function: bool,
    diags: &mut Vec<Diagnostic>,
) {
    if matches!(
        comp.variability,
        Variability::Parameter(_) | Variability::Constant(_)
    ) {
        return;
    }

    let owner = if in_function {
        "function local"
    } else {
        "component"
    };
    for expr in &comp.annotation {
        if !is_evaluate_annotation(expr) {
            continue;
        }
        let label = label_from_expression(
            expr,
            "check_annotation_restrictions/component_evaluate",
            format!("Evaluate has no effect on {owner} '{}'", comp.name),
        )
        .expect("annotation expression must carry a span");
        diags.push(Diagnostic::warning(
            WR006_EVALUATE_WITHOUT_EFFECT,
            format!(
                "annotation Evaluate has no effect on {owner} '{}': only components declared with the parameter prefix are evaluated (MLS §18.3)",
                comp.name
            ),
            label,
        ));
    }
}

fn is_evaluate_annotation(expr: &Expression) -> bool {
    match expr {
        Expression::NamedArgument { name, .. } => name.text.as_ref() == "Evaluate",
        Expression::Modification { target, .. } => target
            .parts
            .first()
            .is_some_and(|part| part.ident.text.as_ref() == "Evaluate"),
        _ => false,
    }
}
