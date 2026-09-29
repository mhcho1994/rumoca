//! Projecting a lowered Modelica function body into the artifact.
//!
//! Separate from `export.rs` because it grows with each statement form the
//! schema learns to carry, and because the all-or-nothing rule below is a
//! decision worth stating once in a place that holds only it.
use crate::schema::*;
use rumoca_ir_dae as dae;

/// A function's Modelica body, or `ElidedModelica` when any statement in it
/// is a form this schema version cannot hold.
///
/// All-or-nothing on purpose. Emitting the statements that fit and dropping
/// the rest would produce a body that reads as complete and computes
/// something else; a consumer has no way to detect the gap. `ElidedModelica`
/// already means "a body exists and is not here", which is exactly true.
pub(super) fn export_function_body(function: dae::FunctionView<'_>) -> RbcFunctionBody {
    let mut statements = Vec::new();
    for statement in function.statements() {
        match statement {
            dae::FunctionStatementView::Assignment { definition } => {
                statements.push(RbcFunctionStatement::Assignment {
                    value: definition.target().ordinal(),
                    expression: ExprId(definition.rhs().index()),
                });
            }
            dae::FunctionStatementView::Assertion {
                condition, message, ..
            } => {
                statements.push(RbcFunctionStatement::Assertion {
                    condition: ExprId(condition.index()),
                    message: ExprId(message.index()),
                });
            }
            // Grouped assignment and folds land in a later step of
            // docs/design/carry-function-bodies.md.
            _ => return RbcFunctionBody::ElidedModelica,
        }
    }
    RbcFunctionBody::Modelica { statements }
}
