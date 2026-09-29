//! Parse explicit identities and reconstruct the current public document.
use super::*;

fn parse_generation(word: &str) -> Option<RbcGeneration> {
    use RbcGeneration::*;
    Some(match word {
        "synthetic_residual" => SyntheticResidual,
        "binding_equation" => BindingEquation,
        "connection_equation" => ConnectionEquation,
        "flow_balance_equation" => FlowBalanceEquation,
        "algorithm_equation" => AlgorithmEquation,
        "discrete_update" => DiscreteUpdate,
        "condition_lowering" => ConditionLowering,
        "pre_value_lowering" => PreValueLowering,
        "clock_lowering" => ClockLowering,
        "delay_lowering" => DelayLowering,
        "semi_linear_lowering" => SemiLinearLowering,
        "terminal_lowering" => TerminalLowering,
        "event_action_lowering" => EventActionLowering,
        "initialization_equation" => InitializationEquation,
        "default_start" => DefaultStart,
        "array_equation_projection" => ArrayEquationProjection,
        "record_equation_projection" => RecordEquationProjection,
        "function_loop_lowering" => FunctionLoopLowering,
        "function_condition_lowering" => FunctionConditionLowering,
        "function_aggregate_lowering" => FunctionAggregateLowering,
        "derived_parameter_lowering" => DerivedParameterLowering,
        "index_reduction" => IndexReduction,
        "alias_elimination" => AliasElimination,
        "runtime_discontinuity" => RuntimeDiscontinuity,
        "other" => Other,
        _ => return None,
    })
}

// ── parsing ──────────────────────────────────────────────────────────────────

/// A line split into tokens, with quoted strings kept whole.
///
/// Hand-rolled rather than a lexer crate: the grammar is one statement per
/// line over a fixed keyword set, and the only subtlety is that a quoted
/// string may contain spaces and escapes.
fn tokenize(line: &str, number: usize) -> Result<Vec<String>, TextError> {
    let mut tokens = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(&character) = chars.peek() {
        if character.is_whitespace() {
            chars.next();
            continue;
        }
        if character == ';' {
            break; // comment to end of line
        }
        if character == '"' {
            chars.next();
            let mut value = String::new();
            loop {
                match chars.next() {
                    None => return Err(TextError::at(number, "unterminated string")),
                    Some('"') => break,
                    Some('\\') => match chars.next() {
                        Some('n') => value.push('\n'),
                        Some('r') => value.push('\r'),
                        Some('t') => value.push('\t'),
                        Some('\\') => value.push('\\'),
                        Some('"') => value.push('"'),
                        Some(other) => value.push(other),
                        None => return Err(TextError::at(number, "trailing escape")),
                    },
                    Some(other) => value.push(other),
                }
            }
            tokens.push(format!("\u{0}{value}")); // marked as a string literal
            continue;
        }
        let mut word = String::new();
        while let Some(&next) = chars.peek() {
            if next.is_whitespace() || next == ';' {
                break;
            }
            word.push(next);
            chars.next();
        }
        tokens.push(word);
    }
    Ok(tokens)
}

/// Cursor over one line's tokens. Every accessor reports the line on failure.
struct Cursor<'a> {
    tokens: &'a [String],
    at: usize,
    line: usize,
}

impl<'a> Cursor<'a> {
    fn new(tokens: &'a [String], line: usize) -> Self {
        Self {
            tokens,
            at: 0,
            line,
        }
    }

    fn done(&self) -> bool {
        self.at >= self.tokens.len()
    }

    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.at).map(String::as_str)
    }

    fn next_raw(&mut self) -> Result<&'a str, TextError> {
        let token = self
            .tokens
            .get(self.at)
            .ok_or_else(|| TextError::at(self.line, "unexpected end of line"))?;
        self.at += 1;
        Ok(token.as_str())
    }

    fn word(&mut self) -> Result<&'a str, TextError> {
        let token = self.next_raw()?;
        if let Some(text) = token.strip_prefix('\u{0}') {
            return Err(TextError::at(
                self.line,
                format!("expected a word, got string {text:?}"),
            ));
        }
        Ok(token)
    }

    fn string(&mut self) -> Result<String, TextError> {
        let token = self.next_raw()?;
        token
            .strip_prefix('\u{0}')
            .map(str::to_string)
            .ok_or_else(|| {
                TextError::at(
                    self.line,
                    format!("expected a quoted string, got `{token}`"),
                )
            })
    }

    /// A sigil-prefixed id: `%3`, `^12`, `$0`, `#1`, `!2`.
    fn id(&mut self, sigil: char) -> Result<u32, TextError> {
        let token = self.word()?;
        let rest = token.strip_prefix(sigil).ok_or_else(|| {
            TextError::at(self.line, format!("expected `{sigil}<id>`, got `{token}`"))
        })?;
        rest.parse()
            .map_err(|_| TextError::at(self.line, format!("`{rest}` is not an id")))
    }

    fn number<T: std::str::FromStr>(&mut self) -> Result<T, TextError> {
        let token = self.word()?;
        token
            .parse()
            .map_err(|_| TextError::at(self.line, format!("`{token}` is not a number")))
    }

    fn expect(&mut self, keyword: &str) -> Result<(), TextError> {
        let token = self.word()?;
        if token != keyword {
            return Err(TextError::at(
                self.line,
                format!("expected `{keyword}`, got `{token}`"),
            ));
        }
        Ok(())
    }

    /// Consume `keyword` if it is next. Used for optional clauses.
    fn eat(&mut self, keyword: &str) -> bool {
        if self.peek() == Some(keyword) {
            self.at += 1;
            return true;
        }
        false
    }

    fn provenance(&mut self) -> Result<RbcProvenance, TextError> {
        let marker = self.word()?;
        let origin = match marker {
            "@src" => RbcOrigin::Source,
            "@gen" => {
                let word = self.word()?;
                let generation = parse_generation(word).ok_or_else(|| {
                    TextError::at(self.line, format!("unknown generation `{word}`"))
                })?;
                RbcOrigin::Generated { generation }
            }
            other => {
                return Err(TextError::at(
                    self.line,
                    format!("expected `@src` or `@gen`, got `{other}`"),
                ));
            }
        };
        Ok(RbcProvenance {
            origin,
            span: RbcSpan {
                source: SourceId(self.number()?),
                start: self.number()?,
                end: self.number()?,
                line: self.number()?,
                column: self.number()?,
            },
        })
    }
}

fn parse_scalar(word: &str, line: usize) -> Result<RbcScalar, TextError> {
    Ok(match word {
        "real" => RbcScalar::Real,
        "integer" => RbcScalar::Integer,
        "boolean" => RbcScalar::Boolean,
        "string" => RbcScalar::String,
        "enumeration" => RbcScalar::Enumeration,
        "record" => RbcScalar::Record,
        other => return Err(TextError::at(line, format!("unknown scalar `{other}`"))),
    })
}

fn parse_role(word: &str, line: usize) -> Result<RbcRole, TextError> {
    Ok(match word {
        "parameter" => RbcRole::Parameter,
        "constant" => RbcRole::Constant,
        "input" => RbcRole::Input,
        "state" => RbcRole::State,
        "algebraic" => RbcRole::Algebraic,
        "output" => RbcRole::Output,
        "discrete_real" => RbcRole::DiscreteReal,
        "discrete_value" => RbcRole::DiscreteValue,
        other => return Err(TextError::at(line, format!("unknown role `{other}`"))),
    })
}

fn parse_causality(word: &str, line: usize) -> Result<RbcCausality, TextError> {
    Ok(match word {
        "input" => RbcCausality::Input,
        "output" => RbcCausality::Output,
        "parameter" => RbcCausality::Parameter,
        "calculated_parameter" => RbcCausality::CalculatedParameter,
        "independent" => RbcCausality::Independent,
        "local" => RbcCausality::Local,
        other => return Err(TextError::at(line, format!("unknown causality `{other}`"))),
    })
}

fn parse_quantity(word: &str, line: usize) -> Result<RbcQuantityKind, TextError> {
    Ok(match word {
        "potential" => RbcQuantityKind::Potential,
        "flow" => RbcQuantityKind::Flow,
        "stream" => RbcQuantityKind::Stream,
        other => {
            return Err(TextError::at(
                line,
                format!("unknown quantity kind `{other}`"),
            ));
        }
    })
}

fn parse_unary(word: &str, line: usize) -> Result<RbcUnaryOp, TextError> {
    Ok(match word {
        "negate" => RbcUnaryOp::Negate,
        "not" => RbcUnaryOp::Not,
        "plus" => RbcUnaryOp::Plus,
        other => return Err(TextError::at(line, format!("unknown unary op `{other}`"))),
    })
}

fn parse_binary(word: &str, line: usize) -> Result<RbcBinaryOp, TextError> {
    use RbcBinaryOp::*;
    Ok(match word {
        "add" => Add,
        "sub" => Subtract,
        "mul" => Multiply,
        "div" => Divide,
        "pow" => Power,
        "eq" => Equal,
        "ne" => NotEqual,
        "lt" => Less,
        "le" => LessEqual,
        "gt" => Greater,
        "ge" => GreaterEqual,
        "and" => And,
        "or" => Or,
        "eadd" => ElementwiseAdd,
        "esub" => ElementwiseSubtract,
        "emul" => ElementwiseMultiply,
        "ediv" => ElementwiseDivide,
        "epow" => ElementwisePower,
        other => return Err(TextError::at(line, format!("unknown binary op `{other}`"))),
    })
}

fn parse_real(word: &str, line: usize) -> Result<f64, TextError> {
    match word {
        "nan" => Ok(f64::NAN),
        "inf" => Ok(f64::INFINITY),
        "-inf" => Ok(f64::NEG_INFINITY),
        other => other
            .parse()
            .map_err(|_| TextError::at(line, format!("`{other}` is not a real"))),
    }
}

fn parse_subscripts(cursor: &mut Cursor<'_>) -> Result<Vec<RbcSubscript>, TextError> {
    let count: usize = cursor.number()?;
    let mut subscripts = Vec::with_capacity(count);
    for _ in 0..count {
        let kind = cursor.word()?;
        subscripts.push(match &*kind {
            "at" => RbcSubscript::Index {
                expression: ExprId(cursor.id('^')?),
            },
            "all" => RbcSubscript::Whole,
            "slice" => RbcSubscript::Slice {
                expression: ExprId(cursor.id('^')?),
            },
            other => {
                return Err(TextError::at(
                    cursor.line,
                    format!("unknown subscript kind `{other}`"),
                ));
            }
        });
    }
    Ok(subscripts)
}

fn parse_coordinate(cursor: &mut Cursor<'_>) -> Result<RbcCoordinate, TextError> {
    use RbcCoordinate::*;
    let kind = cursor.word()?;
    if kind == "time" {
        return Ok(Time);
    }
    if kind == "binder" {
        let domain = DomainId(cursor.id('&')?);
        return Ok(Binder {
            domain,
            ordinal: cursor.number()?,
        });
    }
    if kind == "cond" {
        return Ok(Condition {
            condition: ConditionId(cursor.id('?')?),
        });
    }
    if kind == "clockint" {
        return Ok(ClockInterval {
            clock: ClockId(cursor.number()?),
        });
    }
    if kind == "delay" {
        return Ok(Delay {
            delay: DelayId(cursor.number()?),
        });
    }
    if kind == "previous" {
        return Ok(Previous {
            previous: PreviousId(cursor.number()?),
        });
    }
    if kind == "terminal" {
        return Ok(Terminal {
            terminal: TerminalId(cursor.number()?),
        });
    }
    if kind == "fnparam" {
        let function = FunctionId(cursor.id('~')?);
        return Ok(FunctionParameter {
            function,
            ordinal: cursor.number()?,
        });
    }
    let variable = VariableId(cursor.id('%')?);
    Ok(match kind {
        "param" => Parameter { variable },
        "input" => Input { variable },
        "state" => State { variable },
        "der" => Derivative { variable },
        "alg" => Algebraic { variable },
        "dreal" => DiscreteReal { variable },
        "dval" => DiscreteValue { variable },
        "pre_state" => PreState { variable },
        "pre_alg" => PreAlgebraic { variable },
        "pre_dreal" => PreDiscreteReal { variable },
        "pre_dval" => PreDiscreteValue { variable },
        other => {
            return Err(TextError::at(
                cursor.line,
                format!("unknown coordinate `{other}`"),
            ));
        }
    })
}

fn parse_equation(cursor: &mut Cursor<'_>) -> Result<RbcEquation, TextError> {
    let id = EquationId(cursor.number()?);
    let residual = ExprId(cursor.id('^')?);
    let mut reads = Vec::new();
    let mut reads_derivative = Vec::new();
    let mut reads_previous = Vec::new();
    for (keyword, sink) in [
        ("reads", &mut reads),
        ("dreads", &mut reads_derivative),
        ("preads", &mut reads_previous),
    ] {
        if cursor.eat(keyword) {
            while cursor.peek().is_some_and(|t| t.starts_with('%')) {
                sink.push(VariableId(cursor.id('%')?));
            }
        }
    }
    Ok(RbcEquation {
        id,
        residual,
        provenance: cursor.provenance()?,
        reads,
        reads_derivative,
        reads_previous,
    })
}

/// A provenance for an item whose own line has not been read yet.
///
/// Every item that carries one overwrites this before it is stored; it exists
/// because a partially built record has to hold *something*, not because an
/// unknown origin is meaningful.
fn placeholder_provenance() -> RbcProvenance {
    RbcProvenance {
        origin: RbcOrigin::Source,
        span: RbcSpan {
            source: SourceId::PLACEHOLDER,
            start: 0,
            end: 0,
            line: 0,
            column: 0,
        },
    }
}

fn empty_model() -> RbcModel {
    RbcModel {
        connector_types: Vec::new(),
        connectors: Vec::new(),
        connection_sets: Vec::new(),
        functions: Vec::new(),
        discrete_real_equations: Vec::new(),
        initial_discrete_values: Vec::new(),
        name: String::new(),
        domains: Vec::new(),
        equation_families: Vec::new(),
        initial_equation_families: Vec::new(),
        sources: Vec::new(),
        types: Vec::new(),
        variables: Vec::new(),
        expressions: Vec::new(),
        equations: Vec::new(),
        initial_equations: Vec::new(),
        relations: Vec::new(),
        conditions: Vec::new(),
        roots: Vec::new(),
        events: Vec::new(),
        clocks: Vec::new(),
        clock_ownerships: Vec::new(),
        time_events: Vec::new(),
        connections: Vec::new(),
        components: Vec::new(),
        trace_points: Vec::new(),
        discrete_definitions: Vec::new(),
        model_event_transactions: Vec::new(),
        previous_values: Vec::new(),
        terminals: Vec::new(),
        structured_roots: Vec::new(),
        delays: Vec::new(),
        summary: RbcSummary::default(),
    }
}

/// Read the textual IR back into an artifact.
pub fn parse_text(text: &str) -> Result<RbcFile, TextError> {
    let mut file = RbcFile {
        execution: None,
        magic: RBC_MAGIC.to_string(),
        bitcode_version: RBC_VERSION,
        producer: String::new(),
        model: empty_model(),
    };
    let model = &mut file.model;
    // A discrete definition spans several lines, so it is assembled across
    // iterations and closed by its `end` line.
    let mut open_discrete: Option<RbcDiscreteDefinition> = None;

    for (index, raw) in text.lines().enumerate() {
        let number = index + 1;
        let tokens = tokenize(raw, number)?;
        if tokens.is_empty() {
            continue;
        }
        let mut cursor = Cursor::new(&tokens, number);
        let head = cursor.next_raw()?.to_string();

        // Sigil-led lines declare a numbered item; keyword-led lines do not.
        if let Some(rest) = head.strip_prefix('!') {
            let id = SourceId(
                rest.parse()
                    .map_err(|_| TextError::at(number, "bad source id"))?,
            );
            cursor.expect("source")?;
            let name = cursor.string()?;
            let text = if cursor.eat("text") {
                Some(cursor.string()?)
            } else {
                None
            };
            model.sources.push(RbcSource { id, name, text });
        } else if let Some(rest) = head.strip_prefix('$') {
            let id = TypeId(
                rest.parse()
                    .map_err(|_| TextError::at(number, "bad type id"))?,
            );
            cursor.expect("type")?;
            let scalar = parse_scalar(cursor.word()?, number)?;
            let mut dimensions = Vec::new();
            if cursor.eat("dims") {
                for part in cursor.word()?.split(',') {
                    dimensions.push(
                        part.parse().map_err(|_| {
                            TextError::at(number, format!("bad dimension `{part}`"))
                        })?,
                    );
                }
            }
            let record = if cursor.eat("record") {
                let name = cursor.string()?;
                let count: usize = cursor.number()?;
                let mut fields = Vec::with_capacity(count);
                for _ in 0..count {
                    fields.push(RbcRecordField {
                        name: cursor.string()?,
                        value_type: TypeId(cursor.id('$')?),
                    });
                }
                Some(RbcRecord { name, fields })
            } else {
                None
            };
            model.types.push(RbcType {
                id,
                scalar,
                dimensions,
                record,
            });
        } else if let Some(rest) = head.strip_prefix('~') {
            let id = FunctionId(
                rest.parse()
                    .map_err(|_| TextError::at(number, "bad function id"))?,
            );
            cursor.expect("fn")?;
            let name = cursor.string()?;
            cursor.expect("params")?;
            let count: usize = cursor.number()?;
            let mut parameters = Vec::with_capacity(count);
            for _ in 0..count {
                parameters.push(RbcFunctionParameter {
                    name: cursor.string()?,
                    value_type: TypeId(cursor.id('$')?),
                    declaration: None,
                });
            }
            cursor.expect("results")?;
            let count: usize = cursor.number()?;
            let mut results = Vec::with_capacity(count);
            for _ in 0..count {
                results.push(TypeId(cursor.id('$')?));
            }
            cursor.expect("body")?;
            let body = match &*cursor.word()? {
                "elided" => RbcFunctionBody::ElidedModelica,
                "external" => RbcFunctionBody::External {
                    language: cursor.string()?,
                    symbol: cursor.string()?,
                    purity: RbcPurity::Impure,
                    arguments: Vec::new(),
                    result: None,
                    linkage: RbcExternalLinkage::default(),
                },
                other => {
                    return Err(TextError::at(
                        number,
                        format!("unknown function body `{other}`"),
                    ));
                }
            };
            let inline = if cursor.eat("inline") {
                RbcInline::Requested
            } else if cursor.eat("noinline") {
                RbcInline::Never
            } else {
                RbcInline::Unstated
            };
            model.functions.push(RbcFunction {
                // The text profile does not carry bodies, so it carries no
                // folds either; `emit-text` refuses an artifact whose body
                // it cannot represent rather than writing an empty one.
                folds: Vec::new(),
                values: Vec::new(),
                calls: Vec::new(),
                id,
                name,
                parameters,
                results,
                inline,
                body,
                declaration: cursor.provenance()?,
            });
        } else if let Some(rest) = head.strip_prefix('#') {
            let id = ComponentId(
                rest.parse()
                    .map_err(|_| TextError::at(number, "bad component id"))?,
            );
            cursor.expect("comp")?;
            let path = cursor.string()?;
            let class_name = if cursor.eat("of") {
                Some(cursor.string()?)
            } else {
                None
            };
            model.components.push(RbcComponent {
                id,
                path,
                class_name,
            });
        } else if let Some(rest) = head.strip_prefix('%') {
            let id = VariableId(
                rest.parse()
                    .map_err(|_| TextError::at(number, "bad variable id"))?,
            );
            cursor.expect("var")?;
            let name = cursor.string()?;
            let value_type = TypeId(cursor.id('$')?);
            let role = parse_role(cursor.word()?, number)?;
            let causality = parse_causality(cursor.word()?, number)?;
            cursor.expect("scalars")?;
            let scalar_count = cursor.number()?;
            let mut variable = RbcVariable {
                id,
                name,
                role,
                causality,
                value_type,
                scalar_count,
                declaration: placeholder_provenance(),
                component: None,
                unit: None,
                description: None,
                fixed: None,
                start: None,
                min: None,
                max: None,
                nominal: None,
                binding: None,
                connector: None,
                tunable: false,
                from_source: false,
                physical_quantity: None,
                declaring_class: None,
                discrete_input: false,
                contract: None,
            };
            loop {
                let Some(keyword) = cursor.peek() else { break };
                if keyword.starts_with('@') {
                    break;
                }
                let keyword = cursor.word()?;
                match keyword {
                    "comp" => variable.component = Some(ComponentId(cursor.id('#')?)),
                    "unit" => variable.unit = Some(cursor.string()?),
                    "quantity" => variable.physical_quantity = Some(cursor.string()?),
                    "class" => variable.declaring_class = Some(cursor.string()?),
                    "desc" => variable.description = Some(cursor.string()?),
                    "fixed" => variable.fixed = Some(cursor.word()? == "true"),
                    "tunable" => variable.tunable = true,
                    "discrete" => variable.discrete_input = true,
                    "contract" => {
                        let variability = match &*cursor.word()? {
                            "constant" => RbcVariability::Constant,
                            "parameter" => RbcVariability::Parameter,
                            "discrete" => RbcVariability::Discrete,
                            "continuous" => RbcVariability::Continuous,
                            other => {
                                return Err(TextError::at(
                                    number,
                                    format!("unknown variability `{other}`"),
                                ));
                            }
                        };
                        let mut contract = RbcSymbolContract {
                            variability,
                            is_final: false,
                            is_protected: false,
                            evaluate: false,
                            structural: false,
                            effective_value: None,
                            binding_depends_on: Vec::new(),
                            binding_from_modification: false,
                            declared_in: None,
                        };
                        loop {
                            match cursor.peek().map(|t| t.to_string()) {
                                Some(t) if t == "final" => {
                                    cursor.word()?;
                                    contract.is_final = true
                                }
                                Some(t) if t == "protected" => {
                                    cursor.word()?;
                                    contract.is_protected = true
                                }
                                Some(t) if t == "evaluate" => {
                                    cursor.word()?;
                                    contract.evaluate = true
                                }
                                Some(t) if t == "structural" => {
                                    cursor.word()?;
                                    contract.structural = true
                                }
                                Some(t) if t == "frommod" => {
                                    cursor.word()?;
                                    contract.binding_from_modification = true
                                }
                                Some(t) if t == "value" => {
                                    cursor.word()?;
                                    let raw = cursor.word()?;
                                    contract.effective_value = Some(parse_real(raw, number)?)
                                }
                                Some(t) if t == "uses" => {
                                    cursor.word()?;
                                    while cursor.peek().is_some_and(|t| t.starts_with('%')) {
                                        contract
                                            .binding_depends_on
                                            .push(VariableId(cursor.id('%')?));
                                    }
                                }
                                Some(t) if t == "declaredin" => {
                                    cursor.word()?;
                                    contract.declared_in = Some(cursor.string()?)
                                }
                                _ => break,
                            }
                        }
                        variable.contract = Some(contract);
                    }
                    "from_source" => variable.from_source = true,
                    "start" => variable.start = Some(ExprId(cursor.id('^')?)),
                    "min" => variable.min = Some(ExprId(cursor.id('^')?)),
                    "max" => variable.max = Some(ExprId(cursor.id('^')?)),
                    "nominal" => variable.nominal = Some(ExprId(cursor.id('^')?)),
                    "binding" => variable.binding = Some(ExprId(cursor.id('^')?)),
                    "connector" => {
                        let quantity = parse_quantity(cursor.word()?, number)?;
                        variable.connector = Some(RbcConnectorMember {
                            quantity,
                            connected: cursor.eat("connected"),
                        });
                    }
                    other => {
                        return Err(TextError::at(
                            number,
                            format!("unknown variable attribute `{other}`"),
                        ));
                    }
                }
            }
            variable.declaration = cursor.provenance()?;
            model.variables.push(variable);
        } else if let Some(rest) = head.strip_prefix('&') {
            let id = DomainId(
                rest.parse()
                    .map_err(|_| TextError::at(number, "bad domain id"))?,
            );
            cursor.expect("domain")?;
            cursor.expect("scalars")?;
            let scalar_count = cursor.number()?;
            cursor.expect("extents")?;
            let raw = cursor.word()?;
            let extents = if raw == "-" {
                Vec::new()
            } else {
                raw.split(',')
                    .map(|p| {
                        p.parse()
                            .map_err(|_| TextError::at(number, format!("bad extent `{p}`")))
                    })
                    .collect::<Result<Vec<u32>, _>>()?
            };
            let parent = cursor
                .eat("parent")
                .then(|| cursor.id('&'))
                .transpose()?
                .map(DomainId);
            cursor.expect("binders")?;
            let count: usize = cursor.number()?;
            let mut binders = Vec::with_capacity(count);
            for _ in 0..count {
                binders.push(RbcBinder {
                    id: cursor.number()?,
                    display_name: cursor.string()?,
                    lower: cursor.number()?,
                    upper: cursor.number()?,
                    step: cursor.number()?,
                });
            }
            model.domains.push(RbcDomain {
                id,
                binders,
                parent,
                extents,
                scalar_count,
                provenance: cursor.provenance()?,
            });
        } else if let Some(rest) = head.strip_prefix('^') {
            let id = ExprId(
                rest.parse()
                    .map_err(|_| TextError::at(number, "bad expression id"))?,
            );
            cursor.expect("expr")?;
            let value_type = TypeId(cursor.id('$')?);
            let kind = cursor.word()?;
            let node = match kind {
                "lit" => {
                    let which = cursor.word()?;
                    RbcExprNode::Literal {
                        value: match which {
                            "real" => RbcLiteral::Real {
                                value: parse_real(cursor.word()?, number)?,
                            },
                            "integer" => RbcLiteral::Integer {
                                value: cursor.number()?,
                            },
                            "enum" => RbcLiteral::Enumeration {
                                ordinal: cursor.number()?,
                            },
                            "boolean" => RbcLiteral::Boolean {
                                value: cursor.word()? == "true",
                            },
                            "string" => RbcLiteral::String {
                                value: cursor.string()?,
                            },
                            other => {
                                return Err(TextError::at(
                                    number,
                                    format!("unknown literal kind `{other}`"),
                                ));
                            }
                        },
                    }
                }
                "string_conversion" => RbcExprNode::StringConversion {
                    value: ExprId(cursor.id('^')?),
                    format: serde_json::from_str(&cursor.string()?)
                        .map_err(|e| TextError::at(number, e.to_string()))?,
                },
                "coord" => RbcExprNode::Coordinate {
                    coordinate: parse_coordinate(&mut cursor)?,
                },
                "un" => RbcExprNode::Unary {
                    op: parse_unary(cursor.word()?, number)?,
                    operand: ExprId(cursor.id('^')?),
                },
                "bin" => RbcExprNode::Binary {
                    op: parse_binary(cursor.word()?, number)?,
                    lhs: ExprId(cursor.id('^')?),
                    rhs: ExprId(cursor.id('^')?),
                },
                "cond" => {
                    let count: usize = cursor.number()?;
                    let mut branches = Vec::with_capacity(count);
                    for _ in 0..count {
                        branches.push(RbcBranch {
                            condition: ExprId(cursor.id('^')?),
                            value: ExprId(cursor.id('^')?),
                        });
                    }
                    cursor.expect("else")?;
                    RbcExprNode::Conditional {
                        branches,
                        fallback: ExprId(cursor.id('^')?),
                    }
                }
                "call" => {
                    let name = cursor.string()?;
                    let count: usize = cursor.number()?;
                    let mut arguments = Vec::with_capacity(count);
                    for _ in 0..count {
                        arguments.push(ExprId(cursor.id('^')?));
                    }
                    RbcExprNode::Builtin { name, arguments }
                }
                "array" => {
                    let count: usize = cursor.number()?;
                    let empty_type = cursor
                        .eat("of")
                        .then(|| cursor.id('$'))
                        .transpose()?
                        .map(TypeId);
                    let mut elements = Vec::with_capacity(count);
                    for _ in 0..count {
                        elements.push(ExprId(cursor.id('^')?));
                    }
                    RbcExprNode::Array {
                        elements,
                        empty_type,
                    }
                }
                "record" => {
                    let ty = TypeId(cursor.id('$')?);
                    let count: usize = cursor.number()?;
                    let mut fields = Vec::with_capacity(count);
                    for _ in 0..count {
                        fields.push(ExprId(cursor.id('^')?));
                    }
                    RbcExprNode::Record { ty, fields }
                }
                "field" => RbcExprNode::Field {
                    base: ExprId(cursor.id('^')?),
                    field: cursor.number()?,
                },
                "range" => {
                    let start = ExprId(cursor.id('^')?);
                    let step = if cursor.eat("step") {
                        Some(ExprId(cursor.id('^')?))
                    } else {
                        cursor.expect("nostep")?;
                        None
                    };
                    RbcExprNode::Range {
                        start,
                        step,
                        stop: ExprId(cursor.id('^')?),
                    }
                }
                "comp" => RbcExprNode::Comprehension {
                    domain: DomainId(cursor.id('&')?),
                    body: ExprId(cursor.id('^')?),
                },
                "index" => {
                    let base = ExprId(cursor.id('^')?);
                    RbcExprNode::Index {
                        base,
                        subscripts: parse_subscripts(&mut cursor)?,
                    }
                }
                "update" => {
                    let base = ExprId(cursor.id('^')?);
                    let value = ExprId(cursor.id('^')?);
                    RbcExprNode::ArrayUpdate {
                        base,
                        value,
                        subscripts: parse_subscripts(&mut cursor)?,
                    }
                }
                "invoke" => {
                    let function = FunctionId(cursor.id('~')?);
                    cursor.expect("out")?;
                    let output = cursor.number()?;
                    cursor.expect("owner")?;
                    let owner = ExprId(cursor.id('^')?);
                    let count: usize = cursor.number()?;
                    let mut arguments = Vec::with_capacity(count);
                    for _ in 0..count {
                        arguments.push(ExprId(cursor.id('^')?));
                    }
                    RbcExprNode::Call {
                        owner,
                        function,
                        output,
                        arguments,
                    }
                }
                "ctransfer" => {
                    let kind = match &*cursor.word()? {
                        "sub" => RbcClockTransferKind::SubSample {
                            factor: cursor.number()?,
                        },
                        "super" => RbcClockTransferKind::SuperSample {
                            factor: cursor.number()?,
                        },
                        "shift" => RbcClockTransferKind::ShiftSample {
                            counter: cursor.number()?,
                            resolution: cursor.number()?,
                        },
                        "back" => RbcClockTransferKind::BackSample {
                            counter: cursor.number()?,
                            resolution: cursor.number()?,
                        },
                        other => {
                            return Err(TextError::at(
                                number,
                                format!("unknown clock transfer `{other}`"),
                            ));
                        }
                    };
                    let source = ExprId(cursor.id('^')?);
                    cursor.expect("clocks")?;
                    RbcExprNode::ClockTransfer {
                        transfer: kind,
                        source,
                        source_clock: ClockId(cursor.number()?),
                        target_clock: ClockId(cursor.number()?),
                    }
                }
                "unsupported" => RbcExprNode::Unsupported {
                    detail: cursor.string()?,
                },
                other => {
                    return Err(TextError::at(
                        number,
                        format!("unknown expression kind `{other}`"),
                    ));
                }
            };
            model.expressions.push(RbcExpr {
                id,
                value_type,
                node,
                provenance: cursor.provenance()?,
            });
        } else {
            match head.as_str() {
                "rbc" => file.bitcode_version = cursor.number()?,
                "producer" => file.producer = cursor.string()?,
                "model" => model.name = cursor.string()?,
                "eq" => model.equations.push(parse_equation(&mut cursor)?),
                "ieq" => model.initial_equations.push(parse_equation(&mut cursor)?),
                "family" | "ifamily" => {
                    let id = FamilyId(cursor.number()?);
                    cursor.expect("domain")?;
                    let domain = DomainId(cursor.id('&')?);
                    cursor.expect("rows")?;
                    let scalar_rows = cursor.number()?;
                    cursor.expect("extents")?;
                    let raw = cursor.word()?;
                    let extents = if raw == "-" {
                        Vec::new()
                    } else {
                        raw.split(',')
                            .map(|p| {
                                p.parse()
                                    .map_err(|_| TextError::at(number, format!("bad extent `{p}`")))
                            })
                            .collect::<Result<Vec<u32>, _>>()?
                    };
                    cursor.expect("view")?;
                    let scalar_view = match cursor.word()? {
                        "binder" => RbcScalarView::BinderSubstitution,
                        "rowmajor" => RbcScalarView::RowMajorProjection,
                        "prefix" => RbcScalarView::BinderPrefixProjection {
                            binder_count: cursor.number()?,
                        },
                        other => {
                            return Err(TextError::at(
                                number,
                                format!("unknown scalar view `{other}`"),
                            ));
                        }
                    };
                    cursor.expect("bodies")?;
                    let count: usize = cursor.number()?;
                    let mut bodies = Vec::with_capacity(count);
                    for _ in 0..count {
                        bodies.push(ExprId(cursor.id('^')?));
                    }
                    let mut reads = Vec::new();
                    let mut reads_derivative = Vec::new();
                    if cursor.eat("reads") {
                        while cursor.peek().is_some_and(|t| t.starts_with('%')) {
                            reads.push(VariableId(cursor.id('%')?));
                        }
                    }
                    if cursor.eat("dreads") {
                        while cursor.peek().is_some_and(|t| t.starts_with('%')) {
                            reads_derivative.push(VariableId(cursor.id('%')?));
                        }
                    }
                    let mut reads_previous = Vec::new();
                    if cursor.eat("preads") {
                        while cursor.peek().is_some_and(|t| t.starts_with('%')) {
                            reads_previous.push(VariableId(cursor.id('%')?));
                        }
                    }
                    let family = RbcEquationFamily {
                        id,
                        domain,
                        bodies,
                        scalar_rows,
                        extents,
                        scalar_view,
                        reads,
                        reads_derivative,
                        reads_previous,
                        provenance: cursor.provenance()?,
                    };
                    if head == "family" {
                        model.equation_families.push(family);
                    } else {
                        model.initial_equation_families.push(family);
                    }
                }
                "dreq" => {
                    let id = EquationId(cursor.number()?);
                    let residual = ExprId(cursor.id('^')?);
                    let activation = if cursor.eat("always") {
                        RbcDiscreteRealActivation::Always
                    } else {
                        cursor.expect("when")?;
                        let trigger = ConditionId(cursor.id('?')?);
                        cursor.expect("guard")?;
                        RbcDiscreteRealActivation::When {
                            trigger,
                            guard: ConditionId(cursor.id('?')?),
                        }
                    };
                    let mut reads = Vec::new();
                    let mut reads_derivative = Vec::new();
                    if cursor.eat("reads") {
                        while cursor.peek().is_some_and(|t| t.starts_with('%')) {
                            reads.push(VariableId(cursor.id('%')?));
                        }
                    }
                    if cursor.eat("dreads") {
                        while cursor.peek().is_some_and(|t| t.starts_with('%')) {
                            reads_derivative.push(VariableId(cursor.id('%')?));
                        }
                    }
                    let mut reads_previous = Vec::new();
                    if cursor.eat("preads") {
                        while cursor.peek().is_some_and(|t| t.starts_with('%')) {
                            reads_previous.push(VariableId(cursor.id('%')?));
                        }
                    }
                    model.discrete_real_equations.push(RbcDiscreteRealEquation {
                        id,
                        residual,
                        activation,
                        reads,
                        reads_derivative,
                        reads_previous,
                        provenance: cursor.provenance()?,
                    });
                }
                "idval" => model.initial_discrete_values.push(RbcInitialDiscreteValue {
                    target: VariableId(cursor.id('%')?),
                    value: ExprId(cursor.id('^')?),
                    provenance: cursor.provenance()?,
                }),
                "clock_record" => model.clocks.push(
                    serde_json::from_str(&cursor.string()?)
                        .map_err(|e| TextError::at(number, e.to_string()))?,
                ),
                "clock_ownership" => model.clock_ownerships.push(
                    serde_json::from_str(&cursor.string()?)
                        .map_err(|e| TextError::at(number, e.to_string()))?,
                ),
                "rel" => model.relations.push(RbcRelation {
                    id: RelationId(cursor.number()?),
                    expression: ExprId(cursor.id('^')?),
                    provenance: cursor.provenance()?,
                }),
                "cond" => {
                    let id = ConditionId(cursor.number()?);
                    let kind = cursor.word()?;
                    use RbcConditionNode::*;
                    let node = match kind {
                        "initial" => Initial,
                        "always" => Always,
                        "clock_activation" => ClockActivation {
                            clock: ClockId(cursor.number()?),
                        },
                        "rel" => Relation {
                            relation: RelationId(cursor.number()?),
                        },
                        "discrete" => Discrete {
                            expression: ExprId(cursor.id('^')?),
                        },
                        "not" => Not {
                            operand: ConditionId(cursor.number()?),
                        },
                        "and" => And {
                            lhs: ConditionId(cursor.number()?),
                            rhs: ConditionId(cursor.number()?),
                        },
                        "or" => Or {
                            lhs: ConditionId(cursor.number()?),
                            rhs: ConditionId(cursor.number()?),
                        },
                        "any_rise" => AnyRise {
                            lhs: ConditionId(cursor.number()?),
                            rhs: ConditionId(cursor.number()?),
                        },
                        "unsupported" => Unsupported {
                            detail: cursor.string()?,
                        },
                        other => {
                            return Err(TextError::at(
                                number,
                                format!("unknown condition `{other}`"),
                            ));
                        }
                    };
                    model.conditions.push(RbcCondition {
                        id,
                        node,
                        provenance: cursor.provenance()?,
                    });
                }
                "root" => {
                    let id = RootId(cursor.number()?);
                    cursor.expect("rel")?;
                    let relation = RelationId(cursor.number()?);
                    cursor.expect("act")?;
                    let activation = ConditionId(cursor.number()?);
                    model.roots.push(RbcRoot {
                        id,
                        relation,
                        activation,
                        provenance: cursor.provenance()?,
                    });
                }
                "event" => {
                    let id = EventId(cursor.number()?);
                    cursor.expect("trig")?;
                    let trigger = ConditionId(cursor.number()?);
                    cursor.expect("guard")?;
                    let guard = ConditionId(cursor.number()?);
                    let action = match cursor.word()? {
                        "reinit" => RbcAction::Reinitialize {
                            state: VariableId(cursor.id('%')?),
                            value: ExprId(cursor.id('^')?),
                        },
                        "assert" => {
                            let message = ExprId(cursor.id('^')?);
                            let level = if cursor.eat("level") {
                                Some(ExprId(cursor.id('^')?))
                            } else {
                                None
                            };
                            RbcAction::Assert { message, level }
                        }
                        "terminate" => RbcAction::Terminate {
                            message: ExprId(cursor.id('^')?),
                        },
                        other => {
                            return Err(TextError::at(number, format!("unknown action `{other}`")));
                        }
                    };
                    model.events.push(RbcEventAction {
                        id,
                        trigger,
                        guard,
                        action,
                        provenance: cursor.provenance()?,
                    });
                }
                "tevent" => {
                    let id = EventId(cursor.number()?);
                    let schedule = match cursor.word()? {
                        "static" => RbcSchedule::Static {
                            numerator: cursor.number()?,
                            denominator: cursor.number()?,
                        },
                        "dynamic" => RbcSchedule::Dynamic {
                            deadline: ExprId(cursor.id('^')?),
                        },
                        other => {
                            return Err(TextError::at(
                                number,
                                format!("unknown schedule `{other}`"),
                            ));
                        }
                    };
                    model.time_events.push(RbcTimeEvent {
                        id,
                        schedule,
                        provenance: cursor.provenance()?,
                    });
                }
                "conn" => {
                    let id = ConnectionId(cursor.number()?);
                    let left = VariableId(cursor.id('%')?);
                    let right = VariableId(cursor.id('%')?);
                    let quantity = parse_quantity(cursor.word()?, number)?;
                    let left_connector = cursor.string()?;
                    let right_connector = cursor.string()?;
                    let equation = if cursor.eat("eq") {
                        Some(EquationId(cursor.number()?))
                    } else {
                        None
                    };
                    model.connections.push(RbcConnection {
                        id,
                        left,
                        right,
                        quantity,
                        left_connector,
                        right_connector,
                        equation,
                        provenance: cursor.provenance()?,
                    });
                }
                "connset" => {
                    let id = ConnectionSetId(cursor.number()?);
                    let mut connectors = Vec::new();
                    let mut potentials = Vec::new();
                    let mut balances: Vec<RbcFlowBalance> = Vec::new();
                    let mut potential_equations = Vec::new();
                    let mut unconnected = false;
                    // Clauses first, provenance last: it is the one token that
                    // starts with `@`, so the loop stops there rather than
                    // needing to put anything back.
                    while !cursor.peek().is_some_and(|word| word.starts_with('@')) {
                        match cursor.word()? {
                            "at" => connectors.push(cursor.string()?),
                            "pot" => potentials.push(VariableId(cursor.id('%')?)),
                            "balance" => {
                                let equation = if cursor.eat("eq") {
                                    Some(EquationId(cursor.number()?))
                                } else {
                                    None
                                };
                                let mut terms = Vec::new();
                                while !cursor.eat("end") {
                                    let token = cursor.word()?;
                                    let negated = token.starts_with('-');
                                    let digits = token
                                        .trim_start_matches(['+', '-'])
                                        .trim_start_matches('%');
                                    terms.push(RbcFlowTerm {
                                        variable: VariableId(digits.parse().map_err(|_| {
                                            TextError::at(number, "bad flow member")
                                        })?),
                                        negated,
                                    });
                                }
                                balances.push(RbcFlowBalance { equation, terms });
                            }
                            "poteq" => potential_equations.push(EquationId(cursor.number()?)),
                            "unconnected" => unconnected = true,
                            other => {
                                return Err(TextError::at(
                                    number,
                                    format!("unknown connset clause `{other}`"),
                                ));
                            }
                        }
                    }
                    model.connection_sets.push(RbcConnectionSet {
                        id,
                        connectors,
                        potentials,
                        balances,
                        potential_equations,
                        unconnected,
                        provenance: cursor.provenance()?,
                    });
                }
                "disc" => {
                    let count: usize = cursor.number()?;
                    cursor.expect("targets")?;
                    let mut targets = Vec::with_capacity(count);
                    for _ in 0..count {
                        targets.push(VariableId(cursor.id('%')?));
                    }
                    open_discrete = Some(RbcDiscreteDefinition {
                        targets,
                        branches: Vec::new(),
                        provenance: placeholder_provenance(),
                    });
                }
                "branch" => {
                    let definition = open_discrete
                        .as_mut()
                        .ok_or_else(|| TextError::at(number, "`branch` outside a `disc` block"))?;
                    let activation = match cursor.word()? {
                        "always" => RbcDiscreteActivation::Always,
                        "when" => RbcDiscreteActivation::When {
                            trigger: ConditionId(cursor.number()?),
                            guard: ConditionId(cursor.number()?),
                        },
                        other => {
                            return Err(TextError::at(
                                number,
                                format!("unknown activation `{other}`"),
                            ));
                        }
                    };
                    cursor.expect("values")?;
                    let count: usize = cursor.number()?;
                    let mut values = Vec::with_capacity(count);
                    for _ in 0..count {
                        values.push(ExprId(cursor.id('^')?));
                    }
                    definition.branches.push(RbcDiscreteBranch {
                        activation,
                        values,
                        provenance: cursor.provenance()?,
                    });
                }
                "end" => {
                    let mut definition = open_discrete
                        .take()
                        .ok_or_else(|| TextError::at(number, "`end` outside a `disc` block"))?;
                    definition.provenance = cursor.provenance()?;
                    model.discrete_definitions.push(definition);
                }
                "trace" => {
                    let id = TracePointId(cursor.number()?);
                    let variable = VariableId(cursor.id('%')?);
                    let label = cursor.string()?;
                    let mut point = RbcTracePoint {
                        id,
                        variable,
                        label,
                        connection: None,
                        connection_set: None,
                        quantity: None,
                        unit: None,
                        added_by: None,
                    };
                    while !cursor.done() {
                        match cursor.word()? {
                            "conn" => point.connection = Some(ConnectionId(cursor.number()?)),
                            "connset" => {
                                point.connection_set = Some(ConnectionSetId(cursor.number()?))
                            }
                            "kind" => {
                                point.quantity = Some(parse_quantity(cursor.word()?, number)?)
                            }
                            "unit" => point.unit = Some(cursor.string()?),
                            "added_by" => point.added_by = Some(cursor.string()?),
                            other => {
                                return Err(TextError::at(
                                    number,
                                    format!("unknown trace attribute `{other}`"),
                                ));
                            }
                        }
                    }
                    model.trace_points.push(point);
                }
                other => {
                    return Err(TextError::at(
                        number,
                        format!("unknown statement `{other}`"),
                    ));
                }
            }
        }
    }

    if open_discrete.is_some() {
        return Err(TextError::at(
            text.lines().count(),
            "a `disc` block was never closed by `end`",
        ));
    }

    // The summary is derived, never written: two sources of truth for a count
    // is one source of truth and one way to be wrong.
    crate::validate::recompute_summary(&mut file.model);
    Ok(file)
}
