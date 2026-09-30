# TOOLBUG-087 — `zeroDerivative` naming an inherited input rejected (ER120)

**Status:** fixed.
**Severity:** low.

## What

```modelica
function bicubic_eval
  extends Interpolation.PartialInterpolation;   // declares input String tablesPath
  external "C" z = bicubic_eval(tablesPath, x, y);
  annotation(derivative(zeroDerivative=tablesPath)=bicubic_eval_deriv_dt);  // ER120
end bicubic_eval;
```

FUNC-031 only looked at the function's own components; FUNC-030 (derivative
function must have outputs) had the same blind spot.

Affected (cluster B): TRANSFORM `Math.Interpolation` users — 4 models
(3 x `Interpolation_2D*`, `Media.LookupTableMedia.Examples.LookupTable_Test`).

## Fix

`restrictions/decl.rs`: `find_component_with_bases` / `class_has_output_with_bases`
follow resolved `extends` clauses.

## Test

`overstrict_checks.rs::zero_derivative_accepts_inherited_input`.
