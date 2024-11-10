use logos::Logos;
use std::fmt; // to implement the Display trait later
use std::num::ParseIntError;

#[derive(Default, Debug, Clone, PartialEq)]
pub enum LexicalError {
    InvalidInteger(ParseIntError),
    #[default]
    InvalidToken,
}

impl From<ParseIntError> for LexicalError {
    fn from(err: ParseIntError) -> Self {
        LexicalError::InvalidInteger(err)
    }
}

#[derive(Logos, Clone, Debug, PartialEq)]
#[logos(skip r"[ \t\n\f]+", skip r"#.*\n?", error = LexicalError)]
pub enum Token {
    #[token("algorithm")]
    KeywordAlgorithm,
    #[token("and")]
    KeywordAnd,
    #[token("annotation")]
    KeywordAnnotation,
    #[token("block")]
    KeywordBlock,
    #[token("break")]
    KeywordBreak,
    #[token("class")]
    KeywordClass,
    #[token("connect")]
    KeywordConnect,
    #[token("constant")]
    KeywordConstant,
    #[token("constrainedby")]
    KeywordConstrainedby,
    #[token("der")]
    KeywordDer,
    #[token("discrete")]
    KeywordDiscrete,
    #[token("each")]
    KeywordEach,
    #[token("else")]
    KeywordElse,
    #[token("elseif")]
    KeywordElseif,
    #[token("elsewhen")]
    KeywordElsewhen,
    #[token("encapsulated")]
    KeywordEncapsulated,
    #[token("end")]
    KeywordEnd,
    #[token("enumeration")]
    KeywordEnumeration,
    #[token("equation")]
    KeywordEquation,
    #[token("expandable")]
    KeywordExpandable,
    #[token("extends")]
    KeywordExtends,
    #[token("external")]
    KeywordExternal,
    #[token("false")]
    KeywordFinal,
    #[token("final")]
    KeywordFlow,
    #[token("flow")]
    KeywordFor,
    #[token("for")]
    KeywordFunction,
    #[token("function")]
    KeywordFalse,
    #[token("print")]
    KeywordPrint,

    #[regex("[_a-zA-Z][_0-9a-zA-Z]*", |lex| lex.slice().to_string())]
    Identifier(String),
    #[regex("[1-9][0-9]*", |lex| lex.slice().parse())]
    Integer(i64),

    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("=")]
    Assign,
    #[token(";")]
    Semicolon,

    #[token("+")]
    OperatorAdd,
    #[token("-")]
    OperatorSub,
    #[token("*")]
    OperatorMul,
    #[token("/")]
    OperatorDiv,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
