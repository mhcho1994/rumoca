# TOOLBUG-120 — discrete top-level input connected to a nested input (ED001 / ED020)

**Status:** fixed.
**Severity:** high (any block that routes a Boolean/Integer input into a sub-block).

## What

```modelica
model M
  connector BI = input Boolean;
  block B  BI w; Boolean v; equation v = w; end B;
  BI u;
  B b;
equation
  connect(u, b.w);     // flat: u = b.w
end M;
```

`b.w` is a nested input, so it is a discrete-value unknown; `u` is an external
input. DAE construction only turned a connection equation into a discrete
definition (MLS Appendix B.1c) when *both* sides were discrete-value
coordinates. With one side an external input the row fell through to the
continuous partition: with a `when u` in the sub-block the model came out one
equation over (`ED001`, e.g. 6 equations / 5 unknowns), otherwise the row was
refused as a numeric residual over Booleans (`ED020`). Real inputs were
unaffected because a continuous row between an input and an algebraic is fine.

Affected (cluster F): AixLib `...Safety.BaseClasses.OnPastThreshold`,
`...Safety.BaseClasses.CycleRateBoundary`, and the Boolean-input parts of the
G36 staging subsequences (Buildings `...Subsequences.Initial`, `...Up`).

## Fix

`rumoca-phase-dae/src/construction/analysis/equation_partitions.rs`: a
discrete-connection endpoint may be an external input. External inputs rank as
producers in `discrete_connection_ranks`, so orientation always makes the
nested side the defined coordinate, and an input is never accepted as the
target (two connected external inputs still define nothing).

## Test

`crates/rumoca-contracts/tests/conn_frontend_regressions.rs`:
`boolean_top_level_input_drives_nested_boolean_input`,
`integer_top_level_input_drives_nested_integer_input`.
