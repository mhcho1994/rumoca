#[derive(Clone, Debug, PartialEq)]
pub struct StoredDefinition {
    pub classes: Vec<ClassDefinition>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClassDefinition {
    pub name: String,
    pub components: Vec<ComponentDeclaration>,
    pub equations: Option<Vec<Equation>>
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComponentDeclaration {
    pub name: String,
    pub class: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Statement {

    Variable {
        name: String,
        value: Box<Expression>,
    },
    Print {
        value: Box<Expression>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Equation {
    Simple {
        lhs: Box<Expression>,
        rhs: Box<Expression>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expression {
    Integer(i64),
    Variable(String),
    BinaryOperation {
        lhs: Box<Expression>,
        operator: Operator,
        rhs: Box<Expression>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Operator {
    Add,
    Sub,
    Mul,
    Div,
}


// #[derive(Clone, Debug, PartialEq)]
// pub enum ClassType {
//     Model,
//     Record,
//     OperatorRecord,
//     Block,
//     ExpandableConnector,
//     Connector,
//     Type,
//     Package,
// }
