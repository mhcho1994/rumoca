# TOOLBUG-196 — equation between matrix constructors counted as one scalar

**Status:** fixed.
**Severity:** medium. In R2-balance: OpenIPSL `Machines.PSSE.GENCLS` and
`Examples.Tutorial.Example_4.BaseModels.GeneratingUnits.InfiniteBus`
(2 of 32; both balanced in OpenModelica).

## What

```modelica
model Mat
  parameter Real k = 1;
  Real a, b;
  Real c = time, d = 2*time;
equation
  [a; b] = -k*[1,2;3,4]*[c;d];   // 2 equations, counted as 1
end Mat;                          // ED001: 3 equations, 4 unknowns
```

Flatten's AST shape inference (`equations/shape_inference.rs`) returned
`Other` for every matrix constructor, so the equation fell back to a scalar
count of 1. It also returned `Other` for scalar component references
(only array components have recorded dimensions), so even a rule for
matrix constructors needs to accept those as scalar entries.

## Fix

`matrix_literal_shape`: `[a, b, c]` is `Matrix(1, 3)`, `[a, b; c, d]`
(rows parsed as nested matrix rows) is `Matrix(2, 2)` when every entry is
scalar — a literal, scalar expression, or a component reference without
recorded dimensions. Concatenations of array blocks stay unknown. The
existing matrix-product and equality rules then give the right count; the
DAE already scalarizes such residuals (simulation gives a = -5, b = -11 for
c = 1, d = 2).

## Test

`crates/rumoca-contracts/tests/eqn_contracts.rs`:
`eqn_matrix_constructor_equation_counts_every_element` (balanced +
simulated values).
