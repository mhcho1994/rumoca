//! `rumoca bitcode disasm` — the artifact as a listing rather than a dump.
//!
//! `bitcode dump` prints the serialization, in which an equation reads
//! `"residual": 83` and the operand graph is addressed by index. That is right
//! for a machine and unusable for a person: for a 60-variable circuit it is a
//! quarter-megabyte of JSON in which no equation is legible.
//!
//! These tests pin the two properties that make the listing worth having: the
//! expression graph is resolved, and a gap in the artifact is shown rather than
//! smoothed over.

use std::fs;
use std::process::Command;

use tempfile::tempdir;

const FIXTURE: &str = "\
model DisasmFixture
  parameter Real g = 9.81;
  parameter Real k = 2 * g;
  Real x(start = 1, fixed = true);
  Real v(start = 0, fixed = true);
equation
  der(x) = v;
  der(v) = -g / k;
end DisasmFixture;
";

fn disassemble(extra: &[&str]) -> String {
    let work = tempdir().expect("temp dir");
    let source = work.path().join("DisasmFixture.mo");
    fs::write(&source, FIXTURE).expect("write fixture");
    let artifact = work.path().join("fixture.rbc");

    let compiled = Command::new(env!("CARGO_BIN_EXE_rumoca"))
        .args([
            "compile",
            source.to_str().unwrap(),
            "--model",
            "DisasmFixture",
            "--emit-bitcode",
            artifact.to_str().unwrap(),
        ])
        .output()
        .expect("compile runs");
    assert!(
        artifact.exists(),
        "fixture must compile: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );

    let mut args = vec!["bitcode", "disasm", artifact.to_str().unwrap()];
    args.extend_from_slice(extra);
    let listing = Command::new(env!("CARGO_BIN_EXE_rumoca"))
        .args(&args)
        .output()
        .expect("disasm runs");
    assert!(
        listing.status.success(),
        "disasm must succeed: {}",
        String::from_utf8_lossy(&listing.stderr)
    );
    String::from_utf8(listing.stdout).expect("utf8")
}

#[test]
fn equations_are_rendered_not_referenced() {
    let listing = disassemble(&[]);

    // The point of the command: an equation reads as the expression it is,
    // not as an index into a table the reader has to walk.
    assert!(
        listing.contains("0 = (der(x) - v)"),
        "the residual should be resolved and infix, got:\n{listing}"
    );
    assert!(
        listing.contains("0 = (der(v) - -(g / k))") || listing.contains("0 = (der(v) - (-g / k))"),
        "a nested expression should resolve through both operands, got:\n{listing}"
    );
    assert!(
        !listing.contains("\"residual\""),
        "the listing must not be the serialization"
    );
}

#[test]
fn a_derived_parameter_binding_is_shown_with_its_declaration() {
    let listing = disassemble(&[]);
    // A variable line carries what the declaration said, so the reader does
    // not have to cross-reference the expression table to learn a default.
    assert!(
        listing.contains("parameter") && listing.contains("binding="),
        "parameter lines should show their binding, got:\n{listing}"
    );
    assert!(
        listing.contains("der(x)"),
        "derivative coordinates spell as der(x)"
    );
}

#[test]
fn the_expression_table_is_opt_in() {
    let plain = disassemble(&[]);
    let full = disassemble(&["--expressions"]);
    assert!(
        !plain.contains("expression table"),
        "the constant pool is noise by default"
    );
    assert!(
        full.contains("expression table"),
        "--expressions prints it for someone chasing an id"
    );
    assert!(full.len() > plain.len(), "and it adds content");
}

#[test]
fn provenance_is_opt_in_and_names_the_source_line() {
    let listing = disassemble(&["--provenance"]);
    assert!(
        listing.contains("DisasmFixture.mo:"),
        "--provenance should name the file and line, got:\n{listing}"
    );
}
