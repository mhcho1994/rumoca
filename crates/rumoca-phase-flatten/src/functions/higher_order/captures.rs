use super::*;

pub(super) struct Capture {
    pub(super) slot: usize,
    pub(super) parameter: FunctionParam,
    pub(super) value: Expression,
}

pub(super) struct BoundCallback {
    pub(super) formal: DefId,
    pub(super) target: Function,
    pub(super) reference: Reference,
    pub(super) captures: Vec<Capture>,
    pub(super) span: Span,
}

impl BoundCallback {
    pub(super) fn key(&self) -> CallbackKey {
        CallbackKey {
            formal: self.formal,
            target: self.target.instance_id.unwrap(),
            captures: self
                .captures
                .iter()
                .map(|capture| {
                    self.target.inputs[capture.slot]
                        .def_id
                        .expect("resolved captured formal")
                })
                .collect(),
        }
    }
}

pub(super) fn capture_parameter(
    formal: &FunctionParam,
    input: &FunctionParam,
    next_id: &mut u32,
) -> Result<FunctionParam, FlattenError> {
    let mut parameter = input.clone();
    let id = DefId::new(*next_id);
    *next_id = next_id
        .checked_add(1)
        .ok_or_else(|| FlattenError::internal("generated capture identity overflow"))?;
    parameter.name = format!("$capture{}_{}", formal.def_id.unwrap().index(), id.index());
    parameter.def_id = Some(id);
    parameter.default = None;
    Ok(parameter)
}

fn parameter_reference(parameter: &FunctionParam) -> Result<Expression, FlattenError> {
    let reference = ComponentReference::construct(
        false,
        parameter.span,
        vec![ComponentRefPart {
            ident: parameter.name.clone(),
            span: parameter.span,
            subs: vec![],
            def_id: parameter.def_id.unwrap(),
        }],
    )
    .map_err(|error| FlattenError::internal(error.to_string()))?;
    Ok(Expression::VarRef {
        name: Reference::from_component_reference(reference),
        subscripts: vec![],
        span: parameter.span,
    })
}

pub(super) fn rewrite_capture_metadata(callback: &mut BoundCallback) -> Result<(), FlattenError> {
    let mut values = FxHashMap::default();
    for capture in &callback.captures {
        values.insert(
            callback.target.inputs[capture.slot].def_id.unwrap(),
            parameter_reference(&capture.parameter)?,
        );
    }
    let mut rewriter = CaptureMetadata { values };
    for capture in &mut callback.captures {
        let parameter = &mut capture.parameter;
        parameter.shape_expr = rewriter.rewrite_subscripts(&parameter.shape_expr)?;
        for expression in [&mut parameter.min, &mut parameter.max]
            .into_iter()
            .flatten()
        {
            *expression = rewriter.rewrite_expression(expression)?;
        }
    }
    Ok(())
}

struct CaptureMetadata {
    values: FxHashMap<DefId, Expression>,
}
impl FallibleExpressionRewriter for CaptureMetadata {
    type Error = FlattenError;
    fn rewrite_var_ref_expression(
        &mut self,
        name: &Reference,
        subscripts: &[rumoca_core::Subscript],
        span: Span,
    ) -> Result<Expression, FlattenError> {
        let Some(value) = name
            .target_def_id()
            .and_then(|id| self.values.get(&id))
            .cloned()
        else {
            return self.walk_var_ref_expression(name, subscripts, span);
        };
        if subscripts.is_empty() {
            return Ok(value.with_span(span));
        }
        Ok(Expression::Index {
            base: Box::new(value),
            subscripts: self.rewrite_subscripts(subscripts)?,
            span,
        })
    }
}

pub(super) struct CallbackRewriter {
    callbacks: FxHashMap<DefId, BoundCallback>,
}

impl CallbackRewriter {
    pub(super) fn new(callbacks: Vec<BoundCallback>) -> Result<Self, FlattenError> {
        Ok(Self {
            callbacks: callbacks
                .into_iter()
                .map(|callback| (callback.formal, callback))
                .collect(),
        })
    }

    fn invocation(
        callback: &BoundCallback,
        args: &[Expression],
        span: Span,
    ) -> Result<Expression, FlattenError> {
        // One constructor for every way a callback's argument list can be
        // wrong. Named here so each call site is a single expression.
        let bad = |detail: &'static str| {
            FlattenError::invalid_function_call_args(callback.reference.as_str(), detail, span)
        };
        let unbound_slots = (0..callback.target.inputs.len())
            .filter(|slot| {
                !callback
                    .captures
                    .iter()
                    .any(|capture| capture.slot == *slot)
            })
            .collect::<Vec<_>>();
        let mut supplied = FxHashMap::default();
        let mut positional = 0;
        for argument in args {
            let (slot, value) = if let Some((name, value)) = named_argument_value(argument) {
                let slot = unbound_slots
                    .iter()
                    .copied()
                    .find(|slot| callback.target.inputs[*slot].name == name)
                    .ok_or_else(|| bad("unknown named callback input"))?;
                (slot, value)
            } else {
                let slot = *unbound_slots
                    .get(positional)
                    .ok_or_else(|| bad("too many callback inputs"))?;
                positional += 1;
                (slot, argument)
            };
            if supplied.insert(slot, value).is_some() {
                return Err(bad("duplicate callback input"));
            }
        }
        let mut arguments = Vec::new();
        for slot in 0..callback.target.inputs.len() {
            if let Some(capture) = callback
                .captures
                .iter()
                .find(|capture| capture.slot == slot)
            {
                arguments.push(parameter_reference(&capture.parameter)?);
            } else {
                let value = supplied
                    .get(&slot)
                    .ok_or_else(|| bad("unfilled callback input"))?;
                arguments.push((*value).clone());
            }
        }
        Ok(Expression::FunctionCall {
            name: callback.reference.clone(),
            args: arguments,
            is_constructor: false,
            span,
        })
    }

    fn partial(callback: &BoundCallback) -> Result<Expression, FlattenError> {
        let args = callback
            .captures
            .iter()
            .map(|capture| {
                Ok(Expression::FunctionCall {
                    name: Reference::generated(format!(
                        "{}{}",
                        rumoca_core::NAMED_FUNCTION_ARG_PREFIX,
                        callback.target.inputs[capture.slot].name
                    )),
                    args: vec![parameter_reference(&capture.parameter)?],
                    is_constructor: true,
                    span: callback.span,
                })
            })
            .collect::<Result<_, FlattenError>>()?;
        Ok(Expression::FunctionCall {
            name: callback.reference.clone(),
            args,
            is_constructor: false,
            span: callback.span,
        })
    }
}

impl FallibleExpressionRewriter for CallbackRewriter {
    type Error = FlattenError;
    fn rewrite_expression(&mut self, expression: &Expression) -> Result<Expression, FlattenError> {
        match expression {
            Expression::FunctionCall {
                name,
                args,
                is_constructor: false,
                span,
            } => {
                let args = self.rewrite_expressions(args)?;
                if let Some(callback) = name.target_def_id().and_then(|id| self.callbacks.get(&id))
                {
                    return Self::invocation(callback, &args, *span);
                }
            }
            Expression::VarRef {
                name, subscripts, ..
            } if subscripts.is_empty() => {
                if let Some(callback) = name.target_def_id().and_then(|id| self.callbacks.get(&id))
                {
                    return Self::partial(callback);
                }
            }
            _ => {}
        }
        self.walk_expression(expression)
    }
}
impl FallibleStatementRewriter for CallbackRewriter {}

pub(super) fn rewrite_function(
    function: &mut Function,
    rewriter: &mut impl FallibleStatementRewriter<Error = FlattenError>,
) -> Result<(), FlattenError> {
    for parameter in function
        .inputs
        .iter_mut()
        .chain(&mut function.outputs)
        .chain(&mut function.locals)
    {
        for expression in [
            &mut parameter.default,
            &mut parameter.min,
            &mut parameter.max,
        ]
        .into_iter()
        .flatten()
        {
            *expression = rewriter.rewrite_expression(expression)?;
        }
        parameter.shape_expr = rewriter.rewrite_subscripts(&parameter.shape_expr)?;
    }
    function.body = rewriter.rewrite_statements(&function.body)?;
    Ok(())
}
