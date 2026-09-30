# TOOLBUG-104 — callee introduced by constant substitution was never collected

**Status:** fixed.
**Severity:** high — every model that reads an MSL medium constant defined by
a function call, e.g. `Medium.T_default =
Modelica.Units.Conversions.from_degC(20)` in `PartialMedium`, failed with
`ED008 unresolved Flat reference Modelica.Units.Conversions.from_degC`
(`Buildings...TWetBul_TDryBulPhi` in cluster D; after TOOLBUG-102 also the
IBPSA/AixLib `LumpedVolumeDeclarations` and IDEAS propylene-glycol cases).

## What

```modelica
package P
  package Conv
    function from_degC input Real c; output Real k; algorithm k := c + 273.15; end from_degC;
  end Conv;
  package Medium constant Real T_default = P.Conv.from_degC(20); end Medium;
  model M
    parameter Real T_start = Medium.T_default;
    Real x;
  equation
    x = T_start*time;
  end M;
end P;
```

Flatten collects the functions a model calls, then substitutes known package
constants into the model. The substituted value `P.Conv.from_degC(20)` is a
new call that the collected function table never saw, so DAE construction
found a call to an unknown function.

## Fix

`crates/rumoca-phase-flatten/src/pipeline/flatten_pipeline.rs`:
`substitute_constants_collecting_calls` re-runs function collection after
substitution and, when it found new callees, prepares them exactly like
source calls (constructor marking, canonicalization, default arguments, input
specialization, record-parameter lowering) and substitutes again for the
constants those bodies read.

## Test

`crates/rumoca/tests/suite_core/frontend_functions_names.rs`:
`call_introduced_by_constant_substitution_is_collected` (simulates and checks
`T_start = 293.15`).
