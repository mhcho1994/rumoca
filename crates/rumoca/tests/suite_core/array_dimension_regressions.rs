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
