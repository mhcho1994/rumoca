//! Reject semantic owners the public schema cannot faithfully transport.
use super::*;

pub(super) fn check(view: dae::DaeView<'_>) -> Result<(), ExportError> {
    for index in 0..view.function_count() {
        let Some(function) = view.function_id(index).and_then(|id| view.function(id)) else {
            continue;
        };
        for ordinal in 0..function.fold_count() {
            if let Some(fold) = function
                .fold_id(ordinal)
                .and_then(|id| view.function_fold(id))
                && fold.continuation().is_some()
            {
                return Err(ExportError::UnsupportedOwner("bounded while continuations"));
            }
        }
    }
    Ok(())
}
