# TOOLBUG-181 — initial algorithms with `for` loops, partial assignments, array constructors, enumeration literals

**Status:** fixed (the replay); IBPSA/IDEAS/AixLib `CalendarTime` still stops
on a typed size bound, see below.
**Severity:** medium.

## What

```modelica
initial algorithm
  if zerTim == ZeroTime.Custom then              // enumeration literal
    timOff := timOff + sum({dayInMonth[i] for i in 1:(monthRef - 1)});
  end if;
  year := 0;
  for i in 1:size(timeStampsNewYear, 1) loop    // for loop
    if t < timeStampsNewYear[i] and ... then
      yearIndex := i - 1;                        // assigned on some paths only
    end if;
  end for;
```

Four separate refusals of the symbolic initial-algorithm replay:

1. `for` loops were rejected outright ("loops ... carry implicit memory").
2. A parameter determined from an enumeration literal was refused because
   the initial-owner analysis was given the variable roles, which do not
   contain enumeration literals.
3. An array-constructor iterator (`i` in `{d[i] for i in ...}`) was reported
   as an unsettled read.
4. A coordinate assigned on only some paths was refused ("not defined on
   every path") although MLS §11.1.2 gives it its start value on entry.

Affected in R2-events: `IDEAS.Utilities.Time.CalendarTime`,
`IDEAS.Utilities.Time.Examples.CalendarTime`,
`IDEAS/IBPSA.Utilities.Time.Validation.CalendarTimeMonthsMinus`,
`AixLib.Utilities.Time.DaytimeSwitch` (6 with CSVWriter, which additionally
calls an external `writeLine` in its initial algorithm and stays refused).

## Fix

`crates/rumoca-phase-dae/src/construction/analysis/initial_algorithms.rs`:

- `Replay::unrolled` replays a `for` over a parameter-evaluable range as its
  unrolled sequence (MLS §11.2.2), binding the index as an Integer literal in
  the substitution map; a non-range or non-evaluable bound is a typed error.
- `Replay::entry_value` supplies the declared `start` (or the type default)
  for a coordinate a path leaves unassigned.
- `free_references` honours array-constructor binders.
- `analysis.rs` passes the expression roles (which include enumeration
  literals) to the initial-owner analysis.
- `reject_oversized_replay` bounds a replayed value at 50 000 expression
  nodes. The replay has no shared temporaries, so a loop whose guard reads
  the coordinate it updates copies the previous value three times per
  iteration. `CalendarTime`'s 12-iteration month search
  (`if (t - epochLastMonth)/86400 < d then ... else epochLastMonth := epochLastMonth + d`)
  would be over 500 000 copies, which ran past the 600 s evaluation timeout.
  It now gets a typed ED013 instead. The next step for these models is
  replay temporaries: generated initialization-only coordinates for
  intermediate values.

## Test

`suite_core/frontend_event_lowering.rs::an_initial_algorithm_loop_unrolls_and_keeps_unassigned_starts`.
