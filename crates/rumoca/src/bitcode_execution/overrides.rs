//! Run-local configuration of checked saved programs; never re-lowers equations.
use anyhow::{Context, Result, ensure};
use rumoca_bitcode::schema::{ExprId, RbcExprNode, RbcModel, RbcRole, VariableId};
use rumoca_ir_solve::execution::NumericalProgram;
use std::collections::BTreeSet;

/// Apply run-local overrides to the **derived** program.
///
/// v2 serializes no numerical program, so an override is a run-local edit to
/// the program derived moments earlier and is never persisted. Resolution goes
/// `name -> VariableId -> slot` through `Storage::variable_id`: the v1 path
/// matched storage by name, which is the post-resolution identity rule's exact
/// prohibition.
pub(super) fn apply(
    model: &RbcModel,
    numerical: &mut NumericalProgram,
    parameters: &[String],
    initials: &[String],
) -> Result<()> {
    for (name, value) in parse(parameters)? {
        let variable = model
            .variables
            .iter()
            .find(|v| v.name == name)
            .with_context(|| format!("unknown parameter: {name}"))?;
        ensure!(
            variable.tunable,
            "parameter is structural or constant, not tunable: {name}"
        );
        check_frozen_dependencies(model, variable.id)?;
        let storage = numerical
            .storage
            .iter_mut()
            .find(|v| v.variable_id == Some(variable.id.0) && v.role == "parameter")
            .with_context(|| format!("parameter has no retained runtime storage: {name}"))?;
        storage.start = value;
    }
    for (name, value) in parse(initials)? {
        set_initial(numerical, &name, value)?;
    }
    Ok(())
}

fn check_frozen_dependencies(model: &RbcModel, changed: VariableId) -> Result<()> {
    for variable in &model.variables {
        if variable.id == changed {
            continue;
        }
        let binding_depends = variable.role == RbcRole::Parameter
            && (depends(model, variable.binding, changed)
                || variable
                    .contract
                    .as_ref()
                    .is_some_and(|c| c.binding_depends_on.contains(&changed)));
        ensure!(
            !binding_depends
                && !depends(model, variable.start, changed)
                && !depends(model, variable.nominal, changed),
            "override requires re-lowering dependent binding/start/nominal: {}",
            variable.name
        );
    }
    Ok(())
}

fn depends(model: &RbcModel, root: Option<ExprId>, changed: VariableId) -> bool {
    let mut pending: Vec<_> = root.into_iter().collect();
    let mut visited = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }
        let node = &model.expressions[id.0 as usize].node;
        if let RbcExprNode::Coordinate { coordinate } = node
            && coordinate.variable() == Some(changed)
        {
            return true;
        }
        pending.extend(rumoca_bitcode::build::operands(node));
    }
    false
}

fn parse(pairs: &[String]) -> Result<Vec<(String, f64)>> {
    let mut seen = BTreeSet::new();
    pairs
        .iter()
        .map(|text| {
            let (name, value) = text
                .split_once('=')
                .context("override must be name=value")?;
            let value: f64 = value
                .parse()
                .context("override value must be a real number")?;
            ensure!(
                !name.is_empty() && value.is_finite(),
                "override needs a name and finite value"
            );
            ensure!(seen.insert(name), "duplicate override: {name}");
            Ok((name.to_owned(), value))
        })
        .collect()
}

fn set_initial(program: &mut NumericalProgram, name: &str, value: f64) -> Result<()> {
    let storage = program
        .storage
        .iter_mut()
        .find(|v| v.name == name && v.role == "state")
        .with_context(|| format!("initial override requires retained scalar state: {name}"))?;
    ensure!(
        !program
            .initialization
            .iter()
            .any(|row| row.target == Some(storage.index)),
        "initial override conflicts with an explicit initialization owner: {name}"
    );
    storage.start = value;
    Ok(())
}
