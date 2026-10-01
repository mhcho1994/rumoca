# TOOLBUG-183 — `floor`/`ceil`/`integer` of a continuous argument generated no events

**Status:** fixed.
**Severity:** high (silent wrong results).

## What

```modelica
model FL
  Integer k;
equation
  k = integer(floor(time / 0.3));   // stayed 0 for the whole simulation
end FL;
```

MLS §3.7.2 makes `floor`, `ceil` and `integer` event-generating, like
`div`/`mod`/`rem`. Rumoca already gave `div`/`mod`/`rem` a checked runtime
owner with a `sin(pi * x / y)` event root. The rounding builtins were lowered
as plain pure builtins with no root. A continuous `Real r = floor(time/0.3)`
happened to look right because it is re-evaluated at every output point. A
discrete `Integer` defined by them only updates at events, though, so it never
changed: no error, just a wrong trajectory. I found this while investigating
runtime `sample` starts (ED018) for this cluster; any library model that sets
a discrete value from `integer(...)` of a time expression was affected.

## Fix

`crates/rumoca-phase-dae/src/construction/expression/discontinuities.rs`
(new; it also takes over the existing `div`/`mod`/`rem` routing from
`expression.rs`, which was at 1990 lines): for a continuous-time scalar
argument at model scope, `floor(x)` is lowered as
`if x < div(x, 1) then div(x, 1) - 1 else div(x, 1)` over the checked
runtime-quotient owner of `div(x, 1)`. Its indicator `sin(pi * x)` vanishes
exactly at the integers, where these functions jump. `ceil(x)` becomes
`0 - floor(-x)`, and `integer(x)` applies the builtin to the integral
`floor(x)`. All of these are exact in binary64. Function bodies (MLS §3.7.2:
no events), clocked partitions, and constant, parameter, discrete or array
arguments keep the plain builtin.

## Test

`suite_core/frontend_event_lowering.rs::rounding_a_continuous_argument_updates_discrete_integers_at_each_crossing`
(floor, ceil and integer across negative and positive crossings).
