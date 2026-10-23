# Draft upstream report — Rumoca compiler defects

**Status: DRAFT. Not filed anywhere.**
Target: `Arslan8/rumoca` (this fork) or upstream `cognipilot/rumoca`.

**October 1 review:** BUG-001 and BUG-004 still reproduce on the local branch;
BUG-007's example compiles. Clean-upstream reproduction remains outstanding,
so the historical “Ready to send” heading below does not establish upstream
attribution. See [updated drafts and outputs](reporting-2026-10-01/README.md).

These are defects in the **compiler**, not in any model, so they are not
ModelSan findings — they live here only because this is where the drafts are
collected. The reports themselves are in [`../toolbugs/`](../toolbugs/).

Each was found by compiling a corpus of 847 models rather than by reading code.

## Ready to send

### 1. Panic on a comprehension outside three expression lists

[BUG-001](../verified%20bugs/BUG-001-comprehension-in-for-equation-panic.md) —
**process abort, no diagnostic.** 7-line reproducer, no loop:

```modelica
model C6
  Real x(start=1,fixed=true); Real s;
algorithm
  s := sum({x*i for i in 1:2});
equation
  der(x) = -x;
end C6;
```

Root cause verified: `all_model_expressions` (`expression.rs:1930`) walks 3 of
the 9 expression-bearing fields of `flat::Model`. Four of its five other callers
each compensate with a *different* ad-hoc set of additions;
`analyze_comprehensions` is the one that trusts the name, and it is the one that
panics. Confirmed by prediction: `initial equation` (walked) compiles, `assert`
(not walked) panics.

The `for`-equation trigger is separate and **not** diagnosed — those expressions
are expanded into `flat.equations` and *are* walked. Said so explicitly in the
report rather than implying one cause covers all four triggers.

### 2. `AssertionLevel` unusable — with a fix

[BUG-007](../verified%20bugs/BUG-007-dae-drops-predefined-enum-in-assert.md) —
**fixed in this branch**, so this can go as a patch rather than a report.
Two stacked defects: `validate_assertions` read coordinate roles instead of
expression roles, and `assert_with_level` demanded a numeric level where
MLS §11.2.3 types it as an enumeration. 27 uses across 7 MSL files.

### 3. Record arrays destructured without their subscript

[BUG-004](../verified%20bugs/BUG-004-record-array-destructured-without-subscript.md) —
blocks all of `QuasiStatic.Polyphase`, ~30 corpus models. Report only; the fix
needs a mutation API on `Reference`, whose parts are private. The first attempt
and why it was wrong are recorded in
[coverage-blockers](../toolbugs/coverage-blockers.md).

## Worth sending as feedback rather than bugs

- [LIMITATION-002](../toolbugs/LIMITATION-002-no-solver-tolerance-control.md) —
  `compile-bitcode --simulate` exposes no `--rtol`/`--atol`, so a sub-percent
  trajectory difference against another tool cannot be attributed to either.
  Small surface change; large effect on differential testing.
- Coverage: `ED019` accounts for 267 of 515 compile failures, 86 of them MLS
  §12.9 external objects. See
  [coverage-blockers](../toolbugs/coverage-blockers.md).

## Reviewer checklist before filing

- [ ] Confirm which remote this fork reports to
- [ ] BUG-007 as a PR against this branch, or as an issue upstream?
- [ ] Re-run BUG-001's reproducers against upstream `main`, not this branch
