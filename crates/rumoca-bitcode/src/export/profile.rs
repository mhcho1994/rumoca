//! Reject semantic owners the public schema cannot faithfully transport.
//!
//! Every DAE owner table is carried as of this version, so nothing is
//! refused here. The check stays as the one place a future owner table must
//! be listed until the schema learns to carry it.
use super::*;

pub(super) fn check(view: dae::DaeView<'_>) -> Result<(), ExportError> {
    let owners: [(&'static str, usize); 0] = [];
    let _ = view;
    if let Some((name, _)) = owners.into_iter().find(|(_, count)| *count != 0) {
        return Err(ExportError::UnsupportedOwner(name));
    }
    Ok(())
}
