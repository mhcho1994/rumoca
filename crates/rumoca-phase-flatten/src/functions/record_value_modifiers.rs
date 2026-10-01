//! Record-typed function values declared with field modifiers.
//!
//! MLS §12.4.4: a function output or protected record declared as
//! `output R r(y = {0, 1}, eta = zeros(n))` starts with those field values on
//! entry; the body may then overwrite some of them. The modifiers are the
//! declaration equation of the value, so they are lowered as the record
//! constructor call `R(y = {0, 1}, eta = zeros(n))` (named arguments, the
//! remaining fields keep their declared defaults per MLS §12.6). That default
//! also fixes the extents of any flexible (`:`) field of the record.

use super::*;
use std::sync::Arc;

/// Attribute modifiers that never name a record field.
const BUILTIN_ATTRIBUTES: &[&str] = &[
    "start",
    "fixed",
    "min",
    "max",
    "nominal",
    "unit",
    "displayUnit",
    "quantity",
    "stateSelect",
    "unbounded",
];

/// The record-constructor declaration equation implied by a record-typed
/// function value's field modifiers, or `None` when it has none (or carries a
/// nested modifier that has no single-expression form).
pub(super) fn record_modifier_default(
    class_index: &ast::ClassDefIndex<'_>,
    component: &ast::Component,
    type_def_id: Option<rumoca_core::DefId>,
) -> Option<ast::Expression> {
    let span = component.name_token.location.span();
    let mut args = Vec::new();
    for (field, value) in &component.modifications {
        if BUILTIN_ATTRIBUTES.contains(&field.as_str()) {
            continue;
        }
        if matches!(
            value,
            ast::Expression::ClassModification { .. }
                | ast::Expression::Modification { .. }
                | ast::Expression::Empty { .. }
        ) {
            return None;
        }
        let name = rumoca_core::Token {
            text: Arc::from(field.as_str()),
            ..component.name_token.clone()
        };
        args.push(ast::Expression::NamedArgument {
            name,
            value: Arc::new(value.clone()),
            span: value.span(),
        });
    }
    if args.is_empty() {
        return None;
    }
    let parts = constructor_name_parts(class_index, component, type_def_id?)?;
    Some(ast::Expression::FunctionCall {
        comp: ast::ComponentReference {
            local: false,
            parts,
            span,
            qualified_display_name: None,
        },
        args,
        is_partial_application: false,
        span,
    })
}

/// The record type's name as a call target, each segment carrying the class
/// it names (Flat lowering requires an exact identity on every segment).
fn constructor_name_parts(
    class_index: &ast::ClassDefIndex<'_>,
    component: &ast::Component,
    type_def_id: rumoca_core::DefId,
) -> Option<Vec<ast::ComponentRefPart>> {
    let tokens = &component.type_name.name;
    let mut qualified = String::new();
    let mut parts = Vec::with_capacity(tokens.len());
    for (index, token) in tokens.iter().enumerate() {
        if index > 0 {
            qualified.push('.');
        }
        qualified.push_str(&token.text);
        let def_id = if index + 1 == tokens.len() {
            type_def_id
        } else {
            class_index.get_by_qualified_name(&qualified)?.def_id?
        };
        parts.push(ast::ComponentRefPart {
            ident: token.clone(),
            subs: None,
            def_id: Some(def_id),
        });
    }
    Some(parts)
}
