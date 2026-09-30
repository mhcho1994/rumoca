# TOOLBUG-090 — `:` dimensions not inferred through `end` slices and element-wise builtins

**Status:** fixed (this change).
**Severity:** medium — `EF010 unresolved component dimension for …: :` on
every model that uses the CDL `Integers/Logical.Sources.TimeTable` blocks.

## What

MLS §10.1 sizes a `:` dimension from the declaration's binding. The CDL
integer time table declares

```modelica
final parameter Integer val[:,:] = integer(table[1:end, 2:end] + ones(nT, nout)*small);
```

Three gaps in the binding-shape inference (`rumoca-eval-ast`
`dimension_inference.rs` / `infer_dims_from_func_with_scope`):

1. The element-wise builtins (`integer`, `abs`, `floor`, `sin`, …, MLS
   §3.7.1/§12.4.6) had no shape rule, so `integer(A)` was unknown.
2. A slice `a[lo:end]` could not evaluate `end`, and the fallback silently
   kept the *full* extent of the indexed dimension, so `table[1:end, 2:end]`
   on `[3,3]` was sized `[3,3]` instead of `[3,2]`.
3. The DAE constructor typed `integer(x)` as scalar-only, although the numeric
   conversion is vectorizable (the evaluator already mapped it element-wise).

Reduced repro (`T.M` in the regression test): a block with
`parameter Real table[:,:]` bound from an enclosing modifier and the `val`
declaration above.

Affected cluster-C models (3): `Buildings…Logical.Sources.Validation.TimeTableNegativeStartTime`,
`IBPSA…Logical.Sources.Validation.TimeTable`, `IDEAS…Logical.Sources.Validation.TimeTable`.

## Fix

- `rumoca-eval-ast/src/eval/dimension_inference.rs`: `end` inside a slice is
  replaced by the extent of the indexed dimension before the range length is
  evaluated; an unevaluable slice now leaves the shape unknown instead of
  guessing the full extent.
- `rumoca-eval-ast/src/eval/mod.rs`: `infer_builtin_elementwise_dims` gives the
  element-wise builtins the shape of their widest operand, and reductions a
  scalar shape.
- `rumoca-ir-dae/src/expression/type_rules.rs`: `integer(x)` on a numeric
  operand keeps the operand's shape; `Integer(e)` of an enumeration stays
  scalar.

## Test

`crates/rumoca/tests/suite_core/structural_evaluation_regressions.rs`:
`colon_dimensions_follow_end_slices_and_elementwise_builtins`.
