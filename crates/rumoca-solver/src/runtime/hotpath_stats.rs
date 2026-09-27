use std::cell::Cell;

/// The accepted integration steps and located roots of this thread's
/// simulations since [`reset`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HotpathStatsSnapshot {
    pub solver_steps: u64,
    pub root_hits: u64,
}

thread_local! {
    static SOLVER_STEPS: Cell<u64> = const { Cell::new(0) };
    static ROOT_HITS: Cell<u64> = const { Cell::new(0) };
}

/// Zero this thread's step and root counts.
pub fn reset() {
    SOLVER_STEPS.set(0);
    ROOT_HITS.set(0);
}

/// This thread's step and root counts since [`reset`].
#[must_use]
pub fn snapshot() -> HotpathStatsSnapshot {
    HotpathStatsSnapshot {
        solver_steps: SOLVER_STEPS.get(),
        root_hits: ROOT_HITS.get(),
    }
}

/// Coupled blocks this thread projected through the dense block Newton after
/// their constructor tearing declined, since [`reset_torn_declines`]: the
/// torn-to-dense fallbacks the projection fallback counts record.
#[must_use]
pub fn torn_declines() -> u64 {
    super::fallbacks::projection_fallbacks()
        .sites
        .values()
        .map(|counts| counts.count(super::fallbacks::ProjectionFallback::TornToDense))
        .sum()
}

/// Reset the projection fallback counts [`torn_declines`] reads.
pub fn reset_torn_declines() {
    super::fallbacks::reset_projection_fallbacks();
}

pub(crate) fn inc_solver_step() {
    SOLVER_STEPS.set(SOLVER_STEPS.get() + 1);
}

pub(crate) fn inc_root_hit() {
    ROOT_HITS.set(ROOT_HITS.get() + 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_accumulate_per_thread_until_reset() {
        reset();
        inc_solver_step();
        inc_solver_step();
        inc_root_hit();
        let other = std::thread::spawn(snapshot).join().expect("thread");
        assert_eq!(other, HotpathStatsSnapshot::default());
        assert_eq!(
            snapshot(),
            HotpathStatsSnapshot {
                solver_steps: 2,
                root_hits: 1,
            }
        );
        reset();
        assert_eq!(snapshot(), HotpathStatsSnapshot::default());
    }
}
