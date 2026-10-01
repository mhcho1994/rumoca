# TOOLBUG-168 — structural parameters that reduce array parameters (ET004 → EF010 → ED019 → ED020)

**Status:** fixed.
**Severity:** high.

## What

```modelica
parameter Real AExt[2] = {1, 2};
parameter Real ATotExt = sum(AExt);
final parameter Real AArray[3] = {ATotExt, ATotWin, AInt};
parameter Integer dimension = sum({if A > 0 then 1 else 0 for A in AArray});
parameter Real splitFactor[dimension, 1] = ...;
parameter Integer k = if AArray[2] > 0 then 3 else 1;   // also: x = fill(1.0, k)
```

Every stage lacked array-parameter values:

1. Typecheck kept only scalar values, so `sum(AExt)`, `AArray[2]` and the
   comprehension over the elements of `AArray` were unevaluable
   (`ET004 ... splitFactor`).
2. Flatten re-evaluated `dims_expr` with its own scalar-only evaluator
   (`EF010 unresolved component dimension`), ignoring the extents typecheck
   had proven.
3. The checked DAE owns comprehensions over Integer ranges only
   (`ED019 array comprehension domain`).
4. The DAE requires `fill`/extent arguments to be literals or parameters
   bound to literals, but nothing folded `k`/`dimension`
   (`ED020 array extent must be a nonnegative literal Integer`); the DAE
   constant evaluator also rejected array comprehensions.

Affected in cluster R2-dims: the 7 `thermalZone*.splitFactor` cases
(AixLib/IDEAS VDI6007, ASHRAE140, Buildings GeojsonExportRC).

## Fix

- `rumoca-eval-ast/src/eval/array_values.rs` (+ `real_arrays` in
  `TypeCheckEvalContext`, `sum`/`min`/`max` of Real arrays, `v[k]` lookup,
  Integer comprehensions): element values of evaluable 1-D array parameters
  and comprehension evaluation by substituting each iterator value.
  `rumoca-phase-typecheck/.../dimensions.rs::try_eval_real_array` records them.
- `rumoca-phase-flatten/src/pipeline/context_and_tests/component_dimensions.rs`:
  falls back to the extents typecheck evaluated for the same subscripts.
- `rumoca-phase-flatten/src/postprocess/array_domain_comprehensions.rs`:
  `{e(A) for A in v}` over a model vector becomes `{e(v[A]) for A in 1:n}`.
- `rumoca-eval-flat/src/constant/comprehension_eval.rs`: translation-time
  evaluation of comprehensions.
- `rumoca-phase-dae/src/construction/variable_construction.rs`: a scalar
  Integer parameter whose binding the DAE constant evaluation settled is
  lowered as its literal value (a structural value, MLS §18.3).

## Test

`array_dimension_regressions.rs::structural_parameter_reduces_array_parameter_elements`
(simulates; checks `dimension = 2`).
