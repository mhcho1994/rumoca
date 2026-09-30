//! MLS §8.3.4 parameter if-equation branch selection.
//!
//! An if-equation whose branch conditions are parameter expressions is
//! resolved at translation time: only the equations of the selected branch
//! become part of the flat model, and the branches that are not selected are
//! removed before any later rule (array bounds of MLS §10.5.1, equation type
//! compatibility of MLS §6.7) can apply to them. This is what lets
//! `Modelica.Fluid.Pipes.BaseClasses.PartialTwoPortFlow` write `lengths[2]`
//! in a branch that only exists when `n >= 2`.
//!
//! The late typecheck pass therefore asks this module which branch a
//! parameter if-equation selects, and walks only that branch. When the
//! conditions are not evaluable in the current instance the answer is `None`
//! and every branch is walked, which is the behaviour MLS §8.3.4 requires for
//! conditions that vary at simulation time.

use super::*;
use rumoca_ir_ast::Visitor;
use std::ops::ControlFlow;

impl TypeChecker {
    /// Index of the branch a parameter if-equation selects (MLS §8.3.4).
    ///
    /// Returns `Some(index)` into `cond_blocks`, `Some(cond_blocks.len())`
    /// when the `else` branch is selected, and `None` when the conditions are
    /// not translation-time evaluable in the current instance scope.
    pub(crate) fn select_parameter_if_branch(
        &self,
        cond_blocks: &[rumoca_ir_ast::EquationBlock],
    ) -> Option<usize> {
        self.select_parameter_branch(cond_blocks.iter().map(|block| &block.cond))
    }

    /// Branch an if-expression selects at translation time (MLS §3.6.5 with
    /// the §8.3.4 rule for parameter conditions): only the selected branch is
    /// evaluated, so array-bound rules apply to it alone. Same return
    /// convention as [`Self::select_parameter_if_branch`].
    pub(crate) fn select_parameter_if_expression_branch(
        &self,
        branches: &[(Expression, Expression)],
    ) -> Option<usize> {
        self.select_parameter_branch(branches.iter().map(|(cond, _)| cond))
    }

    fn select_parameter_branch<'a>(
        &self,
        conditions: impl ExactSizeIterator<Item = &'a Expression>,
    ) -> Option<usize> {
        let scope = self.current_instance_scope.as_ref()?.to_flat_string();
        let count = conditions.len();
        for (index, cond) in conditions.enumerate() {
            if !self.condition_is_translation_time(cond) {
                return None;
            }
            let selected =
                rumoca_eval_ast::eval::eval_boolean_with_scope(cond, &self.eval_ctx, &scope)?;
            if selected {
                return Some(index);
            }
        }
        Some(count)
    }

    /// MLS §3.8: an if-equation is resolved at translation time only when its
    /// condition is a parameter expression. A reference this instance declares
    /// with discrete or continuous variability rules that out, so the branch
    /// stays in the model and is checked. References the instance does not
    /// declare (enumeration literals, package constants reached through a type
    /// scope) do not by themselves disqualify the condition; whether they have
    /// a translation-time value is then the evaluator's answer.
    fn condition_is_translation_time(&self, condition: &Expression) -> bool {
        let mut collector = ConditionReferences::default();
        let _ = collector.visit_expression(condition);
        collector
            .references
            .iter()
            .all(|reference| !self.is_simulation_time_reference(reference))
    }

    fn is_simulation_time_reference(&self, reference: &rumoca_ir_ast::ComponentReference) -> bool {
        if reference.parts.len() == 1
            && reference.parts[0].ident.text.as_ref() == "time"
            && reference.parts[0].subs.iter().flatten().next().is_none()
        {
            return true;
        }
        match self.lookup_component_reference_variability(reference) {
            SemanticLookup::Found(
                rumoca_eval_ast::eval::VariabilityLevel::Constant
                | rumoca_eval_ast::eval::VariabilityLevel::Parameter,
            )
            | SemanticLookup::Missing => false,
            SemanticLookup::Found(_) | SemanticLookup::Ambiguous => true,
        }
    }
}

/// Collects the component-reference paths that appear in one expression.
#[derive(Default)]
struct ConditionReferences {
    references: Vec<rumoca_ir_ast::ComponentReference>,
}

impl Visitor for ConditionReferences {
    fn visit_component_reference_ctx(
        &mut self,
        cr: &rumoca_ir_ast::ComponentReference,
        _context: rumoca_ir_ast::ComponentReferenceContext,
    ) -> ControlFlow<()> {
        self.references.push(cr.clone());
        self.visit_component_reference(cr)
    }

    /// MLS §3.8.1/§3.8.3: `size(A, i)` and `ndims(A)` are parameter
    /// expressions whatever the variability of `A` (dimensions are fixed at
    /// translation time), so the array operand does not make the condition
    /// time-varying. The dimension index argument is still collected.
    fn visit_expression(&mut self, expr: &Expression) -> ControlFlow<()> {
        if let Expression::FunctionCall { comp, args, .. } = expr
            && comp.parts.len() == 1
            && matches!(comp.parts[0].ident.text.as_ref(), "size" | "ndims")
        {
            return self.visit_each(args.get(1..).unwrap_or_default(), Self::visit_expression);
        }
        rumoca_ir_ast::visitor::walk_expression_default(self, expr)
    }
}
