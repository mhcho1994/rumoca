use super::*;
use crate::build::Builder;

/// der(x) = -(x * (2 + 3)), with a dead node left in the arena.
fn model() -> RbcModel {
    let mut b = Builder::new("Folding");
    let x = b.state("x", 1.0);
    let _dead = b.real(42.0);
    let two = b.real(2.0);
    let three = b.real(3.0);
    let sum = b.binary(RbcBinaryOp::Add, two, three);
    let xr = b.state_ref(x);
    let product = b.binary(RbcBinaryOp::Multiply, xr, sum);
    let rhs = b.negate_of(product);
    b.derivative_equation(x, rhs);
    b.finish()
}

fn file(model: RbcModel) -> RbcFile {
    RbcFile {
        execution: None,
        magic: RBC_MAGIC.into(),
        bitcode_version: RBC_VERSION,
        producer: "test".into(),
        model,
    }
}

#[test]
fn dead_expressions_are_dropped_and_references_renumbered() {
    let mut file = file(model());
    let before = file.model.expressions.len();
    let reports = run(&mut file, &["dead-expressions".into()]).expect("pass runs");
    assert_eq!(reports[0].rewrites, 1, "only the unreferenced literal goes");
    assert_eq!(file.model.expressions.len(), before - 1);
    for (index, expression) in file.model.expressions.iter().enumerate() {
        assert_eq!(expression.id.0 as usize, index, "ids stay dense");
    }
}

#[test]
fn literal_operators_fold_and_their_operands_become_dead() {
    let mut file = file(model());
    let reports = run(&mut file, &["fold-constants".into()]).expect("pass runs");
    assert_eq!(reports[0].rewrites, 1, "2 + 3 folds; x * 5 does not");
    assert!(
        file.model
            .expressions
            .iter()
            .any(|e| matches!(e.node, RbcExprNode::Literal { value: RbcLiteral::Real { value } } if value == 5.0)),
        "the sum is now the literal 5"
    );
    assert_eq!(
        reports[0].removed_expressions, 3,
        "the two operands and the dead literal are dropped"
    );
}

#[test]
fn folding_stops_where_the_runtime_would_report() {
    use RbcBinaryOp as Op;
    assert_eq!(
        constants_binary(Op::Divide, 1.0, 0.0),
        None,
        "division by zero is the runtime's to report"
    );
    let mut b = Builder::new("Overflow");
    let x = b.state("x", 1.0);
    let big = b.expr(RbcExprNode::Literal {
        value: RbcLiteral::Integer { value: i64::MAX },
    });
    let one = b.expr(RbcExprNode::Literal {
        value: RbcLiteral::Integer { value: 1 },
    });
    let sum = b.binary(Op::Add, big, one);
    let xr = b.state_ref(x);
    let rhs = b.binary(Op::Multiply, xr, sum);
    b.derivative_equation(x, rhs);
    let mut file = file(b.finish());
    let reports = run(&mut file, &["fold-constants".into()]).expect("pass runs");
    assert_eq!(reports[0].rewrites, 0, "an overflowing sum is left alone");
}

fn constants_binary(op: RbcBinaryOp, a: f64, b: f64) -> Option<f64> {
    let mut builder = Builder::new("One");
    let x = builder.state("x", 1.0);
    let lhs = builder.real(a);
    let rhs = builder.real(b);
    let node = builder.binary(op, lhs, rhs);
    let xr = builder.state_ref(x);
    let use_it = builder.binary(RbcBinaryOp::Add, xr, node);
    builder.derivative_equation(x, use_it);
    let mut model = builder.finish();
    constants::FoldConstants.run(&mut model).ok()?;
    match model.expressions[node.0 as usize].node {
        RbcExprNode::Literal {
            value: RbcLiteral::Real { value },
        } => Some(value),
        _ => None,
    }
}

#[test]
fn an_unknown_pass_is_refused_by_name() {
    let mut file = file(model());
    let error = run(&mut file, &["no-such-pass".into()]).unwrap_err();
    assert!(error.to_string().contains("unknown pass `no-such-pass`"));
}
