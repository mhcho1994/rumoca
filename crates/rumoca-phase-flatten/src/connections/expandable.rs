//! MLS §9.1.3 expandable-connector member augmentation.
//!
//! Split out of `connections/mod.rs` under SPEC_0021: this is one coherent
//! elaboration -- grow the buses, then check what the growth implies -- and it
//! reads better beside itself than interleaved with connection-set building.

use indexmap::IndexMap;
use rustc_hash::FxHashMap;

use rumoca_ir_ast as ast;
use rumoca_ir_flat as flat;

use super::endpoint_subscripts::ConnectionEndpointIndex;
use super::equation_generation;
use super::{ConnectionVarIndex, find_sub_variables_indexed};
use crate::errors::FlattenError;

/// Split a trailing-subscripted endpoint into its base path and the number of
/// subscripts it selects with.
///
/// `pwm_rotor_cmd[1]` is `("pwm_rotor_cmd", 1)`. Flat keeps an array component
/// as one variable carrying `dims`, so a connect endpoint that indexes it does
/// not name any key in the model: the base does, and the subscript count says
/// how many leading dimensions the selected element drops.
pub(super) fn split_trailing_indices(path: &str) -> (&str, usize) {
    let mut base = path;
    let mut count = 0;
    while base.ends_with(']')
        && let Some(open) = base.rfind('[')
        && base[open + 1..base.len() - 1]
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch == ',' || ch == ' ')
    {
        count += base[open + 1..base.len() - 1].matches(',').count() + 1;
        base = &base[..open];
    }
    (base, count)
}

/// Mirror every flat variable under `source` onto `target`, returning whether
/// anything was added.
///
/// `source` may be one variable (a signal connector such as `RealInput`, whose
/// whole content is the variable itself) or a prefix with members under it.
/// Both are handled by the same walk, so a bus member added from a scalar and
/// one added from a structured connector reach Flat the same way.
fn mirror_connector_members(
    flat: &mut flat::Model,
    source: &str,
    target: &str,
    span: rumoca_core::Span,
) -> Result<bool, FlattenError> {
    let (source_base, selected) = split_trailing_indices(source);
    let source_name = rumoca_core::VarName::new(source);
    let base_name = rumoca_core::VarName::new(source_base);
    let mut work: Vec<(rumoca_core::VarName, String)> = Vec::new();
    if flat.variables.contains_key(&source_name) {
        work.push((source_name, String::new()));
    } else if flat.variables.contains_key(&base_name) {
        work.push((base_name, String::new()));
    } else {
        let prefix = format!("{source}.");
        for name in flat.variables.keys() {
            if let Some(suffix) = name.as_str().strip_prefix(&prefix) {
                work.push((name.clone(), format!(".{suffix}")));
            }
        }
    }
    if work.is_empty() {
        return Ok(false);
    }

    let mut added = false;
    for (origin, suffix) in work {
        let new_name = rumoca_core::VarName::new(format!("{target}{suffix}"));
        if flat.variables.contains_key(&new_name) {
            continue;
        }
        let Some(template) = flat.variables.get(&origin).cloned() else {
            continue;
        };
        let instance_id = flat.materialize_instance(flat::InstanceRelation {
            owner: None,
            declaration: None,
            indices: Box::new([]),
            kind: flat::InstanceKind::Materialized,
        });
        // Selecting `a[1]` from `Real a[4]` yields a scalar: the member takes
        // the element's shape, not the array's.
        let dims = template.dims.iter().skip(selected).copied().collect();
        flat.variables.insert(
            new_name.clone(),
            flat::Variable {
                instance_id,
                name: new_name,
                dims,
                // The member has no declaration of its own -- the connection
                // is what brings it into being -- so it carries the span of
                // that connection rather than a declaration's.
                source_span: span,
                component_ref: None,
                // An augmented member is a bus member, and nothing downstream
                // should mistake it for a declared one.
                from_expandable_connector: true,
                connected: true,
                // A bus member is a plain potential: causality belongs to the
                // signal connector at the other end, and copying it here would
                // make two inputs or two outputs meet in one connection set.
                causality: rumoca_core::Causality::Empty,
                binding: None,
                ..template
            },
        );
        added = true;
    }
    Ok(added)
}

/// Whether `path` names an expandable connector instance already in Flat.
fn is_expandable_instance(
    flat: &flat::Model,
    path: &str,
    prefix_children: &FxHashMap<String, Vec<rumoca_core::VarName>>,
    var_index: &ConnectionVarIndex,
) -> bool {
    find_sub_variables_indexed(path, prefix_children, var_index)
        .iter()
        .any(|name| {
            flat.variables
                .get(name)
                .is_some_and(|variable| variable.from_expandable_connector)
        })
}

/// Elaborate MLS §9.1.3 expandable-connector member augmentation.
///
/// A `connect` naming a member an expandable connector does not declare *adds*
/// that member, with the shape of whatever it was connected to. Nothing else
/// in Modelica grows a declaration after the fact, which is why this cannot be
/// a lookup fix: the member does not exist until a connection asks for it.
///
/// Two forms occur, and both are handled because real bus models use both
/// together:
///
/// * `connect(bus.newMember, peer)` — the member is mirrored from `peer`.
/// * `connect(busA, busB)` — the buses take the *union* of their members, so a
///   member added to one by the first form reaches the other. Buses chain
///   through intermediate components, so this runs to a fixed point.
///
/// Before this existed the whole case was refused (`EF020`), because
/// connecting only the intersection of declared members would silently drop
/// signals a model relies on.
pub(super) fn augment_expandable_connectors(
    connections: &[&ast::InstanceConnection],
    flat: &mut flat::Model,
    endpoint_index: &ConnectionEndpointIndex,
) -> Result<(), FlattenError> {
    // Each round re-reads the model, because the previous round grew it. The
    // bound is the connection count: a round that adds nothing returns, and no
    // round can add members for more connections than exist.
    for _ in 0..=connections.len() {
        let mut added = false;

        let index = ConnectionVarIndex::new(flat);
        let children = equation_generation::build_prefix_children(flat);
        let mut mirrors: Vec<(String, String, rumoca_core::Span)> = Vec::new();
        for conn in connections {
            let path_a = conn.a.to_flat_string();
            let path_b = conn.b.to_flat_string();
            let has_a = endpoint_is_present(flat, &path_a, &children, &index);
            let has_b = endpoint_is_present(flat, &path_b, &children, &index);
            match (has_a, has_b) {
                (false, true) if endpoint_index.needs_expandable_augmentation(&conn.a) => {
                    mirrors.push((path_b, path_a, conn.span));
                }
                (true, false) if endpoint_index.needs_expandable_augmentation(&conn.b) => {
                    mirrors.push((path_a, path_b, conn.span));
                }
                (true, true)
                    if is_expandable_instance(flat, &path_a, &children, &index)
                        && is_expandable_instance(flat, &path_b, &children, &index) =>
                {
                    mirrors.push((path_a.clone(), path_b.clone(), conn.span));
                    mirrors.push((path_b, path_a, conn.span));
                }
                _ => {}
            }
        }
        for (source, target, span) in mirrors {
            added |= mirror_connector_members(flat, &source, &target, span)?;
        }

        if !added {
            return check_augmented_member_sources(connections, flat, endpoint_index);
        }
    }
    check_augmented_member_sources(connections, flat, endpoint_index)
}

/// Every augmented member must be driven exactly once.
///
/// Augmentation gives a member the shape of what it was connected to, but the
/// resulting signal still obeys MLS §9.3: one source, any number of sinks.
///
/// The count is taken over the whole *connection set*, not over the connects
/// that happen to name the member. A bus member is routinely driven one hop
/// away -- `connect(control.pwm_1, pwm_rotor_cmd[1])` inside a component, with
/// the parent driving `control` -- and counting locally reported every such
/// model as undriven.
fn check_augmented_member_sources(
    connections: &[&ast::InstanceConnection],
    flat: &flat::Model,
    endpoint_index: &ConnectionEndpointIndex,
) -> Result<(), FlattenError> {
    // Union-find over endpoint paths: two paths share a set when a connect
    // names both, which is the same closure connection-set construction takes.
    let mut parent: IndexMap<String, String> = IndexMap::default();
    fn find(parent: &mut IndexMap<String, String>, item: &str) -> String {
        let mut current = item.to_string();
        loop {
            let Some(next) = parent.get(&current).cloned() else {
                parent.insert(current.clone(), current.clone());
                return current;
            };
            if next == current {
                return current;
            }
            current = next;
        }
    }
    let union = |parent: &mut IndexMap<String, String>, a: &str, b: &str| {
        let (ra, rb) = (find(parent, a), find(parent, b));
        if ra != rb {
            parent.insert(ra, rb);
        }
    };
    for conn in connections {
        let (path_a, path_b) = (conn.a.to_flat_string(), conn.b.to_flat_string());
        union(&mut parent, &path_a, &path_b);
        // Connecting two buses connects their members pairwise (MLS §9.1.3).
        // Without this the members stay in separate sets and a member driven
        // one hop away -- the parent drives the bus, the child connects the
        // member -- reads as undriven, which is how every hierarchical bus
        // model looked at first.
        for (left, right) in [(&path_a, &path_b), (&path_b, &path_a)] {
            let prefix = format!("{left}.");
            let members: Vec<String> = flat
                .variables
                .keys()
                .filter_map(|name| {
                    name.as_str()
                        .strip_prefix(&prefix)
                        .map(|suffix| suffix.to_string())
                })
                .collect();
            for suffix in members {
                union(
                    &mut parent,
                    &format!("{left}.{suffix}"),
                    &format!("{right}.{suffix}"),
                );
            }
        }
    }

    // Sources and undriven nested sinks per set.
    let mut outputs: IndexMap<String, usize> = IndexMap::default();
    let mut sinks: IndexMap<String, (usize, String, rumoca_core::Span)> = IndexMap::default();
    let mut augmented: IndexMap<String, (String, rumoca_core::Span)> = IndexMap::default();
    for conn in connections {
        for (endpoint, path) in [
            (&conn.a, conn.a.to_flat_string()),
            (&conn.b, conn.b.to_flat_string()),
        ] {
            let root = find(&mut parent, &path);
            let (base, _) = split_trailing_indices(&path);
            let variable = flat
                .variables
                .get(&rumoca_core::VarName::new(path.as_str()))
                .or_else(|| flat.variables.get(&rumoca_core::VarName::new(base)));
            match variable.map(|variable| &variable.causality) {
                Some(rumoca_core::Causality::Output(..)) => {
                    *outputs.entry(root.clone()).or_insert(0) += 1;
                }
                // A boundary input of the model being compiled is supplied by
                // whoever instantiates it; only a nested one must be driven
                // from within this model. Nesting is read off the structured
                // component reference rather than by splitting the flat name:
                // the declaration already knows how deep it sits.
                Some(rumoca_core::Causality::Input(..))
                    if variable.is_some_and(|variable| {
                        variable
                            .component_ref
                            .as_ref()
                            .is_some_and(|reference| reference.parts().len() > 1)
                    }) =>
                {
                    let entry = sinks
                        .entry(root.clone())
                        .or_insert((0, path.clone(), conn.span));
                    entry.0 += 1;
                }
                _ => {}
            }
            if endpoint_index.needs_expandable_augmentation(endpoint) {
                augmented.entry(root).or_insert((path.clone(), conn.span));
            }
        }
    }

    for (root, (member, span)) in augmented {
        let sources = outputs.get(&root).copied().unwrap_or(0);
        let nested_sinks = sinks.get(&root).map_or(0, |entry| entry.0);
        if sources > 1 || (sources == 0 && nested_sinks > 0) {
            return Err(FlattenError::expandable_member_source_count(
                member, sources, span,
            ));
        }
    }
    Ok(())
}

/// Whether a connect endpoint already names something in Flat.
pub(super) fn endpoint_is_present(
    flat: &flat::Model,
    path: &str,
    prefix_children: &FxHashMap<String, Vec<rumoca_core::VarName>>,
    var_index: &ConnectionVarIndex,
) -> bool {
    let (base, _) = split_trailing_indices(path);
    flat.variables
        .contains_key(&rumoca_core::VarName::new(path))
        || flat
            .variables
            .contains_key(&rumoca_core::VarName::new(base))
        || !find_sub_variables_indexed(path, prefix_children, var_index).is_empty()
        || !find_sub_variables_indexed(base, prefix_children, var_index).is_empty()
}
