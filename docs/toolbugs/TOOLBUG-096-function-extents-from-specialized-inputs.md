# TOOLBUG-096 — array-constructor extents from function inputs

**Status:** fixed (this change).
**Severity:** high — `ED020 … array extent must be a nonnegative literal
Integer` for any function that builds an array whose size is an input:
`fill(u, nout)`, `zeros(n)`, `linspace(x1, x2, N)`, `fill(1.0, size(x, 1))`.

## What

```modelica
function rep  input Integer n; input Real u; output Real y[n];
algorithm y := fill(u, n); end rep;
model M  Real z[2] = rep(2, time);  end M;
```

Function bodies are lowered per value-proven specialization, and the
specialization already proves the inputs that decide the output shape (it
used them for loop ranges: `for i in 1:n` became `1:3`). The extent
arguments of the array constructors were not settled from those proofs, so the
checked DAE constructor saw the symbolic `n` and rejected it.

Affected cluster-C models: `AixLib/Buildings.Utilities.Math.IntegerReplicator`,
`TRANSFORM…Pipe_Wall.GenericPipe` (`fillArray_1D`),
`TRANSFORM.Math.Examples.check_linspace_1D` (`linspace_1D`).

## Fix

`rumoca-phase-dae/src/construction/analysis/function_static_extents.rs`
(new): after loop compaction, every extent argument of `zeros`, `ones`,
`fill`, `linspace` and `identity` that the specialization proves static
(`static_shape_integer_expression`: settled inputs, `size(...)`, integer
arithmetic) is replaced by its literal. An unproven extent is left as written
and is still rejected. Applied to both the returning and non-returning body
paths in `function_bodies.rs`.

## Test

`structural_evaluation_regressions.rs`:
`function_array_constructors_take_extents_from_specialized_inputs`
(simulates and checks the values).
