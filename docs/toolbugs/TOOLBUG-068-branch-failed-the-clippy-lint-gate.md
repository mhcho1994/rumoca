# TOOLBUG-068 — the branch failed the workspace clippy gate

**Status:** fixed (this change).
**Severity:** medium — CI's Lint job (`cargo xtask verify lint`, i.e.
`cargo clippy --workspace --all-targets --all-features -- -D warnings`)
failed, so nothing else on the branch was lint-clean either: a crate whose
clippy fails stops clippy for everything that depends on it.

## What

`cargo clippy --workspace --all-targets` reported 91 denied lints, all in code
written on this branch:

- `rumoca-bitcode`: 88, mostly `excessive_nesting` (threshold 4) and
  `too_many_lines` (threshold 100) in the text parser (one 1032-line
  function) and writer (879 lines), import, validate, export and build;
  plus pointless `drop` calls, collapsible `if`s, a needless deref and an
  indexing loop in the call graph.
- `rumoca-phase-solve`: 3 `redundant_guard`s (`Ok(v) if v == 1.0`) from the
  assertion-level and zero-coefficient checks.
- `rumoca-phase-structural` (found with the per-crate run): two functions over
  100 lines and one nested block in the index-reduction differentiability
  check.

Tests were run with `cargo test`, which does not apply clippy, so none of this
showed up in the suites.

## Fix

Behaviour-preserving refactors: long functions split into per-section
helpers, nesting flattened with early returns and helpers, guards turned into
literal patterns. No `#[allow]` was added.

## Guard

Run `cargo clippy --workspace --all-targets -- -D warnings` before pushing,
not only the test suites.
