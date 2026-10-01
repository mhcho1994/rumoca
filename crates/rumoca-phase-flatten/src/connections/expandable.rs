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

/// Union-find over connect endpoint paths.
///
/// Two paths share a set when a connect names both, which is the same closure
/// connection-set construction takes.
#[derive(Default)]
struct EndpointSets {
    parent: IndexMap<String, String>,
}

impl EndpointSets {
    fn find(&mut self, item: &str) -> String {
        let mut current = item.to_string();
        loop {
            let Some(next) = self.parent.get(&current).cloned() else {
                self.parent.insert(current.clone(), current.clone());
                return current;
            };
            if next == current {
                return current;
            }
            current = next;
        }
    }

    fn union(&mut self, a: &str, b: &str) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent.insert(ra, rb);
        }
    }

    fn union_members(&mut self, flat: &flat::Model, left: &str, right: &str) {
        for suffix in member_suffixes(flat, left) {
            self.union(&format!("{left}.{suffix}"), &format!("{right}.{suffix}"));
        }
    }

    fn build(connections: &[&ast::InstanceConnection], flat: &flat::Model) -> Self {
        let mut sets = Self::default();
        for conn in connections {
            let (path_a, path_b) = (conn.a.to_flat_string(), conn.b.to_flat_string());
            sets.union(&path_a, &path_b);
            // Connecting two buses connects their members pairwise (MLS
            // §9.1.3). Without this a member driven one hop away -- the parent
            // drives the bus, the child connects the member -- sits in a set of
            // its own.
            sets.union_members(flat, &path_a, &path_b);
            sets.union_members(flat, &path_b, &path_a);
        }
        sets
    }
}

/// Suffixes of every flat variable under `path.` (the members of a bus or a
/// structured connector at that path).
fn member_suffixes(flat: &flat::Model, path: &str) -> Vec<String> {
    let prefix = format!("{path}.");
    flat.variables
        .keys()
        .filter_map(|name| name.as_str().strip_prefix(&prefix).map(str::to_string))
        .collect()
}

/// Number of component parts in a connect scope (`""` is the root, `a.b[2]`
/// is two parts). Dots inside subscripts do not occur in a flat prefix, but
/// brackets are skipped anyway so the count never depends on that.
fn scope_depth(scope: &str) -> usize {
    if scope.is_empty() {
        return 0;
    }
    let mut depth = 0usize;
    let mut parts = 1usize;
    for ch in scope.chars() {
        match ch {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            '.' if depth == 0 => parts += 1,
            _ => {}
        }
    }
    parts
}

/// What one connect endpoint contributes to the source count of its set.
struct EndpointRole<'flat> {
    variable: Option<&'flat flat::Variable>,
    /// MLS §9.3 outside connector: a connector of the component in whose
    /// scope the connect is written.
    outside: bool,
}

impl EndpointRole<'_> {
    fn is_output(&self) -> bool {
        self.variable.is_some_and(|variable| {
            matches!(variable.causality, rumoca_core::Causality::Output(..))
        })
    }

    fn is_inside_input(&self) -> bool {
        !self.outside
            && self.variable.is_some_and(|variable| {
                matches!(variable.causality, rumoca_core::Causality::Input(..))
            })
    }

    /// An input of the model being compiled is supplied by whoever
    /// instantiates it. Nesting is read off the structured component
    /// reference: the declaration already knows how deep it sits.
    fn is_top_level_input(&self) -> bool {
        self.variable.is_some_and(|variable| {
            matches!(variable.causality, rumoca_core::Causality::Input(..))
                && variable
                    .component_ref
                    .as_ref()
                    .is_some_and(|reference| reference.parts().len() == 1)
        })
    }
}

fn endpoint_role<'flat>(
    flat: &'flat flat::Model,
    conn: &ast::InstanceConnection,
    endpoint: &ast::QualifiedName,
) -> EndpointRole<'flat> {
    let path = endpoint.to_flat_string();
    let (base, _) = split_trailing_indices(&path);
    let variable = flat
        .variables
        .get(&rumoca_core::VarName::new(path.as_str()))
        .or_else(|| flat.variables.get(&rumoca_core::VarName::new(base)));
    EndpointRole {
        variable,
        outside: endpoint.parts.len() == scope_depth(&conn.scope) + 1,
    }
}

/// Per-set facts gathered over every connect endpoint.
#[derive(Default)]
struct SetFacts {
    /// Endpoint paths that can drive the set: outputs and top-level inputs.
    drivers: IndexMap<String, ()>,
    /// First augmented member of the set, for the diagnostic.
    augmented: Option<(String, rumoca_core::Span)>,
    /// Augmented member of a top-level expandable connector, if any.
    top_level_member: Option<String>,
}

impl SetFacts {
    fn note_augmented(&mut self, path: &str, top_level: bool, span: rumoca_core::Span) {
        self.augmented.get_or_insert((path.to_string(), span));
        if top_level && self.top_level_member.is_none() {
            self.top_level_member = Some(path.to_string());
        }
    }
}

/// An augmented member must not be driven by more than one source, and a
/// member of a top-level bus that nothing drives is an input of the model.
///
/// Augmentation gives a member the shape of what it was connected to, and the
/// resulting signal still obeys MLS §9.3: at most one source.
///
/// The count is taken over the whole *connection set*, not over the connects
/// that happen to name the member, and over distinct connectors rather than
/// connect occurrences. An output that is the outside connector of a connect
/// forwarding another source (`connect(inner.y, y)` in `y`'s own component)
/// is not a source of its own: counting it made every block that routes a
/// sub-block's output onto a bus look doubly driven.
///
/// A set with no source is not an error. MLS §9.1.3 says an input member
/// *should* appear as a non-input elsewhere in its augmentation set, and the
/// model check that follows reports an undriven unknown precisely. When the
/// member belongs to an expandable connector of the model itself, §9.1.3
/// gives it the causality of the input it feeds -- it is an input of the
/// model, supplied by whoever connects the bus -- which is what this marks.
fn check_augmented_member_sources(
    connections: &[&ast::InstanceConnection],
    flat: &mut flat::Model,
    endpoint_index: &ConnectionEndpointIndex,
) -> Result<(), FlattenError> {
    let mut sets = EndpointSets::build(connections, flat);
    let mut forwarded: IndexMap<String, ()> = IndexMap::default();
    let mut facts: IndexMap<String, SetFacts> = IndexMap::default();
    for conn in connections {
        let role_a = endpoint_role(flat, conn, &conn.a);
        let role_b = endpoint_role(flat, conn, &conn.b);
        for (endpoint, role, peer) in [(&conn.a, &role_a, &role_b), (&conn.b, &role_b, &role_a)] {
            let path = endpoint.to_flat_string();
            if role.outside && role.is_output() && !peer.is_inside_input() {
                forwarded.insert(path.clone(), ());
            }
            let entry = facts.entry(sets.find(&path)).or_default();
            if role.is_output() || role.is_top_level_input() {
                entry.drivers.insert(path.clone(), ());
            }
            if endpoint_index.needs_expandable_augmentation(endpoint) {
                entry.note_augmented(&path, endpoint.parts.len() == 2, conn.span);
            }
        }
    }

    for set in facts.into_values() {
        let Some((member, span)) = set.augmented else {
            continue;
        };
        let sources = set
            .drivers
            .keys()
            .filter(|path| !forwarded.contains_key(*path))
            .count();
        if sources > 1 {
            return Err(FlattenError::expandable_member_source_count(
                member, sources, span,
            ));
        }
        if sources == 0
            && let Some(top_level_member) = set.top_level_member
            && let Some(variable) = flat
                .variables
                .get_mut(&rumoca_core::VarName::new(top_level_member.as_str()))
            && variable.from_expandable_connector
        {
            variable.causality = rumoca_core::Causality::Input(Default::default());
            let member_name = rumoca_core::VarName::new(top_level_member.as_str());
            if let [bus, _, ..] = member_name.segments().as_slice() {
                let (bus, _) = split_trailing_indices(bus);
                flat.top_level_connectors.insert(bus.to_string());
            }
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
