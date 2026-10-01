# TOOLBUG-184 — `sample(t0, T)` whose start is a `fixed = false` parameter (ED018)

**Status:** open (analysed, not fixed).
**Severity:** high (largest bucket of cluster R2-events: 17 of 72 models).

## What

Every OBC/CDL block with a sampling grid aligned to the simulation start
(`Logical.Sources.Pulse`, `Integers.Sources.Pulse`, `Discrete.Sampler`,
`ZeroOrderHold`, `TriggeredSampler`, `UnitDelay`, ... in Buildings, IBPSA,
AixLib, IDEAS) does this:

```modelica
parameter Real t0(fixed = false);
initial algorithm
  t0 := round(integer(time / period) * period + mod(shift, period), 6);
  if time + period < t1 then t0 := t0 - period; ... end if;
equation
  when sample(t0, period) then ... end when;
```

`t0` gets its value from the initialization system, at runtime, as a
function of the start time. Rumoca represents every `sample` as a
translation-time `PeriodicClockSchedule`. The phase is either `Absolute` or
`SimulationStart` (start time plus an exact offset), and the solvers turn
that into time events before initialization runs
(`SolveModel::resolved_periodic_schedules_at`). The `SimulationStart` anchor
covers `t0 := time + c`, but not this one: `integer()`/`round()` and the
conditional corrections make `t0 - start` vary with the start time.
Rumoca reports "sample start is not parameter-evaluable: `booPul.t0` is a
`fixed = false` parameter determined by the initialization system". The
diagnostic is accurate; the capability is missing.

## Why it was not fixed here

There are two exact designs. Both are larger than one cluster pass.

1. **Runtime phase anchor.** Add a `ClockPhaseAnchor` whose phase is a
   parameter slot, and resolve schedules *after* initialization instead of at
   the start instant. This touches `rumoca-core` (`PeriodicClockSchedule` is
   `Copy`/`Hash` and keyed structurally), `rumoca-ir-dae` and its wire format,
   bitcode export/import/schema, `rumoca-ir-solve`, galec admissibility,
   and every solver/FMI entry point that resolves schedules today.
2. **Event lowering with generated state.** Rewrite `sample(t0, T)` to
   `edge(c) or (initial() and on_grid)`, where `c = time >= t0 and
   (time - t0)/T - floor((time - t0)/T) < 0.5` is a generated Boolean
   coordinate. Its rising edges are exactly `t0 + kT`, k >= 0, and they
   depend on TOOLBUG-183's event-generating `floor`. This needs generated
   flat coordinates (nothing creates them yet). The initial instant also
   needs an on-grid test with a tolerance when `t0 < start`, to match the
   static schedule, which fires at the start instant.

Accepting the model by treating `t0` as its start value, or by proving
`t0 = shift (mod period)` from this one CDL pattern, would be a silent
approximation and was rejected.

## Test

None (no behaviour change). The existing refusal is pinned by
`rumoca-phase-dae` `construction/tests/events.rs::a_sample_start_settled_without_time_names_the_initialization_system`.
