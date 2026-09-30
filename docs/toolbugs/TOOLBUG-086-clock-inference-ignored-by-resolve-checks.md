# TOOLBUG-086 — Resolve clock checks treat clock-inferred equations as continuous (ER127, ER088)

**Status:** fixed at resolve.
**Severity:** medium.

## What

```modelica
  when Clock() then
    KeyCode = getData();
  end when;
  keyUp = if KeyCode[1] == 1 then true else false;   // ER127
```

and, in a clocked block,

```modelica
  if not previous(initialized) then
    ...
    Modelica.Utilities.Streams.print("...");   // ER088 impure call in continuous-time equation
```

An equation outside a clocked `when` is not continuous-time by itself: clock
inference (MLS §16.7) puts it in the partition of the clocked variables it
uses. Both checks assumed every plain equation was continuous.

Affected (cluster B): Modelica_DeviceDrivers `ClockedBlocks.InputDevices.KeyboardInput`,
`ClockedBlocks.Examples.TestInputKeyboard` (ER127);
`ClockedBlocks.Packaging.SerialPackager.Packager`,
`ClockedBlocks.Examples.TestSerialPackagerBitPack_UDP` (ER088) — 4 models.

## Fix

- `semantic_checks/clocks.rs`: CLK-006 (ER127) only fires for equations that are
  continuous on their own (contain `der()`).
- `semantic_checks/restrictions.rs`: an impure call in a plain equation of a class
  that uses clocked operators (`previous`, `interval`, `subSample`, ...,
  `Clock`) is the warning WR012; purely continuous classes keep ER088.

The models still stop later in DAE construction (clocked partitions with array
equations / `previous` in an if-equation are not lowered yet: ED018/ED019/ED020),
which is outside this cluster.

## Test

`overstrict_checks.rs::clock_inferred_equation_reading_clocked_variable_is_legal`,
`impure_call_in_clocked_class_warns`; contract `clk_006_clocked_variable_continuous_access_rejected`
now uses `der(y) = xc`.
