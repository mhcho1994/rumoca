//! `inline-constants`: replace a reference to a constant with its value.
//!
//! The frontend keeps a named constant named -- `Modelica.Constants.pi` is a
//! declared constant the equations reference (docs/design/minimal-frontend.md)
//! -- so replacing the name by the number is this pass's job, run when
//! optimization is asked for. It runs before `fold-constants`, which then
//! folds the arithmetic the inlined values expose.
//!
//! A structural parameter (`annotation(Evaluate=true)`, or `final`) is not
//! inlined: it is still a parameter, and a backend may treat it as one -- the
//! galec projection folds calls over it itself and records each fold.
//!
//! Only scalars whose binding is itself a literal are inlined: the
//! value is then exactly what the runtime would read, and nothing is
//! evaluated here. The declaration stays; a constant nothing references any
//! more is harmless, and its declared unit and provenance stay available.

use super::*;

pub(super) struct InlineConstants;

impl Pass for InlineConstants {
    fn name(&self) -> &'static str {
        "inline-constants"
    }
    fn description(&self) -> &'static str {
        "replace references to scalar constants with their literal values"
    }
    fn run(&self, model: &mut RbcModel) -> Result<usize, PassError> {
        let values: std::collections::HashMap<VariableId, RbcLiteral> = model
            .variables
            .iter()
            .filter(|variable| {
                variable.scalar_count == 1
                    && variable
                        .contract
                        .as_ref()
                        .is_some_and(|contract| contract.variability == RbcVariability::Constant)
            })
            .filter_map(|variable| {
                let binding = variable.binding?;
                match &model.expressions.get(binding.0 as usize)?.node {
                    RbcExprNode::Literal { value } => Some((variable.id, value.clone())),
                    _ => None,
                }
            })
            .collect();
        if values.is_empty() {
            return Ok(0);
        }
        let mut inlined = 0;
        for expression in &mut model.expressions {
            let RbcExprNode::Coordinate {
                coordinate: RbcCoordinate::Parameter { variable },
            } = &expression.node
            else {
                continue;
            };
            if let Some(value) = values.get(variable) {
                expression.node = RbcExprNode::Literal {
                    value: value.clone(),
                };
                inlined += 1;
            }
        }
        Ok(inlined)
    }
}
