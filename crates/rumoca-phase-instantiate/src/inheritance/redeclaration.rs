use super::*;

pub(super) fn extract_modification_target(expr: &ast::Expression) -> Option<String> {
    match expr {
        ast::Expression::Modification { target, .. }
        | ast::Expression::ClassModification { target, .. } => {
            target.parts.first().map(|p| p.ident.text.to_string())
        }
        // For named arguments like `x = value`, extract the name
        ast::Expression::NamedArgument { name, .. } => Some(name.text.to_string()),
        _ => None,
    }
}

pub(super) fn extract_extend_modification_target(
    extend: &ast::Extend,
    expr: &ast::Expression,
) -> Option<String> {
    let target = match expr {
        ast::Expression::Modification { target, .. }
        | ast::Expression::ClassModification { target, .. } => target,
        ast::Expression::NamedArgument { name, .. } => return Some(name.text.to_string()),
        _ => return None,
    };
    extend_relative_component_target(extend, target)
}

pub(super) fn extend_relative_component_target(
    extend: &ast::Extend,
    target: &ast::ComponentReference,
) -> Option<String> {
    let target_parts = target
        .parts
        .iter()
        .map(|part| part.ident.text.as_ref())
        .collect::<Vec<_>>();
    let first = target_parts.first()?.to_string();
    let base_parts = extend
        .base_name
        .name
        .iter()
        .map(|token| token.text.to_string())
        .collect::<Vec<_>>();
    let base_part_refs = base_parts.iter().map(String::as_str).collect::<Vec<_>>();

    if let Some(next_idx) = target_parts
        .windows(base_part_refs.len())
        .position(|window| window == base_part_refs.as_slice())
        .map(|idx| idx + base_part_refs.len())
        && next_idx < target_parts.len()
    {
        return Some(target_parts[next_idx].to_string());
    }

    if let Some(base_leaf) = base_part_refs.last()
        && let Some(next_idx) = target_parts
            .iter()
            .position(|part| part == base_leaf)
            .map(|idx| idx + 1)
        && next_idx < target_parts.len()
    {
        return Some(target_parts[next_idx].to_string());
    }

    Some(first)
}

/// Extract the value from a modification expression.
///
/// MLS §7.2: Modifications can override component bindings.
/// Handles forms like:
/// - `extends Foo(n=2)` -> returns Literal(2)
/// - `extends Foo(final n=2)` -> returns Literal(2)
/// - `SomeType x(start=0)` -> returns Literal(0)
///
/// Returns None if no value can be extracted.
pub(super) fn extract_modification_value(expr: &ast::Expression) -> Option<ast::Expression> {
    let value = match expr {
        ast::Expression::Modification { value, .. } => Some(value),
        ast::Expression::NamedArgument { value, .. } => Some(value),
        _ => None,
    }?;

    // Don't return Empty values
    if matches!(value.as_ref(), ast::Expression::Empty { .. }) {
        None
    } else {
        Some(value.as_ref().clone())
    }
}

/// Try to extract a value modification for a component from an extends modification.
///
/// MLS §7.2: Value modifications in extends clauses override inherited bindings.
/// Returns Some((name, value)) if this modification applies to a component in the class.
pub(super) fn try_extract_value_modification(
    modification: &ast::ExtendModification,
    extend: &ast::Extend,
    class: &ast::ClassDef,
) -> Option<(String, ast::Expression, bool)> {
    // Only non-redeclare modifications can be value modifications
    if modification.redeclare {
        return None;
    }
    let target_name = extract_extend_modification_target(extend, &modification.expr)?;
    // Only apply if the base class has this component
    if !class.components.contains_key(&target_name) {
        return None;
    }
    let value = extract_modification_value(&modification.expr)?;
    Some((target_name, value, modification.final_))
}

/// Extract a value modification target/value from an extends modification without
/// constraining the target to immediate base-class local components.
///
/// This is used after inherited components are merged so modifications can apply
/// to transitively inherited members (MLS §7.2), e.g. `extends Mid(a(x=2))`
/// when `a` is declared in a grandparent class.
pub(super) fn try_extract_value_modification_any(
    modification: &ast::ExtendModification,
    extend: &ast::Extend,
) -> Option<(String, ast::Expression, bool)> {
    if modification.redeclare {
        return None;
    }
    let target_name = extract_extend_modification_target(extend, &modification.expr)?;
    let value = extract_modification_value(&modification.expr)?;
    // Nested class modifications are merged separately.
    if matches!(value, ast::Expression::ClassModification { .. }) {
        return None;
    }
    Some((target_name, value, modification.final_))
}

/// Extract the new type from a redeclaration modification.
///
/// Redeclarations can have forms like:
/// - `redeclare model M = NewM` -> returns "NewM"
/// - `redeclare Real x` -> returns "Real"
/// - `redeclare type T = Integer` -> returns "Integer"
/// - `redeclare TransientData.CellData cellData` -> returns "TransientData.CellData"
///
/// Returns None if the new type cannot be determined from the expression.
pub(super) fn extract_redeclare_type(expr: &ast::Expression) -> Option<String> {
    match expr {
        // Type assignment: `redeclare model M = NewM` or `redeclare type T = Integer`
        // Also handles: `redeclare TransientData.CellData cellData` where value is ClassModification
        ast::Expression::Modification { value, .. } => {
            // The value might be a component reference to the new type
            if let ast::Expression::ComponentReference(comp_ref) = value.as_ref() {
                return Some(comp_ref.to_string());
            }
            // Or it might be a class modification with the type as target
            // This handles: `Modification { target: cellData, value: ClassModification { target: TypeName, ... } }`
            if let ast::Expression::ClassModification { target, .. } = value.as_ref() {
                return Some(target.to_string());
            }
            None
        }
        // Class modification with type: might have type info in the modification
        ast::Expression::ClassModification { target, .. } => {
            // For class modifications like `redeclare Real x(...)`, the target itself is the type
            // This is a simplified extraction; full parsing would need access to component decl
            Some(target.to_string())
        }
        // Named argument: `redeclare type T = Integer` where value is the new type
        ast::Expression::NamedArgument { value, .. } => {
            if let ast::Expression::ComponentReference(comp_ref) = value.as_ref() {
                return Some(comp_ref.to_string());
            }
            None
        }
        _ => None,
    }
}

/// Redeclarations of an `extends` clause that target components the base
/// class inherited rather than declared (MLS §7.3).
///
/// `merge_class_content` only sees the base class's own declarations, so in
/// `model V3 extends Valve(redeclare Lin r)` where `Valve extends
/// Res(redeclare replaceable PV r)` and `Res` declares `r`, the outer
/// redeclaration would otherwise be lost and the intermediate (possibly
/// partial) `PV` instantiated. The innermost-first merge order of
/// `process_extends_with_cache` makes applying these after the recursive merge
/// the outermost-wins rule of MLS §7.2.
pub(super) fn collect_inherited_redeclarations(
    tree: &ast::ClassTree,
    base_class: &ast::ClassDef,
    inherited: &IndexMap<String, ast::Component>,
    extend: &ast::Extend,
    extend_span: Span,
) -> InstantiateResult<CollectedRedeclarations> {
    let mut collected = CollectedRedeclarations {
        types: IndexMap::default(),
        dims: IndexMap::default(),
        components: IndexSet::new(),
    };
    let mut validation_error: Option<Box<InstantiateError>> = None;
    walk_extend_modifications(extend, |modification| {
        if validation_error.is_some() {
            return;
        }
        let Some((target_name, _)) = redeclare_target_value(modification) else {
            return;
        };
        if base_class.components.contains_key(target_name) {
            return;
        }
        let Some(component) = inherited.get(target_name) else {
            return;
        };
        let new_type = extract_redeclare_type_qualified(&modification.expr, tree);
        let validated = redeclare_target_span(tree, target_name, modification, extend_span)
            .and_then(|span| {
                validate_redeclaration(tree, component, target_name, new_type.as_deref(), span)
            });
        if let Err(err) = validated {
            validation_error = Some(err);
            return;
        }
        collected.components.insert(target_name.to_string());
        if let Some(dims) = redeclared_dimensions(modification) {
            collected.dims.insert(target_name.to_string(), dims);
        }
        if let Some(new_type_name) = new_type {
            collected
                .types
                .insert(target_name.to_string(), new_type_name);
        }
    });
    match validation_error {
        Some(err) => Err(err),
        None => Ok(collected),
    }
}
