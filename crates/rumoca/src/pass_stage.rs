//! The in-compiler bitcode pass stage: `DAE -> RBC -> [passes] -> DAE`.
//!
//! Runs only when passes are requested (`rumoca compile --pass NAME`). The
//! DAE the frontend produced is exported, rewritten by
//! [`rumoca_bitcode::passes`], and rebuilt through the DAE's checked
//! constructors, so the rest of the compiler -- solve, simulation, code
//! generation -- consumes the rewritten model exactly as it would the
//! original. See docs/design/minimal-frontend.md.

use std::sync::Arc;

use rumoca_compile::compile::{Dae, FlatModel};

use crate::CompilerError;

/// Rewrite `dae` through the named passes and return the rebuilt model.
pub(crate) fn apply(
    dae: &Dae,
    flat: &FlatModel,
    model_name: &str,
    passes: &[String],
    verbose: bool,
) -> Result<Arc<Dae>, CompilerError> {
    let failed =
        |stage: &str, error: String| CompilerError::BitcodePassError(format!("{stage}: {error}"));
    let mut file = rumoca_bitcode::export(
        dae,
        Some(flat),
        model_name,
        &rumoca_bitcode::ExportOptions::default(),
    )
    .map_err(|error| failed("export", error.to_string()))?;
    let reports = rumoca_bitcode::passes::run(&mut file, passes)
        .map_err(|error| failed("pass", error.to_string()))?;
    if verbose {
        for report in &reports {
            eprintln!(
                "[rumoca] pass {}: {} rewrite(s), {} expression(s) dropped",
                report.pass, report.rewrites, report.removed_expressions
            );
        }
    }
    let rebuilt =
        rumoca_bitcode::import(&file).map_err(|error| failed("rebuild", error.to_string()))?;
    Ok(Arc::new(rebuilt))
}
