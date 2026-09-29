# TOOLBUG-060 — index reduction could not differentiate `^`, `exp`, `log`, `sqrt`, `tan`

**Status:** fixed (this change); function `derivative` annotations remain
open (below).
**Severity:** medium — complete example models reported "structurally
singular", which the evaluation classified as not standalone; found in the
IBPSA/IDEAS `*DerivativeCheck` examples.

## What

```modelica
model Chain
  Real x;
  Real z(start = 0, fixed = true);
equation
  x = 4*time^3;
  der(z) = der(x);   // structurally singular: 1 matched out of 2
end Chain;
```

`der(x)` of an algebraic makes the system higher-index, and the structural
phase differentiates `x`'s equation. Its differentiator had rules only for
`+ - * /`, `sin`, `cos`, `atan2` and the linear array builtins, so a
constraint containing `^` (or `exp`, `log`, `sqrt`, `tan`) was judged not
differentiable and the system reported singular. The order-aware preflight
also omitted `/`, although the differentiator implements its first
derivative.

## Fix

First-derivative rules, admitted by both preflights for scalar operands:

- `d(a^b) = b*a^(b-1)*da` for a time-invariant exponent, otherwise
  `a^b*(db*log(a) + b*da/a)`;
- `d exp(u) = exp(u) du`, `d log(u) = du/u`, `d sqrt(u) = du/(2 sqrt(u))`,
  `d tan(u) = du/cos(u)^2`;
- `/` in the order-aware preflight (first order only, as its rule).

`Chain` with each of `4*time^3`, `(1+time)^time`, `exp(time)`,
`log(1+time)`, `sqrt(1+time)`, `tan(time)` and `time/(1+time)` gives
`z(1) = x(1) - x(0)`. IBPSA/IDEAS `InverseXDerivativeCheck` runs.

## Still open

Most `*DerivativeCheck` examples differentiate a call to a function with a
`derivative(...)` annotation (`y = saturationPressure(T)`, `der(y)` read).
The checked DAE does not carry the annotation, so those remain structurally
singular; supporting them means carrying the annotation through the DAE and
RBC and substituting `f_der(x, der(x))` in the differentiator.
