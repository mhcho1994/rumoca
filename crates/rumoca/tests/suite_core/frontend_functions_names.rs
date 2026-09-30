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

// TOOLBUG-102: a replaceable package that no modification redeclares denotes
// its declared default (MLS §7.3), so `Medium.nX` in a dimension names the
// default's constant. Post-materialization skipped every scope without
// redeclarations and left the member without identity (EF024).
const DEFAULT_REPLACEABLE_PACKAGE_DIMENSION: &str = r#"
package DefaultReplaceable
  partial package PartialMedium
    constant Integer nX = 2;
  end PartialMedium;
  package Air
    extends PartialMedium(nX = 3);
  end Air;
  model Decl
    replaceable package Medium = PartialMedium;
    parameter Real X_start[Medium.nX] = fill(0.5, Medium.nX);
    Real s(start = 0, fixed = true);
  equation
    der(s) = sum(X_start);
  end Decl;
  model Redeclared
    Decl d(redeclare package Medium = Air);
  end Redeclared;
end DefaultReplaceable;
"#;

fn final_value(compiled: &rumoca::CompilationResult, name: &str) -> f64 {
    let result = simulate_dae_with_diagnostics(
        &compiled.dae,
        &SimOptions {
            t_end: 1.0,
            ..SimOptions::default()
        },
    )
    .expect("the model simulates");
    let column = result
        .names
        .iter()
        .position(|candidate| candidate == name)
        .unwrap_or_else(|| panic!("no result column {name}"));
    *result.data[column].last().unwrap()
}

#[test]
fn unredeclared_replaceable_package_dimension_uses_the_default() {
    let default = compile(
        DEFAULT_REPLACEABLE_PACKAGE_DIMENSION,
        "DefaultReplaceable.Decl",
    );
    let s = final_value(&default, "s");
    assert!(
        (s - 1.0).abs() < 1e-6,
        "default nX = 2 gives s(1) = 1, got {s}"
    );
    let redeclared = compile(
        DEFAULT_REPLACEABLE_PACKAGE_DIMENSION,
        "DefaultReplaceable.Redeclared",
    );
    let s = final_value(&redeclared, "d.s");
    assert!(
        (s - 1.5).abs() < 1e-6,
        "redeclared nX = 3 gives s(1) = 1.5, got {s}"
    );
}

// TOOLBUG-103: a function input dimension written over a field of a package
// record constant (`input Real a[sum(proCoe.nT)]`, IDEAS/AixLib antifreeze
// media) names the field's shared record declaration, which has no value of
// its own; the value belongs to the record constant (EF023).
const RECORD_CONSTANT_FIELD_IN_FUNCTION_SHAPE: &str = r#"
package RecordConstantShape
  record Coef
    parameter Integer n;
    Integer nT[n];
  end Coef;
  package Med
    constant Coef proCoe(n = 2, nT = {2, 1});
    function total
      input Real a[sum(proCoe.nT)];
      output Real f;
    algorithm
      f := sum(a);
    end total;
  end Med;
  model M
    Real s(start = 0, fixed = true);
  equation
    der(s) = Med.total({1, 2, 3});
  end M;
end RecordConstantShape;
"#;

#[test]
fn function_shape_over_a_record_constant_field_reads_the_constant() {
    let compiled = compile(
        RECORD_CONSTANT_FIELD_IN_FUNCTION_SHAPE,
        "RecordConstantShape.M",
    );
    let s = final_value(&compiled, "s");
    assert!(
        (s - 6.0).abs() < 1e-6,
        "sum over 3 coefficients gives s(1) = 6, got {s}"
    );
}

// TOOLBUG-104: a constant whose value is a call (MSL `PartialMedium`
// `T_default = Modelica.Units.Conversions.from_degC(20)`) substituted into a
// binding introduced the call after the call graph was collected, so the
// callee was missing and the DAE reported `ED008 unresolved Flat reference`.
const CALL_INTRODUCED_BY_CONSTANT: &str = r#"
package CallByConstant
  package Conv
    function from_degC
      input Real c;
      output Real k;
    algorithm
      k := c + 273.15;
    end from_degC;
  end Conv;
  package Medium
    constant Real T_default = CallByConstant.Conv.from_degC(20);
  end Medium;
  model M
    parameter Real T_start = Medium.T_default;
    Real s(start = 0, fixed = true);
  equation
    der(s) = T_start;
  end M;
end CallByConstant;
"#;

#[test]
fn call_introduced_by_constant_substitution_is_collected() {
    let compiled = compile(CALL_INTRODUCED_BY_CONSTANT, "CallByConstant.M");
    let s = final_value(&compiled, "s");
    assert!(
        (s - 293.15).abs() < 1e-6,
        "T_start = 293.15 K gives s(1) = 293.15, got {s}"
    );
}
