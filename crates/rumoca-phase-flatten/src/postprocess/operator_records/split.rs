//! Field-wise splitting of record equations (MLS §8.3.1, §8.3.4).
//!
//! The DAE record-equation analysis destructures `r = value` when `r` is a
//! record variable. Record equations of any other shape -- a conditional
//! residual from an if-equation whose branches equate records, or two
//! record-valued expressions -- are split here into one scalar equation per
//! record field, which is what MLS §8.3.1 defines them to mean.

use super::OperatorLowering;
use super::typing::Kind;
use crate::flat;
use rumoca_core::{DefId, Expression, OpBinary, Reference, Span};

pub(super) fn split_record_equations(
    lowering: &OperatorLowering<'_, '_>,
    equations: &mut Vec<flat::Equation>,
    families: &[flat::StructuredEquationFamily],
) -> bool {
    let covered = family_coverage(families);
    let mut appended = Vec::new();
    for (index, equation) in equations.iter_mut().enumerate() {
        if covered.iter().any(|range| range.contains(&index)) {
            continue;
        }
        if let Some(swapped) = lowering.orient_record_equation(&equation.residual) {
            equation.residual = swapped;
            continue;
        }
        let Some(mut fields) = lowering.split_record_equation(&equation.residual) else {
            continue;
        };
        let first = fields.remove(0);
        for residual in fields {
            let mut split = flat::Equation::new(residual, equation.span, equation.origin.clone());
            split.scalar_count = 1;
            appended.push(split);
        }
        equation.residual = first;
        equation.scalar_count = 1;
    }
    let changed = !appended.is_empty();
    equations.extend(appended);
    changed
}

/// Equation index ranges owned by structured families; their scalar views
/// mirror a template and are left as they are.
fn family_coverage(families: &[flat::StructuredEquationFamily]) -> Vec<std::ops::Range<usize>> {
    families
        .iter()
        .map(|family| {
            let start = family.first_equation_index;
            let len = family
                .domain
                .scalar_count()
                .map_or(usize::MAX - start, |points| {
                    points * family.equations_per_point
                });
            start..start.saturating_add(len)
        })
        .collect()
}

impl OperatorLowering<'_, '_> {
    /// `value = r` with `r` a record variable and `value` not: flip it so
    /// the DAE record analysis, which reads the record variable on the left,
    /// destructures it.
    fn orient_record_equation(&self, residual: &Expression) -> Option<Expression> {
        let Expression::Binary {
            op: OpBinary::Sub,
            lhs,
            rhs,
            span,
        } = residual
        else {
            return None;
        };
        if self.record_variable(lhs).is_some() || self.record_variable(rhs).is_none() {
            return None;
        }
        Some(Expression::Binary {
            op: OpBinary::Sub,
            lhs: rhs.clone(),
            rhs: lhs.clone(),
            span: *span,
        })
    }

    fn record_variable(&self, expr: &Expression) -> Option<DefId> {
        match (expr, self.kind(expr)) {
            (Expression::VarRef { .. }, Kind::Record(def_id)) => Some(def_id),
            _ => None,
        }
    }

    /// One residual per record field, or `None` when `residual` is not a
    /// record equation this pass must split (or cannot split exactly).
    fn split_record_equation(&self, residual: &Expression) -> Option<Vec<Expression>> {
        let record = match residual {
            Expression::Binary {
                op: OpBinary::Sub,
                lhs,
                rhs,
                ..
            } if self.record_variable(lhs).is_some()
                && matches!(
                    rhs.as_ref(),
                    Expression::FunctionCall { .. } | Expression::VarRef { .. }
                ) =>
            {
                return None;
            }
            Expression::Binary { .. } | Expression::If { .. } => self.residual_record(residual)?,
            _ => return None,
        };
        let fields = &self.flat.record_types.get(&record)?.fields;
        if fields.is_empty() || fields.iter().any(|field| !field.dims.is_empty()) {
            return None;
        }
        fields
            .iter()
            .map(|field| self.project_residual(residual, &field.name, field.def_id))
            .collect()
    }

    /// The record type every equation leaf of `residual` equates.
    fn residual_record(&self, residual: &Expression) -> Option<DefId> {
        match residual {
            Expression::Binary {
                op: OpBinary::Sub,
                lhs,
                rhs,
                ..
            } => match (self.kind(lhs), self.kind(rhs)) {
                (Kind::Record(a), Kind::Record(b)) if a == b => Some(a),
                _ => None,
            },
            Expression::If {
                branches,
                else_branch,
                ..
            } => {
                let record = self.residual_record(else_branch)?;
                branches
                    .iter()
                    .all(|(_, value)| self.residual_record(value) == Some(record))
                    .then_some(record)
            }
            _ => None,
        }
    }

    fn project_residual(
        &self,
        residual: &Expression,
        field: &str,
        field_def_id: DefId,
    ) -> Option<Expression> {
        match residual {
            Expression::Binary {
                op: OpBinary::Sub,
                lhs,
                rhs,
                span,
            } => Some(Expression::Binary {
                op: OpBinary::Sub,
                lhs: Box::new(self.project(lhs, field, field_def_id, *span)?),
                rhs: Box::new(self.project(rhs, field, field_def_id, *span)?),
                span: *span,
            }),
            Expression::If {
                branches,
                else_branch,
                span,
            } => Some(Expression::If {
                branches: branches
                    .iter()
                    .map(|(condition, value)| {
                        Some((
                            condition.clone(),
                            self.project_residual(value, field, field_def_id)?,
                        ))
                    })
                    .collect::<Option<_>>()?,
                else_branch: Box::new(self.project_residual(else_branch, field, field_def_id)?),
                span: *span,
            }),
            _ => None,
        }
    }

    /// The `field` component of the record-valued `expr`.
    fn project(
        &self,
        expr: &Expression,
        field: &str,
        field_def_id: DefId,
        fallback: Span,
    ) -> Option<Expression> {
        match expr {
            Expression::VarRef {
                name,
                subscripts,
                span,
            } if subscripts.is_empty() => self.field_variable(name, field, *span),
            Expression::FunctionCall { name, args, .. }
                if self.is_record_class(name)
                    && let Some(value) = self.constructor_field(expr, args, field) =>
            {
                Some(value)
            }
            Expression::If {
                branches,
                else_branch,
                span,
            } => Some(Expression::If {
                branches: branches
                    .iter()
                    .map(|(condition, value)| {
                        Some((
                            condition.clone(),
                            self.project(value, field, field_def_id, fallback)?,
                        ))
                    })
                    .collect::<Option<_>>()?,
                else_branch: Box::new(self.project(else_branch, field, field_def_id, fallback)?),
                span: *span,
            }),
            _ => Some(Expression::FieldAccess {
                base: Box::new(expr.clone()),
                field: field.to_string(),
                field_def_id,
                span: expr.span().unwrap_or(fallback),
            }),
        }
    }

    fn field_variable(&self, record: &Reference, field: &str, span: Span) -> Option<Expression> {
        let name = rumoca_core::VarName::new(format!("{}.{field}", record.as_str()));
        let variable = self.flat.variables.get(&name)?;
        if !variable.dims.is_empty() {
            return None;
        }
        let reference = match &variable.component_ref {
            Some(component_ref) => Reference::from_component_reference(component_ref.clone()),
            None => Reference::new(name.as_str()),
        };
        Some(Expression::VarRef {
            name: reference,
            subscripts: Vec::new(),
            span,
        })
    }

    fn is_record_class(&self, name: &Reference) -> bool {
        name.target_def_id()
            .and_then(|def_id| self.class_index.get(def_id))
            .is_some_and(|class| class.class_type == rumoca_core::ClassType::Record)
    }

    /// A positional constructor call supplies the field value directly.
    fn constructor_field(
        &self,
        call: &Expression,
        args: &[Expression],
        field: &str,
    ) -> Option<Expression> {
        let Kind::Record(record) = self.kind(call) else {
            return None;
        };
        let fields = &self.flat.record_types.get(&record)?.fields;
        if args.len() != fields.len() {
            return None;
        }
        let position = fields
            .iter()
            .position(|candidate| candidate.name == field)?;
        Some(args[position].clone())
    }
}
