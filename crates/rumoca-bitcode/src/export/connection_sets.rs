//! The connection graph's nodes, recovered from Flat's connect equations.
use super::*;

/// Flat's generated connection equations, paired with the DAE's by position.
///
/// Flow sums and potential equalities are paired with their DAE equations
/// the same way `export_connections` pairs its own: by position among the
/// generated connection equations, and only when the counts agree.
struct Pairing {
    equation_at: BTreeMap<usize, EquationId>,
    pairable: bool,
}

impl Pairing {
    fn new(flat: &flat::Model, equations: &[RbcEquation]) -> Self {
        let generated: Vec<EquationId> = equations
            .iter()
            .filter(|equation| {
                matches!(
                    equation.provenance.origin,
                    RbcOrigin::Generated {
                        generation: RbcGeneration::ConnectionEquation
                    }
                )
            })
            .map(|equation| equation.id)
            .collect();
        let mut flat_generated = 0usize;
        let mut equation_at: BTreeMap<usize, EquationId> = BTreeMap::new();
        for equation in flat.equations.iter() {
            if !matches!(
                equation.origin,
                flat::EquationOrigin::Connection { .. }
                    | flat::EquationOrigin::FlowSum { .. }
                    | flat::EquationOrigin::UnconnectedFlow { .. }
            ) {
                continue;
            }
            if let Some(id) = generated.get(flat_generated) {
                equation_at.insert(flat_generated, *id);
            }
            flat_generated += 1;
        }
        Self {
            equation_at,
            pairable: flat_generated == generated.len(),
        }
    }

    /// The DAE equation of the `position`-th generated Flat equation, when
    /// the two sides can be paired at all.
    fn equation(&self, position: usize) -> Option<EquationId> {
        self.pairable
            .then(|| self.equation_at.get(&position).copied())
            .flatten()
    }
}

/// Union-find over connector instances, so `a.p -- b.n` and `b.n -- c.p`
/// become one node rather than two edges.
type Parents = BTreeMap<String, String>;

fn root(parent: &mut Parents, of: &str) -> String {
    let mut cursor = of.to_string();
    while let Some(next) = parent.get(&cursor) {
        if next == &cursor {
            break;
        }
        cursor = next.clone();
    }
    cursor
}

fn join(parent: &mut Parents, left: &str, right: &str) {
    parent
        .entry(left.to_string())
        .or_insert_with(|| left.to_string());
    parent
        .entry(right.to_string())
        .or_insert_with(|| right.to_string());
    let (a, b) = (root(parent, left), root(parent, right));
    if a != b {
        parent.insert(a, b);
    }
}

/// The potential side of one node: its equalities and their endpoints.
struct Pending {
    potentials: Vec<VariableId>,
    potential_equations: Vec<EquationId>,
    connectors: BTreeSet<String>,
    span: rumoca_core::Span,
}

/// The flow side of one node.
struct Node {
    connectors: BTreeSet<String>,
    balances: Vec<RbcFlowBalance>,
    unconnected: bool,
    span: rumoca_core::Span,
}

/// Every flow node in first-seen order, keyed by its union-find root.
struct Flows {
    nodes: BTreeMap<String, Node>,
    order: Vec<String>,
}

/// `"battery.pin.v"` → `"battery.pin"`. A serialization-boundary operation.
/// The connection graph's nodes, from Flat's flow sums and connect equalities.
///
/// A `connect` is written pairwise and the object it creates is n-ary: three
/// pins on one node share one potential and one conservation law. The
/// potential side arrives as pairs and is closed transitively into sets here;
/// the flow side arrives already n-ary, as `EquationOrigin::FlowSum`.
///
/// Both halves are needed and only the first was ever exported, so every
/// consumer saw a graph with equalities and no conservation --- which is the
/// half that makes an acausal model worth analysing as a network.
pub(super) fn export_connection_sets(
    flat: Option<&flat::Model>,
    variables: &[RbcVariable],
    equations: &[RbcEquation],
    ctx: &mut Ctx<'_>,
) -> Vec<RbcConnectionSet> {
    let Some(flat) = flat else {
        return Vec::new();
    };
    let by_name: BTreeMap<&str, VariableId> = variables
        .iter()
        .map(|variable| (variable.name.as_str(), variable.id))
        .collect();
    let pairing = Pairing::new(flat, equations);

    let mut parent = Parents::new();
    for equation in flat.equations.iter() {
        if let flat::EquationOrigin::Connection { lhs, rhs } = &equation.origin {
            join(&mut parent, connector_path(lhs), connector_path(rhs));
        }
    }

    let pending = attach_potentials(flat, &mut parent, &by_name, &pairing);
    let flows = accumulate_flows(flat, &mut parent, &by_name, &pairing);
    assemble_sets(flows, pending, ctx)
}

/// Second pass, now that the sets are known: attach each equality to the
/// node its endpoints landed in.
fn attach_potentials(
    flat: &flat::Model,
    parent: &mut Parents,
    by_name: &BTreeMap<&str, VariableId>,
    pairing: &Pairing,
) -> BTreeMap<String, Pending> {
    let mut pending: BTreeMap<String, Pending> = BTreeMap::new();
    let mut position = 0usize;
    for equation in flat.equations.iter() {
        let flat::EquationOrigin::Connection { lhs, rhs } = &equation.origin else {
            if matches!(
                equation.origin,
                flat::EquationOrigin::FlowSum { .. } | flat::EquationOrigin::UnconnectedFlow { .. }
            ) {
                position += 1;
            }
            continue;
        };
        let node = root(parent, connector_path(lhs));
        let entry = pending.entry(node).or_insert_with(|| Pending {
            potentials: Vec::new(),
            potential_equations: Vec::new(),
            connectors: BTreeSet::new(),
            span: equation.span,
        });
        for endpoint in [lhs.as_str(), rhs.as_str()] {
            entry
                .connectors
                .insert(connector_path(endpoint).to_string());
            if let Some(id) = by_name.get(endpoint)
                && !entry.potentials.contains(id)
            {
                entry.potentials.push(*id);
            }
        }
        if let Some(id) = pairing.equation(position) {
            entry.potential_equations.push(id);
        }
        position += 1;
    }
    pending
}

/// The flow side, accumulated **per node**. A connector may declare more
/// than one flow member --- a MultiBody frame conserves a force and a
/// torque --- and each balance is its own equation but the same node.
fn accumulate_flows(
    flat: &flat::Model,
    parent: &mut Parents,
    by_name: &BTreeMap<&str, VariableId>,
    pairing: &Pairing,
) -> Flows {
    let mut flows = Flows {
        nodes: BTreeMap::new(),
        order: Vec::new(),
    };
    let mut position = 0usize;
    for equation in flat.equations.iter() {
        let (members, unconnected) = match &equation.origin {
            flat::EquationOrigin::Connection { .. } => {
                position += 1;
                continue;
            }
            flat::EquationOrigin::FlowSum { members, .. } => (members.clone(), false),
            flat::EquationOrigin::UnconnectedFlow { variable } => (
                vec![flat::FlowMember {
                    variable: variable.clone(),
                    negated: false,
                }],
                true,
            ),
            _ => continue,
        };
        let Some(first) = members.first() else {
            position += 1;
            continue;
        };
        let key = root(parent, connector_path(&first.variable));
        let node = flows.nodes.entry(key.clone()).or_insert_with(|| {
            flows.order.push(key.clone());
            Node {
                connectors: BTreeSet::new(),
                balances: Vec::new(),
                unconnected,
                span: equation.span,
            }
        });
        // One unconnected member does not make a joined node unconnected.
        node.unconnected = node.unconnected && unconnected;
        let mut terms = Vec::new();
        for member in members.iter() {
            node.connectors
                .insert(connector_path(&member.variable).to_string());
            if let Some(id) = by_name.get(member.variable.as_str()) {
                terms.push(RbcFlowTerm {
                    variable: *id,
                    negated: member.negated,
                });
            }
        }
        if !terms.is_empty() {
            node.balances.push(RbcFlowBalance {
                equation: pairing.equation(position),
                terms,
            });
        }
        position += 1;
    }
    flows
}

/// One set per flow node, merged with its potentials, then the potential-only
/// nodes.
fn assemble_sets(
    mut flows: Flows,
    pending: BTreeMap<String, Pending>,
    ctx: &mut Ctx<'_>,
) -> Vec<RbcConnectionSet> {
    let mut sets: Vec<RbcConnectionSet> = Vec::new();
    let mut claimed: BTreeSet<String> = BTreeSet::new();
    for key in flows.order {
        let Some(node) = flows.nodes.remove(&key) else {
            continue;
        };
        let mut connectors = node.connectors;
        let mut potentials = Vec::new();
        let mut potential_equations = Vec::new();
        if let Some(entry) = pending.get(&key) {
            potentials = entry.potentials.clone();
            potential_equations = entry.potential_equations.clone();
            connectors.extend(entry.connectors.iter().cloned());
            claimed.insert(key.clone());
        }
        let span = ctx.span(node.span);
        sets.push(RbcConnectionSet {
            id: ConnectionSetId(sets.len() as u32),
            connectors: connectors.into_iter().collect(),
            potentials,
            balances: node.balances,
            potential_equations,
            unconnected: node.unconnected,
            provenance: generated_provenance(span),
        });
    }

    // A node whose potentials were equated but whose flow sum did not survive
    // lowering still exists, and dropping it would understate the graph.
    let leftover: Vec<(String, Pending)> = pending
        .into_iter()
        .filter(|(node, _)| !claimed.contains(node))
        .collect();
    for (_, entry) in leftover {
        let span = ctx.span(entry.span);
        sets.push(RbcConnectionSet {
            id: ConnectionSetId(sets.len() as u32),
            connectors: entry.connectors.into_iter().collect(),
            potentials: entry.potentials,
            balances: Vec::new(),
            potential_equations: entry.potential_equations,
            unconnected: false,
            provenance: generated_provenance(span),
        });
    }
    sets
}

fn generated_provenance(span: RbcSpan) -> RbcProvenance {
    RbcProvenance {
        origin: RbcOrigin::Generated {
            generation: RbcGeneration::ConnectionEquation,
        },
        span,
    }
}
