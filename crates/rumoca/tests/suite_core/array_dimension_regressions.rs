//! Regressions for library models whose array dimensions or structural
//! parameters Rumoca could not evaluate although the model is valid
//! (TOOLBUG-165..179). Each fixture is the minimal form of the library
//! construct; tests compile through the CLI and, where the value matters,
//! simulate and read the result.

use std::fs;
use std::process::{Command, Output};

use tempfile::tempdir;

fn run(fixture: &str, model: &str, extra: &[&str], simulate: bool) -> (Output, Option<String>) {
    let work = tempdir().expect("temp dir");
    let source = work.path().join("Fixture.mo");
    fs::write(&source, fixture).expect("fixture written");
    let csv = work.path().join("result.csv");
    let mut command = Command::new(env!("CARGO_BIN_EXE_rumoca"));
    if simulate {
        command.args([
            "sim",
            source.to_str().expect("utf-8 path"),
            "-m",
            model,
            "--t-end",
            "1",
            "-o",
            csv.to_str().expect("utf-8 path"),
        ]);
    } else {
        command.args([
            "compile",
            source.to_str().expect("utf-8 path"),
            "--model",
            model,
        ]);
    }
    let output = command.args(extra).output().expect("rumoca runs");
    let result = fs::read_to_string(&csv).ok();
    (output, result)
}

fn assert_compiles(fixture: &str, model: &str) {
    let (output, _) = run(fixture, model, &["--emit", "dae-json"], false);
    assert!(
        output.status.success(),
        "{model} must compile: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Simulate and return the first data row's value of column `column`.
fn initial_value(fixture: &str, model: &str, column: &str) -> f64 {
    let (output, csv) = run(fixture, model, &[], true);
    assert!(
        output.status.success(),
        "{model} must simulate: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let csv = csv.expect("result csv written");
    let mut lines = csv.lines();
    let header: Vec<&str> = lines.next().expect("header").split(',').collect();
    let row: Vec<&str> = lines.next().expect("first row").split(',').collect();
    let index = header
        .iter()
        .position(|name| *name == column)
        .unwrap_or_else(|| panic!("column {column} in {header:?}"));
    row[index].parse().expect("numeric value")
}

/// TOOLBUG-165: a record parameter bound to a function call whose record
/// output fixes a flexible field through its declaration modifier; the body
/// writes the fields inside loops and branches (Buildings/IBPSA/IDEAS/AixLib
/// `Movers.Data.Generic.motorEfficiency_yMot_generic` via
/// `motorEfficiencyCurve`).
#[test]
fn record_field_extent_comes_from_function_output_modifier() {
    const FIXTURE: &str = "\
package D
  record R
    parameter Real y[:];
    parameter Real eta[size(y, 1)];
  end R;
  function f
    input Real p;
    output R res(y={0, 0.5, 1}, eta=zeros(n));
  protected
    constant Integer n = 3;
  algorithm
    if p > 1 then
      for j in 1:n loop
        res.eta[j] := p*j;
      end for;
    else
      res.eta := fill(7, n);
    end if;
    res.eta[1] := -1;
  end f;
  model M
    parameter Real p = 0.5;
    final parameter R r = D.f(p);
    final parameter R r2 = D.f(2);
    Real x;
  equation
    x = r.eta[1] + 10*r.eta[2] + 100*r.y[2] + 1000*r2.eta[2];
  end M;
end D;
";
    // r.eta = {-1, 7, 7}, r.y[2] = 0.5, r2.eta = {-1, 4, 6}.
    let x = initial_value(FIXTURE, "D.M", "x");
    assert!((x - (-1.0 + 70.0 + 50.0 + 4000.0)).abs() < 1e-9, "x = {x}");
}

/// TOOLBUG-165: a record input whose later field's extent reads an earlier
/// field (`Real P[size(V_flow, 1)]`), decomposed into scalar-ABI inputs
/// (IDEAS `Movers.BaseClasses.Characteristics.flowParameters`).
#[test]
fn decomposed_record_input_extent_reads_sibling_field() {
    const FIXTURE: &str = "\
package B
  record R
    parameter Real y[:];
    parameter Real eta[size(y, 1)];
  end R;
  function g
    input R r;
    input Real u;
    output Real s;
  algorithm
    s := u*sum(r.eta) + size(r.y, 1);
  end g;
  model M
    parameter R r(y={1,2,3}, eta={4,5,6});
    Real x;
  equation
    x = g(r, 1);
  end M;
end B;
";
    let x = initial_value(FIXTURE, "B.M", "x");
    assert!((x - 18.0).abs() < 1e-9, "x = {x}");
}

/// TOOLBUG-166: extents instantiation recorded for a literal-size array must
/// stay readable after constant collection clears a redeclared component's
/// alias scope (IDEAS `Movers.Validation.Pump_stratos`:
/// `powEu(V_flow = powEu_internal.V_flow)` inside a redeclared mover).
#[test]
fn literal_extents_survive_component_redeclaration() {
    const FIXTURE: &str = "\
package H
  record PW
    parameter Real V_flow[3];
  end PW;
  record PP
    parameter Real V_flow[:];
    parameter Real P[size(V_flow, 1)];
  end PP;
  partial model Iface
    parameter Real k = 1;
  end Iface;
  model Impl
    extends Iface;
    final parameter PW inter(V_flow = {k, 2*k, 3*k});
    final parameter PP powEu(V_flow = inter.V_flow, P = 2*inter.V_flow);
    Real x;
  equation
    x = powEu.P[3];
  end Impl;
  model Base
    replaceable Iface floMacSta;
  end Base;
  model M
    extends Base(redeclare Impl floMacSta(k = 2));
  end M;
end H;
";
    assert_compiles(FIXTURE, "H.M");
    let x = initial_value(FIXTURE, "H.M", "floMacSta.x");
    assert!((x - 12.0).abs() < 1e-9, "x = {x}");
}

/// TOOLBUG-167: a component's dimension reads a sibling package constant by a
/// relative name (`C.n`), which typecheck never collected.
#[test]
fn component_dimension_reads_sibling_package_constant() {
    const FIXTURE: &str = "\
package L
  package C
    constant Integer n = 2;
  end C;
  model A
    Real x[C.n] = {1, 2};
  end A;
  model B
    A a;
  end B;
end L;
";
    assert_compiles(FIXTURE, "L.B");
}

/// TOOLBUG-167: a component's class redeclares its replaceable package in an
/// `extends` clause with a relative name; the dimension must use the
/// redeclared package's constant, set by an extends modifier
/// (IBPSA/Buildings `Electrical.Interfaces.Source.S[PhaseSystem.n]`).
#[test]
fn component_dimension_uses_extends_redeclared_package() {
    const FIXTURE: &str = "\
package P
  package PhaseSystems
    partial package PartialPhaseSystem
      constant Integer n;
    end PartialPhaseSystem;
    package OnePhase
      extends PartialPhaseSystem(n=2);
    end OnePhase;
    package ThreePhase
      extends PartialPhaseSystem(n=3);
    end ThreePhase;
  end PhaseSystems;
  package Interfaces
    partial model Source
      replaceable package PhaseSystem = P.PhaseSystems.OnePhase
        constrainedby P.PhaseSystems.PartialPhaseSystem;
      Real S[PhaseSystem.n];
    end Source;
  end Interfaces;
  package AC
    package Sources
      model FixedVoltage
        extends P.Interfaces.Source(
          redeclare package PhaseSystem = PhaseSystems.ThreePhase);
      equation
        S = {1, 2, 3};
      end FixedVoltage;
    end Sources;
    model M
      Sources.FixedVoltage E;
      Real s3 = E.S[3];
    end M;
  end AC;
end P;
";
    let s3 = initial_value(FIXTURE, "P.AC.M", "s3");
    assert!((s3 - 3.0).abs() < 1e-9, "s3 = {s3}");
}
