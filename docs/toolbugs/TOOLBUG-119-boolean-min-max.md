# TOOLBUG-119 — `min`/`max` of a Boolean vector

**Status:** fixed (this change).
**Severity:** medium — MSL `Modelica.Math.BooleanVectors.andTrue`
(`result = size(b, 1) == 0 or min(b)`) failed DAE construction with `[ED020]
canonical DAE construction rejected an invalid operation: expected a numeric
expression, found Boolean`. The CDL `BooleanExtractSignal` /
`IntegerExtractSignal` (IBPSA, AixLib, IDEAS) and TRANSFORM `BoundaryCheck`
reach it through their `initial equation assert(andTrue(...))` range checks
once TOOLBUG-116 lets them past the discrete solved-form check.

## What

MLS §10.3.4 defines `min(A)`/`max(A)` as the least/greatest element "as
defined by <", and `false < true`, so the reductions apply to Boolean arrays.
The DAE type rule required numeric elements.

## Fix

- `rumoca-ir-dae` `type_rules.rs`: one-argument `min`/`max` over a Boolean
  array has a Boolean result.
- `rumoca-phase-solve` `typed_functions/tensor.rs`: `min(b)` lowers to the
  `All` reduction, `max(b)` to `not All(not b)`. Scalar and numeric-evaluation
  paths already treat Booleans as 0/1, where the numeric extremum is exact.

## Test

`frontend_event_lowering.rs::boolean_vectors_order_false_below_true`.
