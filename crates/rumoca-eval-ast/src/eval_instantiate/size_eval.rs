//! `size(A, k)` inside structural parameter expressions (MLS §10.3.1).
//!
//! A conditional-component condition or a dimension frequently reads the
//! extent of a declared array, as in `Medium.nX == 1` where
//! `nX = size(substanceNames, 1)`. The extent is the declared dimension when
//! it is an expression, and for a `:` dimension it is the extent of the
//! declaration's binding (MLS §10.1), which must then be an array
//! constructor whose extent can be read without evaluating any element.

use super::{
    IndexMap, IntegerEvalEnv, ast, component_expr_for_structural_eval, static_class_prefix,
    try_eval_integer_expr_with_depth_and_locals,
};
use rustc_hash::FxHashMap;

/// Evaluate `size(array_ref, dimension)` or `None` when it is not decidable.
pub(super) fn eval_integer_size_call(
    args: &[ast::Expression],
    env: IntegerEvalEnv<'_>,
    depth: usize,
    local_ints: Option<&FxHashMap<String, i64>>,
) -> Option<i64> {
    let [ast::Expression::ComponentReference(array_ref), dimension] = args else {
        return None;
    };
    if array_ref.parts.iter().any(|part| part.subs.is_some()) {
        return None;
    }
    let dimension = try_eval_integer_expr_with_depth_and_locals(
        dimension,
        env.mod_env,
        env.effective_components,
        env.tree,
        env.resolve_class_components,
        depth + 1,
        local_ints,
    )?;
    let axis = usize::try_from(dimension).ok()?.checked_sub(1)?;
    if let [part] = array_ref.parts.as_slice() {
        let name = part.ident.text.as_ref();
        let component = env.effective_components.get(name)?;
        // An active modification replaces the declaration binding (MLS §7.2).
        let binding = env
            .mod_env
            .get(&ast::QualifiedName::from_ident(name))
            .map(|value| &value.value)
            .or_else(|| component_expr_for_structural_eval(component));
        return component_extent(
            component,
            binding,
            axis,
            env,
            env.effective_components,
            depth,
        );
    }
    if let Some(extent) = record_field_extent(array_ref, axis, env, depth) {
        return Some(extent);
    }
    let class = static_class_prefix(array_ref, env)?;
    let scope = (env.resolve_class_components)(env.tree, class);
    let component = scope.get(array_ref.parts.last()?.ident.text.as_ref())?;
    if !matches!(component.variability, rumoca_core::Variability::Constant(_)) {
        return None;
    }
    let binding = component_expr_for_structural_eval(component);
    component_extent(component, binding, axis, env, &scope, depth)
}

fn component_extent(
    component: &ast::Component,
    binding: Option<&ast::Expression>,
    axis: usize,
    env: IntegerEvalEnv<'_>,
    scope: &IndexMap<String, ast::Component>,
    depth: usize,
) -> Option<i64> {
    match component.shape_expr.get(axis) {
        Some(ast::Subscript::Expression(expression)) => {
            try_eval_integer_expr_with_depth_and_locals(
                expression,
                env.mod_env,
                scope,
                env.tree,
                env.resolve_class_components,
                depth + 1,
                None,
            )
        }
        Some(ast::Subscript::Empty | ast::Subscript::Range { .. }) => {
            constructor_extent(binding?, axis)
        }
        None if component.shape_expr.is_empty() => component
            .shape
            .get(axis)
            .and_then(|d| i64::try_from(*d).ok()),
        None => None,
    }
}

/// Extent along `axis` of a nested `{...}` array constructor.
fn constructor_extent(expression: &ast::Expression, axis: usize) -> Option<i64> {
    match expression {
        ast::Expression::Parenthesized { inner, .. } => constructor_extent(inner, axis),
        ast::Expression::Array {
            elements,
            is_matrix: false,
            ..
        } => match axis {
            0 => i64::try_from(elements.len()).ok(),
            _ => constructor_extent(elements.first()?, axis - 1),
        },
        ast::Expression::Array {
            elements,
            is_matrix: true,
            ..
        } => matrix_extent(elements, axis),
        _ => None,
    }
}

/// Extent along `axis` of a `[a, b; c, d]` matrix constructor whose entries
/// are scalars (MLS §10.4.2). A row that concatenates arrays has an extent
/// that depends on its operands' shapes and is not answered here.
fn matrix_extent(elements: &[ast::Expression], axis: usize) -> Option<i64> {
    let is_row = |element: &ast::Expression| {
        matches!(
            element,
            ast::Expression::Array {
                is_matrix: true,
                ..
            }
        )
    };
    let scalar_row = |row: &[ast::Expression]| {
        row.iter()
            .all(|entry| !matches!(entry, ast::Expression::Array { .. }))
    };
    let (rows, columns) = if elements.iter().all(is_row) {
        let mut columns = None;
        for row in elements {
            let ast::Expression::Array { elements: row, .. } = row else {
                return None;
            };
            if !scalar_row(row) || columns.is_some_and(|width| width != row.len()) {
                return None;
            }
            columns = Some(row.len());
        }
        (elements.len(), columns?)
    } else if scalar_row(elements) {
        (1, elements.len())
    } else {
        return None;
    };
    match axis {
        0 => i64::try_from(rows).ok(),
        1 => i64::try_from(columns).ok(),
        _ => None,
    }
}

/// Extent of `rec.field` where `rec` is a record component of this scope: a
/// field dimension such as `material[nLay]` is the record's own `rec.nLay`
/// (MLS §7.2 record field lookup), and a `:` dimension is the extent of the
/// field's modifier or binding.
fn record_field_extent(
    array_ref: &ast::ComponentReference,
    axis: usize,
    env: IntegerEvalEnv<'_>,
    depth: usize,
) -> Option<i64> {
    let [record, field] = array_ref.parts.as_slice() else {
        return None;
    };
    let record_name = record.ident.text.as_ref();
    let record_component = env.effective_components.get(record_name)?;
    let record_class = env
        .tree
        .get_class_by_def_id(record_component.type_def_id?)?;
    if record_class.class_type != rumoca_core::ClassType::Record {
        return None;
    }
    let fields = (env.resolve_class_components)(env.tree, record_class);
    let field_name = field.ident.text.as_ref();
    let field_component = fields.get(field_name)?;
    match field_component.shape_expr.get(axis)? {
        ast::Subscript::Expression(expression) => {
            let qualified = prefix_references(expression, record)?;
            try_eval_integer_expr_with_depth_and_locals(
                &qualified,
                env.mod_env,
                env.effective_components,
                env.tree,
                env.resolve_class_components,
                depth + 1,
                None,
            )
        }
        ast::Subscript::Empty | ast::Subscript::Range { .. } => {
            let path = ast::QualifiedName::from_dotted(&format!("{record_name}.{field_name}"));
            let binding = env
                .mod_env
                .get(&path)
                .map(|value| &value.value)
                .or_else(|| component_expr_for_structural_eval(field_component))?;
            constructor_extent(binding, axis)
        }
    }
}

/// Qualify the field names a record dimension expression reads with the
/// record component that owns them (`nLay` -> `layers.nLay`).
fn prefix_references(
    expression: &ast::Expression,
    record: &ast::ComponentRefPart,
) -> Option<ast::Expression> {
    Some(match expression {
        ast::Expression::Terminal { .. } => expression.clone(),
        ast::Expression::ComponentReference(reference) => {
            let mut qualified = reference.clone();
            let mut owner = record.clone();
            owner.subs = None;
            owner.def_id = None;
            qualified.parts.insert(0, owner);
            for part in &mut qualified.parts {
                part.def_id = None;
            }
            qualified.qualified_display_name = None;
            ast::Expression::ComponentReference(qualified)
        }
        ast::Expression::Binary { op, lhs, rhs, span } => ast::Expression::Binary {
            op: op.clone(),
            lhs: std::sync::Arc::new(prefix_references(lhs, record)?),
            rhs: std::sync::Arc::new(prefix_references(rhs, record)?),
            span: *span,
        },
        ast::Expression::Unary { op, rhs, span } => ast::Expression::Unary {
            op: op.clone(),
            rhs: std::sync::Arc::new(prefix_references(rhs, record)?),
            span: *span,
        },
        ast::Expression::Parenthesized { inner, span } => ast::Expression::Parenthesized {
            inner: std::sync::Arc::new(prefix_references(inner, record)?),
            span: *span,
        },
        _ => return None,
    })
}
