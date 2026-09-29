//! Rumoca Bitcode — a public, versioned interchange format for compiled
//! Modelica models.
//!
//! ```text
//! Modelica → Rumoca → .rbc → external pass → .rbc → Rumoca → Solve/codegen
//! ```
//!
//! The file format is the interface. An external pass needs no Rumoca source
//! checkout, no Rust toolchain match, and no link against Rumoca's crates.
//!
//! # Layering
//!
//! [`schema`] is the public contract. [`export()`] projects a checked DAE into
//! it; [`import()`] validates an artifact and rebuilds a checked DAE from it.
//! Neither direction exposes an internal Rumoca type, and the internal
//! `DAE_SCHEMA_VERSION` is free to change without touching
//! [`schema::RBC_VERSION`].
//!
//! # Trust
//!
//! An artifact that has been through an external pass is untrusted. Import
//! runs structural validation first, then rebuilds through the DAE's own
//! checked constructors, so a malformed artifact is rejected rather than
//! producing an invalid model.

pub mod build;
pub mod codec;
mod connector_validation;
pub mod export;
pub mod import;
pub mod link;
pub mod passes;
pub mod schema;
pub mod text;
pub mod validate;

pub use codec::{
    CodecError, Encoding, decode, dump_json, encode, read_file, to_json, transcode, write_file,
};
pub use export::{ExportError, ExportOptions, export};
pub use import::{ImportError, import};
pub use schema::{RBC_MAGIC, RBC_VERSION, RbcFile, RbcModel};
pub use validate::{ValidationError, validate};

#[cfg(test)]
mod tests;
