# TOOLBUG-032 — `cargo xtask verify lint` has never passed on this branch

**Status:** open, unowned. Not introduced by the Execution IR v2 milestone.

## What

`cargo xtask verify lint` runs `cargo clippy --workspace --all-targets
--all-features -- -D warnings`. It fails at branch HEAD `c8f0254f` with **77
findings**, every one of them inside `rumoca-bitcode`:

| File | Findings |
|---|---|
| `crates/rumoca-bitcode/src/text/parser.rs` | 46 |
| `crates/rumoca-bitcode/src/text/writer.rs` | 12 |
| `crates/rumoca-bitcode/src/import.rs` | 12 |
| `crates/rumoca-bitcode/src/validate.rs` | 6 |
| `crates/rumoca-bitcode/src/export.rs` | 5 |
| `crates/rumoca-bitcode/src/build.rs` | 1 |

By lint: 68 `excessive_nesting`, 6 `too_many_lines` (the largest is 936 lines
against a 100-line ceiling), 3 `explicit_deref_methods`, 2 `collapsible_if`,
1 `while_let_loop`.

## Why it was not noticed

Clippy stops at the first crate that fails to compile. Until the Execution IR
v2 milestone, `rumoca-phase-dae` failed first with two findings of its own, so
the run never reached `rumoca-bitcode` and the backlog behind it was invisible.
Fixing those two made the rest appear. Nothing got worse; the measurement got
honest.

## Evidence

Counts and per-file distribution are **byte-identical** between the working
tree and a clean worktree at `c8f0254f`, confirming none of the 77 comes from
the milestone. The milestone's own additions to `rumoca-bitcode` are
`validate_execution_references` and the `ValidationError::ExecutionReference`
variant, at `validate.rs:24-28` and `112-164`; the six `validate.rs` findings
are at lines 449, 585, 598, 616 and 1025.

## Why it is not fixed here

`excessive_nesting` in a recursive-descent parser is not a mechanical fix: the
68 sites are match arms and error closures whose nesting *is* the grammar.
Doing them inside a milestone that touches two of those files would put ~70
unrelated hunks into a diff whose subject is the wire format, and the
`too_many_lines` findings overlap SPEC_0021, which has its own ledger and
acknowledgement process.

## What was fixed

Ten findings that `git blame` attributes to work committed earlier in the same
session, plus two from the milestone itself:

- `rumoca-phase-dae/src/construction/analysis/discrete_values.rs:180,189`
- `rumoca-phase-flatten/src/connections/mod.rs:884`
- `rumoca-phase-flatten/src/connections/expandable.rs:75`
- `rumoca-phase-flatten/src/functions/higher_order/captures.rs` ×3
- `rumoca-phase-flatten/src/functions/higher_order.rs:141`
- `rumoca-eval-solve/src/execution.rs` ×2 (milestone code)

Two pre-existing gate failures outside clippy were also fixed, because they are
one-line mechanical corrections rather than refactors: a `cargo fmt` ordering
nit in `rumoca-solver/src/fmi_me.rs`, and three ambiguous rustdoc intra-doc
links in `rumoca-bitcode` where `validate` and `import` are each both a module
and a function.

## Suggested disposition

Treat the 68 nesting findings in the text parser and writer as one focused
cleanup with its own commit, and the 6 `too_many_lines` findings as SPEC_0021
ledger entries. Until then `verify lint` stays red and cannot gate anything.
