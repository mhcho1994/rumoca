# TOOLBUG-101 — function call inside an array constructor lost the iterator

**Status:** fixed.
**Severity:** medium — `ED008 unresolved Flat reference i` on
`IBPSA/IDEAS/AixLib.Utilities.Psychrometrics.X_pTphi` and the models that
use it (`MassFraction_pTphi`, `TWetBul_TDryBulXi`), and on Buildings
`ConductorSingleLayerCylinder`, `Comfort.Examples.Fanger` (6 cases in
cluster D report this message).

## What

```modelica
model M
  function g input Real a; output Real r; algorithm r := 2*a; end g;
  constant Real v[2] = {3, 4};
  parameter Real s = sum({g(v[i]) for i in 1:2});
  Real x;
equation
  x = s*time;
end M;
```

DAE construction proves the shape of every function call before lowering it.
Call discovery (`FunctionShapeDiscovery::discover_calls`) walked into an
array constructor through the generic child iterator, so the call `g(v[i])`
was shape-checked in the model scope, where the iterator `i` is not bound.
The shape-proof callback had the same defect in the later pass: the closure
that resolves a call's result shape captured the environment it was created
in, not the one the call was written in.

## Fix

`crates/rumoca-phase-dae/src/construction/function_shapes/`:
- `discover_comprehension_calls` discovers calls in the body and filter of an
  array constructor with the iterators bound (with their proven range
  bounds), and in the ranges without them (MLS §10.4.2).
- `FunctionResultShape` now receives the environment of the call site, so
  both closures prove argument shapes in the scope that binds the iterator.

## Test

`crates/rumoca/tests/suite_core/frontend_functions_names.rs`:
`call_reading_a_comprehension_iterator_is_shaped_in_the_iterator_scope`
(simulates and checks `s = 14`).

## Remaining

`X_pTphi`'s binding compares `Medium.substanceNames[i]` with a string through
`Modelica.Utilities.Strings.isEqual`. It now compiles, but the binding is not
folded at translation time, and simulation preparation cannot pass a String
to a pure call (`EC006 String cannot be evaluated as a numeric value`).
