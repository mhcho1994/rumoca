# TOOLBUG-055 — derivatives read by discrete equations, roots and implicit state rows refused

**Status:** fixed (this change).
**Severity:** medium — refused models the compiler accepts ("derivative
coordinate escaped checked structural substitution", "state equation is not
a subtractive derivative residual"); found in OpenIPSL and ThermoPower.

## What

A state derivative has no Solve storage: a row that reads one recomputes the
defining row's right-hand side. Only continuous algebraic rows and initial
rows were given that index. Two ordinary forms fell outside it:

```modelica
// OpenIPSL PIwithVariableLimiter: a Boolean equation with a relation on der
or1.u2 = if abs(limit1 - y) <= eps and der(integral.y) > 0 then true else ...;
```

The discrete-value owner, and the root function its relation needs, were
compiled without the index and failed. A unit test pinned the refusal as the
intended scope, but nothing in the specs or the semantics requires it: at an
event instant a derivative definition is a function of the current state,
algebraics and discretes, which a discrete row already reads.

Separately, index reduction differentiates rows of any shape, and a
differentiated row matched to a derivative need not be `lhs - rhs`
(ThermoPower `TestElectrical1/3/5`, where generator and load angles are tied
by algebraic equations); those were refused before the affine forms of
TOOLBUG-053 were tried.

## Fix

- The derivative-row index is stored on the lowered layout as soon as
  structural matching proves it, and every scalar compiler built after that
  point -- discrete owners, roots, event conditions -- resolves a derivative
  read through it. Derivatives of affine derivative blocks still need their
  block's system, which only the continuous lowering has; those reads are
  still refused with the existing message.
- A residual that is not a subtraction is tried as the summed form with a
  zero right-hand side: `a + c*der(x) = 0`.

`PIwithVariableLimiter`, `TestElectrical1`, `TestElectrical3` and
`TestElectrical5` run; `DC4B`, `REPCA1` and `PSAT_WT` lower and fail at run
time instead (initialization, event iteration, NaN under held inputs).
