# TOOLBUG-140 — function body scoped in a redeclared package's partial default

**Status:** fixed.
**Severity:** high — every `Medium.f(state)` call whose medium is selected by
`redeclare package Medium = ...` on a component or extends clause, and whose
implementation reads a member of the redeclared `ThermodynamicState`, failed
with `EF024 flat variable is missing structured identity: T` (or `X`, `h`, ...)
at `T := state.T` in `PartialSimpleMedium.temperature` and similar.

## What

```modelica
partial package PM
  replaceable record State end State;
  replaceable partial function temperature input State state; output Real T; end temperature;
end PM;
partial package PS
  extends PM;
  redeclare record extends State Real T; end State;
  redeclare function extends temperature algorithm T := state.T; end temperature;
end PS;
package W extends PS; end W;
model Q
  replaceable package Medium = PM;
  Medium.State st(T = 2);
  Real T = Medium.temperature(st);
end Q;
model D Q q(redeclare package Medium = W); end D;   // also: extends Q(redeclare ...)
```

Instantiation selects `PS.temperature` correctly, but Flatten converted its
body under the exposed name `Q.Medium.temperature`. In the class tree,
`Q.Medium` still denotes the declared default `PM`, so the formal `state` was
typed `PM.State` (empty): `state.T` had no member identity (EF024), and once
that was papered over the record argument expanded to no fields
(`Medium.temperature()`).

Affected in cluster R2-media: 21 of the 30 EF024 cases (the `T`/`X`
`missing structured identity` group: AixLib/Buildings/IBPSA/IDEAS
MixingVolumes validations, `PressureDrop.*Optimised`, `Media.Examples.*`,
`Sensors.Examples.TemperatureDryBulb`, ThermoPower `Flow1DFEM`, ...). They
now reach later, unrelated errors.

## Fix

`crates/rumoca-phase-flatten/src/functions.rs`:
`request_exposed_qualified_name` accepts the owner-prefix exposure only when
that name, looked up in the class tree, denotes the selected implementation.
Otherwise the function is converted under its declaring class
(`PS.temperature`), whose scope holds the redeclared `State`.

## Test

`crates/rumoca-contracts/tests/func_contracts.rs::function_selected_by_instance_package_redeclaration_keeps_record_members`
(component and extends redeclaration; simulates and checks `der(w) = state.T`).
