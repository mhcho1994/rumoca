//! Projecting a lowered Modelica function body into the artifact.
//!
//! Separate from `export.rs` because it grows with each statement form the
//! schema learns to carry, and because the all-or-nothing rule below is a
//! decision worth stating once in a place that holds only it.
use crate::schema::*;
use rumoca_ir_dae as dae;

/// Definitions of one transition group, projected in order.
fn definitions_of(values: dae::FunctionDefinitionValues<'_>) -> Vec<RbcFunctionDefinition> {
    values
        .iter()
        .map(|definition| RbcFunctionDefinition {
            value: definition.target().ordinal(),
            expression: ExprId(definition.rhs().index()),
        })
        .collect()
}

/// Every fold a body contains, in owner-local ordinal order.
///
/// Returns `None` when any fold's own statements contain a form this
/// version cannot hold, which elides the whole body for the same reason a
/// partial statement list would be worse than none.
pub(super) fn export_folds<'dae>(
    view: dae::DaeView<'dae>,
    function: dae::FunctionView<'dae>,
) -> Option<Vec<RbcFunctionFold>> {
    let mut folds = Vec::new();
    for index in 0..function.fold_count() {
        let id = function.fold_id(index)?;
        let fold = view.function_fold(id)?;
        folds.push(RbcFunctionFold {
            ordinal: id.ordinal(),
            domain: DomainId(fold.domain().index()),
            parent: fold.parent_ordinal(),
            targets: fold.targets().map(|value| value.ordinal()).collect(),
            iteration_locals: fold
                .iteration_locals()
                .map(|value| value.ordinal())
                .collect(),
            parameters: definitions_of(fold.parameter_values()),
            initial: definitions_of(fold.initial_values()),
            update: definitions_of(fold.update_values()),
            output: definitions_of(fold.output_values()),
        });
    }
    Some(folds)
}

/// Project one statement list, or `None` if it holds a form not carried.
fn statements_of<'dae>(
    view: dae::DaeView<'dae>,
    source: dae::FunctionStatements<'dae>,
) -> Option<Vec<RbcFunctionStatement>> {
    let mut statements = Vec::new();
    for statement in source {
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
            dae::FunctionStatementView::AssignmentGroup {
                definitions,
                conditional,
            } => {
                let values = definitions
                    .iter()
                    .map(|definition| definition.target().ordinal())
                    .collect();
                // Unconditional groups carry one expression per value;
                // conditional ones take their values from the branches, so
                // carrying both would state the same thing twice and let
                // the two disagree.
                let (conditional, expressions) = match conditional {
                    Some(correlation) => (
                        Some(RbcFunctionConditional {
                            conditions: correlation
                                .conditions()
                                .map(|id| ExprId(id.index()))
                                .collect(),
                            branches: (0..correlation.branch_count())
                                .filter_map(|ordinal| {
                                    correlation
                                        .branch(ordinal)
                                        .map(|values| values.map(|id| ExprId(id.index())).collect())
                                })
                                .collect(),
                            fallback: correlation
                                .fallback()
                                .map(|id| ExprId(id.index()))
                                .collect(),
                        }),
                        Vec::new(),
                    ),
                    None => (
                        None,
                        definitions
                            .iter()
                            .map(|definition| ExprId(definition.rhs().index()))
                            .collect(),
                    ),
                };
                statements.push(RbcFunctionStatement::AssignmentGroup {
                    values,
                    conditional,
                    expressions,
                });
            }
            dae::FunctionStatementView::For {
                fold,
                statements: body,
                ..
            } => {
                statements.push(RbcFunctionStatement::For {
                    fold: fold.ordinal(),
                    statements: statements_of(view, body)?,
                });
            }
        }
    }
    Some(statements)
}

/// A function's Modelica body and its folds, or `ElidedModelica` when any
/// statement is a form this schema version cannot hold.
///
/// All-or-nothing on purpose. Emitting the statements that fit and dropping
/// the rest would produce a body that reads as complete and computes
/// something else; a consumer has no way to detect the gap.
/// `ElidedModelica` already means "a body exists and is not here", which is
/// exactly true.
pub(super) fn export_function_body<'dae>(
    view: dae::DaeView<'dae>,
    function: dae::FunctionView<'dae>,
) -> (RbcFunctionBody, Vec<RbcFunctionFold>) {
    let Some(statements) = statements_of(view, function.statements()) else {
        return (RbcFunctionBody::ElidedModelica, Vec::new());
    };
    let Some(folds) = export_folds(view, function) else {
        return (RbcFunctionBody::ElidedModelica, Vec::new());
    };
    (RbcFunctionBody::Modelica { statements }, folds)
}

/// Project a reference to a value inside a function body.
///
/// Split from the main expression projection to keep `export.rs` inside the
/// SPEC_0021 file-size limit, and because these three belong with the body
/// projection they refer into rather than with equation expressions.
pub(super) fn function_node(
    operation: &dae::ExpressionOperation<'_>,
) -> Result<RbcExprNode, super::ExportError> {
    Ok(match operation {
        dae::ExpressionOperation::FunctionValue {
            value, definition, ..
        } => RbcExprNode::FunctionValue {
            function: FunctionId(value.function().index()),
            value: value.ordinal(),
            definition: definition.id().ordinal(),
        },
        dae::ExpressionOperation::FunctionFoldParameter {
            fold,
            carried,
            definition,
            ..
        } => RbcExprNode::FunctionFoldParameter {
            function: FunctionId(fold.function().index()),
            fold: fold.ordinal(),
            carried: *carried,
            definition: definition.id().ordinal(),
        },
        dae::ExpressionOperation::FunctionFoldOutput {
            fold,
            carried,
            definition,
            ..
        } => RbcExprNode::FunctionFoldOutput {
            function: FunctionId(fold.function().index()),
            fold: fold.ordinal(),
            carried: *carried,
            definition: definition.id().ordinal(),
        },
        _ => {
            return Err(super::ExportError::Projection(
                "function_node called with a non-function-body operation".into(),
            ));
        }
    })
}
