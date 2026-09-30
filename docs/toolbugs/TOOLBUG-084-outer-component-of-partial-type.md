# TOOLBUG-084 — `outer` component of a partial type rejected as partial instantiation

**Status:** fixed.
**Severity:** medium.

## What

```modelica
model DummyTyre
  outer VehicleInterfaces.Roads.Interfaces.Base road;   // ER005, then EI012
```

A pure `outer` declaration references the matching `inner` element (MLS §5.4);
it is not an instantiation, so its type may be partial. Both the resolve check
(ER005) and instantiation (EI012) treated it as one.

Affected (cluster B): VehicleInterfaces — 3 models.

## Fix

`semantic_checks/mod.rs::check_partial_class_instantiation_restriction` and
`rumoca-phase-instantiate/src/lib.rs::validate_partial_component_instantiation`
skip `outer` (non-`inner`) components. A standalone model with no matching
`inner` still reports the missing inner.

## Test

`overstrict_checks.rs::outer_component_of_partial_type_is_not_instantiation`.
