//! In-compiler bitcode passes: `Flat -> DAE -> RBC -> [passes] -> DAE`.
//!
//! External passes are programs that read and write `.rbc`
//! ([writing-a-bitcode-pass.md](../../../docs/writing-a-bitcode-pass.md)).
//! These are the same idea run inside `rumoca compile`, so an optimization
//! that lives here is still applied by the compiler, and one that is *moved*
//! here out of lowering keeps working for everyone who never touches an
//! artifact.
//!
//! A pass rewrites an [`RbcModel`] in place. The pipeline then drops
//! expressions nothing references any more, recomputes the summary and
//! validates, so a pass may simply stop referring to a node and leave the
//! bookkeeping to the pipeline. The result is rebuilt into a checked DAE by
//! [`crate::import()`], so no pass can produce a model the compiler's own
//! constructors would reject.

mod asserts;
mod constants;
mod evaluate;
mod functions;
mod inline_constants;
mod pure_calls;

use crate::schema::*;
use crate::validate::{ValidateOptions, recompute_summary, validate};

/// A rewrite over the equation IR.
pub trait Pass: Sync {
    /// Stable name used on the command line and in reports.
    fn name(&self) -> &'static str;
    /// One line on what it does.
    fn description(&self) -> &'static str;
    /// Rewrite the model; report how many rewrites were made.
    fn run(&self, model: &mut RbcModel) -> Result<usize, PassError>;
}

#[derive(Debug, thiserror::Error)]
#[error("bitcode pass: {0}")]
pub struct PassError(pub String);

impl From<crate::link::LinkError> for PassError {
    fn from(error: crate::link::LinkError) -> Self {
        Self(error.0)
    }
}

/// What one pass did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassReport {
    pub pass: &'static str,
    /// Rewrites the pass made.
    pub rewrites: usize,
    /// Expressions the pipeline dropped after it because nothing referenced
    /// them any more.
    pub removed_expressions: usize,
}

static DEAD_EXPRESSIONS: DeadExpressions = DeadExpressions;
static PRUNE_FUNCTIONS: functions::PruneFunctions = functions::PruneFunctions;
static INLINE_CONSTANTS: inline_constants::InlineConstants = inline_constants::InlineConstants;
static FOLD_CONSTANTS: constants::FoldConstants = constants::FoldConstants;
static FOLD_PURE_CALLS: pure_calls::FoldPureCalls = pure_calls::FoldPureCalls;
static FOLD_ASSERTS: asserts::FoldAsserts = asserts::FoldAsserts;

/// Every pass this build knows, in the order `--pass default` runs them.
///
/// Constants are inlined before folding, so the arithmetic they feed folds
/// in the same run. Folding first, then pruning: a call folded to its value leaves its
/// callee unreachable, and dropping it is the point of asking for
/// optimization. (The frontend must prune *before* it folds, because there
/// the result is the compiler's default artifact and a declared function
/// vanishing from it because of how one call site was written is not
/// recoverable -- TOOLBUG-029. A pass runs only when asked.)
pub fn catalog() -> [&'static dyn Pass; 6] {
    [
        &INLINE_CONSTANTS,
        &FOLD_CONSTANTS,
        &FOLD_PURE_CALLS,
        &FOLD_ASSERTS,
        &PRUNE_FUNCTIONS,
        &DEAD_EXPRESSIONS,
    ]
}

/// The pass with this name.
pub fn find(name: &str) -> Option<&'static dyn Pass> {
    catalog().into_iter().find(|pass| pass.name() == name)
}

/// Run named passes in order over an artifact.
///
/// `default` expands to the whole catalog. Each pass is followed by dead
/// expression removal and validation, so a pass that breaks the model is
/// named as the one that did.
pub fn run(file: &mut RbcFile, names: &[String]) -> Result<Vec<PassReport>, PassError> {
    let mut passes = Vec::new();
    for name in names {
        if name == "default" {
            passes.extend(catalog());
            continue;
        }
        // Round trip only: export and rebuild with nothing in between, which
        // is how the stage's own fidelity is measured. (`none` alone never
        // reaches here: the compiler skips the stage for it.)
        if name == "round-trip" || name == "none" {
            continue;
        }
        passes.push(find(name).ok_or_else(|| {
            let known: Vec<_> = catalog().iter().map(|pass| pass.name()).collect();
            PassError(format!(
                "unknown pass `{name}`; known passes: default, none, round-trip, {}",
                known.join(", ")
            ))
        })?);
    }
    let mut reports = Vec::with_capacity(passes.len());
    for pass in passes {
        let rewrites = pass.run(&mut file.model)?;
        let removed_expressions = remove_dead_expressions(&mut file.model)?;
        recompute_summary(&mut file.model);
        validate(
            &file.model,
            &ValidateOptions {
                reject_unsupported: true,
            },
        )
        .map_err(|errors| {
            PassError(format!(
                "`{}` produced an invalid model: {}",
                pass.name(),
                errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            ))
        })?;
        reports.push(PassReport {
            pass: pass.name(),
            rewrites,
            removed_expressions,
        });
    }
    Ok(reports)
}

/// Which expressions are reachable from outside the arena, operands closed.
pub(crate) fn live_expressions(model: &RbcModel) -> Result<Vec<bool>, PassError> {
    let mut live = crate::link::expression_roots(model)?;
    // Operands are strictly earlier, so one backward sweep closes the set.
    for index in (0..live.len()).rev() {
        if !live[index] {
            continue;
        }
        let id = ExprId(index as u32);
        for operand in crate::build::references(id, &model.expressions[index].node) {
            if let Some(slot) = live.get_mut(operand.0 as usize) {
                *slot = true;
            }
        }
    }
    Ok(live)
}

/// Old-to-new ids for a keep mask.
pub(crate) fn compaction(keep: &[bool]) -> Vec<Option<u32>> {
    let mut next = 0u32;
    keep.iter()
        .map(|&kept| {
            kept.then(|| {
                next += 1;
                next - 1
            })
        })
        .collect()
}

/// Drop every expression nothing references; return how many.
pub fn remove_dead_expressions(model: &mut RbcModel) -> Result<usize, PassError> {
    let live = live_expressions(model)?;
    let removed = live.iter().filter(|kept| !**kept).count();
    if removed == 0 {
        return Ok(0);
    }
    let functions: Vec<Option<u32>> = (0..model.functions.len() as u32).map(Some).collect();
    crate::link::renumber(model, &compaction(&live), &functions)?;
    Ok(removed)
}

/// Dead expression removal as a pass of its own.
struct DeadExpressions;

impl Pass for DeadExpressions {
    fn name(&self) -> &'static str {
        "dead-expressions"
    }
    fn description(&self) -> &'static str {
        "drop expressions nothing references"
    }
    fn run(&self, model: &mut RbcModel) -> Result<usize, PassError> {
        remove_dead_expressions(model)
    }
}

#[cfg(test)]
mod tests;
