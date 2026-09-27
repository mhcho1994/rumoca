//! The digest depends on what a program references, and on nothing else.
//!
//! These are unit tests on the encoding rather than end-to-end runs, because
//! the properties under test are about *what the hash does not see*: type
//! table numbering, an unreferenced variable, a field order. An end-to-end
//! test can only show that one artifact still runs.
use super::dependency_digest;
use rumoca_bitcode::build::Builder;
use rumoca_bitcode::schema::{RbcCausality, RbcModel, RbcScalar, RbcType, TypeId};
use rumoca_ir_solve::execution::{LoweringProfile, TracePointRef};
use std::collections::{BTreeMap, BTreeSet};

fn referenced(ids: &[u32]) -> BTreeSet<TracePointRef> {
    ids.iter().copied().map(TracePointRef).collect()
}

fn digest(model: &RbcModel, ids: &[u32]) -> String {
    dependency_digest(model, LoweringProfile::default(), &referenced(ids))
        .expect("digest a well-formed model")
}

/// One traced `Real` state, plus a second state on an `Integer` type so the
/// type table holds more than one entry to permute.
fn traced_model() -> RbcModel {
    let mut builder = Builder::new("Digest");
    let x = builder.state("x", 0.0);
    let y = builder.state("y", 0.0);
    builder.trace_point(x, "x", "test");
    builder.trace_point(y, "y", "test");
    let mut model = builder.finish();
    let integer = TypeId(model.types.len() as u32);
    model.types.push(RbcType {
        id: integer,
        scalar: RbcScalar::Integer,
        dimensions: Vec::new(),
        record: None,
    });
    model.variables[1].value_type = integer;
    model
}

/// Renumber the type table by reversing it and repointing every reference,
/// leaving each entry's *contents* untouched.
fn renumber_types(model: &mut RbcModel) {
    model.types.reverse();
    let mut mapping = BTreeMap::new();
    for (position, entry) in model.types.iter_mut().enumerate() {
        let fresh = TypeId(position as u32);
        mapping.insert(entry.id, fresh);
        entry.id = fresh;
    }
    for variable in &mut model.variables {
        variable.value_type = mapping[&variable.value_type];
    }
    for expression in &mut model.expressions {
        expression.value_type = mapping[&expression.value_type];
    }
    for entry in &mut model.types {
        if let Some(record) = entry.record.as_mut() {
            for field in &mut record.fields {
                field.value_type = mapping[&field.value_type];
            }
        }
    }
}

#[test]
fn type_identity_is_contents_not_the_id_that_happens_to_hold_them() {
    let model = traced_model();
    let mut renumbered = traced_model();
    renumber_types(&mut renumbered);
    assert_ne!(
        model.variables[0].value_type, renumbered.variables[0].value_type,
        "the fixture must actually move the traced variable's type id"
    );
    assert_eq!(
        digest(&model, &[0]),
        digest(&renumbered, &[0]),
        "the same type contents under different TypeId numbering must hash \
         alike; type ids are per-compilation"
    );
}

#[test]
fn a_retype_that_reuses_the_id_still_changes_the_digest() {
    let model = traced_model();
    let mut retyped = traced_model();
    // Same TypeId, different contents. Hashing the id number would miss this,
    // which is the defect this encoding exists to avoid.
    let held = retyped.variables[0].value_type;
    retyped
        .types
        .iter_mut()
        .find(|candidate| candidate.id == held)
        .expect("the traced variable's type")
        .scalar = RbcScalar::Integer;
    assert_eq!(
        model.variables[0].value_type,
        retyped.variables[0].value_type
    );
    assert_ne!(digest(&model, &[0]), digest(&retyped, &[0]));
}

#[test]
fn a_dimension_change_changes_the_digest() {
    let model = traced_model();
    let mut reshaped = traced_model();
    let held = reshaped.variables[0].value_type;
    reshaped
        .types
        .iter_mut()
        .find(|candidate| candidate.id == held)
        .expect("the traced variable's type")
        .dimensions = vec![3];
    assert_ne!(digest(&model, &[0]), digest(&reshaped, &[0]));
}

#[test]
fn causality_is_hashed_by_tag_and_a_change_is_visible() {
    let model = traced_model();
    let mut recast = traced_model();
    recast.variables[0].causality = RbcCausality::Output;
    assert_ne!(digest(&model, &[0]), digest(&recast, &[0]));
}

#[test]
fn an_unreferenced_variable_is_outside_the_digest() {
    let model = traced_model();
    let mut edited = traced_model();
    // Trace point 1 exists, but the program does not reference it.
    edited.variables[1].causality = RbcCausality::Output;
    edited.variables[1].unit = Some("kg".into());
    assert_eq!(digest(&model, &[0]), digest(&edited, &[0]));
    assert_ne!(digest(&model, &[0, 1]), digest(&edited, &[0, 1]));
}

#[test]
fn a_program_that_references_nothing_still_has_a_digest() {
    let model = traced_model();
    let empty = digest(&model, &[]);
    assert!(empty.starts_with("sha1:"));
    assert_ne!(empty, digest(&model, &[0]));
}

#[test]
fn an_undeclared_type_is_named_rather_than_hashed_as_a_number() {
    let mut model = traced_model();
    model.variables[0].value_type = TypeId(9999);
    let failure = dependency_digest(&model, LoweringProfile::default(), &referenced(&[0]))
        .expect_err("an undeclared type must fail the digest");
    assert!(
        format!("{failure}").contains("type 9999 is referenced but not declared"),
        "unexpected: {failure}"
    );
}
