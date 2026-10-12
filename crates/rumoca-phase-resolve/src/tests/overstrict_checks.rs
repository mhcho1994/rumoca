//! Inherited declaration identity and enclosing-reference regressions.

use super::*;

/// Resolve `source`, assert it has no error diagnostics, and return the
/// warning codes it produced.
fn resolve_warning_codes(source: &str) -> Vec<String> {
    let success = match resolve_with_diagnostics(parsed_tree_from_source(source)) {
        Ok(success) => success,
        Err(failure) => panic!("source must resolve: {:?}", failure.diagnostics()),
    };
    let (_, diagnostics) = success.into_parts();
    assert!(
        !diagnostics.has_errors(),
        "source must resolve without errors: {diagnostics:?}"
    );
    diagnostics
        .iter()
        .filter_map(|diagnostic| diagnostic.code.clone())
        .collect()
}

fn expect_error_code(source: &str, code: &str) {
    let diagnostics = resolve_test_source(source).expect_err("source must be rejected");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_deref() == Some(code)),
        "expected {code}, got: {diagnostics:?}"
    );
}

/// TOOLBUG-080: `redeclare function extends product` names the inherited
/// `product`, not the builtin reduction of the same name, so the inherited
/// inputs `x`/`y` and output `z` resolve (Buildings/IBPSA/AixLib/IDEAS
/// `PhaseSystems.OnePhase.product`).
#[test]
fn class_extends_prefers_inherited_slot_over_builtin_name() {
    let source = r#"
package P
  partial package Base
    constant Integer n = 2;
    replaceable partial function product
      input Real x[n];
      input Real y[n];
      output Real z[n];
    end product;
  end Base;
  package One
    extends Base;
    redeclare function extends product
    algorithm
      z := {x[1]*y[1] - x[2]*y[2], x[1]*y[2] + x[2]*y[1]};
    end product;
  end One;
end P;
"#;
    resolve_warning_codes(source);
}

/// TOOLBUG-081: a nested class that inherits the same declaration as its
/// enclosing class reaches it through its own inheritance, not through the
/// enclosing class (Buildings `TrimAndRespond.UnitDelay`, IDEAS
/// `Adsolair58.DummyExchanger`).
#[test]
fn nested_class_inheriting_same_member_is_not_enclosing_reference() {
    let source = r#"
package P
  partial block Discrete
  protected
    discrete Boolean firstTrigger(start=false, fixed=true);
  end Discrete;
  partial block SISO
    extends Discrete;
  end SISO;
  block Delay
    extends SISO;
  end Delay;
  block Outer
    extends SISO(firstTrigger(start=false, fixed=true));
    Inner i;
  protected
    block Inner
      extends Delay(firstTrigger(start=false, fixed=true));
    equation
      when sample(0, 1) then
        firstTrigger = not pre(firstTrigger);
      end when;
    end Inner;
  end Outer;
end P;
"#;
    resolve_warning_codes(source);
}

#[test]
fn genuine_enclosing_non_constant_reference_still_rejected() {
    let source = r#"
model Outer
  Real v = 1;
  model Inner
    Real w = v;
  end Inner;
  Inner i;
end Outer;
"#;
    expect_error_code(source, "ER130");
}
