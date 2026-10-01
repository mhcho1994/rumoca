//! Sibling-field references in the extents of decomposed record inputs.
//!
//! MLS §10.1 lets a record field's extent read an earlier field of the same
//! record (`Real y[:]; Real eta[size(y, 1)];`). Decomposing a record input `r`
//! into field inputs `r_y`, `r_eta` moves those extents into the function's
//! scope, where the bare sibling name `y` no longer exists; it is renamed to
//! the decomposed input `r_y`. For an array of records (`R r[n]`), the
//! decomposed input carries the outer axes first, so `size(y, k)` becomes
//! `size(r_y, k + rank(r))`.

use rumoca_core::{BuiltinFunction, Expression, ExpressionRewriter, Literal, Subscript};

pub(super) fn qualify_sibling_field_extents(
    param: &mut rumoca_core::FunctionParam,
    record_param: &str,
    fields: &[rumoca_core::FunctionParam],
    outer_rank: usize,
    field_extents: usize,
) {
    let mut rewriter = SiblingFieldRewriter {
        record_param,
        fields,
        outer_rank,
    };
    // Only the field's own extents are written in the record's scope; the
    // leading ones come from the record input's declaration.
    let own = param.shape_expr.len().min(field_extents);
    let first_own = param.shape_expr.len() - own;
    for subscript in &mut param.shape_expr[first_own..] {
        if let Subscript::Expr { expr, .. } = subscript {
            **expr = rewriter.rewrite_expression(expr);
        }
    }
}

struct SiblingFieldRewriter<'a> {
    record_param: &'a str,
    fields: &'a [rumoca_core::FunctionParam],
    outer_rank: usize,
}

impl SiblingFieldRewriter<'_> {
    fn sibling(&self, name: &str) -> Option<String> {
        self.fields
            .iter()
            .any(|field| field.name == name)
            .then(|| format!("{}_{}", self.record_param, name))
    }

    /// `size(y, k)` on a sibling: rename the array and shift a literal axis
    /// past the record array's own axes.
    fn rewrite_size(&mut self, args: &[Expression]) -> Option<Vec<Expression>> {
        let [
            Expression::VarRef {
                name,
                subscripts,
                span,
            },
            axis,
        ] = args
        else {
            return None;
        };
        if !subscripts.is_empty() {
            return None;
        }
        let renamed = self.sibling(name.as_str())?;
        let axis = match axis {
            Expression::Literal {
                value: Literal::Integer(k),
                span,
            } => Expression::Literal {
                value: Literal::Integer(k + self.outer_rank as i64),
                span: *span,
            },
            _ if self.outer_rank == 0 => axis.clone(),
            _ => return None,
        };
        let array = Expression::VarRef {
            name: renamed_reference(name, renamed),
            subscripts: Vec::new(),
            span: *span,
        };
        Some(vec![array, axis])
    }
}

impl ExpressionRewriter for SiblingFieldRewriter<'_> {
    fn rewrite_expression(&mut self, expr: &Expression) -> Expression {
        match expr {
            Expression::BuiltinCall {
                function: BuiltinFunction::Size,
                args,
                span,
            } => match self.rewrite_size(args) {
                Some(args) => Expression::BuiltinCall {
                    function: BuiltinFunction::Size,
                    args,
                    span: *span,
                },
                None => self.walk_expression(expr),
            },
            // A bare scalar sibling (`Real x[n]` with `Integer n`) is only
            // renamed for a scalar record input; for a record array it would
            // need an element subscript the declaration does not name.
            Expression::VarRef {
                name,
                subscripts,
                span,
            } if self.outer_rank == 0 && subscripts.is_empty() => {
                match self.sibling(name.as_str()) {
                    Some(renamed) => Expression::VarRef {
                        name: renamed_reference(name, renamed),
                        subscripts: Vec::new(),
                        span: *span,
                    },
                    None => expr.clone(),
                }
            }
            _ => self.walk_expression(expr),
        }
    }
}

/// The sibling reference under its decomposed input name, keeping the exact
/// field declaration it targets (the decomposed input is that declaration).
fn renamed_reference(name: &rumoca_core::Reference, renamed: String) -> rumoca_core::Reference {
    let Some(component_ref) = name.component_ref() else {
        return rumoca_core::Reference::generated(renamed);
    };
    let parts = component_ref
        .parts()
        .iter()
        .map(|part| rumoca_core::ComponentRefPart {
            ident: renamed.clone(),
            ..part.clone()
        })
        .collect();
    match component_ref.with_replaced_parts(parts) {
        Ok(component_ref) => name.with_rewritten_component_reference(renamed, component_ref),
        Err(_) => rumoca_core::Reference::generated(renamed),
    }
}
