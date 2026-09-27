//! MLS §4.8: a type's attribute modifications bound every variable of that type.
//!
//! `type Mass = Real(min=0)` bounds every `SI.Mass` whether or not the
//! declaration repeats it. Instantiation inherited `quantity` and `unit` from
//! the type hierarchy and not `min`/`max`/`nominal`, so the DAE understated
//! what the model declares — 29 SI types in MSL, and 59% of this project's own
//! findings claimed "nothing bounds this" about a bounded declaration.

use rumoca_compile::compile::{Session, SessionConfig};

const FIXTURE: &str = "\
package P
  type Bounded = Real(quantity=\"Bounded\", unit=\"kg\", min=0, max=100);
  type Unbounded = Real(quantity=\"Unbounded\", unit=\"Ohm\");
  type Narrowed = Bounded;
  model M
    parameter Bounded fromType = 1;
    parameter Bounded overridden(min=2) = 3;
    parameter Unbounded none = 1;
    parameter Narrowed inherited = 1;
    Real x(start = 0, fixed = true);
  equation
    der(x) = fromType + overridden + none + inherited;
  end M;
end P;
";

fn bound(flat: &rumoca_compile::compile::FlatModel, name: &str, which: &str) -> String {
    let interned = rumoca_core::VarName::intern(name);
    let variable = flat
        .variables
        .get(&interned)
        .unwrap_or_else(|| panic!("no variable `{name}`"));
    let slot = match which {
        "min" => &variable.min,
        _ => &variable.max,
    };
    slot.as_ref()
        .map_or_else(|| "<none>".to_string(), |e| format!("{e:?}"))
}

fn compiled() -> rumoca_compile::compile::CompilationResult {
    let mut session = Session::new(SessionConfig::default());
    session
        .add_document("p.mo", FIXTURE)
        .expect("fixture parses");
    session.compile_model("P.M").expect("fixture compiles")
}

#[test]
fn a_bound_declared_on_the_type_reaches_the_flat_model() {
    let result = compiled();
    let min = bound(&result.flat, "fromType", "min");
    assert!(
        min.contains('0') && !min.contains("<none>"),
        "`min=0` on the type must bound a variable of that type, got {min}"
    );
    let max = bound(&result.flat, "fromType", "max");
    assert!(max.contains("100"), "`max` is inherited too, got {max}");
}

#[test]
fn a_declaration_modifier_wins_over_the_type() {
    // MLS §7.2.5: the declaration is more specific and may narrow the type's
    // bound. Inheriting over the top of it would silently widen the model.
    let min = bound(&compiled().flat, "overridden", "min");
    assert!(
        min.contains('2'),
        "the declaration's `min=2` must win over the type's `min=0`, got {min}"
    );
}

#[test]
fn a_type_with_no_bound_still_has_none() {
    // The inheritance must not invent a bound. `SI.Resistance` genuinely
    // declares none, and reporting one would hide a real defect class.
    assert_eq!(bound(&compiled().flat, "none", "min"), "<none>");
}

#[test]
fn a_bound_is_inherited_through_a_type_alias() {
    let min = bound(&compiled().flat, "inherited", "min");
    assert!(
        min.contains('0') && !min.contains("<none>"),
        "an alias of a bounded type is bounded, got {min}"
    );
}
