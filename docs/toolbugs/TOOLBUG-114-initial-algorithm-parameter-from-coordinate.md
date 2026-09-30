# TOOLBUG-114 — `initial algorithm p0 := PMECH0` for a `fixed = false` parameter

**Status:** fixed (this change).
**Severity:** medium — OpenIPSL turbine governors (`GAST`, `IEEEG2`,
`IEESGO`) failed with `[ED013] unsupported initial algorithm in canonical
DAE: 'pm0' is determined from 'PMECH0', which is not settled when parameters
are computed`.

## What

```modelica
model Gov
  Real pmech = 2 + time;            // a generator output in OpenIPSL
  parameter Real p0(fixed = false);
  Real x;
initial algorithm
  p0 := pmech;
initial equation
  x = p0;
equation
  der(x) = -x;
end Gov;
```

The initial-algorithm replay turns a `fixed = false` parameter target into a
calculated-parameter binding, which may read only parameters. Here the value
comes from a coordinate the initialization system solves, so it has no
parameter-time value. The written initial *equation* `p0 = pmech` was already
supported (the initialization projection solves the deferred parameter).

## Fix

`crates/rumoca-phase-dae/src/construction/initial_algorithm_equations.rs`:
before analysis, an initial algorithm made only of assignments to distinct,
unbound, scalar `fixed = false` parameters, none of which reads a target of
the same section, and at least one of which reads a non-parameter coordinate,
is rewritten to the equivalent initial equations `p = e` (MLS §8.6 solves both
forms as one initialization problem). All other sections keep the replay.

## Test

`frontend_event_lowering.rs::an_initial_algorithm_parameter_read_from_a_coordinate_is_solved_at_initialization`.

## Related (not fixed)

`Real x(start = p0, fixed = true)` with `p0` a `fixed = false` parameter uses
the parameter's guess value instead of the solved one (the `start` binding is
evaluated at parameter-set time); the `initial equation x = p0` spelling is
correct. OpenIPSL uses the initial-equation form through `initType`.
