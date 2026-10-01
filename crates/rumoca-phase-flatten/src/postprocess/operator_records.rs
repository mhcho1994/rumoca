//! MLS §14 operator-record overloading in equations.
//!
//! `a + b`, `-a`, `k*a`, ... on operator records (`Complex`, and every
//! library operator record) denote calls of the operator functions the record
//! declares (MLS §14.5). Flat keeps them as plain `Binary`/`Unary` nodes until
//! this pass rewrites them into `FunctionCall`s of the selected overload, so
//! the call graph collects the operator functions and the DAE record-equation
//! analysis sees a record-valued call on the right-hand side.
//!
//! Record-valued equations that the DAE cannot destructure by itself (a
//! conditional residual from an if-equation, or neither side a record
//! variable) are split here into one equation per record field (MLS §8.3.1:
//! a record equation stands for one equation per primitive field).

use crate::{ast, flat};
use rumoca_core::{
    ComponentRefPart, ComponentReference, DefId, Expression, ExpressionRewriter, OpBinary, OpUnary,
    Reference, Span,
};

mod split;
mod typing;

use typing::{Kind, OperatorFunction};

/// Lower operator-record arithmetic in the model's equations and split
/// record equations the DAE record analysis cannot destructure.
///
/// Returns whether anything changed, so the caller re-collects functions.
pub(crate) fn lower_operator_record_equations(
    flat: &mut flat::Model,
    class_index: &ast::ClassDefIndex<'_>,
) -> bool {
    let mut equations = std::mem::take(&mut flat.equations);
    let mut initial_equations = std::mem::take(&mut flat.initial_equations);
    let mut changed = false;
    {
        let mut lowering = OperatorLowering::new(flat, class_index);
        for equation in equations.iter_mut().chain(initial_equations.iter_mut()) {
            let rewritten = lowering.rewrite_equation_form(&equation.residual);
            if rewritten != equation.residual {
                equation.residual = rewritten;
                changed = true;
            }
        }
    }
    let mut families = std::mem::take(&mut flat.structured_equations);
    let mut initial_families = std::mem::take(&mut flat.initial_structured_equations);
    {
        let lowering = OperatorLowering::new(flat, class_index);
        changed |= lowering.lower_family_templates(&mut families);
        changed |= lowering.lower_family_templates(&mut initial_families);
        changed |= split::split_record_equations(&lowering, &mut equations, &families);
        changed |=
            split::split_record_equations(&lowering, &mut initial_equations, &initial_families);
    }
    flat.equations = equations;
    flat.initial_equations = initial_equations;
    flat.structured_equations = families;
    flat.initial_structured_equations = initial_families;
    changed
}

pub(super) struct OperatorLowering<'a, 'tree> {
    flat: &'a flat::Model,
    class_index: &'a ast::ClassDefIndex<'tree>,
}

impl<'a, 'tree> OperatorLowering<'a, 'tree> {
    fn new(flat: &'a flat::Model, class_index: &'a ast::ClassDefIndex<'tree>) -> Self {
        Self { flat, class_index }
    }

    /// Rewrite an equation residual: a top-level `lhs - rhs` (also inside
    /// the branches of a conditional residual) is the equation itself, not
    /// a record subtraction, so only its operands are lowered.
    fn rewrite_equation_form(&mut self, residual: &Expression) -> Expression {
        match residual {
            Expression::Binary {
                op: OpBinary::Sub,
                lhs,
                rhs,
                span,
            } => Expression::Binary {
                op: OpBinary::Sub,
                lhs: Box::new(self.rewrite_expression(lhs)),
                rhs: Box::new(self.rewrite_expression(rhs)),
                span: *span,
            },
            Expression::If {
                branches,
                else_branch,
                span,
            } => Expression::If {
                branches: branches
                    .iter()
                    .map(|(condition, value)| {
                        (
                            self.rewrite_expression(condition),
                            self.rewrite_equation_form(value),
                        )
                    })
                    .collect(),
                else_branch: Box::new(self.rewrite_equation_form(else_branch)),
                span: *span,
            },
            other => self.rewrite_expression(other),
        }
    }

    fn lower_family_templates(&self, families: &mut [flat::StructuredEquationFamily]) -> bool {
        let mut lowering = OperatorLowering::new(self.flat, self.class_index);
        let mut changed = false;
        let templates = families
            .iter_mut()
            .filter_map(|family| family.template.as_mut());
        for expression in templates.flat_map(|template| template.body.iter_mut()) {
            let rewritten = lowering.rewrite_equation_form(expression);
            if rewritten != *expression {
                *expression = rewritten;
                changed = true;
            }
        }
        changed
    }

    fn lower_binary(
        &self,
        op: &OpBinary,
        lhs: &Expression,
        rhs: &Expression,
        span: Span,
    ) -> Option<Expression> {
        let operator = binary_operator_name(op)?;
        let (lhs_kind, rhs_kind) = (self.kind(lhs), self.kind(rhs));
        let records = [lhs_kind, rhs_kind]
            .into_iter()
            .filter_map(|kind| self.operator_record(kind))
            .collect::<Vec<_>>();
        if records.is_empty() {
            return None;
        }
        if let Some(distributed) = self.distribute_over_if(lhs, rhs, |lowering, lhs, rhs| {
            lowering
                .lower_binary(op, &lhs, &rhs, span)
                .unwrap_or_else(|| binary(op, lhs, rhs, span))
        }) {
            return Some(distributed);
        }
        let mut candidates: Vec<OperatorFunction> = Vec::new();
        let declared = records
            .iter()
            .flat_map(|record| self.operator_functions(*record, operator));
        for function in declared {
            if !candidates.iter().any(|c| c.def_id == function.def_id) {
                candidates.push(function);
            }
        }
        if let Some(function) = candidates.iter().find(|f| f.accepts(&[lhs_kind, rhs_kind])) {
            return Some(self.call(function.def_id, vec![lhs.clone(), rhs.clone()], span));
        }
        // MLS §14.5: one operand may reach the overload through a unique
        // constructor conversion of the operator record it must become.
        for function in &candidates {
            let converted = self.convert_operands(function, [lhs, rhs], [lhs_kind, rhs_kind], span);
            if let Some(args) = converted {
                return Some(self.call(function.def_id, args, span));
            }
        }
        None
    }

    /// A record-valued conditional operand is lowered branch by branch:
    /// `k*(if c then a else b)` becomes `if c then k*a else k*b`, so each
    /// operator call takes record variables the call ABI can destructure.
    fn distribute_over_if(
        &self,
        lhs: &Expression,
        rhs: &Expression,
        apply: impl Fn(&Self, Expression, Expression) -> Expression + Copy,
    ) -> Option<Expression> {
        let (conditional, on_left) = match (lhs, rhs) {
            (Expression::If { .. }, _) => (lhs, true),
            (_, Expression::If { .. }) => (rhs, false),
            _ => return None,
        };
        let Expression::If {
            branches,
            else_branch,
            span,
        } = conditional
        else {
            return None;
        };
        let combine = |value: &Expression| {
            if on_left {
                apply(self, value.clone(), rhs.clone())
            } else {
                apply(self, lhs.clone(), value.clone())
            }
        };
        Some(Expression::If {
            branches: branches
                .iter()
                .map(|(condition, value)| (condition.clone(), combine(value)))
                .collect(),
            else_branch: Box::new(combine(else_branch)),
            span: *span,
        })
    }

    fn convert_operands(
        &self,
        function: &OperatorFunction,
        operands: [&Expression; 2],
        kinds: [Kind; 2],
        span: Span,
    ) -> Option<Vec<Expression>> {
        if !function.accepts_arity(2) {
            return None;
        }
        let mut args = Vec::with_capacity(2);
        let mut converted = 0;
        for (index, (operand, kind)) in operands.into_iter().zip(kinds).enumerate() {
            let param = function.inputs[index].kind;
            if Kind::accepts(param, kind) {
                args.push(operand.clone());
                continue;
            }
            let Kind::Record(target) = param else {
                return None;
            };
            let constructor = self.constructor_for(target, kind)?;
            args.push(self.call(constructor, vec![operand.clone()], span));
            converted += 1;
        }
        (converted == 1).then_some(args)
    }

    fn constructor_for(&self, record: DefId, kind: Kind) -> Option<DefId> {
        let mut matches = self
            .operator_functions(record, "'constructor'")
            .into_iter()
            .filter(|function| function.accepts(&[kind]));
        let first = matches.next()?;
        matches.next().is_none().then_some(first.def_id)
    }

    fn lower_unary(&self, op: &OpUnary, rhs: &Expression, span: Span) -> Option<Expression> {
        let kind = self.kind(rhs);
        let record = self.operator_record(kind)?;
        if let Expression::If {
            branches,
            else_branch,
            span,
        } = rhs
        {
            let lower = |value: &Expression| {
                self.lower_unary(op, value, *span)
                    .unwrap_or_else(|| Expression::Unary {
                        op: op.clone(),
                        rhs: Box::new(value.clone()),
                        span: *span,
                    })
            };
            return Some(Expression::If {
                branches: branches
                    .iter()
                    .map(|(condition, value)| (condition.clone(), lower(value)))
                    .collect(),
                else_branch: Box::new(lower(else_branch)),
                span: *span,
            });
        }
        match op {
            OpUnary::Minus | OpUnary::DotMinus => {
                let function = self
                    .operator_functions(record, "'-'")
                    .into_iter()
                    .find(|function| function.accepts(&[kind]))?;
                Some(self.call(function.def_id, vec![rhs.clone()], span))
            }
            OpUnary::Plus | OpUnary::DotPlus => Some(rhs.clone()),
            _ => None,
        }
    }

    /// A call of the class `def_id`, referenced through its full ancestry so
    /// function collection resolves it by identity.
    fn call(&self, def_id: DefId, args: Vec<Expression>, span: Span) -> Expression {
        let name = self.function_reference(def_id, span);
        Expression::FunctionCall {
            name,
            args,
            is_constructor: false,
            span,
        }
    }

    fn function_reference(&self, def_id: DefId, span: Span) -> Reference {
        let parts = self
            .class_index
            .def_ancestry(def_id)
            .into_iter()
            .filter_map(|id| {
                Some(ComponentRefPart {
                    ident: self.class_index.local_name(id)?.to_string(),
                    span,
                    subs: Vec::new(),
                    def_id: id,
                })
            })
            .collect::<Vec<_>>();
        match ComponentReference::construct(false, span, parts) {
            Ok(component_ref) => Reference::from_component_reference(component_ref),
            Err(_) => Reference::new(self.class_index.qualified_name(def_id).unwrap_or_default()),
        }
    }
}

impl ExpressionRewriter for OperatorLowering<'_, '_> {
    fn rewrite_expression(&mut self, expr: &Expression) -> Expression {
        let walked = self.walk_expression(expr);
        let lowered = match &walked {
            Expression::Binary { op, lhs, rhs, span } => self.lower_binary(op, lhs, rhs, *span),
            Expression::Unary { op, rhs, span } => self.lower_unary(op, rhs, *span),
            _ => None,
        };
        lowered.unwrap_or(walked)
    }
}

fn binary(op: &OpBinary, lhs: Expression, rhs: Expression, span: Span) -> Expression {
    Expression::Binary {
        op: op.clone(),
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        span,
    }
}

fn binary_operator_name(op: &OpBinary) -> Option<&'static str> {
    Some(match op {
        OpBinary::Add | OpBinary::AddElem => "'+'",
        OpBinary::Sub | OpBinary::SubElem => "'-'",
        OpBinary::Mul | OpBinary::MulElem => "'*'",
        OpBinary::Div | OpBinary::DivElem => "'/'",
        OpBinary::Exp | OpBinary::ExpElem => "'^'",
        OpBinary::Eq => "'=='",
        OpBinary::Neq => "'<>'",
        OpBinary::Lt => "'<'",
        OpBinary::Le => "'<='",
        OpBinary::Gt => "'>'",
        OpBinary::Ge => "'>='",
        _ => return None,
    })
}
