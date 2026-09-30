//! The staleness digest: a hash of the identities a program depends on.
//!
//! v1 hashed the entire serialized model, so adding a trace point or
//! rewriting an unrelated residual invalidated a logging program that
//! referenced neither. This covers exactly the referenced trace points, the
//! lowering profile's expansion, and the two contract versions.
//!
//! The encoding is hand-written and consumes nothing but bytes. It never goes
//! through `serde`, so field order, `skip_serializing_if` and every other
//! serialization choice are outside the hash by construction rather than by
//! test.
use anyhow::{Context, Result, bail};
use rumoca_bitcode::schema::{self, RbcModel};
use sha1::{Digest, Sha1};

/// How deep a record type may nest before the encoder gives up.
///
/// Record fields precede the record that names them (`schema::RbcRecord`), so
/// the type table is acyclic and this bound is unreachable for a valid
/// artifact. It exists so a malformed one cannot recurse without end.
const MAX_TYPE_DEPTH: u32 = 32;

fn encode(out: &mut Vec<u8>, part: &[u8]) {
    out.extend_from_slice(&(part.len() as u64).to_le_bytes());
    out.extend_from_slice(part);
}

/// Stable tag per scalar kind.
///
/// Exhaustive with no wildcard, so adding a scalar kind fails to compile here
/// until somebody chooses its tag. `format!("{:?}")` would have been shorter
/// and would have made the hash depend on a `Debug` impl, which is not a
/// contract anybody promised to keep.
const fn scalar_tag(scalar: schema::RbcScalar) -> u8 {
    match scalar {
        schema::RbcScalar::Real => 1,
        schema::RbcScalar::Integer => 2,
        schema::RbcScalar::Boolean => 3,
        schema::RbcScalar::String => 4,
        schema::RbcScalar::Enumeration => 5,
        schema::RbcScalar::Record => 6,
    }
}

/// Stable tag per causality. Same reasoning as `scalar_tag`.
const fn causality_tag(causality: schema::RbcCausality) -> u8 {
    match causality {
        schema::RbcCausality::Input => 1,
        schema::RbcCausality::Output => 2,
        schema::RbcCausality::Parameter => 3,
        schema::RbcCausality::CalculatedParameter => 4,
        schema::RbcCausality::Independent => 5,
        schema::RbcCausality::Local => 6,
    }
}

/// Encode a type by its **contents**, never by its id.
///
/// `TypeId` numbering is per-compilation. Hashing the number would make the
/// digest depend on how a particular run happened to lay out its type table,
/// and — worse — would hide a retype that lands on the same id, which is the
/// exact change the digest exists to catch.
fn encode_value_type(
    out: &mut Vec<u8>,
    model: &RbcModel,
    id: schema::TypeId,
    depth: u32,
) -> Result<()> {
    if depth > MAX_TYPE_DEPTH {
        bail!("type {} nests deeper than the digest will follow", id.0);
    }
    let declared = model
        .types
        .iter()
        .find(|candidate| candidate.id == id)
        .with_context(|| format!("type {} is referenced but not declared", id.0))?;
    encode(out, &[scalar_tag(declared.scalar)]);
    encode(out, &(declared.dimensions.len() as u64).to_le_bytes());
    for extent in &declared.dimensions {
        encode(out, &extent.to_le_bytes());
    }
    match &declared.record {
        None => encode(out, &[0]),
        Some(record) => {
            encode(out, &[1]);
            encode(out, record.name.as_bytes());
            encode(out, &(record.fields.len() as u64).to_le_bytes());
            for field in &record.fields {
                encode(out, field.name.as_bytes());
                encode_value_type(out, model, field.value_type, depth + 1)?;
            }
        }
    }
    Ok(())
}

/// The component this variable belongs to, plus its own name, as two
/// length-prefixed records.
///
/// The two parts stay separate rather than being joined into one dotted
/// string: a joined string invites a reader to split it again, and
/// SPEC_0007's no-tokenizing rule is what that would violate. The hash
/// consumes them as bytes and never re-parses.
///
/// `RbcComponent.path` is itself a dotted display string, so a component
/// literally named `a.b` and a component `a` owning `b` still hash alike.
/// That ambiguity lives in the equation IR's own representation of a
/// component path, not in this encoding, and closing it means giving
/// `RbcComponent` a structured path.
fn encode_component_path(out: &mut Vec<u8>, model: &RbcModel, variable: &schema::RbcVariable) {
    let component = variable
        .component
        .and_then(|id| model.components.iter().find(|c| c.id == id));
    match component {
        Some(component) => encode(out, component.path.as_bytes()),
        None => encode(out, b""),
    }
    encode(out, variable.name.as_bytes());
}

/// Hash the identities this program actually depends on.
pub fn dependency_digest(
    model: &RbcModel,
    lowering: rumoca_ir_solve::execution::LoweringProfile,
    referenced: &std::collections::BTreeSet<rumoca_ir_solve::execution::TracePointRef>,
) -> Result<String> {
    let mut bytes = Vec::new();
    encode(&mut bytes, b"rumoca.execution.dependency.v2");
    encode(
        &mut bytes,
        &rumoca_bitcode::schema::RBC_VERSION.to_le_bytes(),
    );
    encode(
        &mut bytes,
        &rumoca_ir_solve::execution::EXECUTION_VERSION.to_le_bytes(),
    );
    let expansion = lowering.expansion();
    encode(&mut bytes, expansion.name.as_bytes());
    encode(&mut bytes, expansion.backend_family.as_bytes());
    encode(&mut bytes, &[u8::from(expansion.scalar_only)]);
    encode(&mut bytes, &[u8::from(expansion.file_effects)]);
    // Referenced trace points, in id order so the set's own ordering cannot
    // change the digest.
    for id in referenced {
        let point = model
            .trace_points
            .iter()
            .find(|p| p.id.0 == id.0)
            .with_context(|| format!("trace point {id} is referenced but not declared"))?;
        let variable = model
            .variables
            .iter()
            .find(|v| v.id == point.variable)
            .with_context(|| format!("trace point {id} names an unknown variable"))?;
        encode(&mut bytes, &id.0.to_le_bytes());
        encode_component_path(&mut bytes, model, variable);
        encode_value_type(&mut bytes, model, variable.value_type, 0)?;
        encode(&mut bytes, &[causality_tag(variable.causality)]);
        encode(
            &mut bytes,
            variable.unit.as_deref().unwrap_or("").as_bytes(),
        );
    }
    Ok(format!("sha1:{:x}", Sha1::digest(&bytes)))
}

#[cfg(test)]
mod tests;
