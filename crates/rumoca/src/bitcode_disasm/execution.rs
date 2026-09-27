//! Readable listing of the authored host program.
//!
//! `disasm` is a lossy human listing, not a wire form, so printing the
//! program here does not create a third representation of it: an artifact
//! with an execution section cannot be re-assembled from text at all (see
//! `emit-text`), and this listing does not pretend otherwise.
use rumoca_ir_solve::execution::{
    ExecutionArtifact, Function, Instruction, ProgramExpr, ProgramExprId,
};

pub(super) fn print(execution: &ExecutionArtifact) {
    println!();
    println!(
        "execution v{} ({:?}, revision {})",
        execution.version, execution.lowering, execution.revision
    );
    println!("  digest     {}", execution.dependency_digest);
    for record in &execution.passes {
        println!("  pass       {} v{}", record.id, record.version);
    }
    for sink in &execution.program.sinks {
        let columns: Vec<String> = sink
            .columns
            .iter()
            .map(|column| format!("{}:{:?}", column.name, column.ty))
            .collect();
        println!(
            "  sink       {} -> {} [{}]",
            sink.key,
            sink.filename,
            columns.join(", ")
        );
    }
    for (name, function) in &execution.program.functions {
        let locals: Vec<String> = function
            .locals
            .iter()
            .map(|local| format!("{}:{:?}", local.name, local.ty))
            .collect();
        println!();
        println!("  fn {name}({})", locals.join(", "));
        body(function, &function.body, 4);
    }
}

fn body(function: &Function, instructions: &[Instruction], indent: usize) {
    let pad = " ".repeat(indent);
    for instruction in instructions {
        match instruction {
            Instruction::Time { result } => println!("{pad}{result} = snapshot.time"),
            Instruction::Sequence { result } => println!("{pad}{result} = snapshot.sequence"),
            Instruction::Phase { result } => println!("{pad}{result} = snapshot.phase"),
            Instruction::Value {
                result,
                trace_point,
            } => println!("{pad}{result} = snapshot.value #{trace_point}"),
            Instruction::Compute { result, expr } => {
                println!("{pad}{result} = {}", expression(function, *expr));
            }
            Instruction::Open { sink } => println!("{pad}csv.open {sink}"),
            Instruction::Close { sink } => println!("{pad}csv.close {sink}"),
            Instruction::Write { sink, values } => {
                println!("{pad}csv.write_row {sink} ({})", values.join(", "));
            }
            Instruction::Call { function: callee } => println!("{pad}call {callee}"),
            Instruction::Assert { condition, message } => {
                println!("{pad}assert {condition} else {message}");
            }
            Instruction::If {
                condition,
                then_body,
                else_body,
            } => {
                println!("{pad}if {condition} {{");
                body(function, then_body, indent + 2);
                if !else_body.is_empty() {
                    println!("{pad}}} else {{");
                    body(function, else_body, indent + 2);
                }
                println!("{pad}}}");
            }
        }
    }
}

fn expression(function: &Function, id: ProgramExprId) -> String {
    let Some(node) = function.expressions.get(id.0 as usize) else {
        return format!("<invalid expr {}>", id.0);
    };
    match node {
        ProgramExpr::Local { name } => name.clone(),
        ProgramExpr::Real { value } => format!("{value}"),
        ProgramExpr::Integer { value } => format!("{value}"),
        ProgramExpr::Boolean { value } => format!("{value}"),
        ProgramExpr::Text { value } => format!("{value:?}"),
        ProgramExpr::Unary { op, operand } => {
            format!("{op:?}({})", expression(function, *operand))
        }
        ProgramExpr::Binary { op, lhs, rhs } => format!(
            "({} {op:?} {})",
            expression(function, *lhs),
            expression(function, *rhs)
        ),
        ProgramExpr::Compare { op, lhs, rhs } => format!(
            "({} {op:?} {})",
            expression(function, *lhs),
            expression(function, *rhs)
        ),
    }
}
