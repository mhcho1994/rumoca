# TOOLBUG-116 — discrete array element equations written in a `for` loop

**Status:** fixed (this change).
**Severity:** medium — CDL `BooleanExtractSignal`/`IntegerExtractSignal`
(IBPSA, AixLib, IDEAS) and TRANSFORM `BoundaryCheck` failed with `[ED010]
invalid Appendix B discrete solved form: a discrete-valued equation must have
one unsubscripted resolved coordinate as its left-hand side` (one-element
arrays) or `[ED020] ... expression shape mismatch` (longer arrays).

## What

```modelica
block Extract
  parameter Integer nout = 1;
  parameter Integer extract[nout] = 1:nout;
  input Boolean u[3];
  output Boolean y[nout];
equation
  for i in 1:nout loop
    y[i] = u[extract[i]];
  end for;
end Extract;
```

Two defects:

1. An element equation `y[1] = e` of a one-element discrete array was skipped
   by aggregate coverage ("the selection denotes the whole coordinate") and
   then rejected by the scalar path because its left side is subscripted.
2. Flat keeps the loop as a materialized structured family *and* its element
   rows. Aggregate coverage correctly packs the rows into one definition of
   the whole `y`, but the family lowering then used that whole-array
   definition as a per-point family body, whose shape does not match.

## Fix

`analysis/equation_partitions.rs`:
- `whole_aggregate_element_assignment` owns `y[1] = e` as `y = {e}` when the
  selection denotes the whole coordinate.
- `whole_coordinate_element_families` finds materialized families whose rows
  are all whole-coordinate definitions (aggregate owners/members or the case
  above); `analysis.rs` removes their rows from the family-owned set and
  `equation_systems.rs` excludes the family view, so the rows are lowered as
  rows.

## Test

`frontend_event_lowering.rs::discrete_element_equations_in_a_for_loop_define_the_whole_array`.
