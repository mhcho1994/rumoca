//! Operand typing and overload candidates for operator-record lowering.

use super::OperatorLowering;
use crate::ast;
use rumoca_core::{BuiltinFunction, ClassType, DefId, Expression, Literal, OpBinary, OpUnary};

/// The type class an operand needs for MLS §14.5 overload matching.
/// `Unknown` never matches, so an operand the pass cannot type leaves the
/// expression untouched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Record(DefId),
    Integer,
    Real,
    Unknown,
}

impl Kind {
    /// Whether an argument of kind `arg` binds to a parameter of kind
    /// `param` without conversion (Integer widens to Real, MLS §10.6.13).
    pub(super) fn accepts(param: Kind, arg: Kind) -> bool {
        match (param, arg) {
            (Kind::Record(a), Kind::Record(b)) => a == b,
            (Kind::Real, Kind::Real | Kind::Integer) => true,
            (Kind::Integer, Kind::Integer) => true,
            _ => false,
        }
    }

    fn numeric(self) -> bool {
        matches!(self, Kind::Real | Kind::Integer)
    }
}

pub(super) struct OperatorInput {
    pub(super) kind: Kind,
    pub(super) has_default: bool,
}

pub(super) struct OperatorFunction {
    pub(super) def_id: DefId,
    pub(super) inputs: Vec<OperatorInput>,
}

impl OperatorFunction {
    pub(super) fn accepts_arity(&self, arity: usize) -> bool {
        self.inputs.len() >= arity && self.inputs[arity..].iter().all(|input| input.has_default)
    }

    pub(super) fn accepts(&self, args: &[Kind]) -> bool {
        self.accepts_arity(args.len())
            && args
                .iter()
                .zip(&self.inputs)
                .all(|(arg, input)| Kind::accepts(input.kind, *arg))
    }
}

impl OperatorLowering<'_, '_> {
    pub(super) fn operator_record(&self, kind: Kind) -> Option<DefId> {
        let Kind::Record(def_id) = kind else {
            return None;
        };
        self.class_index
            .get(def_id)
            .is_some_and(|class| class.operator_record)
            .then_some(def_id)
    }

    /// The functions of operator `name` (an `operator` holding functions, or
    /// an `operator function`) declared in `record` or inherited by it.
    pub(super) fn operator_functions(&self, record: DefId, name: &str) -> Vec<OperatorFunction> {
        let Some(class) = self.find_member_class(record, name, 0) else {
            return Vec::new();
        };
        let functions: Vec<&ast::ClassDef> = match class.class_type {
            ClassType::Function => vec![class],
            ClassType::Operator => class
                .classes
                .values()
                .filter(|member| member.class_type == ClassType::Function)
                .collect(),
            _ => Vec::new(),
        };
        functions
            .into_iter()
            .filter_map(|function| {
                Some(OperatorFunction {
                    def_id: function.def_id?,
                    inputs: self.function_inputs(function),
                })
            })
            .collect()
    }

    fn find_member_class(&self, owner: DefId, name: &str, depth: usize) -> Option<&ast::ClassDef> {
        let class = self.class_index.get(owner)?;
        if let Some(member) = class.classes.get(name) {
            return Some(member);
        }
        if depth > 16 {
            return None;
        }
        class
            .extends
            .iter()
            .filter_map(|extend| extend.base_def_id)
            .find_map(|base| self.find_member_class(base, name, depth + 1))
    }

    fn function_inputs(&self, function: &ast::ClassDef) -> Vec<OperatorInput> {
        function
            .components
            .values()
            .filter(|component| matches!(component.causality, rumoca_core::Causality::Input(_)))
            .map(|component| OperatorInput {
                kind: self.component_kind(component),
                has_default: component.binding.is_some(),
            })
            .collect()
    }

    fn component_kind(&self, component: &ast::Component) -> Kind {
        if !component.shape.is_empty() || !component.shape_expr.is_empty() {
            return Kind::Unknown;
        }
        if let Some(class) = component
            .type_def_id
            .and_then(|id| self.class_index.get(id))
            && class.class_type == ClassType::Record
        {
            return component.type_def_id.map_or(Kind::Unknown, Kind::Record);
        }
        match component.type_name.name.last().map(|token| &*token.text) {
            Some("Integer") => Kind::Integer,
            Some("Real") => Kind::Real,
            Some("Boolean" | "String") => Kind::Unknown,
            _ if self.is_real_type(component.type_def_id) => Kind::Real,
            _ => Kind::Unknown,
        }
    }

    /// Real, or a type derived from it (`Modelica.Units.SI.Voltage`).
    fn is_real_type(&self, type_def_id: Option<DefId>) -> bool {
        let Some(def_id) = type_def_id else {
            return false;
        };
        match self.class_index.get(def_id) {
            None => self.class_index.local_name(def_id) == Some("Real"),
            Some(class) if class.class_type == ClassType::Type => {
                class
                    .extends
                    .first()
                    .map_or(class.name.text.as_ref() == "Real", |base| {
                        &*base
                            .base_name
                            .name
                            .last()
                            .map_or("".into(), |t| t.text.clone())
                            == "Real"
                            || self.is_real_type(base.base_def_id)
                    })
            }
            Some(_) => false,
        }
    }

    /// The overload-matching kind of a (lowered) Flat expression.
    pub(super) fn kind(&self, expr: &Expression) -> Kind {
        match expr {
            Expression::Literal { value, .. } => match value {
                Literal::Integer(_) => Kind::Integer,
                Literal::Real(_) => Kind::Real,
                _ => Kind::Unknown,
            },
            Expression::VarRef {
                name, subscripts, ..
            } => self.var_kind(name.var_name(), subscripts.len()),
            Expression::Unary {
                op: OpUnary::Minus | OpUnary::Plus | OpUnary::DotMinus | OpUnary::DotPlus,
                rhs,
                ..
            } => Some(self.kind(rhs))
                .filter(|kind| kind.numeric())
                .unwrap_or(Kind::Unknown),
            Expression::Binary { op, lhs, rhs, .. } => self.binary_kind(op, lhs, rhs),
            Expression::FunctionCall { name, .. } => self.call_kind(name),
            Expression::BuiltinCall { function, args, .. } => self.builtin_kind(*function, args),
            Expression::If {
                branches,
                else_branch,
                ..
            } => {
                let kind = self.kind(else_branch);
                let agree = branches.iter().all(|(_, value)| self.kind(value) == kind);
                if agree { kind } else { Kind::Unknown }
            }
            _ => Kind::Unknown,
        }
    }

    fn var_kind(&self, name: &rumoca_core::VarName, subscripts: usize) -> Kind {
        if let Some(record) = self.flat.record_instances.get(name) {
            return if record.dims.len() == subscripts && subscripts == 0 {
                Kind::Record(record.type_def_id)
            } else {
                Kind::Unknown
            };
        }
        let Some(variable) = self.flat.variables.get(name) else {
            return Kind::Unknown;
        };
        if !variable.dims.is_empty() && subscripts != variable.dims.len() {
            return Kind::Unknown;
        }
        let types = self.flat.predefined_types;
        if variable.type_id == types.integer {
            Kind::Integer
        } else if variable.type_id == types.boolean || variable.type_id == types.string {
            Kind::Unknown
        } else {
            Kind::Real
        }
    }

    fn binary_kind(&self, op: &OpBinary, lhs: &Expression, rhs: &Expression) -> Kind {
        let (lhs, rhs) = (self.kind(lhs), self.kind(rhs));
        if !lhs.numeric() || !rhs.numeric() {
            return Kind::Unknown;
        }
        match op {
            OpBinary::Add
            | OpBinary::Sub
            | OpBinary::Mul
            | OpBinary::AddElem
            | OpBinary::SubElem
            | OpBinary::MulElem
                if lhs == Kind::Integer && rhs == Kind::Integer =>
            {
                Kind::Integer
            }
            OpBinary::Add
            | OpBinary::Sub
            | OpBinary::Mul
            | OpBinary::Div
            | OpBinary::Exp
            | OpBinary::AddElem
            | OpBinary::SubElem
            | OpBinary::MulElem
            | OpBinary::DivElem
            | OpBinary::ExpElem => Kind::Real,
            _ => Kind::Unknown,
        }
    }

    fn builtin_kind(&self, function: BuiltinFunction, args: &[Expression]) -> Kind {
        let scalar_args = args.iter().all(|arg| self.kind(arg).numeric());
        match function {
            BuiltinFunction::Der
            | BuiltinFunction::Pre
            | BuiltinFunction::Abs
            | BuiltinFunction::Sqrt
            | BuiltinFunction::Sin
            | BuiltinFunction::Cos
            | BuiltinFunction::Tan
            | BuiltinFunction::Asin
            | BuiltinFunction::Acos
            | BuiltinFunction::Atan
            | BuiltinFunction::Atan2
            | BuiltinFunction::Sinh
            | BuiltinFunction::Cosh
            | BuiltinFunction::Tanh
            | BuiltinFunction::Exp
            | BuiltinFunction::Log
            | BuiltinFunction::Log10
            | BuiltinFunction::Min
            | BuiltinFunction::Max
            | BuiltinFunction::NoEvent
            | BuiltinFunction::Smooth
                if scalar_args =>
            {
                Kind::Real
            }
            _ => Kind::Unknown,
        }
    }

    /// The result kind of a call: a record constructor yields its record,
    /// a function its single output.
    fn call_kind(&self, name: &rumoca_core::Reference) -> Kind {
        if let Some(class) = name.target_def_id().and_then(|id| self.class_index.get(id)) {
            return match class.class_type {
                ClassType::Record => class.def_id.map_or(Kind::Unknown, Kind::Record),
                ClassType::Function => self.function_output_kind(class),
                _ => Kind::Unknown,
            };
        }
        let Some(function) = self.flat.functions.get(name.var_name()) else {
            return Kind::Unknown;
        };
        let [output] = function.outputs.as_slice() else {
            return Kind::Unknown;
        };
        if !output.shape_expr.is_empty() {
            return Kind::Unknown;
        }
        match (
            output.type_class.as_ref(),
            output.type_def_id,
            output.type_name.as_str(),
        ) {
            (Some(ClassType::Record), Some(def_id), _) => Kind::Record(def_id),
            (_, _, "Integer") => Kind::Integer,
            (_, _, "Real") => Kind::Real,
            _ => Kind::Unknown,
        }
    }

    fn function_output_kind(&self, function: &ast::ClassDef) -> Kind {
        let mut outputs = function
            .components
            .values()
            .filter(|component| matches!(component.causality, rumoca_core::Causality::Output(_)));
        match (outputs.next(), outputs.next()) {
            (Some(output), None) => self.component_kind(output),
            _ => Kind::Unknown,
        }
    }
}
