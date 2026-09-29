# TOOLBUG-051 — initialization refused every row that reads an algebraic

**Status:** fixed (this change), for rows whose only obstacle is an
algebraic/output read.
**Severity:** high for runtime coverage — the largest single backend reason
models failed to initialize in the corpus runtime pass (36 of 39
initialization-projection failures in the first six libraries).

## What

```modelica
model UA
  Real x(start = 0, fixed = false);
  Real a;
equation
  a = 2*time + 5;
  der(x) = a - x;
initial equation
  x = a + 7;          // OpenModelica: x(0) = 12
end UA;
```

```
initial variable projection did not satisfy the complete residual system ...
owner=none(row reads a coordinate outside the planned initialization unknown
space: a continuous algebraic/output reconstructed from the continuous
equations but not owned by the reduced initialization projection or its total
derivative)
```

The reduced initialization planner excluded any row that read an
algebraic/output coordinate. The runtime, meanwhile, already re-settles every
algebraic from the current unknowns on each initialization residual
evaluation (`refreshes_algebraic_reads`) and takes the Jacobian-vector
product through that refresh -- the capability the planner's own header
names as the way to close the gap.

## Fix

A row whose only coordinates outside the unknown space are algebraic/output
reads now joins the projection through that refresh. Its exact incidence
through the continuous system is implicit, so it conservatively joins every
projection unknown -- the rule `ImplicitAlgebraic` checks already follow --
and is recorded as `SolvedThroughAlgebraicRefresh` or
`SurplusAlgebraicCheck`. A zero sensitivity can make a block fail; it cannot
certify a wrong value, because the certificate evaluates the refreshed
residual.

`UA` initializes to 12, and the header's other historical case
(`initial equation der(x) = 0`, which once silently gave `x(0) = 0`) to
OpenModelica's 5. The two tests that pinned the refusal now pin those values.
On the 39 corpus models that failed this way, 24 now run; the rest also read a
discrete coordinate (still excluded -- discretes in the unknown space are
separate work), produce a NaN in a `fixed = false` parameter solve, or do not
converge.
