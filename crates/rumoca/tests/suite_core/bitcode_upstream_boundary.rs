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

const ORDERED: &str = r#"
model Ordered
  function choose
    input Real u;
    output Real y;
  protected
    Real selected;
  algorithm
    if u > 0 then
      selected := u;
      assert(selected > 0, "selected positive branch");
    else
      selected := -u;
      assert(u <= 0, "selected nonpositive branch");
    end if;
    if u > 0 then
      y := selected;
    else
      y := -u;
    end if;
    for i in 1:2 loop
      y := y + i;
      for j in 1:2 loop
        if y > 0 then
          y := y + i*j;
          assert(y > 0, "selected assertion");
        else
          y := y - i*j;
          assert(y <= 0, "unselected assertion");
        end if;
      end for;
    end for;
  end choose;
  Real y = choose(time + 1);
end Ordered;
"#;

fn ordered_artifact() -> rumoca_bitcode::RbcFile {
    let model = Compiler::new()
        .model("Ordered")
        .compile_str(ORDERED, "Ordered.mo")
        .expect("ordered branches compile");
    rumoca_bitcode::export(
        &model.dae,
        Some(&model.flat),
        "Ordered",
        &Default::default(),
    )
    .expect("ordered branches export")
}

#[test]
fn bitcode_ordered_branches_preserve_assertions_and_nested_loops() {
    let artifact = ordered_artifact();
    let before = serde_json::to_value(&artifact.model.functions).unwrap();
    let rebuilt = rumoca_bitcode::import(&artifact).expect("ordered branches import");
    let after = rumoca_bitcode::export(&rebuilt, None, "Ordered", &Default::default()).unwrap();
    assert_eq!(
        before,
        serde_json::to_value(&after.model.functions).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&artifact.model.expressions).unwrap(),
        serde_json::to_value(&after.model.expressions).unwrap()
    );
    let text = before.to_string();
    assert!(text.contains("assertion"));
}

#[test]
fn bitcode_rejects_a_forged_conditional_owner() {
    let mut artifact = ordered_artifact();
    let expression = artifact
        .model
        .expressions
        .iter_mut()
        .find(|expression| {
            matches!(
                expression.node,
                rumoca_bitcode::schema::RbcExprNode::FunctionConditionalOutput { .. }
            )
        })
        .expect("conditional output");
    if let rumoca_bitcode::schema::RbcExprNode::FunctionConditionalOutput { conditional, .. } =
        &mut expression.node
    {
        *conditional = u32::MAX;
    }
    assert!(rumoca_bitcode::import(&artifact).is_err());
}
