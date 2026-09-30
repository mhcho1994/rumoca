# TOOLBUG-117 — `suspend = subgraphStatePort.suspend` gave `suspend` two owners

**Status:** fixed (this change).
**Severity:** medium — every model with an MSL `StateGraphRoot` whose
`suspend`/`resume` are not otherwise used (Buildings
`DataCenters.ChillerCooled.Controls.Reheat` and its validation) failed with
`[ED010] invalid Appendix B discrete solved form: 'stateGraphRoot.suspend' has
more than one semantic definition owner`.

## What

`Modelica.StateGraph.Interfaces.CompositeStepState` declares
`output Boolean suspend = false` and also writes the equation
`suspend = subgraphStatePort.suspend`. Appendix B.1c orients a discrete-valued
equation by its left-hand coordinate, so both the binding and the equation
claimed `suspend`, while nothing defined `subgraphStatePort.suspend`. An
equality between two coordinates is symmetric; the equation is what defines
the port.

## Fix

`analysis/equation_partitions.rs::reversed_discrete_equalities`: an equality
`a = b` (non-connection, non-binding row) between two whole discrete-valued
coordinates owns `b` with value `a` when `a` is already defined by another row
or its declaration binding and `b` is defined by no row, has no binding, and
is not an input. `equation_partition` applies the orientation for those rows
(also for balance and topology analysis, which share it). All other rows keep
their written orientation.

## Test

`frontend_event_lowering.rs::a_discrete_equality_defines_the_side_no_other_row_defines`.
