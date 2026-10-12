//! Conditional record values and equations retain their field-wise shape.
use super::*;

impl Resolver<'_, '_, '_> {
    pub(super) fn rewrite_conditional_equation(
        &mut self,
        model: &ModelScope,
        equation: &mut flat::Equation,
    ) -> Option<bool> {
        let Expression::If {
            branches,
            else_branch,
            span,
        } = equation.residual.clone()
        else {
            return None;
        };
        let branches: Vec<_> = branches
            .into_iter()
            .map(|(condition, value)| (self.rewrite_expression(&condition), value))
            .collect();
        let mut changed = false;
        let mut scalar_count = equation.scalar_count;
        let mut rewrite = |value: Expression| {
            let mut branch = flat::Equation {
                residual: value,
                ..equation.clone()
            };
            changed |= self.rewrite_equation(model, &mut branch, true);
            scalar_count = branch.scalar_count;
            branch.residual
        };
        let branches = branches
            .into_iter()
            .map(|(condition, value)| (condition, rewrite(value)))
            .collect();
        let else_branch = Box::new(rewrite(*else_branch));
        equation.residual = Expression::If {
            branches,
            else_branch,
            span,
        };
        equation.scalar_count = scalar_count;
        Some(changed)
    }

    pub(super) fn distribute_conditional_binary(
        &mut self,
        op: &OpBinary,
        lhs: &Expression,
        rhs: &Expression,
        span: Span,
    ) -> Option<Expression> {
        let (conditional, other, on_left) = if matches!(lhs, Expression::If { .. }) {
            (lhs, rhs, true)
        } else if matches!(rhs, Expression::If { .. }) {
            (rhs, lhs, false)
        } else {
            return None;
        };
        let Expression::If {
            branches,
            else_branch,
            ..
        } = conditional
        else {
            unreachable!()
        };
        let mut apply = |value: &Expression| {
            if on_left {
                self.resolve_binary(op, value.clone(), other.clone(), span)
            } else {
                self.resolve_binary(op, other.clone(), value.clone(), span)
            }
        };
        Some(Expression::If {
            branches: branches
                .iter()
                .map(|(condition, value)| (condition.clone(), apply(value)))
                .collect(),
            else_branch: Box::new(apply(else_branch)),
            span,
        })
    }
}
