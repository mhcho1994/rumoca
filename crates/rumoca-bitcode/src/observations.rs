//! Carrying what a DAE cannot hold across a rebuild.
//!
//! The in-compiler pass stage rebuilds its artifact into a DAE, and the
//! artifact `--emit-bitcode` writes is a fresh export of that DAE, so every
//! derived field -- `reads`, `binding_depends_on`, `effective_value`, types
//! -- describes the model as rewritten. A fresh export cannot contain the
//! trace points and the execution section an external pass added, because
//! the DAE has no place for them. [`carry_observations`] moves them from the
//! pass stage's artifact onto the fresh export.

use crate::schema::{ConnectionId, ConnectionSetId, RbcFile, RbcModel, VariableId};
use crate::validate::{ValidateOptions, recompute_summary, validate};

#[derive(Debug, thiserror::Error)]
#[error("cannot carry {0}")]
pub struct CarryError(String);

/// Move `from`'s trace points and execution section onto `into`, a fresh
/// export of the model `from` was rebuilt into.
///
/// When the variable and connection tables line up entry for entry, both
/// move verbatim. Otherwise a trace point is remapped by variable name and
/// by an equal connection or connection set (dropping a connection it can no
/// longer name), and an execution section, whose numerical program is laid
/// out over the old ids, is refused. The result is validated.
pub fn carry_observations(from: &RbcFile, into: &mut RbcFile) -> Result<(), CarryError> {
    if from.model.trace_points.is_empty() && from.execution.is_none() {
        return Ok(());
    }
    if tables_line_up(&from.model, &into.model) {
        into.model.trace_points = from.model.trace_points.clone();
        into.execution = from.execution.clone();
    } else {
        if from.execution.is_some() {
            return Err(CarryError(
                "the execution section: the rebuild renumbered the variables it is laid out over"
                    .into(),
            ));
        }
        into.model.trace_points = remapped_trace_points(&from.model, &into.model)?;
    }
    recompute_summary(&mut into.model);
    validate(
        &into.model,
        &ValidateOptions {
            reject_unsupported: false,
        },
    )
    .map_err(|errors| {
        CarryError(format!(
            "trace points: {}",
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        ))
    })
}

fn tables_line_up(from: &RbcModel, into: &RbcModel) -> bool {
    from.variables.len() == into.variables.len()
        && from
            .variables
            .iter()
            .zip(&into.variables)
            .all(|(a, b)| a.name == b.name)
        && same_json(&from.connections, &into.connections)
        && same_json(&from.connection_sets, &into.connection_sets)
}

fn same_json<T: serde::Serialize>(a: &T, b: &T) -> bool {
    serde_json::to_value(a).ok() == serde_json::to_value(b).ok()
}

fn remapped_trace_points(
    from: &RbcModel,
    into: &RbcModel,
) -> Result<Vec<crate::schema::RbcTracePoint>, CarryError> {
    let variable = |id: VariableId| -> Result<VariableId, CarryError> {
        let name = &from.variables[id.0 as usize].name;
        into.variables
            .iter()
            .find(|candidate| &candidate.name == name)
            .map(|candidate| candidate.id)
            .ok_or_else(|| {
                CarryError(format!(
                    "a trace point on `{name}`: no such variable after the rebuild"
                ))
            })
    };
    let connection = |id: ConnectionId| {
        let old = serde_json::to_value(remap_free(&from.connections[id.0 as usize], from)).ok();
        into.connections
            .iter()
            .find(|candidate| serde_json::to_value(remap_free(candidate, into)).ok() == old)
            .map(|candidate| candidate.id)
    };
    let connection_set = |id: ConnectionSetId| {
        let old = &from.connection_sets[id.0 as usize].connectors;
        into.connection_sets
            .iter()
            .find(|candidate| &candidate.connectors == old)
            .map(|candidate| candidate.id)
    };
    from.trace_points
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let mut point = point.clone();
            point.id = crate::schema::TracePointId(index as u32);
            point.variable = variable(point.variable)?;
            point.connection = point.connection.and_then(connection);
            point.connection_set = point.connection_set.and_then(connection_set);
            Ok(point)
        })
        .collect()
}

/// A connection with its endpoints named rather than numbered, so two
/// tables that number variables differently can be compared.
fn remap_free(connection: &crate::schema::RbcConnection, model: &RbcModel) -> serde_json::Value {
    let mut value = serde_json::to_value(connection).unwrap_or_default();
    if let Some(object) = value.as_object_mut() {
        object.remove("id");
        for (side, id) in [("left", connection.left), ("right", connection.right)] {
            object.insert(
                side.into(),
                model
                    .variables
                    .get(id.0 as usize)
                    .map(|variable| variable.name.clone())
                    .into(),
            );
        }
    }
    value
}
