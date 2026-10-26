use super::*;

/// Expand a simple equation with an array RHS into per-element equations.
///
/// For `x = {e1, e2, e3}` where x is a ast::ComponentReference, produces:
/// `x[1] = e1, x[2] = e2, x[3] = e3`
///
/// Handles nested arrays recursively for multi-dimensional cases.
/// Falls back to a single equation when the RHS is not an array.
pub(super) fn expand_array_assignment(
    lhs: &ast::Expression,
    rhs: &ast::Expression,
) -> Vec<SimpleEquation> {
    // A named aggregate is one authoritative tensor equation. Preserve it so
    // conditional branches such as `x = zeros(2)` and `x = {a, b}` have the
    // same owner cardinality; scalar rows derive from that owner downstream.
    if matches!(lhs, ast::Expression::ComponentReference(_)) {
        return vec![SimpleEquation {
            lhs: lhs.clone(),
            rhs: rhs.clone(),
        }];
    }
    let rhs_elements = match rhs {
        ast::Expression::Array { elements, .. } if !elements.is_empty() => elements,
        _ => {
            return vec![SimpleEquation {
                lhs: lhs.clone(),
                rhs: rhs.clone(),
            }];
        }
    };
    expand_array_lhs_elements(lhs, rhs, rhs_elements)
}

/// Expand array assignment given the RHS elements extracted from an Array expression.
fn expand_array_lhs_elements(
    lhs: &ast::Expression,
    rhs: &ast::Expression,
    rhs_elements: &[ast::Expression],
) -> Vec<SimpleEquation> {
    match lhs {
        ast::Expression::Array {
            elements: lhs_elements,
            ..
        } => lhs_elements
            .iter()
            .zip(rhs_elements.iter())
            .flat_map(|(l, r)| expand_array_assignment(l, r))
            .collect(),
        _ => vec![SimpleEquation {
            lhs: lhs.clone(),
            rhs: rhs.clone(),
        }],
    }
}

/// Expand a for-equation to simple equations.
pub(super) fn expand_for_to_simple(
    ctx: &Context,
    indices: &[ast::ForIndex],
    equations: &[ast::Equation],
    prefix: &ast::QualifiedName,
    span: rumoca_core::Span,
) -> Result<Vec<SimpleEquation>, FlattenError> {
    if indices.is_empty() {
        return expand_to_simple_equations(ctx, equations, prefix, span);
    }

    let first_index = &indices[0];
    let remaining_indices = &indices[1..];

    let index_values = expand_range_indices(ctx, &first_index.range, prefix, span)?;
    let index_name = IndexSubstitution::from_index(first_index)?;

    let mut result = Vec::new();
    for value in index_values {
        // Substitute index variable in all equations
        let substituted: Vec<ast::Equation> = equations
            .iter()
            .map(|eq| substitute_index_in_equation(eq, index_name, value))
            .collect();

        // Recursively expand remaining indices
        let expanded = expand_for_to_simple(ctx, remaining_indices, &substituted, prefix, span)?;
        result.extend(expanded);
    }

    Ok(result)
}
