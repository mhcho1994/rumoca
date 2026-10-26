//! Replay ordered branches through the DAE's scoped construction capabilities.
use super::*;

pub(super) enum BranchFinish<'dae> {
    Body(dae::FunctionBranchFinish<dae::FunctionBody<'dae>>),
    Loop(dae::FunctionBranchFinish<dae::FunctionLoop<'dae>>),
}

pub(super) struct Conditional<'m, 'dae> {
    pub(super) ordinal: u32,
    pub(super) conditions: &'m [ExprId],
    pub(super) outputs: &'m [RbcFunctionDefinition],
    pub(super) provenance: RbcProvenance,
    pub(super) branches: Vec<dae::FunctionBranchOwner<'dae>>,
}

impl<'m, 'dae> Replay<'m, 'dae> {
    pub(super) fn apply_conditional_operation(
        &mut self,
        construction: &mut dae::DaeConstruction<'dae>,
        ctx: &Rebuild<'_>,
        stream: &mut Stream<'dae>,
        operation: Operation<'m>,
    ) -> Result<bool, dae::DaeConstructionError> {
        match operation {
            Operation::BeginConditional {
                ordinal,
                conditions,
                outputs,
                provenance,
            } => {
                if !conditions
                    .iter()
                    .all(|id| (id.0 as usize) < stream.built.len())
                {
                    return Ok(false);
                }
                self.conditionals.push(conditionals::Conditional {
                    ordinal,
                    conditions,
                    outputs,
                    provenance,
                    branches: Vec::new(),
                });
            }
            Operation::BeginBranch { provenance } => {
                self.begin_branch(construction, ctx, provenance)?
            }
            Operation::EndBranch => self.end_branch(construction, ctx)?,
            Operation::EndConditional => {
                if !self
                    .conditionals
                    .last()
                    .is_some_and(|frame| frame.outputs.is_empty())
                {
                    return Ok(false);
                }
                self.end_conditional(construction, ctx, stream)?;
            }
            Operation::RequireAssigned { .. } => return Ok(false),
            _ => return Err(ctx.unsupported("expected a conditional operation")),
        }
        Ok(true)
    }

    pub(super) fn begin_branch(
        &mut self,
        construction: &mut dae::DaeConstruction<'dae>,
        ctx: &Rebuild<'_>,
        provenance: RbcProvenance,
    ) -> Result<(), dae::DaeConstructionError> {
        let at = ctx.provenance(provenance)?;
        let (child, finish) = match self.capability.take() {
            Some(Capability::Body(parent)) => {
                let (child, finish) = construction.functions(|f| f.begin_branch(parent, at))?;
                (child, BranchFinish::Body(finish))
            }
            Some(Capability::Loop(parent)) => {
                let (child, finish) =
                    construction.functions(|f| f.begin_branch_loop(parent, at))?;
                (child, BranchFinish::Loop(finish))
            }
            None => return Err(ctx.unsupported("branch has no parent capability")),
        };
        self.branches.push((finish, self.depth));
        self.depth = 0;
        self.capability = Some(Capability::Body(child));
        Ok(())
    }

    pub(super) fn end_branch(
        &mut self,
        construction: &mut dae::DaeConstruction<'dae>,
        ctx: &Rebuild<'_>,
    ) -> Result<(), dae::DaeConstructionError> {
        let Some(Capability::Body(child)) = self.capability.take() else {
            return Err(ctx.unsupported("branch ends with an open loop"));
        };
        let (finish, depth) = self
            .branches
            .pop()
            .ok_or_else(|| ctx.unsupported("branch without opener"))?;
        let (parent, owner) = match finish {
            BranchFinish::Body(finish) => {
                let (parent, owner) = construction.functions(|f| f.finish_branch(finish, child))?;
                (Capability::Body(parent), owner)
            }
            BranchFinish::Loop(finish) => {
                let (parent, owner) =
                    construction.functions(|f| f.finish_branch_loop(finish, child))?;
                (Capability::Loop(parent), owner)
            }
        };
        self.capability = Some(parent);
        self.depth = depth;
        self.conditionals
            .last_mut()
            .ok_or_else(|| ctx.unsupported("branch without conditional"))?
            .branches
            .push(owner);
        Ok(())
    }

    pub(super) fn end_conditional(
        &mut self,
        construction: &mut dae::DaeConstruction<'dae>,
        ctx: &Rebuild<'_>,
        stream: &mut Stream<'dae>,
    ) -> Result<(), dae::DaeConstructionError> {
        let mut frame = self
            .conditionals
            .pop()
            .ok_or_else(|| ctx.unsupported("conditional without opener"))?;
        if frame.branches.len() != frame.conditions.len() + 1 {
            return Err(ctx.unsupported("conditional branch count mismatch"));
        }
        let fallback = frame
            .branches
            .pop()
            .ok_or_else(|| ctx.unsupported("conditional without fallback"))?;
        let conditions = frame
            .conditions
            .iter()
            .map(|id| resolve(&stream.built, id.0, "expression", ctx))
            .collect::<Result<Vec<_>, _>>()?;
        let branches = conditions.into_iter().zip(frame.branches).collect();
        let at = ctx.provenance(frame.provenance)?;
        let capability = self.capability_mut(ctx)?;
        let owner = construction.functions(|f| match capability {
            Capability::Body(parent) => f.finish_conditional(parent, branches, fallback, at),
            Capability::Loop(parent) => f.finish_conditional_loop(parent, branches, fallback, at),
        })?;
        if owner.ordinal() != frame.ordinal {
            return Err(ctx.unsupported("conditional owner ordinal mismatch"));
        }
        for output in frame.outputs {
            self.record_conditional_output(construction, ctx, stream, *output, frame.ordinal, at)?;
        }
        Ok(())
    }

    fn record_conditional_output(
        &self,
        construction: &mut dae::DaeConstruction<'dae>,
        ctx: &Rebuild<'_>,
        stream: &mut Stream<'dae>,
        output: RbcFunctionDefinition,
        conditional: u32,
        at: dae::DaeProvenance,
    ) -> Result<(), dae::DaeConstructionError> {
        let target = self.value(ctx, output.value)?;
        let body = self.body(ctx)?;
        let definition = construction.functions(|f| f.current_definition_id(body, target, at))?;
        let position = stream.built.len();
        let matches = ctx.model.expressions.get(position).is_some_and(|expression| {
            matches!(&expression.node,
                RbcExprNode::FunctionConditionalOutput { function, conditional: owner, value, definition: ordinal }
                if function.0 == self.index as u32 && *owner == conditional
                    && *value == output.value && *ordinal == definition.ordinal())
        });
        if output.expression.0 as usize != position || !matches {
            return Err(ctx.unsupported("conditional output does not match its issued definition"));
        }
        let issued = construction.functions(|f| f.current_definition_rhs(body, target, at))?;
        push_expected(ctx, stream, issued)
    }

    pub(super) fn conditional_transition(
        &mut self,
        construction: &mut dae::DaeConstruction<'dae>,
        ctx: &Rebuild<'_>,
        stream: &mut Stream<'dae>,
    ) -> Result<(), dae::DaeConstructionError> {
        while self.apply_next(construction, ctx, stream)? {}
        match self.operations.get(self.next).copied() {
            Some(Operation::EndConditional) => self.end_conditional(construction, ctx, stream)?,
            Some(Operation::RequireAssigned {
                value,
                expression,
                provenance,
            }) => {
                let target = self.value(ctx, value)?;
                let at = ctx.provenance(provenance)?;
                let capability = self.capability_mut(ctx)?;
                construction.functions(|f| match capability {
                    Capability::Body(body) => f.require_assigned(body, target, at),
                    Capability::Loop(body) => f.require_assigned_loop(body, target, at),
                })?;
                let Some(RbcExpr {
                    node: RbcExprNode::FunctionConditionalOutput { conditional, .. },
                    ..
                }) = ctx.model.expressions.get(stream.built.len())
                else {
                    return Err(ctx.unsupported("assigned read without conditional output"));
                };
                self.record_conditional_output(
                    construction,
                    ctx,
                    stream,
                    RbcFunctionDefinition { value, expression },
                    *conditional,
                    at,
                )?;
            }
            _ => return Err(ctx.unsupported("conditional output without corresponding statement")),
        }
        self.next += 1;
        Ok(())
    }
}

/// Earlier version-2 groups carry the same immutable guards on each join.
/// Verify that correlation before reconstructing the simultaneous assignment.
pub(super) fn check_legacy_group(
    ctx: &Rebuild<'_>,
    expressions: &[ExprId],
    correlation: &RbcFunctionConditional,
) -> Result<(), dae::DaeConstructionError> {
    for (index, expression) in expressions.iter().enumerate() {
        let Some(RbcExpr {
            node: RbcExprNode::Conditional { branches, fallback },
            ..
        }) = ctx.model.expressions.get(expression.0 as usize)
        else {
            return Err(ctx.unsupported("conditional group must carry its shared joins"));
        };
        let matches = branches.len() == correlation.conditions.len()
            && correlation.fallback.get(index) == Some(fallback)
            && branches.iter().enumerate().all(|(branch_index, branch)| {
                correlation.conditions.get(branch_index) == Some(&branch.condition)
                    && correlation
                        .branches
                        .get(branch_index)
                        .and_then(|values| values.get(index))
                        == Some(&branch.value)
            });
        if !matches {
            return Err(ctx.unsupported("conditional group correlation disagrees with its joins"));
        }
    }
    Ok(())
}

/// Check owner identities as well as arena positions after scoped construction.
pub(in crate::import) fn verify_conditional_outputs(
    dae: &dae::Dae,
    model: &RbcModel,
) -> Result<(), super::super::ImportError> {
    dae.inspect(|view| {
        for expression in &model.expressions {
            let RbcExprNode::FunctionConditionalOutput { function, conditional, value, definition } = expression.node else { continue; };
            let actual = view.expression_id(expression.id.0 as usize).and_then(|id| view.expression(id));
            let matches = actual.is_some_and(|actual| matches!(actual.operation(),
                dae::ExpressionOperation::FunctionConditionalOutput { conditional: owner, value: target, definition: issued }
                if owner.function().index() == function.0 && owner.ordinal() == conditional
                    && target.ordinal() == value && issued.id().ordinal() == definition));
            if !matches {
                return Err(super::super::ImportError::Unsupported(format!(
                    "conditional output {} disagrees with its checked owner", expression.id.0)));
            }
        }
        Ok(())
    })
}
