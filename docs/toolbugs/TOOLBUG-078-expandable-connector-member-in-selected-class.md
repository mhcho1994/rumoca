# TOOLBUG-078 — undeclared expandable-connector member rejected when re-proved in a selected class

**Status:** fixed (this change).
**Severity:** medium — EI007 ("redeclaration error for `TConOutMea`: selected
redeclare class has no such member").

## What

```modelica
model OperationalEnvelope
  extends BaseClasses.PartialOperationalEnvelope;  // declares sigBus
equation
  if use_TConOutHea then
    connect(bouMapHea.TUseSid, sigBus.TConOutMea);
  ...
```

`sigBus` is an `expandable connector RefrigerantMachineControlBus` with no
declared members; `TConOutMea` is created by the connection (MLS §9.1.3).
When the reference is re-proved against the class instantiation selected
(here `opeEnv` is a replaceable component), every segment after the root had
to be a declared component or nested class, so the undeclared member was
reported as missing.

Affected cluster-A models: IBPSA/IDEAS
`HeatPumps.ModularReversible.Controls.Safety.Examples.{OperationalEnvelope,Safety}`,
IDEAS `HeatPumps.ModularReversible.RefrigerantCycle.ConstantCarnotEffectiveness`
(5 models).

## Fix

`crates/rumoca-phase-instantiate/src/type_overrides/selected_class_members.rs`
(`resolve_member_reference_in_class`): a segment that names no declared
member of an expandable connector ends the proof; the remaining segments
stay unresolved, as they are for a statically typed expandable connector.

## Test

`crates/rumoca-phase-instantiate/src/type_overrides/tests.rs`:
`selected_expandable_connector_member_is_not_required_to_be_declared`.
`arr_013_size_of_undeclared_expandable_member_rejected` (a member no
connection creates) is still rejected, now by Typecheck (ET001) instead of
Instantiate (EI007).
