//use lalrpop_util::lalrpop_mod;

mod ast;
mod lexer;
mod tokens;

use lexer::Lexer;
use parser::StoredDefinitionParser;

use lalrpop_util::lalrpop_mod;

lalrpop_mod!(
    #[allow(clippy::ptr_arg)]
    #[rustfmt::skip]
    parser
);

fn main() {
    let source_code = std::fs::read_to_string("src/model.mo").unwrap();
    let lexer = Lexer::new(&source_code);
    let parser = StoredDefinitionParser::new();
    let ast = parser.parse(lexer).expect("failed to parse");
    println!("{:#?}", ast);
}
