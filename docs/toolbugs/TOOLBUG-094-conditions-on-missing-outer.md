# TOOLBUG-094 — conditions that read a missing `outer` element

**Status:** fixed (this change).
**Severity:** medium — `EI006 conditional component sphere …` for any model
that uses `MultiBody.Parts.Body` without declaring `inner world`.

## What

`Parts.Body` declares `sphere … if world.enableAnimation and animation and
sphereDiameter > 0` with `sphereDiameter = world.defaultBodyDiameter`. MLS
§5.4 lets a tool synthesize the default inner when none is declared, which
Rumoca does, but:

1. the first instantiation pass failed on the condition (no inner yet) and
   returned that error before the synthesized-inner retry could run, and
2. the retry registered only the synthesized inner's Boolean parameters, not
   its Real ones (`defaultBodyDiameter = nominalLength/9`).

Affected cluster-C models: `VehicleInterfaces.Chassis.MinimalChassis2/3`.

## Fix

- `rumoca-phase-instantiate/src/entry.rs`: when the first pass fails and left
  inners missing, retry with synthesized inners; the original error stands if
  the retry fails too.
- `rumoca-phase-instantiate/src/inner_outer.rs`: register the synthesized
  inner's Real parameter defaults as a declared, unmodified inner's are.

## Test

`structural_evaluation_regressions.rs`:
`conditions_on_missing_outer_use_the_synthesized_inner`.
