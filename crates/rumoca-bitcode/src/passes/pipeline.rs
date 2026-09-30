//! Pass pipelines: what `--pass` accepts, and how it is scheduled.
//!
//! A pipeline is a comma-separated list, and repeated `--pass` flags
//! concatenate. Each element is one of
//!
//! - a pass name (`fold-constants`);
//! - a group: `default` (every pass this build knows, in catalog order),
//!   `O1` (the same), `O0` / `none` / `round-trip` (nothing);
//! - `exec:COMMAND`: an external pass, run as `COMMAND IN.rbc -o OUT.rbc`
//!   ([`super::external`]);
//! - `fixpoint(PIPELINE)`: the inner pipeline, repeated until a round
//!   changes nothing (at most [`FIXPOINT_ROUNDS`] rounds).
//!
//! Scheduling: steps run in the order written, like LLVM's `-passes=`. The
//! scheduler keeps a model generation that every change bumps, and skips a
//! built-in pass when the model has not changed since that pass last ran and
//! left it untouched: built-in passes are deterministic, so the rerun could
//! only report nothing again. That is what makes `default,default` or a
//! converged `fixpoint(...)` round free. External passes are never skipped;
//! nothing is known about them.
//!
//! Every step that runs is followed by dead expression removal, a summary
//! recompute and validation, so a step that breaks the model is named as the
//! one that did.

use std::collections::HashMap;

use super::{Pass, PassError, PassReport, catalog, find, remove_dead_expressions};
use crate::schema::RbcFile;
use crate::validate::{ValidateOptions, recompute_summary, validate};

/// The most rounds a `fixpoint(...)` runs before it gives up converging.
pub const FIXPOINT_ROUNDS: usize = 16;

/// One parsed pipeline element.
#[derive(Clone)]
pub enum Step {
    Builtin(&'static dyn Pass),
    External(String),
    Fixpoint(Vec<Step>),
}

impl std::fmt::Debug for Step {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Step::Builtin(pass) => write!(f, "{}", pass.name()),
            Step::External(command) => write!(f, "exec:{command}"),
            Step::Fixpoint(steps) => write!(f, "fixpoint({steps:?})"),
        }
    }
}

/// Parse `--pass` values into steps.
pub fn parse(values: &[String]) -> Result<Vec<Step>, PassError> {
    let mut steps = Vec::new();
    for value in values {
        steps.extend(parse_list(value)?);
    }
    Ok(steps)
}

fn parse_list(text: &str) -> Result<Vec<Step>, PassError> {
    let mut steps = Vec::new();
    for element in split_top_level(text)? {
        steps.extend(parse_element(element.trim())?);
    }
    Ok(steps)
}

/// Split at commas outside parentheses. An `exec:` element runs to the end
/// of its level, so its command may contain commas.
fn split_top_level(text: &str) -> Result<Vec<&str>, PassError> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, ch) in text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| PassError(format!("unbalanced `)` in pass pipeline `{text}`")))?
            }
            ',' if depth == 0 && !text[start..].trim_start().starts_with("exec:") => {
                parts.push(&text[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(PassError(format!(
            "unbalanced `(` in pass pipeline `{text}`"
        )));
    }
    parts.push(&text[start..]);
    Ok(parts)
}

fn parse_element(element: &str) -> Result<Vec<Step>, PassError> {
    if let Some(command) = element.strip_prefix("exec:") {
        let command = command.trim();
        if command.is_empty() {
            return Err(PassError("`exec:` needs a command".into()));
        }
        return Ok(vec![Step::External(command.to_string())]);
    }
    if let Some(inner) = element
        .strip_prefix("fixpoint(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        let inner = parse_list(inner)?;
        return Ok(if inner.is_empty() {
            Vec::new()
        } else {
            vec![Step::Fixpoint(inner)]
        });
    }
    match element {
        "" | "none" | "O0" | "round-trip" => Ok(Vec::new()),
        "default" | "O1" => Ok(catalog().into_iter().map(Step::Builtin).collect()),
        name => find(name)
            .map(|pass| vec![Step::Builtin(pass)])
            .ok_or_else(|| {
                let known: Vec<_> = catalog().iter().map(|pass| pass.name()).collect();
                PassError(format!(
                    "unknown pass `{name}`; known passes: default, O1, O0, none, round-trip, \
                 exec:COMMAND, fixpoint(...), {}",
                    known.join(", ")
                ))
            }),
    }
}

/// Runs steps over one artifact, tracking what each change invalidates.
#[derive(Default)]
pub struct Scheduler {
    generation: u64,
    /// For a built-in pass: the generation at which it last ran and changed
    /// nothing.
    clean_at: HashMap<&'static str, u64>,
    pub reports: Vec<PassReport>,
    /// Built-in passes skipped because the model had not changed.
    pub skipped: usize,
}

impl Scheduler {
    pub fn run(&mut self, file: &mut RbcFile, steps: &[Step]) -> Result<(), PassError> {
        for step in steps {
            match step {
                Step::Builtin(pass) => self.builtin(file, *pass)?,
                Step::External(command) => self.external(file, command)?,
                Step::Fixpoint(inner) => self.fixpoint(file, inner)?,
            }
        }
        Ok(())
    }

    fn builtin(&mut self, file: &mut RbcFile, pass: &'static dyn Pass) -> Result<(), PassError> {
        if self.clean_at.get(pass.name()) == Some(&self.generation) {
            self.skipped += 1;
            return Ok(());
        }
        let rewrites = pass.run(&mut file.model)?;
        self.settle(file, pass.name().to_string(), rewrites)?;
        if self.reports.last().is_some_and(PassReport::changed) {
            self.clean_at.remove(pass.name());
        } else {
            self.clean_at.insert(pass.name(), self.generation);
        }
        Ok(())
    }

    fn external(&mut self, file: &mut RbcFile, command: &str) -> Result<(), PassError> {
        let changed = super::external::run(file, command)?;
        self.settle(file, format!("exec:{command}"), usize::from(changed))
    }

    fn fixpoint(&mut self, file: &mut RbcFile, inner: &[Step]) -> Result<(), PassError> {
        for _ in 0..FIXPOINT_ROUNDS {
            let before = self.generation;
            self.run(file, inner)?;
            if self.generation == before {
                return Ok(());
            }
        }
        Err(PassError(format!(
            "fixpoint({inner:?}) did not converge in {FIXPOINT_ROUNDS} rounds"
        )))
    }

    /// Drop dead expressions, recompute the summary and validate after one
    /// step; record its report and bump the generation if it changed anything.
    fn settle(
        &mut self,
        file: &mut RbcFile,
        pass: String,
        rewrites: usize,
    ) -> Result<(), PassError> {
        let removed_expressions = remove_dead_expressions(&mut file.model).map_err(|error| {
            PassError(format!("`{pass}` produced an invalid model: {}", error.0))
        })?;
        recompute_summary(&mut file.model);
        validate(
            &file.model,
            &ValidateOptions {
                reject_unsupported: true,
            },
        )
        .map_err(|errors| {
            PassError(format!(
                "`{pass}` produced an invalid model: {}",
                errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            ))
        })?;
        let report = PassReport {
            pass,
            rewrites,
            removed_expressions,
        };
        if report.changed() {
            self.generation += 1;
        }
        self.reports.push(report);
        Ok(())
    }
}
