//! Public bitcode v2 must not silently drop new internal IR effects.
use rumoca::Compiler;

fn export_refusal(source: &str, owner: &str) {
    let model = Compiler::new()
        .model("Boundary")
        .compile_str(source, "Boundary.mo")
        .expect("the current compiler supports this source");
    let error = rumoca_bitcode::export(
        &model.dae,
        Some(&model.flat),
        "Boundary",
        &rumoca_bitcode::ExportOptions::default(),
    )
    .expect_err("bitcode v2 cannot carry this owner losslessly");
    assert!(
        matches!(error, rumoca_bitcode::ExportError::UnsupportedOwner(actual) if actual == owner)
    );
}

#[test]
fn terminal_print_cannot_disappear_from_bitcode() {
    export_refusal(
        r#"
package Streams
  impure function print
    input String string = "";
    input String fileName = "";
  external "C" ModelicaInternal_print(string, fileName);
  end print;
end Streams;
model Boundary
  Real x(start = 1, fixed = true);
equation
  der(x) = -x;
algorithm
  when initial() then
    Streams.print("started");
  end when;
end Boundary;
"#,
        "terminal print actions",
    );
}

#[test]
fn bounded_while_continuation_cannot_disappear_from_bitcode() {
    export_refusal(
        r#"
model Boundary
  function halve
    input Real x;
    output Real y;
  protected
    Integer i;
    Boolean found;
  algorithm
    y := x;
    found := false;
    i := 1;
    while i < 100 and not found loop
      y := y/2;
      found := y < 0.1;
      i := i + 1;
    end while;
  end halve;
  Real y = halve(1 + time);
end Boundary;
"#,
        "bounded while continuations",
    );
}
