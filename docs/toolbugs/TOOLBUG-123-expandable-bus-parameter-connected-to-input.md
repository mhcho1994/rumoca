# TOOLBUG-123 — parameter member of an expandable connector read by an input (EF028)

**Status:** fixed (accepted without a warning; see below).
**Severity:** low.

## What

```modelica
expandable connector CAN_Msg1  parameter Real myParam = 2; Real rSi1; end CAN_Msg1;
...
connect(gain1.u, cANBus.cAN_Msg1.myParam);   // EF028
```

MLS §9.3 only lets parameters connect to parameters. OpenModelica accepts this
pair when the parameter is a member of an expandable connector and generates
`gain1.u = cANBus.cAN_Msg1.myParam` (checked with omc 1.2x on a reduced
model); for an ordinary connector it keeps the §9.3 assertion. Rumoca rejected
both.

Affected (cluster F, EF028): Modelica_DeviceDrivers
`Incubate.Bustesting.Comp2`, `Incubate.Bustesting.TestTheBus`.

## Fix

`rumoca-phase-flatten/src/connections/member_pairing.rs`: a structural member
declared in an expandable connector paired with a variable is connected, so
the variable side gets the equality. Ordinary connectors still get `EF028`
(CONN-028 contract unchanged).

Flatten has no warning channel into the session diagnostics, so the MLS
deviation is not reported as a warning; that plumbing is left open.

## Test

`member_pairing_tests.rs::expandable_bus_parameter_paired_with_variable_connects`,
`conn_frontend_regressions.rs::expandable_bus_parameter_feeds_block_input`.
