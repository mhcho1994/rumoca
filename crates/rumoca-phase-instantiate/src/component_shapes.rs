use super::*;

pub(super) fn resolve_component_shape(
    tree: &ast::ClassTree,
    comp: &ast::Component,
    ctx: &InstantiateContext,
    overlay: &ast::InstanceOverlay,
    class_def: Option<&ast::ClassDef>,
    effective_components: &IndexMap<String, ast::Component>,
    imports: &[(String, String)],
) -> InstantiateResult<(Vec<i64>, Vec<ast::Subscript>)> {
    let type_dims =
        resolve_type_alias_dimensions(tree, class_def, ctx.mod_env(), effective_components)?;
    let (mut dims, dims_expr) = resolve_component_dimensions(
        comp,
        &type_dims,
        ctx.mod_env(),
        effective_components,
        tree,
        imports,
    );
    // MLS §10.7: the component loop already established this empty domain.
    // Preserve it even when symbolic extents are deferred for typecheck, and
    // retain dims_expr so that typecheck still verifies the final modifiers.
    if let Some(empty_dims) = overlay
        .array_parent_dims
        .get(&ctx.current_path().to_component_path())
        .filter(|dims| dims.contains(&0))
    {
        dims = empty_dims.clone();
        dims.extend_from_slice(&type_dims);
    }
    Ok((dims, dims_expr))
}

/// The exact shape of a nest of `{...}` array constructors whose leaves are
/// not themselves constructors, or `None` when the nest is ragged or empty.
/// Leaves are taken as scalars, so callers check the rank against the
/// declaration before trusting the result.
pub(super) fn array_constructor_shape(expression: &ast::Expression) -> Option<Vec<usize>> {
    let ast::Expression::Array {
        elements,
        kind: rumoca_core::ArrayConstructor::Array,
        ..
    } = expression
    else {
        return Some(Vec::new());
    };
    let (first, rest) = elements.split_first()?;
    let inner = array_constructor_shape(first)?;
    for element in rest {
        if array_constructor_shape(element)? != inner {
            return None;
        }
    }
    let mut shape = vec![elements.len()];
    shape.extend(inner);
    Some(shape)
}
