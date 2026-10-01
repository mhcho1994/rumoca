# TOOLBUG-173 — connect slice extent from a package constant (EI004)

**Status:** fixed.
**Severity:** medium.

## What

```modelica
block Src
  replaceable package Medium = ...;
  RealOutput X_in_internal[Medium.nX];
  RealInput Xi_in_internal[Medium.nXi];
equation
  connect(X_in_internal[1:Medium.nXi], Xi_in_internal);
end Src;
```

Connection extraction resolves subscript names only against the class
instance's Integer *parameters*; a package constant such as `Medium.nXi` was
never in that table:
`EI004 cannot evaluate structural parameter connection range`.

Affected in cluster R2-dims: the 6 `connection range` cases
(`AixLib.Airflow.Multizone.Examples.PressurizationData`, Buildings
`MassFlowSource_WeatherData`, EnergyPlus `OneZoneBuilding`, `Zone`,
`ZoneTemperatureInitialization`, IDEAS `OutsideAir`), all from
`Fluid.Sources` boundary models. They now pass this point; the next errors
are elsewhere (e.g. `singleSubstance` conditional, matrix-concatenation
binding shape).

## Fix

`crates/rumoca-phase-instantiate/src/connection_subscript_constants.rs`:
before connection extraction, every multi-segment name in a connect
subscript is evaluated with the scope's modification-aware evaluator and
added to the connection Integer table.

## Test

`array_dimension_regressions.rs::connect_slice_reads_package_constant`.
