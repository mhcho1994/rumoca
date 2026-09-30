# TOOLBUG-077 — `Medium.f(...)` through a redeclared package alias has no exact exposure

**Status:** fixed (this change).
**Severity:** high — EF025 ("missing exact function-selection identity for
`Medium.setState_pTX`: exact callable owner does not expose the selected
implementation") on every model calling a medium function through a
replaceable `Medium` whose selected package redeclares the function.

## What

```modelica
partial package PM
  replaceable partial function f input Real x; output Real y; end f;
end PM;
package Air
  extends PM;
  redeclare function extends f algorithm y := 2*x; end f;
end Air;
model Src
  replaceable package Medium = PM;
  parameter Real T = 3;
  Real z = Medium.f(T);
end Src;
model Top
  Src s(redeclare package Medium = Air);
end Top;
```

Instantiate retargets the call to the selected implementation `Air.f`, but
the prefix keeps its lexical identity, the alias `Src.Medium` (= `PM`).
Flatten looked for an exposure of `Air.f` only in the alias's hierarchy
(`PM`), found none, and refused the call. The baseline binary fails on this
reduction too.

Affected cluster-A models: after TOOLBUG-073/074/075/076 this was the next
error of 16 cluster models (IBPSA/Buildings/AixLib/IDEAS `MixingVolumes`,
`SolarCollectors`, `HeatPumps`, TRANSFORM valves and closure relations).

## Fix

`crates/rumoca-phase-flatten/src/pipeline/function_overrides_and_dims/function_selection.rs`:
when the prefix owner is a replaceable class alias and exposes nothing,
`collect_selected_package_exposures` takes the exposures from the package
that declares the selected implementation. Non-replaceable owners keep the
strict check.

## Test

`crates/rumoca-contracts/tests/func_contracts.rs`:
`func_redeclared_package_function_is_selected_through_alias` simulates the
reduction and checks `s.z = 6`.
