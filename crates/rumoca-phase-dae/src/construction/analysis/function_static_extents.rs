//! Static array-constructor extents inside a value-proven function
//! specialization (MLS §10.3.3, §12.2).
//!
//! `fill(u, nout)`, `zeros(n)`, `linspace(a, b, N)` and `identity(n)` inside a
//! function body take their extents from the function's inputs. A checked DAE
//! array constructor needs a literal extent, which a specialization provides:
//! it proves the inputs that decide its output shape (`output Real y[nout]`).
//! Every extent argument the specialization settles is replaced by that
//! literal; an extent it does not settle is left as written and is rejected
//! by the constructor exactly as before.

use super::*;
use rumoca_core::{ExpressionRewriter, StatementRewriter};

pub(super) fn settle_static_extents(
    statements: &[rumoca_core::Statement],
    static_integers: &HashMap<VarName, i64>,
    shapes: &ShapeEnvironment,
) -> Vec<rumoca_core::Statement> {
    StaticExtents {
        static_integers,
        shapes,
    }
    .rewrite_statements(statements)
}

struct StaticExtents<'a> {
    static_integers: &'a HashMap<VarName, i64>,
    shapes: &'a ShapeEnvironment,
}

impl StaticExtents<'_> {
    fn settle(&mut self, argument: &Expression, span: Span) -> Expression {
        let rewritten = self.rewrite_expression(argument);
        match static_shape_integer_expression(&rewritten, self.static_integers, self.shapes) {
            Ok(Some(value)) => Expression::Literal {
                value: Literal::Integer(value),
                span,
            },
            _ => rewritten,
        }
    }
}

impl ExpressionRewriter for StaticExtents<'_> {
    fn walk_builtin_call_expression(
        &mut self,
        function: BuiltinFunction,
        args: &[Expression],
        span: Span,
    ) -> Expression {
        let first_extent = match function {
            BuiltinFunction::Zeros | BuiltinFunction::Ones | BuiltinFunction::Identity => 0,
            BuiltinFunction::Fill => 1,
            BuiltinFunction::Linspace => 2,
            _ => args.len(),
        };
        let args = args
            .iter()
            .enumerate()
            .map(|(index, argument)| {
                if index >= first_extent {
                    self.settle(argument, span)
                } else {
                    self.rewrite_expression(argument)
                }
            })
            .collect();
        Expression::BuiltinCall {
            function,
            args,
            span,
        }
    }
}

impl StatementRewriter for StaticExtents<'_> {}
