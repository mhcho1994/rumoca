//! Resolve checks that used to reject models OpenModelica accepts
//! (TOOLBUG-080..089). Each case must resolve without errors, and a
//! spec deviation that is only tolerated must still leave its advisory.

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

/// TOOLBUG-082: a quoted function name is only an operator function when it
/// is an MLS §14 operator name; MSL `ComplexMath.'abs'` is a plain function.
#[test]
fn quoted_non_operator_function_name_is_not_an_operator() {
    let source = r#"
package CM
  function 'abs'
    input Real x;
    output Real y;
  algorithm
    y := if x < 0 then -x else x;
  end 'abs';
end CM;
"#;
    resolve_warning_codes(source);
}

/// TOOLBUG-083: extending an empty icon class of another restriction
/// (OpenIPSL `package TGData extends Modelica.Icons.Record`, TRANSFORM
/// `model ... extends TRANSFORM.Icons.Function`) is accepted with WR008;
/// a non-empty incompatible base keeps ER091.
#[test]
fn empty_incompatible_base_warns_non_empty_rejects() {
    let source = r#"
package P
  partial record Rec end Rec;
  partial function Fn end Fn;
  package Data
    extends Rec;
    constant Real c = 1;
  end Data;
  model M
    extends Fn;
    Real x = 1;
  end M;
end P;
"#;
    let codes = resolve_warning_codes(source);
    assert_eq!(
        codes.iter().filter(|code| *code == "WR008").count(),
        2,
        "{codes:?}"
    );
    let rejected = r#"
package P
  function F
    input Real u;
    output Real y = u;
  end F;
  model M
    extends F;
  end M;
end P;
"#;
    expect_error_code(rejected, "ER091");
}

/// TOOLBUG-084: an `outer` declaration of a partial type refers to the
/// matching `inner` element and is not an instantiation (VehicleInterfaces
/// `outer Roads.Interfaces.Base road`).
#[test]
fn outer_component_of_partial_type_is_not_instantiation() {
    let source = r#"
package P
  partial model Road
    Real h = 1;
  end Road;
  model FlatRoad
    extends Road;
  end FlatRoad;
  model Tyre
    outer Road road;
    Real z = road.h;
  end Tyre;
  model Vehicle
    inner FlatRoad road;
    Tyre t;
  end Vehicle;
end P;
"#;
    resolve_warning_codes(source);
}

/// TOOLBUG-085: external-object members of records/connectors are accepted
/// with WR011 (Modelica_DeviceDrivers `PackageOut.pkg`, `Comedi` handles).
#[test]
fn external_object_record_and_connector_members_warn() {
    let source = r#"
package P
  class Handle
    extends ExternalObject;
    function constructor
      input Integer n;
      output Handle h;
      external "C" h = handle_new(n);
    end constructor;
    function destructor
      input Handle h;
      external "C" handle_free(h);
    end destructor;
  end Handle;
  connector HOut
    output Handle h;
  end HOut;
  record Cfg
    Handle h;
  end Cfg;
end P;
"#;
    let codes = resolve_warning_codes(source);
    assert_eq!(
        codes.iter().filter(|code| *code == "WR011").count(),
        2,
        "{codes:?}"
    );
}

/// TOOLBUG-086: an equation outside a clocked when-clause is clock-inferred,
/// not continuous (Modelica_DeviceDrivers `KeyboardInput`); only an equation
/// that is continuous on its own (contains `der`) proves ER127.
#[test]
fn clock_inferred_equation_reading_clocked_variable_is_legal() {
    let source = r#"
model K
  Integer code(start=0);
  Boolean up;
equation
  when Clock(0.1) then
    code = previous(code) + 1;
  end when;
  up = code >= 3;
end K;
"#;
    resolve_warning_codes(source);
    let continuous = r#"
model K
  Real xc(start=0);
  Real y;
equation
  when Clock(0.1) then
    xc = previous(xc) + 1;
  end when;
  der(y) = xc;
end K;
"#;
    expect_error_code(continuous, "ER127");
}

/// TOOLBUG-086: an impure call in a plain equation of a class with clocked
/// partitions is accepted with WR012 (Modelica_DeviceDrivers clocked
/// `Packager`); in a purely continuous class it keeps ER088.
#[test]
fn impure_call_in_clocked_class_warns() {
    let source = r#"
impure function logn
  input Integer n;
  external "C" log_n(n);
end logn;
block Pr
  Integer n(start=0);
  Boolean init(start=false);
equation
  when Clock(0.1) then
    init = true;
  end when;
  if not previous(init) then
    n = 1;
    logn(n);
  else
    n = previous(n);
  end if;
end Pr;
model C
  Real x = time;
equation
  logn(1);
end C;
"#;
    let diagnostics = resolve_test_source(source).expect_err("continuous class keeps ER088");
    let codes: Vec<_> = diagnostics
        .iter()
        .filter_map(|diagnostic| diagnostic.code.clone())
        .collect();
    assert_eq!(
        codes.iter().filter(|code| *code == "ER088").count(),
        1,
        "only the continuous class is rejected: {codes:?}"
    );
    assert!(codes.iter().any(|code| code == "WR012"), "{codes:?}");
}

/// TOOLBUG-087: `zeroDerivative` may name an input inherited from a base
/// function (TRANSFORM `bicubic_eval extends PartialInterpolation`).
#[test]
fn zero_derivative_accepts_inherited_input() {
    let source = r#"
package P
  partial function Partial
    input String path;
    input Real x;
    output Real z;
  end Partial;
  function F_dt
    input String path;
    input Real x;
    input Real dx;
    output Real dz;
  algorithm
    dz := dx;
  end F_dt;
  function F
    extends Partial;
  algorithm
    z := x;
    annotation(derivative(zeroDerivative=path)=F_dt);
  end F;
end P;
"#;
    resolve_warning_codes(source);
}

/// TOOLBUG-088: `each` on a scalar component and `parameter input` on a
/// function formal are tolerated with WR009 / WR010.
#[test]
fn each_on_scalar_and_function_parameter_input_warn() {
    let source = r#"
package P
  function f
    input Real u;
    parameter input Real k;
    output Real y;
  algorithm
    y := k*u;
  end f;
  model Sub
    parameter Real T[2] = {1, 1};
  end Sub;
  model M
    Sub s(each T = {2, 3});
    Real y = f(time, 2);
  end M;
end P;
"#;
    let codes = resolve_warning_codes(source);
    assert!(codes.iter().any(|code| code == "WR009"), "{codes:?}");
    assert!(codes.iter().any(|code| code == "WR010"), "{codes:?}");
}

/// TOOLBUG-141: a function without a purity prefix that calls an impure
/// function (MSL `ModelicaServices.ExternalReferences.loadResource` calling
/// `Files.fullPathName`) is accepted as impure with WR013, as OpenModelica
/// does; an explicitly `pure` caller gets no such allowance here.
#[test]
fn undeclared_purity_function_calling_impure_warns() {
    let source = r#"
package P
  impure function g
    input Real x;
    output Real y;
    external "C" y = sin(x);
  end g;
  function f
    input Real x;
    output Real y;
  algorithm
    y := P.g(x);
  end f;
  model M
    parameter Real p = f(1);
  end M;
end P;
"#;
    let codes = resolve_warning_codes(source);
    assert!(codes.iter().any(|code| code == "WR013"), "{codes:?}");
}
