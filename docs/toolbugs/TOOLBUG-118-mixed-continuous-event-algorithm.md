# TOOLBUG-118 — an event algorithm with a continuous prefix

**Status:** fixed (this change).
**Severity:** low-medium — ThermoSysPro `Discret.ConvAD` failed with
`[ED013] unsupported model algorithm in canonical DAE: a mixed continuous/
event algorithm requires one checked atomic owner`.

## What

```modelica
algorithm
  qInterval := (maxval - minval)/2^bits;
  when sample(SampleOffset, SampleInterval) then
    uBound := ...;
    y.signal := qInterval*floor(abs(uBound/qInterval) + 0.5)*sign(uBound);
  end when;
```

An algorithm owner is declarative (continuous targets) or an event
transaction (discrete targets), never both.

## Fix

`crates/rumoca-phase-dae/src/construction/model_algorithm_split.rs`: before
analysis, the leading top-level assignments of an algorithm that contains a
`when` are moved into their own algorithm when each writes a scalar continuous
coordinate that nothing else in the section writes and reads none of the
section's other targets. Such a statement computes the same value as the first
statement of the section or as its own section, and every later read in the
section sees that value (MLS §11.1.2), so the split is exact. It runs with the
TOOLBUG-114 rewrite in `construction.rs::normalize_flat`.

## Test

`frontend_event_lowering.rs::the_continuous_prefix_of_an_event_algorithm_is_split_off`.
