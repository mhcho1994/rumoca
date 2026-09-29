# TOOLBUG-062 — runtime evaluation ignored `experiment(StartTime=...)`

**Status:** fixed (this change).
**Severity:** medium — false runtime failures in the evaluation; 31 IBPSA
models declare a non-zero StartTime (and their IDEAS/AixLib/Buildings
copies).

## What

The runtime pass simulated every model over `[0, 1]`. Some validations are
only defined after their declared start:

```modelica
model ExponentialIntegralE1
  Real E1 = IBPSA.Utilities.Math.Functions.exponentialIntegralE1(time);
  annotation(experiment(StartTime=0.01, StopTime=3.0));
end ExponentialIntegralE1;
```

From t = 0 it fails with `non-finite (inf) value computed for E1`, which is
`E1(0)`, never reached by the model's own experiment. `compile-bitcode` had
no way to start elsewhere although `SimOptions` carries `t_start`.

## Fix

- `rumoca compile-bitcode --simulate --t-start T` (default 0); a window with
  `--t-end` not after `--t-start` is refused.
- The modelsan Rumoca backend takes `t_start` and passes it; the window is
  validated at construction.
- The evaluation runner reads each model's `experiment(StartTime=...)` from
  its declaration and simulates `[StartTime, StartTime + stop]`, recording
  `t_start` per model.

`LogFromStart` (`y = log(time)`) fails from 0 and runs from 0.5
(`bitcode_simulation_window`); IBPSA `ExponentialIntegralE1` and
`FiniteLineSource_Integrand` run.
