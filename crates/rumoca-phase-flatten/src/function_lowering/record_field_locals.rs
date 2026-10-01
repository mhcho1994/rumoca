//! Field-wise writes to a record-typed function value under control flow.
//!
//! MLS §12.4.4 lets a function body assign the fields of a record output or
//! protected record one at a time, including inside loops and branches
//! (`for j in 1:n loop r.eta[j] := ...; end for;`). The checked function IR
//! assembles a record value only from straight-line field writes, so such a
//! value whose every field has an entry value is carried as one local per
//! field instead: each local starts from the
//! field's entry value (the value's constructor default, else the field's own
//! declaration equation), the body reads and writes the locals, and the record
//! is assembled from them by its constructor wherever the function returns.
//! This is the record value's semantics written out, not an approximation: a
//! record is exactly the tuple of its fields (MLS §12.6).

use rumoca_core::{
    ComponentRefPart, ComponentReference, Expression, ExpressionRewriter, Reference, Statement,
    StatementRewriter,
};
use rumoca_ir_flat as flat;
use std::collections::HashMap;

/// Localize the fields of every eligible record value in every function.
pub(super) fn localize_record_value_fields(flat: &mut flat::Model) {
    let names: Vec<_> = flat
        .functions
        .iter()
        .filter(|(_, function)| !function.is_constructor && function.external.is_none())
        .map(|(name, _)| name.clone())
        .collect();
    for name in names {
        let candidates = localization_candidates(flat, &flat.functions[&name]);
        let function = flat
            .functions
            .get_mut(&name)
            .expect("function name collected above");
        for candidate in candidates {
            localize_value(function, &candidate);
        }
    }
}

/// One record value to localize, with its constructor layout.
struct Candidate {
    value: String,
    constructor: Reference,
    fields: Vec<rumoca_core::FunctionParam>,
}

fn localization_candidates(flat: &flat::Model, function: &rumoca_core::Function) -> Vec<Candidate> {
    function
        .outputs
        .iter()
        .chain(&function.locals)
        .filter(|value| {
            value.type_class == Some(rumoca_core::ClassType::Record)
                && value.dimensions().is_empty()
                && writes_fields_under_control_flow(&function.body, &value.name, 0)
        })
        .filter_map(|value| {
            let type_def_id = value.type_def_id?;
            let constructor = rumoca_core::resolve_record_constructor(
                flat.functions.values(),
                &value.type_name,
                type_def_id,
            )
            .ok()?;
            let instance_id = constructor.instance_id?;
            let fields = constructor.inputs.to_vec();
            // Every field needs an entry value; a field the body may leave
            // unwritten without one stays with the record-assembly checks,
            // which report it by its field name.
            let entry_values = constructor_default(function, &value.name)
                .is_some_and(|args| args.len() == fields.len())
                || fields.iter().all(|field| field.default.is_some());
            if !entry_values {
                return None;
            }
            let clash = fields.iter().any(|field| {
                let local = local_name(&value.name, &field.name);
                field.def_id.is_none() || declares(function, &local)
            });
            (!clash).then(|| Candidate {
                value: value.name.clone(),
                constructor: Reference::from_var_name(constructor.name.clone())
                    .with_resolved_function(rumoca_core::ResolvedFunctionReference {
                        instance_id,
                        base_part_count: 0,
                        transitively_non_replaceable: false,
                    }),
                fields,
            })
        })
        .collect()
}

fn local_name(value: &str, field: &str) -> String {
    format!("{value}_{field}")
}

fn declares(function: &rumoca_core::Function, name: &str) -> bool {
    function
        .inputs
        .iter()
        .chain(&function.outputs)
        .chain(&function.locals)
        .any(|declared| declared.name == name)
}

fn is_field_target(comp: &ComponentReference, value: &str) -> bool {
    let parts = comp.parts();
    parts.len() >= 2 && parts[0].ident == value && parts[0].subs.is_empty()
}

fn writes_fields_under_control_flow(statements: &[Statement], value: &str, depth: usize) -> bool {
    statements.iter().any(|statement| match statement {
        Statement::Assignment { comp, .. } => depth > 0 && is_field_target(comp, value),
        Statement::For { equations, .. } => {
            writes_fields_under_control_flow(equations, value, depth + 1)
        }
        Statement::While { block, .. } => {
            writes_fields_under_control_flow(&block.stmts, value, depth + 1)
        }
        Statement::If {
            cond_blocks,
            else_block,
            ..
        } => {
            cond_blocks
                .iter()
                .any(|block| writes_fields_under_control_flow(&block.stmts, value, depth + 1))
                || else_block
                    .as_deref()
                    .is_some_and(|stmts| writes_fields_under_control_flow(stmts, value, depth + 1))
        }
        _ => false,
    })
}

/// Rewrite one value's field reads and writes to its field locals and add the
/// field locals plus the constructor assembly at every return. Declines (and
/// leaves the function untouched) when the body uses the record as a whole.
fn localize_value(function: &mut rumoca_core::Function, candidate: &Candidate) {
    let Some(span) = function
        .outputs
        .iter()
        .chain(&function.locals)
        .find(|value| value.name == candidate.value)
        .map(|value| value.span)
    else {
        return;
    };
    let locals: HashMap<String, (String, rumoca_core::DefId)> = candidate
        .fields
        .iter()
        .filter_map(|field| {
            let local = local_name(&candidate.value, &field.name);
            Some((field.name.clone(), (local, field.def_id?)))
        })
        .collect();
    let mut localizer = FieldLocalizer {
        value: &candidate.value,
        locals: &locals,
        assembly: assembly_statement(candidate, &locals, span),
        whole_use: false,
    };
    let mut body = localizer.rewrite_statements(&function.body);
    if localizer.whole_use {
        return;
    }
    body.push(localizer.assembly.clone());
    let entry_default = constructor_default(function, &candidate.value);
    for (index, field) in candidate.fields.iter().enumerate() {
        let mut local = field.clone();
        local.name = locals[&field.name].0.clone();
        let mut renamer = SiblingRenamer { locals: &locals };
        local.default = entry_default
            .as_ref()
            .and_then(|args| args.get(index).cloned())
            .or_else(|| {
                field
                    .default
                    .as_ref()
                    .map(|d| renamer.rewrite_expression(d))
            });
        for subscript in &mut local.shape_expr {
            if let rumoca_core::Subscript::Expr { expr, .. } = subscript {
                **expr = renamer.rewrite_expression(expr);
            }
        }
        function.locals.push(local);
    }
    function.body = body;
}

/// The value's constructor default as positional field values. The value
/// keeps it: it still fixes the shapes of the value's flexible fields.
fn constructor_default(function: &rumoca_core::Function, value: &str) -> Option<Vec<Expression>> {
    let declaration = function
        .outputs
        .iter()
        .chain(&function.locals)
        .find(|declared| declared.name == value)?;
    match declaration.default.as_ref()? {
        Expression::FunctionCall {
            args,
            is_constructor: true,
            ..
        } if !args.is_empty() => Some(args.clone()),
        _ => None,
    }
}

fn local_reference(local: &str, def_id: rumoca_core::DefId, span: rumoca_core::Span) -> Reference {
    let part = ComponentRefPart {
        ident: local.to_string(),
        span,
        subs: Vec::new(),
        def_id,
    };
    let component = ComponentReference::construct(false, span, vec![part])
        .expect("a one-part local reference is well formed");
    Reference::with_component_reference(local, component)
}

fn assembly_statement(
    candidate: &Candidate,
    locals: &HashMap<String, (String, rumoca_core::DefId)>,
    span: rumoca_core::Span,
) -> Statement {
    let args = candidate
        .fields
        .iter()
        .filter_map(|field| {
            let (local, def_id) = locals.get(&field.name)?;
            Some(Expression::VarRef {
                name: local_reference(local, *def_id, span),
                subscripts: Vec::new(),
                span,
            })
        })
        .collect();
    let target = ComponentReference::construct(
        false,
        span,
        vec![ComponentRefPart {
            ident: candidate.value.clone(),
            span,
            subs: Vec::new(),
            def_id: candidate.fields[0].def_id.unwrap_or_default(),
        }],
    )
    .expect("a one-part value reference is well formed");
    Statement::Assignment {
        comp: target,
        value: Expression::FunctionCall {
            name: candidate.constructor.clone(),
            args,
            is_constructor: true,
            span,
        },
        span,
    }
}

struct FieldLocalizer<'a> {
    /// The record value whose `value.field` references move to locals.
    value: &'a str,
    locals: &'a HashMap<String, (String, rumoca_core::DefId)>,
    assembly: Statement,
    whole_use: bool,
}

impl FieldLocalizer<'_> {
    fn localized_parts(&self, parts: &[ComponentRefPart]) -> Option<Vec<ComponentRefPart>> {
        let (local, def_id) = self.locals.get(&parts.get(1)?.ident)?;
        let mut localized = vec![ComponentRefPart {
            ident: local.clone(),
            span: parts[1].span,
            subs: parts[1].subs.clone(),
            def_id: *def_id,
        }];
        localized.extend(parts[2..].iter().cloned());
        Some(localized)
    }

    fn walk_reference_parts(&mut self, reference: &ComponentReference) -> ComponentReference {
        let parts = reference
            .parts()
            .iter()
            .map(|part| self.rewrite_component_ref_part(part))
            .collect();
        reference
            .with_replaced_parts(parts)
            .expect("rewriting subscripts preserves a checked reference")
    }

    fn names_value(&self, parts: &[ComponentRefPart]) -> bool {
        parts.first().is_some_and(|part| part.ident == self.value)
    }
}

impl ExpressionRewriter for FieldLocalizer<'_> {
    fn rewrite_expression(&mut self, expr: &Expression) -> Expression {
        if let Expression::FieldAccess {
            base, field, span, ..
        } = expr
            && let Expression::VarRef {
                name, subscripts, ..
            } = base.as_ref()
            && subscripts.is_empty()
            && name.as_str() == self.value
            && let Some((local, def_id)) = self.locals.get(field)
        {
            return Expression::VarRef {
                name: local_reference(local, *def_id, *span),
                subscripts: Vec::new(),
                span: *span,
            };
        }
        self.walk_expression(expr)
    }

    fn rewrite_var_ref_expression(
        &mut self,
        name: &Reference,
        subscripts: &[rumoca_core::Subscript],
        span: rumoca_core::Span,
    ) -> Expression {
        let Some(component) = name.component_ref() else {
            self.whole_use |= name.as_str().starts_with(self.value);
            return self.walk_var_ref_expression(name, subscripts, span);
        };
        if !self.names_value(component.parts()) {
            return self.walk_var_ref_expression(name, subscripts, span);
        }
        let Some(parts) = self.localized_parts(component.parts()) else {
            self.whole_use = true;
            return self.walk_var_ref_expression(name, subscripts, span);
        };
        let Ok(localized) = component.with_replaced_parts(parts) else {
            self.whole_use = true;
            return self.walk_var_ref_expression(name, subscripts, span);
        };
        let reference =
            name.with_rewritten_component_reference(localized.to_var_name().as_str(), localized);
        self.walk_var_ref_expression(&reference, subscripts, span)
    }
}

impl StatementRewriter for FieldLocalizer<'_> {
    fn rewrite_statements(&mut self, statements: &[Statement]) -> Vec<Statement> {
        let mut rewritten = Vec::with_capacity(statements.len());
        for statement in statements {
            if matches!(statement, Statement::Return { .. }) {
                rewritten.push(self.assembly.clone());
            }
            rewritten.push(self.rewrite_statement(statement));
        }
        rewritten
    }

    fn rewrite_component_reference(
        &mut self,
        reference: &ComponentReference,
    ) -> ComponentReference {
        if self.names_value(reference.parts()) {
            match self
                .localized_parts(reference.parts())
                .and_then(|parts| reference.with_replaced_parts(parts).ok())
            {
                Some(localized) => return self.walk_reference_parts(&localized),
                None => self.whole_use = true,
            }
        }
        self.walk_reference_parts(reference)
    }
}

/// Renames bare sibling field names inside a field declaration.
struct SiblingRenamer<'a> {
    locals: &'a HashMap<String, (String, rumoca_core::DefId)>,
}

impl ExpressionRewriter for SiblingRenamer<'_> {
    fn rewrite_var_ref_expression(
        &mut self,
        name: &Reference,
        subscripts: &[rumoca_core::Subscript],
        span: rumoca_core::Span,
    ) -> Expression {
        if let Some(component) = name.component_ref()
            && let [part] = component.parts()
            && part.subs.is_empty()
            && let Some((local, def_id)) = self.locals.get(&part.ident)
        {
            let reference = local_reference(local, *def_id, span);
            return self.walk_var_ref_expression(&reference, subscripts, span);
        }
        self.walk_var_ref_expression(name, subscripts, span)
    }
}
