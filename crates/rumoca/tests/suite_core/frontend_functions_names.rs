//! Function and name-resolution regressions reduced from real libraries
//! (TOOLBUG-100..109). Each model here compiled in OpenModelica but was
//! refused by Rumoca's front end.

use rumoca::Compiler;
use rumoca_sim::{SimOptions, simulate_dae_with_diagnostics};

fn compile(source: &str, model: &str) -> rumoca::CompilationResult {
    Compiler::new()
        .model(model)
        .compile_str(source, &format!("{model}.mo"))
        .unwrap_or_else(|error| panic!("{model} compiles: {error}"))
}

fn compile_error(source: &str, model: &str) -> String {
    match Compiler::new()
        .model(model)
        .compile_str(source, &format!("{model}.mo"))
    {
        Ok(_) => panic!("{model} must be refused"),
        Err(error) => format!("{error:?}"),
    }
}

// TOOLBUG-100: `AssertionLevel.error` inside a function body is the default
// severity (MLS §8.3.7); it resolved at the model level but not in a function.
const FUNCTION_ASSERTION_LEVEL: &str = r#"
model FunctionAssertionLevel
  function f
    input Real w;
    output Real a;
  algorithm
    assert(w <= 2, "too big", AssertionLevel.error);
    a := 2*w;
  end f;
  Real x(start = 0, fixed = true);
equation
  der(x) = f(time);
end FunctionAssertionLevel;
"#;

const FUNCTION_ASSERTION_WARNING: &str = r#"
model FunctionAssertionWarning
  function f
    input Real w;
    output Real a;
  algorithm
    assert(w <= 2, "too big", level = AssertionLevel.warning);
    a := 2*w;
  end f;
  Real x(start = 0, fixed = true);
equation
  der(x) = f(time);
end FunctionAssertionWarning;
"#;

#[test]
fn function_assertion_with_error_level_is_the_default_assertion() {
    let compiled = compile(FUNCTION_ASSERTION_LEVEL, "FunctionAssertionLevel");
    let result = simulate_dae_with_diagnostics(
        &compiled.dae,
        &SimOptions {
            t_end: 1.0,
            ..SimOptions::default()
        },
    )
    .expect("the assertion holds on [0, 1]");
    let x = result.names.iter().position(|name| name == "x").unwrap();
    let last = *result.data[x].last().unwrap();
    assert!(
        (last - 1.0).abs() < 1e-4,
        "der(x) = 2*time gives x(1) = 1, got {last}"
    );
}

#[test]
fn function_assertion_with_warning_level_is_refused_not_promoted() {
    let error = compile_error(FUNCTION_ASSERTION_WARNING, "FunctionAssertionWarning");
    assert!(
        error.contains("non-default assertion level"),
        "a warning-level function assertion must not lower as an error: {error}"
    );
}

// TOOLBUG-101: a call inside an array constructor whose argument reads the
// iterator (MLS §10.4.2) was shape-checked in the enclosing scope, where the
// iterator is not bound, and failed with `unresolved Flat reference i`.
const CALL_IN_COMPREHENSION: &str = r#"
model CallInComprehension
  function g
    input Real a;
    output Real r;
  algorithm
    r := 2*a;
  end g;
  constant Real v[2] = {3, 4};
  parameter Real s = sum({g(v[i]) for i in 1:2});
  Real x(start = 0, fixed = true);
equation
  der(x) = s;
end CallInComprehension;
"#;

#[test]
fn call_reading_a_comprehension_iterator_is_shaped_in_the_iterator_scope() {
    let compiled = compile(CALL_IN_COMPREHENSION, "CallInComprehension");
    let result = simulate_dae_with_diagnostics(
        &compiled.dae,
        &SimOptions {
            t_end: 1.0,
            ..SimOptions::default()
        },
    )
    .expect("the model simulates");
    let x = result.names.iter().position(|name| name == "x").unwrap();
    let last = *result.data[x].last().unwrap();
    assert!(
        (last - 14.0).abs() < 1e-6,
        "s = g(3) + g(4) = 14, got x(1) = {last}"
    );
}
