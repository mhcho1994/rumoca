# TOOLBUG-076 — constraining-clause modifiers not applied to the declaration

**Status:** fixed (this change).
**Severity:** high — EI012 ("cannot instantiate partial class
`Medium.BaseProperties` for component `volDyn.dynBal.medium`") and silently
wrong parameter values.

## What

```modelica
replaceable IBPSA.Fluid.MixingVolumes.MixingVolume volDyn
  constrainedby IBPSA.Fluid.MixingVolumes.MixingVolume(
    redeclare package Medium = Medium, V = 1, nPorts = 2, ...);
```

MLS §7.3.2: "The modifiers following the constraining type name are applied
both for the purpose of defining the actual constraining type and they are
automatically applied in the declaration and in any subsequent
redeclaration. ... declaration modifiers override constraining type
modifiers." The parser kept these modifiers only as `__constrainedby__.`
prefixed copies that were activated on redeclaration. Without a
redeclaration `volDyn` got the partial default `Medium`, `V` and `nPorts`.

Affected cluster-A models: the Buildings/IBPSA/AixLib
`MixingVolumes.Validation` family (`MixingVolumeHeatReverseFlow*`,
`MixingVolumeMoistureReverseFlow*`) and every library model declaring a
replaceable sub-model with constraining modifiers.

## Fix

`crates/rumoca-phase-parse/src/elements.rs`:
`apply_constraining_clause_to_declaration` adds each constraining modifier
the declaration does not state itself to the declaration's source
modifications (with its each/final/redeclare flags) and processes it like a
declaration modifier. The prefixed copies still serve redeclarations.

## Test

`crates/rumoca-contracts/tests/inst_contracts.rs`:
`inst_043_constraining_clause_modifiers_apply_to_the_declaration` (medium
selected, `V = 2` from the clause, `V = 5` when the declaration states it).
