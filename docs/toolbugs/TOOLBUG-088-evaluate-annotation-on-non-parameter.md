# TOOLBUG-088 — `annotation(Evaluate=true)` on non-parameters rejected (ER070)

**Status:** fixed (accepted with warning WR006; annotation ignored).
**Severity:** high — 18 models in cluster B.

## What

MLS §18.3: Evaluate "only has effect for a component declared with the prefix
parameter"; it is not made illegal elsewhere, and OpenModelica ignores it.
Rumoca raised ER070 for it on non-parameter components (CDL
`Constant Dzero(...) annotation(Evaluate=true)`, TRANSFORM/IBPSA/IDEAS
`y_reset_internal`) and on class/extends annotations (ThermoPower
`Flow1DBase`). Function locals already had the WR006 advisory.

Affected (cluster B): aixlib 3, buildings 3, ibpsa 3, ideas 3, transform 3,
thermopower 3 — 18 models.

## Fix

- `semantic_checks/annotations.rs`: every non-parameter/non-constant component,
  class and extends annotation `Evaluate` is WR006 ("has no effect").
- `rumoca-phase-instantiate/src/evaluate_annotation.rs`: the annotation sets the
  instance `evaluate` flag only for parameter/constant components, so an ignored
  annotation cannot turn a variable (or the members of a model component) into
  structural values.

## Test

`semantic_rules.rs::test_evaluate_on_model_local_component_is_an_ignored_advisory`;
contracts `ann_008_*_ignored_with_warning`.
