# TOOLBUG-091 — connecting a top-level Integer/Boolean input to a component input

**Status:** fixed (this change).
**Severity:** high — `ED020 … expression type mismatch: expected Real, found
Integer` (or `expected a numeric expression, found Boolean`) for any model
whose top-level discrete input connector is connected to a sub-component's
input, i.e. most CDL/G36 blocks checked on their own.

## What

```modelica
connector II = input Integer;
block GT  II u; output Boolean y; equation y = u > 0; end GT;
model M   II u; GT g; equation connect(u, g.u); end M;
```

The connection equation `u = g.u` must define the discrete coordinate
`g.u` from the environment-driven input `u`. The DAE analysis only oriented
connections whose both ends were discrete-value coordinates
(`discrete_value_base_reference`), and ranked producers without counting
top-level inputs, so this equation fell through to the continuous partition
and was lowered as a Real residual of Integer operands.

Affected cluster-C models: `Buildings…Staging.SetPoints.Subsequences.Down`,
`Buildings…CoolingOnly.Subsequences.ActiveAirFlow` (Integer), and the Boolean
connection in `AixLib…Logical.Proof`.

## Fix

`rumoca-phase-dae/src/construction/analysis/equation_partitions.rs`: a
discrete connection endpoint may be a top-level `Input`; such an input is a
rank-0 producer, and a connection between an input and a discrete-value
coordinate always assigns the coordinate from the input.

## Test

`structural_evaluation_regressions.rs`:
`top_level_discrete_inputs_drive_connected_inputs`.
