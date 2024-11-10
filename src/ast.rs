#[derive(Clone, Debug, PartialEq)]
pub struct StoredDefinition {
    pub classes: Vec<ClassDefinition>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClassDefinition {
    pub name: String,
    pub class_type: ClassType,
    pub partial: bool,
    pub components: Vec<ComponentDeclaration>,
    pub equations: Option<Vec<Equation>>,
    pub algorithms: Option<Vec<Statement>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComponentDeclaration {
    pub name: String,
    pub class: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Statement {
    Assignment {
        comp: ComponentReference,
        expr: Box<Expression>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComponentReference {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Equation {
    Simple {
        lhs: Box<Expression>,
        rhs: Box<Expression>,
    },
    If {
        if_cond: Box<Expression>,
        if_eqs: Vec<Equation>,
        else_if_blocks: Vec<EquationBlock>,
        else_eqs: Option<Vec<Equation>>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct EquationBlock {
    pub cond: Box<Expression>,
    pub eqs: Vec<Equation>
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expression {
    UnsignedInteger(i64),
    UnsignedReal(f64),
    Boolean(bool),
    Ref {
        comp: ComponentReference,
    },
    Add {
        lhs: Box<Expression>,
        rhs: Box<Expression>,
    },
    ElemAdd {
        lhs: Box<Expression>,
        rhs: Box<Expression>,
    },
    Sub {
        lhs: Box<Expression>,
        rhs: Box<Expression>,
    },
    ElemSub {
        lhs: Box<Expression>,
        rhs: Box<Expression>,
    },
    Mul {
        lhs: Box<Expression>,
        rhs: Box<Expression>,
    },
    ElemMul {
        lhs: Box<Expression>,
        rhs: Box<Expression>,
    },
    Div {
        lhs: Box<Expression>,
        rhs: Box<Expression>,
    },
    ElemDiv {
        lhs: Box<Expression>,
        rhs: Box<Expression>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClassPrefixes {
    pub class_type: ClassType,
    pub partial: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClassType {
    Model,
    Record,
    OperatorRecord,
    Block,
    ExpandableConnector,
    Connector,
    Type,
    Package,
}
