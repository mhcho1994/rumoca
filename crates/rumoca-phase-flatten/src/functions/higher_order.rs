//! MLS §12.4.2.1: specialize statically selected functional inputs and lift
//! captures into ordinary call slots before the checked DAE value boundary.

use rumoca_core::{
    ClassType, ComponentRefPart, ComponentReference, DefId, Expression, FallibleExpressionRewriter,
    FallibleStatementRewriter, Function, FunctionInstanceId, FunctionParam, Reference,
    ResolvedFunctionReference, Span, VarName,
};
use rumoca_ir_ast as ast;
use rumoca_ir_flat as flat;
use rustc_hash::FxHashMap;

use super::call_args::{named_argument_value, rewrite_model_expressions};
use crate::FlattenError;

mod captures;
use captures::{
    BoundCallback, CallbackRewriter, Capture, capture_parameter, rewrite_capture_metadata,
    rewrite_function,
};

#[derive(Clone, PartialEq, Eq, Hash)]
struct CallbackKey {
    formal: DefId,
    target: FunctionInstanceId,
    captures: Vec<DefId>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct SpecializationKey {
    function: FunctionInstanceId,
    callbacks: Vec<CallbackKey>,
}

pub(crate) fn specialize_function_inputs(
    model: &mut flat::Model,
    tree: &ast::ClassTree,
) -> Result<(), FlattenError> {
    let mut catalog = flat::Model::new();
    catalog.functions = model.functions.clone();
    let original_count = catalog.functions.len();
    let mut specializer = Specializer {
        catalog,
        cache: FxHashMap::default(),
        next_capture_id: tree.def_map.keys().map(|id| id.index()).max().unwrap_or(0) + 1,
    };
    rewrite_model_expressions(model, &mut specializer)?;
    for function in specializer
        .catalog
        .functions
        .into_values()
        .skip(original_count)
    {
        model.add_function(function);
    }
    Ok(())
}

struct Specializer {
    catalog: flat::Model,
    cache: FxHashMap<SpecializationKey, Reference>,
    // These are generated instance DefIds, never source declaration ids.
    // Allocation begins beyond Resolve's complete declaration namespace.
    next_capture_id: u32,
}

impl Specializer {
    fn function(&self, reference: &Reference) -> Option<&Function> {
        self.catalog
            .get_function_instance(reference.resolved_function()?.instance_id)
    }

    fn callback(
        &mut self,
        formal: &FunctionParam,
        value: &Expression,
    ) -> Result<Option<BoundCallback>, FlattenError> {
        let (name, arguments, span) = match value {
            Expression::FunctionCall {
                name,
                args,
                is_constructor: false,
                span,
            } => (name, args.as_slice(), *span),
            Expression::VarRef {
                name,
                subscripts,
                span,
            } if subscripts.is_empty() => (name, &[][..], *span),
            _ => return Ok(None),
        };
        let Some(target) = self.function(name).cloned() else {
            return Ok(None);
        };
        let mut captures = Vec::new();
        for argument in arguments {
            let Some((input_name, value)) = named_argument_value(argument) else {
                return Ok(None);
            };
            let Some((slot, input)) = target
                .inputs
                .iter()
                .enumerate()
                .find(|(_, input)| input.name == input_name)
            else {
                return Ok(None);
            };
            let parameter = capture_parameter(formal, input, &mut self.next_capture_id)?;
            captures.push(Capture {
                slot,
                parameter,
                value: value.clone(),
            });
        }
        captures.sort_by_key(|capture| capture.slot);
        Ok(Some(BoundCallback {
            formal: formal
                .def_id
                .ok_or_else(|| FlattenError::internal("functional input identity"))?,
            target,
            reference: name.clone(),
            captures,
            span,
        }))
    }

    fn specialize_call(
        &mut self,
        name: &Reference,
        function: &Function,
        args: &[Expression],
        span: Span,
    ) -> Result<Option<Expression>, FlattenError> {
        if args.len() != function.inputs.len() {
            return Ok(None);
        }
        let mut callbacks = Vec::new();
        let mut values = Vec::new();
        for (input, value) in function.inputs.iter().zip(args) {
            if input.type_class != Some(ClassType::Function) {
                values.push(self.rewrite_expression(value)?);
                continue;
            }
            // A function input this pass cannot bind leaves the call alone.
            match self.callback(input, value)? {
                Some(callback) => callbacks.push(callback),
                None => return Ok(None),
            }
        }
        if callbacks.is_empty() {
            return Ok(None);
        }
        for callback in &callbacks {
            for capture in &callback.captures {
                values.push(self.rewrite_expression(&capture.value)?);
            }
        }
        let key = SpecializationKey {
            function: function
                .instance_id
                .expect("catalog function has instance identity"),
            callbacks: callbacks.iter().map(BoundCallback::key).collect(),
        };
        let reference = if let Some(reference) = self.cache.get(&key) {
            reference.clone()
        } else {
            self.construct_specialization(name, function, callbacks, key)?
        };
        Ok(Some(Expression::FunctionCall {
            name: reference,
            args: values,
            is_constructor: false,
            span,
        }))
    }

    fn construct_specialization(
        &mut self,
        name: &Reference,
        function: &Function,
        mut callbacks: Vec<BoundCallback>,
        key: SpecializationKey,
    ) -> Result<Reference, FlattenError> {
        for callback in &mut callbacks {
            rewrite_capture_metadata(callback)?;
        }
        let mut specialized = function.clone();
        specialized.name = VarName::new(format!(
            "{}.$specialization{}",
            function.name,
            self.catalog.functions.len()
        ));
        specialized
            .inputs
            .retain(|input| input.type_class != Some(ClassType::Function));
        for callback in &callbacks {
            specialized.inputs.extend(
                callback
                    .captures
                    .iter()
                    .map(|capture| capture.parameter.clone()),
            );
        }
        let mut binder = CallbackRewriter::new(callbacks)?;
        rewrite_function(&mut specialized, &mut binder)?;
        let specialized_name = specialized.name.clone();
        self.catalog.add_function(specialized.clone());
        let instance_id = self.catalog.functions[&specialized_name]
            .instance_id
            .unwrap();
        let reference = name
            .clone()
            .with_var_name(specialized_name.clone())
            .with_resolved_function(ResolvedFunctionReference {
                instance_id,
                base_part_count: name.resolved_function().unwrap().base_part_count,
                transitively_non_replaceable: function.transitively_non_replaceable,
            });
        self.cache.insert(key, reference.clone());
        rewrite_function(&mut specialized, self)?;
        specialized.instance_id = Some(instance_id);
        self.catalog.functions.insert(specialized_name, specialized);
        Ok(reference)
    }
}

impl FallibleExpressionRewriter for Specializer {
    type Error = FlattenError;
    fn rewrite_expression(&mut self, expression: &Expression) -> Result<Expression, FlattenError> {
        if let Expression::FunctionCall {
            name,
            args,
            is_constructor: false,
            span,
        } = expression
            && let Some(function) = self.function(name).cloned()
            && function
                .inputs
                .iter()
                .any(|input| input.type_class == Some(ClassType::Function))
            && let Some(specialized) = self.specialize_call(name, &function, args, *span)?
        {
            return Ok(specialized);
        }
        self.walk_expression(expression)
    }
}
impl FallibleStatementRewriter for Specializer {}
