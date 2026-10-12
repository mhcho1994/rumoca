//! Bounded opt-in numerical diagnostics. These are not physical trace samples.
#[cfg(test)]
mod tests;

use crate::runtime::projection::BlockJacobian;
use serde_json::{Value, json};
use std::{cell::RefCell, marker::PhantomData, rc::Rc};

const LIMIT: usize = 4096;
const MATRIX_LIMIT: usize = 64;

#[derive(Default)]
struct Evidence {
    records: Vec<Value>,
    omitted: u64,
}

thread_local! {
    static EVIDENCE: RefCell<Option<Evidence>> = const { RefCell::new(None) };
}

/// A synchronous run-local observer, disabled outside this thread-affine scope.
pub struct Scope(PhantomData<Rc<()>>);

impl Scope {
    pub fn start() -> Result<Self, String> {
        EVIDENCE.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.is_some() {
                return Err("nested solver diagnostic scope".into());
            }
            *slot = Some(Evidence::default());
            Ok(Self(PhantomData))
        })
    }

    pub fn finish(self) -> Value {
        let evidence = EVIDENCE.with(|slot| slot.borrow_mut().take().unwrap_or_default());
        json!({"schema_version": 1, "coordinates": "native-internal",
            "coverage": "me-accepted-proposals-events-projection-blocks",
            "records": evidence.records, "omitted": evidence.omitted})
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        EVIDENCE.with(|slot| *slot.borrow_mut() = None);
    }
}

fn record(build: impl FnOnce() -> Value) {
    EVIDENCE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(evidence) = slot.as_mut() else {
            return;
        };
        if evidence.records.len() >= LIMIT {
            evidence.omitted = evidence.omitted.saturating_add(1);
        } else {
            evidence.records.push(build());
        }
    });
}

pub(crate) fn accepted_proposal(start: f64, end: f64) {
    record(|| {
        json!({"kind": "accepted-proposal", "time": end,
        "start": start, "step_size": end - start})
    });
}

pub(crate) fn event(time: f64) {
    record(|| json!({"kind": "event", "time": time}));
}

pub(crate) fn event_iterations(time: f64, iterations: usize, converged: bool) {
    record(|| {
        json!({"kind": "event-iterations", "time": time,
        "iterations": iterations, "converged": converged})
    });
}

/// The projection owner supplies the matrix/residual it actually used. No extra
/// model evaluations, numerical differentiation, rank threshold or SVD is added.
pub(crate) fn projection(
    phase: &str,
    time: f64,
    rows: &[usize],
    columns: &[usize],
    residual: &[f64],
    jacobian: &BlockJacobian,
) {
    record(|| {
        let bounded = rows.len() <= MATRIX_LIMIT && columns.len() <= MATRIX_LIMIT;
        let values = bounded.then(|| jacobian.as_dense().as_slice().to_vec());
        json!({"kind": "projection", "phase": phase, "time": time,
            "rows": if bounded { Some(rows) } else { None },
            "columns": if bounded { Some(columns) } else { None },
            "residual": if bounded { Some(residual) } else { None },
            "jacobian_column_major": values,
            "row_count": rows.len(), "column_count": columns.len(),
            "values_omitted": !bounded})
    });
}
