# TOOLBUG-122 — `size(a, k)` in a component-array dimension (EF002)

**Status:** fixed.
**Severity:** medium, and silent in part: an unevaluable component-array
dimension made `evaluate_array_dimensions` answer "scalar".

## What

```modelica
parameter Real timPer[5] = {10,30,120,240,360}*60;
Subtract sub[nTimPer];
protected parameter Integer nTimPer = size(timPer, 1);
equation connect(reaScaRep.y, sub.u1);  // EF002: dims [5] vs []
```

The instantiation-time integer evaluator knew `integer/mod/div/abs/min/max`
but not `size`, so `sub[nTimPer]` could not be sized and `sub` was
instantiated as one scalar block. The same with a `:` dimension sized by its
binding (Buildings `SortRuntime`: `idxEquAlt[:] = {i for i in 1:nin}`,
`nEquAlt = size(idxEquAlt, 1)`, `addWeiUna[nEquAlt]`).

Affected (cluster F, EF002): Buildings
`DHC.Plants.Combined.Controls.BaseClasses.TankChargeFractionRate`,
`Templates.Plants.Controls.StagingRotation.SortRuntime`.

## Fix

`rumoca-eval-ast/src/eval_instantiate/{mod.rs,function_eval.rs}`: `size(a, k)`
on a component of the scope, when the call is the predefined `size`, answers
from the declared extent (evaluated with the same modifier environment), or
for a leading `:` from the binding when it is an array constructor or a
single-iterator range comprehension. Anything else stays unevaluated.

## Test

`conn_frontend_regressions.rs`:
`component_array_sized_by_size_of_parameter_array`,
`component_array_sized_by_empty_comprehension_binding` (0 equations, 0
unknowns, as OpenModelica reports).
