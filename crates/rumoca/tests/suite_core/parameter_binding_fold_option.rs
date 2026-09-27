//! `SessionConfig::fold_parameter_declaration_bindings` — keep a derived
//! parameter's binding as written.
//!
//! By default `parameter Real d = k * 10` with `k = 2` reaches Flat as
//! `d = 20`. That is the value a solver wants, but it erases the fact that `d`
//! is derived from `k`, and no later phase can recover it: the arithmetic has
//! already been carried out. An analysis that asks "which knob does the user
//! turn to reach this denominator" needs the chain, so the fold is optional.
//!
//! What the option must *not* do is change the model. MLS §18.3 structural
//! parameters decide array extents and branch selection, and Integer/Boolean/
//! String parameters seed those decisions for nested components, so both are
//! resolved either way. These tests pin that split: the Real chain survives,
//! the shape does not move.

use rumoca_compile::compile::{Session, SessionConfig};

fn session(fold: bool) -> Session {
    Session::new(SessionConfig {
        fold_parameter_declaration_bindings: fold,
        ..SessionConfig::default()
    })
}

/// The rendered binding of a flat variable, or `"<none>"` when it has none.
fn binding_of(flat: &rumoca_compile::compile::FlatModel, name: &str) -> String {
    let interned = rumoca_core::VarName::intern(name);
    let variable = flat
        .variables
        .get(&interned)
        .unwrap_or_else(|| panic!("flat model has no variable `{name}`"));
    variable
        .binding
        .as_ref()
        .map_or_else(|| "<none>".to_string(), |expr| format!("{expr:?}"))
}

const DERIVED: &str = "\
model Derived
  parameter Real k = 2;
  parameter Real d = k * 10;
  Real x;
equation
  der(x) = 1 / d;
end Derived;
";

#[test]
fn folding_on_replaces_a_derived_real_binding_with_its_value() {
    let mut session = session(true);
    session
        .add_document("derived.mo", DERIVED)
        .expect("fixture parses");
    let compiled = session.compile_model("Derived").expect("model compiles");

    let binding = binding_of(&compiled.flat, "d");
    assert!(
        !binding.contains('k'),
        "default folding should have consumed the reference to `k`, got {binding}"
    );
}

#[test]
fn folding_off_keeps_the_derived_real_binding_as_written() {
    let mut session = session(false);
    session
        .add_document("derived.mo", DERIVED)
        .expect("fixture parses");
    let compiled = session.compile_model("Derived").expect("model compiles");

    let binding = binding_of(&compiled.flat, "d");
    assert!(
        binding.contains('k'),
        "unfolded binding should still name `k`, got {binding}"
    );
    // The point of keeping it is that the dependency is readable, not merely
    // that some expression survived: `d` must still be the product.
    assert!(
        binding.contains("Mul") || binding.contains("Multiply") || binding.contains('*'),
        "unfolded binding should still be the product, got {binding}"
    );
}

const STRUCTURAL: &str = "\
model Structural
  parameter Integer base = 2;
  parameter Integer n = base + 1;
  parameter Boolean useExtra = base > 1;
  parameter Real gain = base * 4;
  Real y[n];
  Real extra if useExtra;
equation
  for i in 1:n loop
    der(y[i]) = gain * i;
  end for;
  if useExtra then
    extra = gain;
  end if;
end Structural;
";

#[test]
fn folding_off_still_resolves_structural_and_discrete_parameters() {
    let mut compiled = Vec::new();
    for fold in [true, false] {
        let mut session = session(fold);
        session
            .add_document("structural.mo", STRUCTURAL)
            .expect("fixture parses");
        compiled.push(
            session
                .compile_model("Structural")
                .unwrap_or_else(|error| panic!("fold={fold} should still compile: {error:?}")),
        );
    }
    let (folded, unfolded) = (&compiled[0], &compiled[1]);

    for name in ["n", "useExtra"] {
        assert_eq!(
            binding_of(&folded.flat, name),
            binding_of(&unfolded.flat, name),
            "`{name}` decides model shape and must resolve identically either way"
        );
    }

    // Shape is the real claim: the array extent and the conditional component
    // are decided before any of this, so the variable sets must match exactly.
    let names = |result: &rumoca_compile::compile::CompilationResult| {
        let mut found: Vec<String> = result
            .flat
            .variables
            .keys()
            .map(ToString::to_string)
            .collect();
        found.sort();
        found
    };
    assert_eq!(
        names(folded),
        names(unfolded),
        "unfolding a Real binding must not change which variables exist"
    );
    assert_eq!(
        folded.flat.equations.len(),
        unfolded.flat.equations.len(),
        "unfolding a Real binding must not change the equation count"
    );

    // And the Real parameter is the one thing that does differ.
    assert!(binding_of(&unfolded.flat, "gain").contains("base"));
    assert!(!binding_of(&folded.flat, "gain").contains("base"));
}

#[test]
fn folding_is_on_by_default() {
    assert!(
        SessionConfig::default().fold_parameter_declaration_bindings,
        "turning this off by default would change every consumer's flat output"
    );
}
