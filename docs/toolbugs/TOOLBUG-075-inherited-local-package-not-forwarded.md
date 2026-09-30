# TOOLBUG-075 — `redeclare package Medium = Medium` in a derived model forwards the partial default

**Status:** fixed (this change).
**Severity:** high — EI012 ("cannot instantiate partial class
`Medium.BaseProperties` for component `sou.medium`").

## What

```modelica
partial model Base
  package Medium = Air;
  Source sou(redeclare package Medium = Medium);
end Base;
model Derived
  extends Base;
end Derived;
```

The forwarding redeclaration `redeclare package Medium = Medium` is resolved
through the type-override map of the class being instantiated. That map
listed the class's *own* nested classes, its enclosing package's, and its
extends-modification redeclarations, but not nested classes inherited from
base classes. `Base` alone compiled; `Derived` found no `Medium`, left
`sou.Medium` at its partial default and failed on `Medium.BaseProperties`.
This is the Buildings/IBPSA/AixLib validation pattern
(`MixingVolumes.Validation.BaseClasses.MixingVolumeReverseFlow` declares
`package Medium = ...Air`, the validation models extend it).

Affected cluster-A models: `MixingVolumes.Validation.MixingVolume*ReverseFlow*`
(AixLib, Buildings, IBPSA), `HeatPumps.Validation.*_ScalingFactor`,
`Movers.Validation.FlowControlled_*` (IDEAS), `Templates.Components.Pumps.Multiple`
and similar EI012 `Medium.BaseProperties` cases.

## Fix

`crates/rumoca-phase-instantiate/src/type_overrides/override_collection.rs`
(`build_type_override_map`): after the class's own nested classes, the nested
classes and extends-modification redeclarations of its base classes are
collected (most-derived first, not overriding entries already present),
before the enclosing package's. The class's own extends-modification
redeclarations still override them.

## Test

`crates/rumoca-contracts/tests/inst_contracts.rs`:
`inst_043_forwarded_package_redeclare_in_derived_model_uses_inherited_package`
simulates the reduced model (`sou.medium.p = 1`).
