# TOOLBUG-151 — Enumeration literals in assertion conditions and initial-algorithm levels (ED008)

**Status:** fixed.
**Severity:** medium.

## What

Two paths validated assertion arguments against the coordinate plan, which
does not contain enumeration literals (MLS §4.9.5):

* the condition of every continuous/initial `assert`
  (`assert(not referenceFrequency == ReferenceFrequency.adaptiveReferenceGenerators, ...)`,
  PowerGrids `Electrical.System`);
* the `message`/`level` of an `assert` statement in an `initial algorithm`
  (`assert(a == b, "...", AssertionLevel.error)`, Buildings/IDEAS
  `RefrigerantCycleConditional`/`RefrigerantCycle`), which
  `analyze_initial_algorithms` re-validated before the shared assertion check.

Both failed with `ED008 unresolved Flat reference`.

Affected (cluster R2-fnbody): PowerGrids `BaseControllerFramework`,
`IEEE_AC4A`, `IEEE_TGOV1`; Buildings `RefrigerantCycleConditional`; IDEAS
`RefrigerantCycle` — 5 models.

## Fix

`crates/rumoca-phase-dae/src/construction/analysis.rs`,
`analysis/event_conditions.rs`, `analysis/initial_algorithms.rs`: assertion
conditions are validated with the expression roles plus the model's literal
catalog (`validate_assertion_condition`, the same rule a `when` activation
guard uses); the initial-algorithm replay only checks the condition and leaves
`message`/`level` to the shared assertion validation.

## Test

`function_body_and_binding_regressions.rs::assertions_resolve_enumeration_literals`.
