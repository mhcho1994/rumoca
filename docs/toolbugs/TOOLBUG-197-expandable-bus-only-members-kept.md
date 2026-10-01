# TOOLBUG-197 — declared expandable-connector members connected only bus-to-bus are kept

**Status:** open (analysed, not fixed).
**Severity:** low-medium. In R2-balance: Modelica_DeviceDrivers
`Incubate.Bustesting.TestTheBus` (rumoca 22 eq / 26 unknowns, OpenModelica
18/18) and TRANSFORM
`Examples.SystemOfSubSystems.BaseClasses.PlaceHolderModels.SubSystem_PlaceHolder`
(rumoca 18/27, OpenModelica 0/0).

## What

A variable declared in an expandable connector is only *potentially
present* (MLS §9.1.3). When the connection set of such a member contains
nothing but other expandable-connector members (bus connected to bus, no
real connector and no reference supplies or consumes it), OpenModelica
drops the member and its equalities. Rumoca generates the equalities
(`bus1.x = bus2.x`, `bus2.x = bus3.x`) and marks every member `connected`,
so the `UnusedExpandable` role (`rumoca-phase-dae/.../model_roles.rs`,
`from_expandable_connector && !connected && binding.is_none()`) does not
apply: n members become n unknowns with n-1 equations per set.

## Suggested fix

In flatten connection equation generation
(`rumoca-phase-flatten/src/connections/equation_generation.rs`), skip a
potential-variable connection set whose members are all declared
expandable-connector members with no binding and no reference outside
connect equations; leave them unmarked so the DAE treats them as
`UnusedExpandable`.

## Test

None yet.
