# TOOLBUG-063 — initialization settled algebraics before the parameters it determines

**Status:** fixed (this change).
**Severity:** medium — complete models failed to initialize ("algebraic
projection did not converge at event boundary"); found in IBPSA's
`InvertingPowerLaw_m_flow`/`_dp` validations.

## What

```modelica
model Implicit
  parameter Real a(fixed = false);
  parameter Real b(fixed = false);
  Real x(start = 0);
initial equation
  a = 2;
  b = 1;
equation
  4*(time - 0.5) = a*x + b*x^3;
end Implicit;
```

Entering initialization, the Model Exchange kernel built its starting
coordinate by fully settling the algebraics first (`current_solver_y`). The
parameters the initialization determines were still at their seeds (0), so
`x` had no solution (the residual stayed at -2 with a zero Jacobian) and the
strict settle aborted before `settle_initialization_system` could solve `a`,
`b` and `x` together. With non-zero starts the same model ran, which hid the
ordering.

## Fix

`initialization_solver_y` uses the settled coordinate when the algebraics
settle, and otherwise, on an evaluation failure, the unsettled coordinate
(states over the retained guess). It is only the initialization's starting
point: the initialization then solves parameters and algebraics together and
still fails with its own diagnostic when they have no solution.

`Implicit` initializes and satisfies its equation at every output point;
IBPSA `InvertingPowerLaw_m_flow` and `InvertingPowerLaw_dp` run.

## Still open

`PowerLaw_dp_DerivativeCheck` evaluates a non-finite value inside the
initialization itself while its `fixed = false` coefficients are at their
seeds (a zero `dp_turbulent` selects the `dp^m` branch at `dp = 0`).
