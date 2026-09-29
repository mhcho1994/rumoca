# TOOLBUG-061 — index reduction could not differentiate conditionals or multi-statement function bodies

**Status:** fixed (this change), extending TOOLBUG-060; function `derivative`
annotations remain unused (below).
**Severity:** medium — complete example models reported "structurally
singular"; 42 of the 57 such examples in IBPSA, IDEAS, TRANSFORM and
OpenIPSL run after this change and TOOLBUG-060.

## What

Differentiating a higher-index constraint stopped at:

- a conditional expression (`if time > 0.5 then time^2 + 0.25 else time`),
  `smooth(n, e)` and `noEvent(e)`;
- a call whose body is more than one assignment -- local definitions and an
  `if` statement, as in TRANSFORM's `Easing` functions and IBPSA's flow
  functions -- because the call was followed only for a single-assignment
  body;
- `abs`, `sign`, `sinh`, `cosh`, `tanh`, `asin`, `acos`, `atan`, `log10`.

## Fix

- A conditional is differentiated branch by branch under the same conditions;
  a condition is rebuilt, not differentiated, and is admitted when it can be
  rebuilt (`is_rebuildable_in_context`). `smooth`/`noEvent` pass through.
- A call is followed through any body of assignments and assignment groups
  (an `if` statement's merged definitions; assertions are ignored): the
  output is one expression DAG over the parameters and earlier definitions,
  and the differentiator, both preflights and the value rebuilder follow a
  `FunctionValue` to its definition.
- First-derivative rules for the listed builtins; `d|u| = sign(u) du` and
  `d sign(u) = 0` between events.

Each rule integrates `der(z) = der(x)` to `x(1) - x(0)` in
`index_reduction_differentiates_elementary_and_piecewise_definitions`, and a
function with a local definition and an `if` statement in
`index_reduction_differentiates_through_a_function_body_with_an_if_statement`.

## Still open

- Using `derivative` annotations. The derivative function is never called,
  so class reachability prunes it, and DAE functions are built per call-shape
  specialization, which it has none of. Using the annotation needs the
  derivative function retained, its specialization derived from the annotated
  call's, the annotation carried through the DAE and RBC, and the
  differentiator to substitute `f_der(inputs, der(inputs))`. Nine examples
  still report singular for this.
- `PerfectGasDerivativeCheck` initializes `hSym = hCod` to 1.6e-7 relative
  (0.41 on 2.5e6), inside the default `rtol = 1e-6` but outside the model's
  absolute 1e-2 check; its `experiment(Tolerance=1e-8)` is not honored
  (LIMITATION-002).
- `PowerLaw_dp_DerivativeCheck` evaluates a singular branch at the initial
  guess of its `fixed = false` parameters before they are solved.
