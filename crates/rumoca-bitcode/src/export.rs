//! DAE → Rumoca Bitcode v2.
//!
//! The exporter reads the checked DAE through `Dae::inspect` and, optionally,
//! the Flat model for connector provenance the DAE does not retain. It writes
//! only public schema types: no internal identity, lifetime brand, or wire
//! record crosses this boundary.
//!
//! Dependency edges on each equation are computed here, using the compiler's
//! own scalar coordinate projection, so a consumer never has to re-derive them
//! by walking expressions.

mod clocks;
mod paths;
mod profile;
mod strings;

use paths::{component_classes, connector_path};

use std::collections::{BTreeMap, BTreeSet};

use rumoca_core::{Span, text_position};
use rumoca_ir_dae as dae;
use rumoca_ir_flat as flat;

use crate::schema::*;

mod function_body;
use function_body::export_function_body;

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("bitcode v2 cannot represent semantic owner table `{0}`")]
    UnsupportedOwner(&'static str),
    #[error("expression {0} references operand {1}, which is not yet defined")]
    ForwardOperand(u32, u32),
    #[error("dependency projection failed: {0}")]
    Projection(String),
}

/// What to include in the artifact.
///
/// Non-exhaustive: construct with `..ExportOptions::default()` so a future
/// option does not break callers.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ExportOptions {
    /// Embed full source text, making the artifact self-contained.
    pub embed_sources: bool,
    /// Emit `Unsupported` nodes for expression forms this schema version does
    /// not model, instead of failing. Off by default: a silent placeholder is
    /// worse than a refusal.
    pub tolerate_unsupported: bool,
    /// Compute per-equation dependency edges. Costs one projection pass.
    pub dependency_edges: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            embed_sources: true,
            tolerate_unsupported: true,
            dependency_edges: true,
        }
    }
}

/// Export a compiled model.
///
/// `flat` is optional. Without it the artifact has no `connections` and no
/// connector classification on variables, because the DAE does not retain them.
pub fn export(
    model: &dae::Dae,
    flat: Option<&flat::Model>,
    model_name: &str,
    options: &ExportOptions,
) -> Result<RbcFile, ExportError> {
    let rbc = model.inspect(|view| build(view, model, flat, model_name, options))?;
    Ok(RbcFile {
        execution: None,
        magic: RBC_MAGIC.to_string(),
        bitcode_version: RBC_VERSION,
        producer: format!("rumoca {}", env!("CARGO_PKG_VERSION")),
        model: rbc,
    })
}

struct Ctx<'a> {
    /// Maps a `rumoca_core::SourceId` (a name-derived hash) to a dense RBC id.
    sources: BTreeMap<u64, SourceId>,
    /// Source text by RBC source id, for line/column resolution.
    texts: Vec<String>,
    names: Vec<String>,
    source_map: &'a rumoca_core::SourceMap,
}

impl Ctx<'_> {
    fn span(&mut self, span: Span) -> RbcSpan {
        let raw = span.source.0;
        let source = *self.sources.entry(raw).or_insert_with(|| {
            let id = SourceId(self.names.len() as u32);
            let (name, text) = self
                .source_map
                .get_source(span.source)
                .map(|(name, text)| (name.to_string(), text.to_string()))
                .unwrap_or_else(|| (format!("<unknown:{raw}>"), String::new()));
            self.names.push(name);
            self.texts.push(text);
            id
        });
        let start = span.start.0 as u32;
        let end = span.end.0 as u32;
        let text = &self.texts[source.0 as usize];
        let (line, column) = if text.is_empty() || start as usize > text.len() {
            (0, 0)
        } else {
            let position = text_position::byte_offset_to_position(text, start as usize);
            // RBC reports 1-based positions; TextPosition is 0-based.
            (position.line + 1, position.character + 1)
        };
        RbcSpan {
            source,
            start,
            end,
            line,
            column,
        }
    }

    fn provenance(&mut self, provenance: dae::DaeProvenance) -> RbcProvenance {
        RbcProvenance {
            origin: match provenance.origin() {
                dae::DaeProvenanceOrigin::Source => RbcOrigin::Source,
                dae::DaeProvenanceOrigin::Generated(generation) => RbcOrigin::Generated {
                    generation: generation_of(generation),
                },
            },
            span: self.span(provenance.span()),
        }
    }
}

fn generation_of(generation: dae::DaeGeneration) -> RbcGeneration {
    use dae::DaeGeneration as G;
    match generation {
        G::SyntheticResidual => RbcGeneration::SyntheticResidual,
        G::BindingEquation => RbcGeneration::BindingEquation,
        G::ConnectionEquation => RbcGeneration::ConnectionEquation,
        G::FlowBalanceEquation => RbcGeneration::FlowBalanceEquation,
        G::AlgorithmEquation => RbcGeneration::AlgorithmEquation,
        G::DiscreteUpdate => RbcGeneration::DiscreteUpdate,
        G::ConditionLowering => RbcGeneration::ConditionLowering,
        G::PreValueLowering => RbcGeneration::PreValueLowering,
        G::ClockLowering => RbcGeneration::ClockLowering,
        G::DelayLowering => RbcGeneration::DelayLowering,
        G::SemiLinearLowering => RbcGeneration::SemiLinearLowering,
        G::TerminalLowering => RbcGeneration::TerminalLowering,
        G::EventActionLowering => RbcGeneration::EventActionLowering,
        G::InitializationEquation => RbcGeneration::InitializationEquation,
        G::DefaultStart => RbcGeneration::DefaultStart,
        G::ArrayEquationProjection => RbcGeneration::ArrayEquationProjection,
        G::RecordEquationProjection => RbcGeneration::RecordEquationProjection,
        G::FunctionLoopLowering => RbcGeneration::FunctionLoopLowering,
        G::FunctionConditionLowering => RbcGeneration::FunctionConditionLowering,
        G::FunctionAggregateLowering => RbcGeneration::FunctionAggregateLowering,
        G::DerivedParameterLowering => RbcGeneration::DerivedParameterLowering,
        G::IndexReduction => RbcGeneration::IndexReduction,
        G::AliasElimination => RbcGeneration::AliasElimination,
        G::RuntimeDiscontinuity => RbcGeneration::RuntimeDiscontinuity,
    }
}

fn build(
    view: dae::DaeView<'_>,
    model: &dae::Dae,
    flat: Option<&flat::Model>,
    model_name: &str,
    options: &ExportOptions,
) -> Result<RbcModel, ExportError> {
    profile::check(view)?;
    let mut ctx = Ctx {
        sources: BTreeMap::new(),
        texts: Vec::new(),
        names: Vec::new(),
        source_map: model.source_map(),
    };

    let types = export_types(view, &mut ctx);
    let expressions = export_expressions(view, &mut ctx, options)?;
    let (components, component_of) = export_components(view, flat);
    let variables = export_variables(view, &mut ctx, flat, &component_of);
    let equations = export_equations(view, &mut ctx, options, EquationKind::Continuous)?;
    let initial_equations = export_equations(view, &mut ctx, options, EquationKind::Initial)?;
    let domains = export_domains(view, &mut ctx);
    let equation_families =
        export_equation_families(view, &mut ctx, EquationKind::Continuous, options);
    let initial_equation_families =
        export_equation_families(view, &mut ctx, EquationKind::Initial, options);
    let relations = export_relations(view, &mut ctx);
    let conditions = export_conditions(view, &mut ctx);
    let clocks = clocks::export_clocks(view, &mut ctx);
    let clock_ownerships = clocks::export_ownerships(view, &mut ctx);
    let roots = export_roots(view, &mut ctx);
    let events = export_events(view, &mut ctx);
    let discrete_definitions = export_discrete_definitions(view, &mut ctx);
    let time_events = export_time_events(view, &mut ctx)?;
    let connections = export_connections(flat, &variables, &equations, &mut ctx);
    let connection_sets = export_connection_sets(flat, &variables, &equations, &mut ctx);

    let sources = ctx
        .names
        .iter()
        .enumerate()
        .map(|(index, name)| RbcSource {
            id: SourceId(index as u32),
            name: name.clone(),
            text: options
                .embed_sources
                .then(|| ctx.texts[index].clone())
                .filter(|text| !text.is_empty()),
        })
        .collect();

    let discrete_real_equations = export_discrete_real_equations(view, &mut ctx, options)?;
    let initial_discrete_values = export_initial_discrete_values(view, &mut ctx);

    let mut summary = summarize(
        &variables,
        &expressions,
        &equations,
        &initial_equations,
        &equation_families,
        &domains,
        &relations,
        &conditions,
        &roots,
        &events,
        &time_events,
        &discrete_definitions,
        &connections,
        &connection_sets,
        &components,
        &discrete_real_equations,
        &initial_discrete_values,
    );
    summary.clocks = clocks.len() as u32;
    summary.clock_ownerships = clock_ownerships.len() as u32;

    Ok(RbcModel {
        connector_types: Vec::new(),
        connectors: Vec::new(),
        name: model_name.to_string(),
        sources,
        types,
        variables,
        expressions,
        equations,
        initial_equations,
        domains,
        functions: export_functions(view, &mut ctx),
        discrete_real_equations,
        initial_discrete_values,
        equation_families,
        initial_equation_families,
        relations,
        conditions,
        clocks,
        clock_ownerships,
        roots,
        events,
        time_events,
        discrete_definitions,
        connections,
        connection_sets,
        components,
        trace_points: Vec::new(),
        summary,
    })
}

fn export_types(view: dae::DaeView<'_>, ctx: &mut Ctx<'_>) -> Vec<RbcType> {
    (0..view.value_type_count())
        .filter_map(|index| {
            let id = view.value_type_id(index)?;
            let ty = view.value_type(id)?;
            let _ = ctx;
            Some(RbcType {
                id: TypeId(index as u32),
                scalar: scalar_of(ty),
                dimensions: ty.dimensions().to_vec(),
                record: record_of(view, id, ty),
            })
        })
        .collect()
}

/// A record type's name and fields.
///
/// A record used to export as a field-free `String`, which made `Complex` and
/// every model built on it unimportable: a `Record` node rebuilt against a
/// type claiming zero fields was rejected for arity.
fn record_of<'dae>(
    view: dae::DaeView<'dae>,
    id: dae::ValueTypeId<'dae>,
    ty: &dae::ValueType,
) -> Option<RbcRecord> {
    if !ty.is_record() {
        return None;
    }
    let mut fields = Vec::with_capacity(ty.record_field_count());
    for ordinal in 0..ty.record_field_count() {
        let (name, value_type) = view.record_field(id, ordinal)?;
        fields.push(RbcRecordField {
            name: name.to_string(),
            value_type: TypeId(value_type.index()),
        });
    }
    Some(RbcRecord {
        name: ty.record_name()?.to_string(),
        fields,
    })
}

fn scalar_of(ty: &dae::ValueType) -> RbcScalar {
    if ty.is_record() {
        return RbcScalar::Record;
    }
    match ty.scalar_type() {
        dae::ScalarType::Real => RbcScalar::Real,
        dae::ScalarType::Integer => RbcScalar::Integer,
        dae::ScalarType::Boolean => RbcScalar::Boolean,
        dae::ScalarType::String => RbcScalar::String,
        dae::ScalarType::Enumeration => RbcScalar::Enumeration,
        dae::ScalarType::Record => RbcScalar::Record,
    }
}

/// Derive component instances from flattened variable paths.
///
/// Name segmentation at a serialization boundary is explicitly permitted;
/// compiler-internal identity elsewhere uses `DefId`/`VarName`.
fn export_components(
    view: dae::DaeView<'_>,
    flat: Option<&flat::Model>,
) -> (Vec<RbcComponent>, BTreeMap<String, ComponentId>) {
    let classes = component_classes(flat);
    let mut paths = BTreeMap::new();
    for index in 0..view.variable_count() {
        let Some(id) = view.variable_id(index) else {
            continue;
        };
        let Some(variable) = view.variable(id) else {
            continue;
        };
        let name = variable.name().to_string();
        if let Some((component, _)) = rumoca_core::split_first_top_level(&name) {
            let next = ComponentId(paths.len() as u32);
            paths.entry(component.to_string()).or_insert(next);
        }
    }
    // Re-index densely in sorted order so output is deterministic.
    let mut components = Vec::new();
    let mut map = BTreeMap::new();
    for (index, path) in paths.keys().enumerate() {
        let id = ComponentId(index as u32);
        map.insert(path.clone(), id);
        components.push(RbcComponent {
            id,
            path: path.clone(),
            class_name: classes.get(path).cloned(),
        });
    }
    (components, map)
}

fn export_variables(
    view: dae::DaeView<'_>,
    ctx: &mut Ctx<'_>,
    flat: Option<&flat::Model>,
    component_of: &BTreeMap<String, ComponentId>,
) -> Vec<RbcVariable> {
    (0..view.variable_count())
        .filter_map(|index| {
            let id = view.variable_id(index)?;
            let variable = view.variable(id)?;
            let name = variable.name().to_string();
            let connector = flat.and_then(|flat| connector_member(flat, &name));
            let component = rumoca_core::split_first_top_level(&name)
                .and_then(|(prefix, _)| component_of.get(prefix).copied());
            Some(RbcVariable {
                id: VariableId(index as u32),
                role: role_of(variable.role()),
                causality: causality_of(variable.causality()),
                value_type: TypeId(variable.value_type_id().index()),
                scalar_count: variable.scalar_count() as u32,
                discrete_input: variable.role() == dae::VariableRole::Input
                    && variable.variability() != dae::ExpressionVariability::Continuous,
                declaration: ctx.provenance(variable.declaration()),
                component,
                unit: variable.unit().map(str::to_string),
                physical_quantity: flat.and_then(|flat| physical_quantity(flat, &name)),
                declaring_class: flat.and_then(|flat| declaring_class(flat, &name)),
                contract: flat.and_then(|flat| contract(view, flat, &name, variable)),
                description: variable.description().map(str::to_string),
                binding: variable.binding().map(|e| ExprId(e.index())),
                start: variable.start().map(|e| ExprId(e.index())),
                min: variable.minimum().map(|e| ExprId(e.index())),
                max: variable.maximum().map(|e| ExprId(e.index())),
                nominal: variable.nominal().map(|e| ExprId(e.index())),
                fixed: variable.fixed(),
                tunable: variable.is_tunable(),
                from_source: matches!(variable.origin(), dae::VariableOrigin::Source),
                connector,
                name,
            })
        })
        .collect()
}

/// Recover connector semantics for one flattened variable from the Flat model.
/// The declared `quantity` attribute, which the DAE does not carry but Flat does.
/// What the declaration promises about `name`.
///
/// The prefixes come from Flat, because the DAE keeps the Appendix-B partition
/// and discards them: `constant` and `parameter` are both role `parameter` by
/// the time a coordinate exists, and an analysis that cannot tell them apart
/// will offer to set `pi` to zero. The binding's value and dependencies come
/// from the DAE, through the same projection every other incidence set uses.
fn contract<'dae>(
    view: dae::DaeView<'dae>,
    flat: &flat::Model,
    name: &str,
    variable: dae::VariableView<'dae>,
) -> Option<RbcSymbolContract> {
    use rumoca_core::Variability;

    let interned = rumoca_core::VarName::intern(name);
    let declared = flat.variables.get(&interned)?;
    let variability = match declared.variability {
        Variability::Constant(_) => RbcVariability::Constant,
        Variability::Parameter(_) => RbcVariability::Parameter,
        Variability::Discrete(_) => RbcVariability::Discrete,
        Variability::Continuous(_) | Variability::Empty => RbcVariability::Continuous,
    };

    let binding = variable.binding();
    let (effective_value, depends_on) = match binding {
        Some(expression) => (
            literal_value(view, expression),
            dependencies(view, expression)
                .map(|reads| reads.take().0)
                .unwrap_or_default(),
        ),
        None => (None, Vec::new()),
    };

    Some(RbcSymbolContract {
        variability,
        is_final: declared.is_final,
        is_protected: declared.is_protected,
        evaluate: declared.evaluate,
        // MLS §18.3: a structural parameter is one the translation depends on.
        structural: declared.evaluate
            && matches!(
                variability,
                RbcVariability::Parameter | RbcVariability::Constant
            ),
        effective_value,
        binding_depends_on: depends_on,
        binding_from_modification: declared.binding_from_modification,
        declared_in: flat.variable_declaring_classes.get(&interned).cloned(),
    })
}

/// A binding that is a bare numeric literal, as a number.
///
/// Deliberately only the literal case. Anything else is what
/// `binding_depends_on` is for: a consumer walks the chain rather than being
/// handed a value the compiler guessed at.
fn literal_value<'dae>(view: dae::DaeView<'dae>, expression: dae::ExprId<'dae>) -> Option<f64> {
    let node = view.expression(expression)?;
    match node.operation() {
        dae::ExpressionOperation::Literal(dae::DaeLiteral::Real(value)) => Some(*value),
        dae::ExpressionOperation::Literal(dae::DaeLiteral::Integer(value)) => Some(*value as f64),
        _ => None,
    }
}

/// The class that declared `name`, which the DAE does not carry but Flat does.
fn declaring_class(flat: &flat::Model, name: &str) -> Option<String> {
    let interned = rumoca_core::VarName::intern(name);
    flat.variable_declaring_classes.get(&interned).cloned()
}

fn physical_quantity(flat: &flat::Model, name: &str) -> Option<String> {
    let interned = rumoca_core::VarName::intern(name);
    flat.variables.get(&interned)?.quantity.clone()
}

fn connector_member(flat: &flat::Model, name: &str) -> Option<RbcConnectorMember> {
    let interned = rumoca_core::VarName::intern(name);
    let variable = flat.variables.get(&interned)?;
    let quantity = if variable.stream {
        RbcQuantityKind::Stream
    } else if variable.flow {
        RbcQuantityKind::Flow
    } else if variable.connected {
        RbcQuantityKind::Potential
    } else {
        return None;
    };
    Some(RbcConnectorMember {
        quantity,
        connected: variable.connected,
    })
}

fn role_of(role: dae::VariableRole) -> RbcRole {
    match role {
        dae::VariableRole::Parameter => RbcRole::Parameter,
        dae::VariableRole::Constant => RbcRole::Constant,
        dae::VariableRole::Input => RbcRole::Input,
        dae::VariableRole::State => RbcRole::State,
        dae::VariableRole::Algebraic => RbcRole::Algebraic,
        dae::VariableRole::Output => RbcRole::Output,
        dae::VariableRole::DiscreteReal => RbcRole::DiscreteReal,
        dae::VariableRole::DiscreteValue => RbcRole::DiscreteValue,
    }
}

fn causality_of(causality: dae::VariableCausality) -> RbcCausality {
    match causality {
        dae::VariableCausality::Input => RbcCausality::Input,
        dae::VariableCausality::Output => RbcCausality::Output,
        dae::VariableCausality::Parameter => RbcCausality::Parameter,
        dae::VariableCausality::CalculatedParameter => RbcCausality::CalculatedParameter,
        dae::VariableCausality::Independent => RbcCausality::Independent,
        dae::VariableCausality::Local => RbcCausality::Local,
    }
}

fn export_expressions(
    view: dae::DaeView<'_>,
    ctx: &mut Ctx<'_>,
    options: &ExportOptions,
) -> Result<Vec<RbcExpr>, ExportError> {
    let mut out = Vec::with_capacity(view.expression_count());
    for index in 0..view.expression_count() {
        let Some(id) = view.expression_id(index) else {
            continue;
        };
        let Some(expression) = view.expression(id) else {
            continue;
        };
        let node = expression_node(expression, index as u32, options)?;
        out.push(RbcExpr {
            id: ExprId(index as u32),
            value_type: TypeId(expression.value_type_id().index()),
            node,
            provenance: ctx.provenance(expression.provenance()),
        });
    }
    Ok(out)
}

fn expression_node(
    expression: dae::ExpressionView<'_>,
    index: u32,
    options: &ExportOptions,
) -> Result<RbcExprNode, ExportError> {
    let check = |operand: dae::ExprId<'_>| -> Result<ExprId, ExportError> {
        if operand.index() >= index {
            return Err(ExportError::ForwardOperand(index, operand.index()));
        }
        Ok(ExprId(operand.index()))
    };
    let node = match expression.operation() {
        dae::ExpressionOperation::StringConversion { value, format, .. } => {
            RbcExprNode::StringConversion {
                value: check(value)?,
                format: strings::format(format, check)?,
            }
        }
        dae::ExpressionOperation::Literal(literal) => RbcExprNode::Literal {
            value: literal_of(literal),
        },
        dae::ExpressionOperation::Coordinate(coordinate) => match coordinate_of(coordinate) {
            Some(coordinate) => RbcExprNode::Coordinate { coordinate },
            // Naming the kind is the difference between a report a maintainer
            // can act on and one that only says something is missing.
            None => unsupported(
                &format!(
                    "coordinate kind {} not in bitcode v2",
                    coordinate_kind_name(coordinate)
                ),
                options,
            )?,
        },
        dae::ExpressionOperation::Unary { operator, operand } => match unary_of(operator) {
            Some(op) => RbcExprNode::Unary {
                op,
                operand: check(operand)?,
            },
            None => unsupported("unary operator has no bitcode v2 encoding", options)?,
        },
        dae::ExpressionOperation::Binary { operator, lhs, rhs } => match binary_of(operator) {
            Some(op) => RbcExprNode::Binary {
                op,
                lhs: check(lhs)?,
                rhs: check(rhs)?,
            },
            None => unsupported("binary operator not in bitcode v2", options)?,
        },
        dae::ExpressionOperation::Conditional(operands) => {
            // Packed as [cond, value, cond, value, ..., fallback]: an even
            // number of branch operands followed by the required fallback.
            let count = operands.len();
            if count == 0 || count % 2 == 0 {
                unsupported("malformed conditional operand packing", options)?
            } else {
                let mut branches = Vec::with_capacity(count / 2);
                for pair in 0..count / 2 {
                    let condition = operands
                        .get(pair * 2)
                        .ok_or_else(|| ExportError::Projection("conditional operand".into()))?;
                    let value = operands
                        .get(pair * 2 + 1)
                        .ok_or_else(|| ExportError::Projection("conditional operand".into()))?;
                    branches.push(RbcBranch {
                        condition: check(condition)?,
                        value: check(value)?,
                    });
                }
                let fallback = operands
                    .get(count - 1)
                    .ok_or_else(|| ExportError::Projection("conditional fallback".into()))?;
                RbcExprNode::Conditional {
                    branches,
                    fallback: check(fallback)?,
                }
            }
        }
        dae::ExpressionOperation::Builtin { builtin, arguments } => {
            let mut operands = Vec::with_capacity(arguments.len());
            for position in 0..arguments.len() {
                let argument = arguments
                    .get(position)
                    .ok_or_else(|| ExportError::Projection("builtin argument".into()))?;
                operands.push(check(argument)?);
            }
            RbcExprNode::Builtin {
                name: builtin_name(builtin).to_string(),
                arguments: operands,
            }
        }
        dae::ExpressionOperation::Array(operands) => {
            let mut elements = Vec::with_capacity(operands.len());
            for position in 0..operands.len() {
                let element = operands
                    .get(position)
                    .ok_or_else(|| ExportError::Projection("array element".into()))?;
                elements.push(check(element)?);
            }
            // A zero-element array has no operand from which the importer
            // could re-derive its element type, so the type travels with it.
            let empty_type = elements
                .is_empty()
                .then(|| TypeId(expression.value_type_id().index()));
            RbcExprNode::Array {
                elements,
                empty_type,
            }
        }
        dae::ExpressionOperation::Record(operands) => {
            let mut fields = Vec::with_capacity(operands.len());
            for position in 0..operands.len() {
                let field = operands
                    .get(position)
                    .ok_or_else(|| ExportError::Projection("record field".into()))?;
                fields.push(check(field)?);
            }
            RbcExprNode::Record {
                ty: TypeId(expression.value_type_id().index()),
                fields,
            }
        }
        dae::ExpressionOperation::Field { base, field } => RbcExprNode::Field {
            base: check(base)?,
            field,
        },
        dae::ExpressionOperation::Range(range) => RbcExprNode::Range {
            start: check(range.start().expression())?,
            step: range
                .explicit_step()
                .map(|step| check(step.expression()))
                .transpose()?,
            stop: check(range.stop().expression())?,
        },
        dae::ExpressionOperation::Comprehension { domain, body } => RbcExprNode::Comprehension {
            domain: DomainId(domain.index()),
            body: check(body)?,
        },
        dae::ExpressionOperation::Index { base, subscripts } => RbcExprNode::Index {
            base: check(base)?,
            subscripts: subscripts_of(subscripts, &check)?,
        },
        dae::ExpressionOperation::Call {
            owner,
            function,
            output,
            arguments,
        } => {
            let mut operands = Vec::with_capacity(arguments.len());
            for position in 0..arguments.len() {
                let argument = arguments
                    .get(position)
                    .ok_or_else(|| ExportError::Projection("call argument".into()))?;
                operands.push(check(argument)?);
            }
            RbcExprNode::Call {
                // A projection may be its own owner, so this is `<=`, not the
                // strict earlier-operand check the others use.
                owner: ExprId(owner.index()),
                function: FunctionId(function.index()),
                output,
                arguments: operands,
            }
        }
        dae::ExpressionOperation::ArrayUpdate {
            base,
            value,
            subscripts,
        } => RbcExprNode::ArrayUpdate {
            base: check(base)?,
            value: check(value)?,
            subscripts: subscripts_of(subscripts, &check)?,
        },
        other => unsupported(
            &format!(
                "expression form {} not in bitcode v2",
                operation_name(&other)
            ),
            options,
        )?,
    };
    Ok(node)
}

fn subscripts_of(
    subscripts: dae::SubscriptsView<'_>,
    check: &impl Fn(dae::ExprId<'_>) -> Result<ExprId, ExportError>,
) -> Result<Vec<RbcSubscript>, ExportError> {
    subscripts
        .iter()
        .map(|subscript| {
            Ok(match subscript {
                dae::SubscriptView::Index { expression, .. } => RbcSubscript::Index {
                    expression: check(expression)?,
                },
                dae::SubscriptView::Whole { .. } => RbcSubscript::Whole,
                dae::SubscriptView::Slice { expression, .. } => RbcSubscript::Slice {
                    expression: check(expression)?,
                },
            })
        })
        .collect()
}

fn operation_name(operation: &dae::ExpressionOperation<'_>) -> &'static str {
    use dae::ExpressionOperation as O;
    match operation {
        O::Literal(_) => "literal",
        O::Coordinate(_) => "coordinate",
        O::Unary { .. } => "unary",
        O::Binary { .. } => "binary",
        O::Conditional(_) => "conditional",
        O::Array(_) => "array",
        O::Record(_) => "record",
        O::Field { .. } => "field",
        O::Range(_) => "range",
        O::Comprehension { .. } => "comprehension",
        O::Index { .. } => "index",
        O::ArrayUpdate { .. } => "array_update",
        O::Builtin { .. } => "builtin",
        O::Call { .. } => "call",
        O::StringConversion { .. } => "string_conversion",
        O::ClockTransfer { .. } => "clock_transfer",
        O::FunctionValue { .. } => "function_value",
        O::FunctionFoldParameter { .. } => "function_fold_parameter",
        O::FunctionFoldOutput { .. } => "function_fold_output",
    }
}

fn coordinate_kind_name(coordinate: dae::CoordinateView<'_>) -> &'static str {
    use dae::CoordinateView as C;
    match coordinate {
        C::Parameter(_) => "parameter",
        C::Input(_) => "input",
        C::State(_) => "state",
        C::Derivative(_) => "derivative",
        C::Algebraic(_) => "algebraic",
        C::DiscreteReal(_) => "discrete_real",
        C::DiscreteValue(_) => "discrete_value",
        C::PreDiscreteReal(_) => "pre_discrete_real",
        C::PreDiscreteValue(_) => "pre_discrete_value",
        C::PreState(_) => "pre_state",
        C::PreAlgebraic(_) => "pre_algebraic",
        C::Time => "time",
        C::ClockInterval(_) => "clock_interval",
        C::Condition(_) => "condition",
        C::Delay(_) => "delay",
        C::Previous(_) => "previous",
        C::Terminal(_) => "terminal",
        C::Binder(_) => "binder",
        C::FunctionParameter(_) => "function_parameter",
    }
}

fn unsupported(detail: &str, options: &ExportOptions) -> Result<RbcExprNode, ExportError> {
    if options.tolerate_unsupported {
        Ok(RbcExprNode::Unsupported {
            detail: detail.to_string(),
        })
    } else {
        Err(ExportError::Projection(detail.to_string()))
    }
}

fn literal_of(literal: &dae::DaeLiteral) -> RbcLiteral {
    match literal {
        dae::DaeLiteral::Real(value) => RbcLiteral::Real { value: *value },
        dae::DaeLiteral::Integer(value) => RbcLiteral::Integer { value: *value },
        dae::DaeLiteral::Boolean(value) => RbcLiteral::Boolean { value: *value },
        dae::DaeLiteral::String(value) => RbcLiteral::String {
            value: value.to_string(),
        },
        dae::DaeLiteral::Enumeration(ordinal) => RbcLiteral::Enumeration { ordinal: *ordinal },
    }
}

fn coordinate_of(coordinate: dae::CoordinateView<'_>) -> Option<RbcCoordinate> {
    use dae::CoordinateView as C;
    let variable = |index: u32| VariableId(index);
    Some(match coordinate {
        C::Parameter(id) => RbcCoordinate::Parameter {
            variable: variable(id.index()),
        },
        C::Input(id) => RbcCoordinate::Input {
            variable: variable(id.index()),
        },
        C::State(id) => RbcCoordinate::State {
            variable: variable(id.index()),
        },
        C::Derivative(id) => RbcCoordinate::Derivative {
            variable: variable(id.index()),
        },
        C::Algebraic(id) => RbcCoordinate::Algebraic {
            variable: variable(id.index()),
        },
        C::DiscreteReal(id) => RbcCoordinate::DiscreteReal {
            variable: variable(id.index()),
        },
        C::DiscreteValue(id) => RbcCoordinate::DiscreteValue {
            variable: variable(id.index()),
        },
        C::PreState(id) => RbcCoordinate::PreState {
            variable: variable(id.index()),
        },
        C::PreAlgebraic(id) => RbcCoordinate::PreAlgebraic {
            variable: variable(id.index()),
        },
        C::PreDiscreteReal(id) => RbcCoordinate::PreDiscreteReal {
            variable: variable(id.index()),
        },
        C::PreDiscreteValue(id) => RbcCoordinate::PreDiscreteValue {
            variable: variable(id.index()),
        },
        C::Time => RbcCoordinate::Time,
        C::Condition(id) => RbcCoordinate::Condition {
            condition: ConditionId(id.index()),
        },
        C::FunctionParameter(parameter) => RbcCoordinate::FunctionParameter {
            function: FunctionId(parameter.function().index()),
            ordinal: parameter.ordinal(),
        },
        C::Binder(binder) => RbcCoordinate::Binder {
            domain: DomainId(binder.domain().index()),
            ordinal: binder.ordinal(),
        },
        _ => return None,
    })
}

/// Modelica spelling of a pure built-in.
///
/// Names are the contract, not enum ordinals: a consumer matches `"sqrt"`, and
/// appending a variant upstream cannot silently change what an existing
/// artifact means.
fn builtin_name(builtin: dae::PureBuiltin) -> &'static str {
    use dae::PureBuiltin as B;
    match builtin {
        B::Abs => "abs",
        B::Sign => "sign",
        B::Sqrt => "sqrt",
        B::Div => "div",
        B::Mod => "mod",
        B::Rem => "rem",
        B::Floor => "floor",
        B::Ceil => "ceil",
        B::Integer => "integer",
        B::Sin => "sin",
        B::Cos => "cos",
        B::Tan => "tan",
        B::Asin => "asin",
        B::Acos => "acos",
        B::Atan => "atan",
        B::Atan2 => "atan2",
        B::Sinh => "sinh",
        B::Cosh => "cosh",
        B::Tanh => "tanh",
        B::Exp => "exp",
        B::Log => "log",
        B::Log10 => "log10",
        B::Smooth => "smooth",
        B::NoEvent => "noEvent",
        B::Homotopy => "homotopy",
        B::Min => "min",
        B::Max => "max",
        B::Sum => "sum",
        B::Product => "product",
        B::Size => "size",
        B::Zeros => "zeros",
        B::Ones => "ones",
        B::Fill => "fill",
        B::Linspace => "linspace",
        B::Cross => "cross",
        B::Identity => "identity",
        B::Vector => "vector",
        B::Transpose => "transpose",
        B::Diagonal => "diagonal",
        B::OuterProduct => "outerProduct",
        B::Skew => "skew",
        B::PromotedCat1 => "cat1",
        B::PromotedCat2 => "cat2",
    }
}

fn unary_of(operator: dae::UnaryOperator) -> Option<RbcUnaryOp> {
    match operator {
        dae::UnaryOperator::Negate => Some(RbcUnaryOp::Negate),
        dae::UnaryOperator::Not => Some(RbcUnaryOp::Not),
        // Constant folding usually consumes unary plus before export, which is
        // how its absence went unnoticed: `parameter SI.Voltage Vps=+15`
        // survives only when folding is off, and the export then failed.
        dae::UnaryOperator::Plus => Some(RbcUnaryOp::Plus),
    }
}

fn binary_of(operator: dae::BinaryOperator) -> Option<RbcBinaryOp> {
    use dae::BinaryOperator as B;
    Some(match operator {
        B::Add => RbcBinaryOp::Add,
        B::Subtract => RbcBinaryOp::Subtract,
        B::Multiply => RbcBinaryOp::Multiply,
        B::Divide => RbcBinaryOp::Divide,
        B::Power => RbcBinaryOp::Power,
        B::Equal => RbcBinaryOp::Equal,
        B::NotEqual => RbcBinaryOp::NotEqual,
        B::Less => RbcBinaryOp::Less,
        B::LessEqual => RbcBinaryOp::LessEqual,
        B::Greater => RbcBinaryOp::Greater,
        B::GreaterEqual => RbcBinaryOp::GreaterEqual,
        B::And => RbcBinaryOp::And,
        B::Or => RbcBinaryOp::Or,
        _ => return None,
    })
}

#[derive(Clone, Copy)]
enum EquationKind {
    Continuous,
    Initial,
}

fn export_equations(
    view: dae::DaeView<'_>,
    ctx: &mut Ctx<'_>,
    options: &ExportOptions,
    kind: EquationKind,
) -> Result<Vec<RbcEquation>, ExportError> {
    let count = match kind {
        EquationKind::Continuous => view.continuous_equation_count(),
        EquationKind::Initial => view.initialization_equation_count(),
    };
    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        let equation = match kind {
            EquationKind::Continuous => view.continuous_equation(index),
            EquationKind::Initial => view.initialization_equation(index),
        };
        let Some(equation) = equation else { continue };
        let residual = equation.residual();
        let (reads, reads_derivative, reads_previous) = if options.dependency_edges {
            dependencies(view, residual)?.take()
        } else {
            Default::default()
        };
        out.push(RbcEquation {
            id: EquationId(index as u32),
            residual: ExprId(residual.index()),
            provenance: ctx.provenance(equation.provenance()),
            reads,
            reads_derivative,
            reads_previous,
        });
    }
    Ok(out)
}

/// Export the iteration domains the families range over.
///
/// A family without its domain can be counted but not rebuilt, which is why
/// import had to refuse one. `StructuredIndexDomain` is a binder list and
/// serialises directly; the extents and scalar count are carried alongside
/// because the DAE derives them and a consumer should not have to.
/// Export the MLS Appendix B.1b partition.
///
/// A discrete-Real variable is determined here, not by a continuous residual,
/// so omitting this partition made the artifact over-constrained by exactly
/// the number of such variables.
fn export_discrete_real_equations(
    view: dae::DaeView<'_>,
    ctx: &mut Ctx<'_>,
    options: &ExportOptions,
) -> Result<Vec<RbcDiscreteRealEquation>, ExportError> {
    let mut out = Vec::with_capacity(view.discrete_real_equation_count());
    for (index, equation) in view.discrete_real_equations().enumerate() {
        let residual = equation.residual();
        let (reads, reads_derivative, reads_previous) = if options.dependency_edges {
            dependencies(view, residual)?.take()
        } else {
            Default::default()
        };
        out.push(RbcDiscreteRealEquation {
            id: EquationId(index as u32),
            residual: ExprId(residual.index()),
            activation: match equation.activation() {
                dae::DiscreteRealActivation::Always => RbcDiscreteRealActivation::Always,
                dae::DiscreteRealActivation::When { trigger, guard } => {
                    RbcDiscreteRealActivation::When {
                        trigger: ConditionId(trigger.index()),
                        guard: ConditionId(guard.index()),
                    }
                }
            },
            reads,
            reads_derivative,
            reads_previous,
            provenance: ctx.provenance(equation.provenance()),
        });
    }
    Ok(out)
}

fn export_initial_discrete_values(
    view: dae::DaeView<'_>,
    ctx: &mut Ctx<'_>,
) -> Vec<RbcInitialDiscreteValue> {
    view.initial_discrete_values()
        .map(|entry| RbcInitialDiscreteValue {
            target: VariableId(entry.target().index()),
            value: ExprId(entry.value().index()),
            provenance: ctx.provenance(entry.provenance()),
        })
        .collect()
}

/// Export function *declarations*: the signature a call site needs.
///
/// Not the bodies. A body is its own IR — SSA definitions, loop transitions,
/// conditionals, external interfaces — and carrying it is a larger piece of
/// work than the whole of the rest of this schema. Without the declarations a
/// call could not be expressed at all, so every call in the model exported as
/// `Unsupported` and every expression under it became unreachable: 3621 calls
/// across 156 of 491 MSL models, whose arguments no consumer could see.
/// Callee edges for every function, indexed by function ordinal.
///
/// Built in one pass: every expression knows the function scope it belongs to
/// (`function_scope`), so the calls inside a body are recoverable here even
/// though the body itself is never exported. This is the same traversal
/// `rumoca-phase-structural` uses to order functions by dependency.
fn call_graph(view: dae::DaeView<'_>) -> Vec<Vec<FunctionId>> {
    let mut edges: Vec<Vec<FunctionId>> = vec![Vec::new(); view.function_count()];
    for index in 0..view.expression_count() {
        let Some(id) = view.expression_id(index) else {
            continue;
        };
        let Some(expression) = view.expression(id) else {
            continue;
        };
        let Some(owner) = expression.function_scope() else {
            continue;
        };
        let dae::ExpressionOperation::Call { function, .. } = expression.operation() else {
            continue;
        };
        if let Some(slot) = edges.get_mut(owner.index() as usize) {
            slot.push(FunctionId(function.index()));
        }
    }
    for callees in &mut edges {
        callees.sort_unstable_by_key(|id| id.0);
        callees.dedup();
    }
    edges
}

fn export_functions(view: dae::DaeView<'_>, ctx: &mut Ctx<'_>) -> Vec<RbcFunction> {
    let edges = call_graph(view);
    (0..view.function_count())
        .filter_map(|index| {
            let id = view.function_id(index)?;
            let function = view.function(id)?;
            let parameters = function
                .parameters()
                .map(|parameter| RbcFunctionParameter {
                    name: parameter.name().to_string(),
                    value_type: TypeId(parameter.value_type().index()),
                })
                .collect();
            let results = function
                .result_types()
                .iter()
                .map(|ty| TypeId(ty.index()))
                .collect();
            let body = match function.external() {
                Some(external) => RbcFunctionBody::External {
                    language: format!("{:?}", external.language()).to_lowercase(),
                    symbol: external.symbol().to_string(),
                },
                // A Modelica body is carried when every statement in it is a
                // form this schema version holds. Partial bodies are not
                // emitted: half a body is worse than none, because a
                // consumer cannot tell which half is missing.
                None => export_function_body(function),
            };
            Some(RbcFunction {
                id: FunctionId(index as u32),
                calls: edges.get(index).cloned().unwrap_or_default(),
                name: function.name().to_string(),
                parameters,
                results,
                inline: match function.inline() {
                    rumoca_core::InlineAnnotation::Unstated => RbcInline::Unstated,
                    rumoca_core::InlineAnnotation::Requested => RbcInline::Requested,
                    rumoca_core::InlineAnnotation::Never => RbcInline::Never,
                },
                body,
                declaration: ctx.provenance(function.declaration()),
            })
        })
        .collect()
}

fn export_domains(view: dae::DaeView<'_>, ctx: &mut Ctx<'_>) -> Vec<RbcDomain> {
    // Every domain, not the ones reachable from equation families.
    //
    // Collecting by reachability had two consequences, and the artifact was
    // invalid under both. Domains named only from an expression — a
    // `Comprehension`, a `Binder` coordinate — were never emitted, so the
    // artifact referenced domains it did not declare. And the survivors kept
    // their original DAE indices, so the table was sparse where the
    // validator requires `entry at position N` to declare id N. One measured
    // artifact carried 127 domains numbered 3..156.
    //
    // DAE domain ids are dense indices into storage, so emitting all of them
    // keeps every existing reference correct with no remapping: the exported
    // id *is* the DAE index. Unreferenced domains are cheap; a dangling
    // reference is not.
    let mut out = Vec::with_capacity(view.domain_count());
    for (id, domain) in view.domains() {
        out.push(RbcDomain {
            id: DomainId(id.index()),
            binders: domain
                .structured()
                .binders
                .iter()
                .map(|binder| RbcBinder {
                    id: binder.id as u32,
                    display_name: binder.display_name.clone(),
                    lower: binder.lower,
                    upper: binder.upper,
                    step: binder.step,
                })
                .collect(),
            parent: domain.parent().map(|p| DomainId(p.index())),
            extents: domain.extents().to_vec(),
            scalar_count: domain.scalar_count(),
            provenance: ctx.provenance(domain.provenance()),
        });
    }
    out
}

/// Export the array/`for` equations, which `export_equations` does not carry.
///
/// These were omitted entirely, so the artifact described fewer equations than
/// the model has and still validated — `Real x[3]` with a `for` equation
/// exported three unknowns and zero equations. Any consumer reasoning about
/// solvability read an incomplete system with no way to detect it
/// (TOOLBUG-014).
///
/// Kept in the compact form the DAE holds rather than expanded: expanding
/// multiplies the artifact by the array extent and loses the fact that the
/// rows share one source equation. `scalar_rows` is what a balance or matching
/// analysis needs, and it is carried explicitly.
fn export_equation_families(
    view: dae::DaeView<'_>,
    ctx: &mut Ctx<'_>,
    kind: EquationKind,
    options: &ExportOptions,
) -> Vec<RbcEquationFamily> {
    let count = match kind {
        EquationKind::Continuous => view.continuous_family_count(),
        EquationKind::Initial => view.initialization_family_count(),
    };
    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        let family = match kind {
            EquationKind::Continuous => view.continuous_family(index),
            EquationKind::Initial => view.initialization_family(index),
        };
        let Some(family) = family else { continue };
        let extents = view
            .domain(family.domain())
            .map(|domain| domain.extents().to_vec())
            .unwrap_or_default();
        let (reads, reads_derivative, reads_previous) = if options.dependency_edges {
            family_dependencies(view, family).unwrap_or_default()
        } else {
            Default::default()
        };
        out.push(RbcEquationFamily {
            id: FamilyId(index as u32),
            domain: DomainId(family.domain().index()),
            bodies: family.bodies().iter().map(|b| ExprId(b.index())).collect(),
            reads,
            reads_derivative,
            reads_previous,
            scalar_rows: family.scalar_rows(),
            extents,
            scalar_view: match family.scalar_view() {
                rumoca_core::ComprehensionScalarView::BinderSubstitution => {
                    RbcScalarView::BinderSubstitution
                }
                rumoca_core::ComprehensionScalarView::RowMajorProjection => {
                    RbcScalarView::RowMajorProjection
                }
                rumoca_core::ComprehensionScalarView::BinderPrefixProjection { binder_count } => {
                    RbcScalarView::BinderPrefixProjection { binder_count }
                }
            },
            provenance: ctx.provenance(family.provenance()),
        });
    }
    out
}

/// Compute the variables an equation *family* reads, over all of its rows.
///
/// A family body is symbolic: `x[i] = y[i]` mentions `x` and `y` once, and the
/// projection has to be given the domain point to learn which scalar that is.
/// Projecting the body once, without a point, reports only the coordinates of
/// the first row: on `IMC_Transformer` that lost `pin[2]` and `pin[3]` from 38
/// of 76 families, and the missing incidence edges turned into 23 phantom
/// `STRUCTURAL_MATCH_FAILURE`s in a model the compiler had already proved
/// balanced.
///
/// This walks the rows the same way `rumoca-phase-structural` does, because
/// disagreeing with the compiler's own incidence is the bug being fixed.
fn family_dependencies<'dae>(
    view: dae::DaeView<'dae>,
    family: dae::StructuredFamilyView<'dae>,
) -> Option<(Vec<VariableId>, Vec<VariableId>, Vec<VariableId>)> {
    let domain = view.domain(family.domain())?;
    let mut reads = Reads::default();
    let mut cache = rumoca_eval_dae::ScalarCoordinateProjectionCache::default();
    for point in 0..domain.scalar_count() as usize {
        let values = domain.structured().index_tuple_at(point).ok()??;
        for body in family.bodies().iter() {
            let scalar = family.scalar_view().body_scalar(point, domain.extents())?;
            rumoca_eval_dae::for_each_scalar_coordinate_cached(
                view,
                body,
                scalar,
                Some((family.domain(), &values)),
                &mut cache,
                |coordinate, _| reads.visit(coordinate),
            )
            .ok()?;
        }
    }
    Some(reads.take())
}

/// Compute the variables one equation reads, using the compiler's own scalar
/// coordinate projection rather than a private expression walk.
fn dependencies<'dae>(
    view: dae::DaeView<'dae>,
    residual: dae::ExprId<'dae>,
) -> Result<Reads, ExportError> {
    let mut reads = Reads::default();
    rumoca_eval_dae::for_each_scalar_coordinate(view, residual, 0, None, |coordinate, _| {
        reads.visit(coordinate);
    })
    .map_err(|error| ExportError::Projection(error.to_string()))?;
    Ok(reads)
}

/// What one residual reads, split by *how* it reads it.
///
/// Current value, derivative and left limit are three different relationships
/// to a variable, and a consumer that flattens them gets the wrong answer: an
/// equation reading `pre(x)` depends on `x` but cannot determine it.
#[derive(Default)]
struct Reads {
    values: BTreeSet<u32>,
    derivatives: BTreeSet<u32>,
    previous: BTreeSet<u32>,
}

impl Reads {
    fn visit(&mut self, coordinate: dae::CoordinateView<'_>) {
        use dae::CoordinateView as C;
        let (set, id) = match coordinate {
            C::Derivative(id) => (&mut self.derivatives, id.index()),
            C::PreState(id) => (&mut self.previous, id.index()),
            C::PreAlgebraic(id) => (&mut self.previous, id.index()),
            C::PreDiscreteReal(id) => (&mut self.previous, id.index()),
            C::PreDiscreteValue(id) => (&mut self.previous, id.index()),
            other => {
                if let Some(variable) = coordinate_of(other).and_then(RbcCoordinate::variable) {
                    self.values.insert(variable.0);
                }
                return;
            }
        };
        set.insert(id);
    }

    fn take(self) -> (Vec<VariableId>, Vec<VariableId>, Vec<VariableId>) {
        let ids = |set: BTreeSet<u32>| set.into_iter().map(VariableId).collect();
        (ids(self.values), ids(self.derivatives), ids(self.previous))
    }
}

fn export_relations(view: dae::DaeView<'_>, ctx: &mut Ctx<'_>) -> Vec<RbcRelation> {
    (0..view.relation_count())
        .filter_map(|index| {
            let id = view.relation_id(index)?;
            let relation = view.relation(id)?;
            Some(RbcRelation {
                id: RelationId(index as u32),
                expression: ExprId(relation.expression().index()),
                provenance: ctx.provenance(relation.provenance()),
            })
        })
        .collect()
}

fn export_conditions(view: dae::DaeView<'_>, ctx: &mut Ctx<'_>) -> Vec<RbcCondition> {
    use dae::ConditionOperation as Op;
    (0..view.condition_count())
        .filter_map(|index| {
            let id = view.condition_id(index)?;
            let condition = view.condition(id)?;
            let node = match condition.operation() {
                Op::Initial => RbcConditionNode::Initial,
                Op::Always => RbcConditionNode::Always,
                Op::Relation(relation) => RbcConditionNode::Relation {
                    relation: RelationId(relation.index()),
                },
                Op::Discrete(expression) => RbcConditionNode::Discrete {
                    expression: ExprId(expression.index()),
                },
                Op::Clock(clock) => RbcConditionNode::ClockActivation {
                    clock: ClockId(clock.index()),
                },
                Op::Not(operand) => RbcConditionNode::Not {
                    operand: ConditionId(operand.index()),
                },
                Op::And(lhs, rhs) => RbcConditionNode::And {
                    lhs: ConditionId(lhs.index()),
                    rhs: ConditionId(rhs.index()),
                },
                Op::Or(lhs, rhs) => RbcConditionNode::Or {
                    lhs: ConditionId(lhs.index()),
                    rhs: ConditionId(rhs.index()),
                },
                Op::AnyRise(lhs, rhs) => RbcConditionNode::AnyRise {
                    lhs: ConditionId(lhs.index()),
                    rhs: ConditionId(rhs.index()),
                },
            };
            Some(RbcCondition {
                id: ConditionId(index as u32),
                node,
                provenance: ctx.provenance(condition.provenance()),
            })
        })
        .collect()
}

fn export_roots(view: dae::DaeView<'_>, ctx: &mut Ctx<'_>) -> Vec<RbcRoot> {
    (0..view.root_count())
        .filter_map(|index| {
            let id = view.root_id(index)?;
            let root = view.root(id)?;
            Some(RbcRoot {
                id: RootId(index as u32),
                relation: RelationId(root.relation().index()),
                activation: ConditionId(root.activation().index()),
                provenance: ctx.provenance(root.provenance()),
            })
        })
        .collect()
}

fn export_events(view: dae::DaeView<'_>, ctx: &mut Ctx<'_>) -> Vec<RbcEventAction> {
    use dae::EventActionOperation as Op;
    (0..view.event_action_count())
        .filter_map(|index| {
            let id = view.event_action_id(index)?;
            let action = view.event_action(id)?;
            let operation = match action.operation() {
                Op::Reinitialize { state, value } => RbcAction::Reinitialize {
                    state: VariableId(state.index()),
                    value: ExprId(value.index()),
                },
                Op::Assert { message, level } => RbcAction::Assert {
                    message: ExprId(message.index()),
                    level: level.map(|level| ExprId(level.index())),
                },
                Op::Terminate { message } => RbcAction::Terminate {
                    message: ExprId(message.index()),
                },
            };
            Some(RbcEventAction {
                id: EventId(index as u32),
                trigger: ConditionId(action.trigger().index()),
                guard: ConditionId(action.guard().index()),
                action: operation,
                provenance: ctx.provenance(action.provenance()),
            })
        })
        .collect()
}

/// MLS Appendix B.1c: what each discrete-valued variable equals.
///
/// This is a separate arena from the residual equations, and omitting it
/// produced an artifact that declared `discrete_value` variables and never
/// defined them — see docs/bugs/BUG-009.
fn export_discrete_definitions(
    view: dae::DaeView<'_>,
    ctx: &mut Ctx<'_>,
) -> Vec<RbcDiscreteDefinition> {
    (0..view.discrete_value_owner_count())
        .filter_map(|index| {
            let id = view.discrete_value_owner_id(index)?;
            let owner = view.discrete_value_owner(id)?;
            let targets = owner
                .targets()
                .iter()
                .map(|target| VariableId(dae::VariableId::from(target).index()))
                .collect();
            let branches = owner
                .branches()
                .iter()
                .map(|branch| RbcDiscreteBranch {
                    activation: match branch.activation() {
                        dae::DiscreteBranchActivation::Always => RbcDiscreteActivation::Always,
                        dae::DiscreteBranchActivation::When { trigger, guard } => {
                            RbcDiscreteActivation::When {
                                trigger: ConditionId(trigger.index()),
                                guard: ConditionId(guard.index()),
                            }
                        }
                    },
                    values: branch
                        .values()
                        .iter()
                        .map(|(value, _)| ExprId(value.index()))
                        .collect(),
                    provenance: ctx.provenance(branch.provenance()),
                })
                .collect();
            Some(RbcDiscreteDefinition {
                targets,
                branches,
                provenance: ctx.provenance(owner.provenance()),
            })
        })
        .collect()
}

fn export_time_events(
    view: dae::DaeView<'_>,
    ctx: &mut Ctx<'_>,
) -> Result<Vec<RbcTimeEvent>, ExportError> {
    use dae::TimeEventOperation as Op;
    let mut out = Vec::with_capacity(view.time_event_count());
    for index in 0..view.time_event_count() {
        let Some(id) = view.time_event_id(index) else {
            continue;
        };
        let Some(event) = view.time_event(id) else {
            continue;
        };
        let schedule = match event.operation() {
            Op::Static(instant) => {
                // The DAE carries an exact i128 rational. Bitcode v2 declares
                // i64; refuse rather than silently truncate an instant.
                let numerator = i64::try_from(instant.numerator()).map_err(|_| {
                    ExportError::Projection(
                        "time-event instant does not fit bitcode v2's i64 rational".into(),
                    )
                })?;
                let denominator = i64::try_from(instant.denominator()).map_err(|_| {
                    ExportError::Projection(
                        "time-event instant does not fit bitcode v2's i64 rational".into(),
                    )
                })?;
                RbcSchedule::Static {
                    numerator,
                    denominator,
                }
            }
            Op::Dynamic(deadline) => RbcSchedule::Dynamic {
                deadline: ExprId(deadline.index()),
            },
        };
        out.push(RbcTimeEvent {
            id: EventId(index as u32),
            schedule,
            provenance: ctx.provenance(event.provenance()),
        });
    }
    Ok(out)
}

/// Recover `connect(...)` relationships from the Flat model.
///
/// The DAE keeps only a `ConnectionEquation` / `FlowBalanceEquation` tag, so
/// endpoints come from Flat's per-equation `EquationOrigin`. Where the number
/// of Flat connection equations matches the number of DAE connection equations,
/// they are paired in order; otherwise the link is left unset rather than
/// guessed.
fn export_connections(
    flat: Option<&flat::Model>,
    variables: &[RbcVariable],
    equations: &[RbcEquation],
    ctx: &mut Ctx<'_>,
) -> Vec<RbcConnection> {
    let Some(flat) = flat else {
        return Vec::new();
    };
    let by_name: BTreeMap<&str, VariableId> = variables
        .iter()
        .map(|variable| (variable.name.as_str(), variable.id))
        .collect();

    let dae_connection_equations: Vec<EquationId> = equations
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

    let flat_connections: Vec<(&str, &str, Span)> = flat
        .equations
        .iter()
        .filter_map(|equation| match &equation.origin {
            flat::EquationOrigin::Connection { lhs, rhs } => {
                Some((lhs.as_str(), rhs.as_str(), equation.span))
            }
            _ => None,
        })
        .collect();

    let pairable = flat_connections.len() == dae_connection_equations.len();

    // Numbered *after* filtering, not before. A connector endpoint that is not
    // a DAE variable — a clocked signal removed during lowering — drops its
    // entry, and taking the id from the pre-filter index then leaves a hole:
    // `SubSample` exported connections `[0, 2]` and the artifact failed its own
    // validator, which requires `position == id`. 28 of 35 Clocked models were
    // unloadable for this reason.
    //
    // `equation` still pairs by the *original* index, because the DAE's
    // connection equations are in the unfiltered order.
    flat_connections
        .into_iter()
        .enumerate()
        .filter_map(|(index, (lhs, rhs, span))| {
            let left = *by_name.get(lhs)?;
            let right = *by_name.get(rhs)?;
            let quantity = variables
                .get(left.0 as usize)
                .and_then(|variable| variable.connector)
                .map(|connector| connector.quantity)
                .unwrap_or(RbcQuantityKind::Potential);
            Some(RbcConnection {
                // Renumbered below; the original index is kept for pairing.
                id: ConnectionId(index as u32),
                left,
                right,
                quantity,
                left_connector: connector_path(lhs).to_string(),
                right_connector: connector_path(rhs).to_string(),
                equation: pairable
                    .then(|| dae_connection_equations.get(index).copied())
                    .flatten(),
                provenance: RbcProvenance {
                    origin: RbcOrigin::Generated {
                        generation: RbcGeneration::ConnectionEquation,
                    },
                    span: ctx.span(span),
                },
            })
        })
        .enumerate()
        .map(|(position, connection)| RbcConnection {
            id: ConnectionId(position as u32),
            ..connection
        })
        .collect()
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
fn export_connection_sets(
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

    // Flow sums and potential equalities are paired with their DAE equations
    // the same way `export_connections` pairs its own: by position among the
    // generated connection equations, and only when the counts agree.
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
        if matches!(
            equation.origin,
            flat::EquationOrigin::Connection { .. }
                | flat::EquationOrigin::FlowSum { .. }
                | flat::EquationOrigin::UnconnectedFlow { .. }
        ) {
            if let Some(id) = generated.get(flat_generated) {
                equation_at.insert(flat_generated, *id);
            }
            flat_generated += 1;
        }
    }
    let pairable = flat_generated == generated.len();

    // Union-find over connector instances, so `a.p -- b.n` and `b.n -- c.p`
    // become one node rather than two edges.
    let mut parent: BTreeMap<String, String> = BTreeMap::new();
    fn root(parent: &mut BTreeMap<String, String>, of: &str) -> String {
        let mut cursor = of.to_string();
        while let Some(next) = parent.get(&cursor) {
            if next == &cursor {
                break;
            }
            cursor = next.clone();
        }
        cursor
    }
    let join = |parent: &mut BTreeMap<String, String>, left: &str, right: &str| {
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
    };

    struct Pending {
        potentials: Vec<VariableId>,
        potential_equations: Vec<EquationId>,
        connectors: BTreeSet<String>,
        span: rumoca_core::Span,
    }
    let mut pending: BTreeMap<String, Pending> = BTreeMap::new();
    let mut index = 0usize;
    for equation in flat.equations.iter() {
        match &equation.origin {
            flat::EquationOrigin::Connection { lhs, rhs } => {
                join(&mut parent, connector_path(lhs), connector_path(rhs));
                index += 1;
            }
            flat::EquationOrigin::FlowSum { .. } | flat::EquationOrigin::UnconnectedFlow { .. } => {
                index += 1;
            }
            _ => {}
        }
    }
    let _ = index;

    // Second pass, now that the sets are known: attach each equality to the
    // node its endpoints landed in.
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
        let node = root(&mut parent, connector_path(lhs));
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
            if let Some(id) = by_name.get(endpoint) {
                if !entry.potentials.contains(id) {
                    entry.potentials.push(*id);
                }
            }
        }
        if pairable {
            if let Some(id) = equation_at.get(&position) {
                entry.potential_equations.push(*id);
            }
        }
        position += 1;
    }

    // The flow side, accumulated **per node**. A connector may declare more
    // than one flow member --- a MultiBody frame conserves a force and a
    // torque --- and each balance is its own equation but the same node.
    struct Node {
        connectors: BTreeSet<String>,
        balances: Vec<RbcFlowBalance>,
        unconnected: bool,
        span: rumoca_core::Span,
    }
    let mut nodes: BTreeMap<String, Node> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
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
        let key = root(&mut parent, connector_path(&first.variable));
        let node = nodes.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
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
                equation: pairable
                    .then(|| equation_at.get(&position).copied())
                    .flatten(),
                terms,
            });
        }
        position += 1;
    }

    let mut sets: Vec<RbcConnectionSet> = Vec::new();
    let mut claimed: BTreeSet<String> = BTreeSet::new();
    for key in order {
        let Some(node) = nodes.remove(&key) else {
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
            provenance: RbcProvenance {
                origin: RbcOrigin::Generated {
                    generation: RbcGeneration::ConnectionEquation,
                },
                span,
            },
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
            provenance: RbcProvenance {
                origin: RbcOrigin::Generated {
                    generation: RbcGeneration::ConnectionEquation,
                },
                span,
            },
        });
    }
    sets
}

// Passing the assembled `RbcModel` instead would invert the order: the summary
// is computed to put *into* that model.
// SPEC_0021: Exception - the summary counts every table, so its arity is the schema's width.
#[allow(clippy::too_many_arguments)]
fn summarize(
    variables: &[RbcVariable],
    expressions: &[RbcExpr],
    equations: &[RbcEquation],
    initial_equations: &[RbcEquation],
    equation_families: &[RbcEquationFamily],
    domains: &[RbcDomain],
    relations: &[RbcRelation],
    conditions: &[RbcCondition],
    roots: &[RbcRoot],
    events: &[RbcEventAction],
    time_events: &[RbcTimeEvent],
    discrete_definitions: &[RbcDiscreteDefinition],
    connections: &[RbcConnection],
    connection_sets: &[RbcConnectionSet],
    components: &[RbcComponent],
    discrete_real_equations: &[RbcDiscreteRealEquation],
    initial_discrete_values: &[RbcInitialDiscreteValue],
) -> RbcSummary {
    let count = |role: RbcRole| {
        variables
            .iter()
            .filter(|variable| variable.role == role)
            .count() as u32
    };
    RbcSummary {
        discrete_definitions: discrete_definitions.len() as u32,
        discrete_real_equations: discrete_real_equations.len() as u32,
        initial_discrete_values: initial_discrete_values.len() as u32,
        variables: variables.len() as u32,
        states: count(RbcRole::State),
        parameters: count(RbcRole::Parameter),
        constants: count(RbcRole::Constant),
        inputs: count(RbcRole::Input),
        outputs: count(RbcRole::Output),
        algebraics: count(RbcRole::Algebraic),
        discrete_reals: count(RbcRole::DiscreteReal),
        discrete_values: count(RbcRole::DiscreteValue),
        equations: equations.len() as u32,
        initial_equations: initial_equations.len() as u32,
        domains: domains.len() as u32,
        equation_families: equation_families.len() as u32,
        family_scalar_rows: equation_families.iter().map(|f| f.scalar_rows).sum(),
        expressions: expressions.len() as u32,
        relations: relations.len() as u32,
        conditions: conditions.len() as u32,
        clocks: 0,
        clock_ownerships: 0,
        roots: roots.len() as u32,
        events: events.len() as u32,
        time_events: time_events.len() as u32,
        connections: connections.len() as u32,
        connection_sets: connection_sets.len() as u32,
        components: components.len() as u32,
        trace_points: 0,
    }
}
