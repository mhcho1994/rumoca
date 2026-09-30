# TOOLBUG-115 — `delay(u, 0)` was rejected

**Status:** fixed (this change).
**Severity:** low-medium — `Modelica.Blocks.Nonlinear.FixedDelay(delayTime =
0)` (OpenIPSL GGOV1 `Teng = 0`) failed with `[ED018] unsupported runtime
operator 'delay': delayTime must evaluate to a finite positive scalar Real`.

## What

MLS §3.7.4.1 defines `delay(expr, delayTime)` as `expr(time - delayTime)` for
`0 <= delayTime <= delayMax`; a zero delay is `expr` itself. The delay plan
required a strictly positive delay (the history buffer needs one).

Affected in the cluster: `OpenIPSL...TG.BaseClasses.GGOV1.Turbine`,
`OpenIPSL...TG.GGOV1DU` (2).

## Fix

`analysis/delays.rs`: a `delayTime` that evaluates to exactly `0` gets
`DelayPlan::Identity`; `expression/calls.rs::lower_delay` then lowers the
source expression itself. Negative, non-finite or non-evaluable delays keep
their diagnostics.

## Test

`frontend_event_lowering.rs::a_zero_delay_is_its_source`.
