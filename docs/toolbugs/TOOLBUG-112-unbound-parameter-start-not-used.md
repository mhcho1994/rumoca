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
`ThermoPower.Examples.HRB.Models.DigitalPI`, OpenIPSL `DEGOV`
(`parameter Time TD` with no value at all feeds `FixedDelay.delayTime`), and
`IDEAS.Controls.OBC.CDL.Discrete.ZeroOrderHold` (whose `samplePeriod` has no
value and no `start`, so it now correctly reports a zero sample interval).

## Fix

`constant_context` (`unbound_parameter_value`) gives a constant or fixed
parameter without a binding its `start` value, or its type's default start
(`0.0`, `0`, `false`) when no `start` is written, as OpenModelica does (with a
warning). A `start` that does not evaluate leaves it without a value.
`fixed = false` parameters are still excluded (initialization settles them).

## Test

`frontend_event_lowering.rs::an_unbound_sample_period_uses_its_start_value`.
