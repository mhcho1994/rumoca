//! Array comprehensions iterating over the elements of an array.
//!
//! MLS §10.4.1: in `{e(A) for A in v}` the iterator takes the elements of the
//! vector `v`. The checked DAE owns comprehensions over rectangular Integer
//! ranges only, so an iterator over a one-dimensional model array `v` of
//! extent `n` is written as its index form `{e(v[A]) for A in 1:n}` — the same
//! elements in the same order (AixLib/IDEAS/Buildings `ReducedOrder.RC`
//! `dimension = sum({if A > 0 then 1 else 0 for A in AArray})`).
//!
//! The same late rewrite folds `size(e, k)` of a syntactically shaped `e`.

use super::*;
use rumoca_core::{
    ComprehensionIndex, Expression, ExpressionRewriter, FallibleExpressionRewriter,
    FallibleStatementRewriter, Literal, Subscript,
};
use std::collections::HashMap;

pub(crate) fn rewrite_array_domain_comprehensions(
    flat: &mut flat::Model,
) -> Result<(), FlattenError> {
    let vectors = flat
        .variables
        .iter()
        .filter_map(|(name, variable)| match variable.dims.as_slice() {
            [extent] if *extent >= 0 => Some((name.as_str().to_string(), *extent)),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let mut rewriter = ArrayDomainRewriter { vectors: &vectors };
    crate::functions::rewrite_model_expressions(flat, &mut rewriter)
}

struct ArrayDomainRewriter<'a> {
    vectors: &'a HashMap<String, i64>,
}

impl ArrayDomainRewriter<'_> {
    /// `(vector reference, extent)` when the index iterates a model vector.
    fn vector_domain(&self, range: &Expression) -> Option<(Expression, i64)> {
        let Expression::VarRef {
            name, subscripts, ..
        } = range
        else {
            return None;
        };
        if !subscripts.is_empty() {
            return None;
        }
        let extent = *self.vectors.get(name.as_str())?;
        Some((range.clone(), extent))
    }
}

impl FallibleExpressionRewriter for ArrayDomainRewriter<'_> {
    type Error = FlattenError;

    fn rewrite_expression(&mut self, expr: &Expression) -> Result<Expression, FlattenError> {
        let walked = self.walk_expression(expr)?;
        // Constant substitution can leave `size({...}, k)` over a literal;
        // the DAE needs that extent as a literal (MLS §10.3.1).
        if let Expression::BuiltinCall {
            function: rumoca_core::BuiltinFunction::Size,
            args,
            ..
        } = &walked
            && let Some(extent) = crate::ast_lower::static_size::fold_static_size(args)
        {
            return Ok(extent);
        }
        let Expression::ArrayComprehension {
            expr: body,
            indices,
            filter,
            span,
        } = walked
        else {
            return Ok(walked);
        };
        let mut body = *body;
        let mut filter = filter;
        let mut rewritten = Vec::with_capacity(indices.len());
        for index in indices {
            let Some((vector, extent)) = self.vector_domain(&index.range) else {
                rewritten.push(index);
                continue;
            };
            let mut element = ElementOf {
                index: &index.name,
                vector: &vector,
            };
            body = element.rewrite_expression(&body);
            filter = filter.map(|cond| Box::new(element.rewrite_expression(&cond)));
            let range_span = index.range.span().unwrap_or(span);
            rewritten.push(ComprehensionIndex {
                name: index.name,
                range: Expression::Range {
                    start: Box::new(integer(1, range_span)),
                    step: None,
                    end: Box::new(integer(extent, range_span)),
                    span: range_span,
                },
            });
        }
        Ok(Expression::ArrayComprehension {
            expr: Box::new(body),
            indices: rewritten,
            filter,
            span,
        })
    }
}

impl FallibleStatementRewriter for ArrayDomainRewriter<'_> {}

fn integer(value: i64, span: rumoca_core::Span) -> Expression {
    Expression::Literal {
        value: Literal::Integer(value),
        span,
    }
}

/// Replaces the iterator by the element of the vector it indexes.
struct ElementOf<'a> {
    index: &'a str,
    vector: &'a Expression,
}

impl ExpressionRewriter for ElementOf<'_> {
    fn rewrite_var_ref_expression(
        &mut self,
        name: &rumoca_core::Reference,
        subscripts: &[Subscript],
        span: rumoca_core::Span,
    ) -> Expression {
        if name.as_str() != self.index || !subscripts.is_empty() {
            return self.walk_var_ref_expression(name, subscripts, span);
        }
        let Expression::VarRef {
            name: vector,
            span: vector_span,
            ..
        } = self.vector
        else {
            return self.walk_var_ref_expression(name, subscripts, span);
        };
        let index = Expression::VarRef {
            name: name.clone(),
            subscripts: Vec::new(),
            span,
        };
        Expression::VarRef {
            name: vector.clone(),
            subscripts: vec![Subscript::Expr {
                expr: Box::new(index),
                span,
            }],
            span: *vector_span,
        }
    }
}
