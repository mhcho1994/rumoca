# TOOLBUG-112 — a parameter without a binding had no value for `sample`

**Status:** fixed (this change).
**Severity:** medium — every block extending
`Modelica.Blocks.Interfaces.DiscreteBlock` without setting `samplePeriod`
failed with `[ED018] unsupported runtime operator 'sample': sample interval is
not parameter-evaluable: unknown variable: samplePeriod`.

## What

```modelica
model SP
  parameter Real samplePeriod(start = 0.1);   // DiscreteBlock declares it so
  discrete Real y(start = 0, fixed = true);
equation
  when sample(0, samplePeriod) then
    y = pre(y) + 1;
  end when;
end SP;
```

MLS §8.6: a fixed parameter with no binding equation uses its `start` value
(tools warn). The DAE analysis constant table
(`construction/analysis.rs::constant_context`) folded only bindings, so the
sample schedule, delay bounds, clock arguments and any other translation-time
consumer saw an unknown name.

Affected in the cluster: `IBPSA.Controls.Discrete.BooleanDelay`,
`IDEAS.Controls.OBC.CDL.Discrete.ZeroOrderHold`,
`ThermoPower.Examples.HRB.Models.DigitalPI` (3).

## Fix

`constant_context` falls back to the `start` expression of a constant or a
fixed parameter that has no binding. A `start` that does not evaluate is
skipped (it was never an error to lack a value there). `fixed = false`
parameters are still excluded (they are settled by initialization).

## Test

`frontend_event_lowering.rs::an_unbound_sample_period_uses_its_start_value`.
