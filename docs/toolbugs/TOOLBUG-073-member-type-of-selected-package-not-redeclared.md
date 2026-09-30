# TOOLBUG-073 — `Medium.fluidConstants[1].criticalPressure` not found in the selected medium

**Status:** fixed (this change).
**Severity:** high — EI007 ("redeclaration error for `criticalPressure`:
selected redeclare class has no such member") on every model reading a
two-phase medium's critical data through a replaceable `Medium`.

## What

```modelica
partial package PartialMedium
  replaceable record FluidConstants = Types.Basic;
  constant FluidConstants[1] fluidConstants;
end PartialMedium;
partial package PartialTwoPhaseMedium
  extends PartialMedium(redeclare replaceable record FluidConstants = Types.TwoPhase);
end PartialTwoPhaseMedium;
model Valve
  replaceable package Medium = PartialTwoPhaseMedium;
  Real x = Medium.fluidConstants[1].criticalPressure;
end Valve;
```

Once instantiation selected the concrete medium, the deferred reference was
re-proved member by member. The owner class of `fluidConstants` was taken
from the component's lexically resolved type id, i.e. `PartialMedium`'s
`FluidConstants = Types.Basic`, which has no `criticalPressure`. The
redeclaration in `PartialTwoPhaseMedium`'s extends modification (the MSL
structure) was never consulted.

Affected cluster-A models (EI007 `criticalPressure`/`Tsat`-style first
errors): ThermoPower `Test.WaterComponents.TestValveChoked`, TRANSFORM
`Fluid.Valves.ValveVaporizing`, `...ClosureRelations.MassTransfer.Models.Lumped.ConstantTimeDelay`,
`...Pipes_Obsolete...Constant_NusseltNumber_Lumped`, AixLib
`Fluid.Storage.TwoPhaseSeparator`, `Fluid.Movers.Compressors.Validation.EfficiencyModels`.

## Fix

`crates/rumoca-phase-instantiate/src/type_overrides/selected_class_members.rs`:
`member_owner_type_def_id` looks a simple type name up as a member type of
the selected class (`type_lookup::find_member_type_in_class`, which honours
extends-modification redeclarations) before falling back to the lexical type
id.

## Test

`crates/rumoca-contracts/tests/inst_contracts.rs`:
`inst_043_member_type_is_looked_up_in_the_selected_package` simulates the
reduced model and checks `v.x = 22.064e6`.
