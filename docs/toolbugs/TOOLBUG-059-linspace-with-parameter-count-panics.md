# TOOLBUG-059 — `linspace` with a parameter count panics the DAE evaluator

**Status:** fixed (this change).
**Severity:** high — a panic (exit 101) mid-simulation, which also left the
`--domain-diagnostics` file empty, so the evaluation harness reported
"invalid domain diagnostics"; found in TRANSFORM's void-fraction examples.

## What

```modelica
parameter Integer n = 3;
parameter Real v[n] = linspace(0, 1, n);
Real x(start = v[2], fixed = true);
```

```
internal error: entered unreachable code: checked linspace extent is a literal Integer
```

The checked DAE constructor accepts any static Integer extent -- a literal or
a structural parameter expression -- but the numeric evaluator destructured
the third argument as an Integer literal and treated anything else as
unreachable.

## Fix

The extent is evaluated like any other operand. `Spaced` starts at
`x = 0.5`; `Volume_wSlipRatio_noSlip` and `SlipRatio_simpleVelocity_Test` run.
