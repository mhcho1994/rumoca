# TOOLBUG-064 — a non-finite algebraic at the seed iterate aborted initialization

**Status:** fixed (this change), following TOOLBUG-063.
**Severity:** medium — IBPSA's `PowerLaw_dp_DerivativeCheck` and
`PowerLaw_dp_DerivativeCheck2` failed with "non-finite (inf) value computed
for `m_flow`" during initialization.

## What

An initialization row that reads an algebraic is evaluated on a view whose
algebraics are refreshed from the current unknowns (TOOLBUG-051). At the seed
iterate, the `fixed = false` coefficients the initialization determines are
0, and the continuous equations can make an algebraic non-finite there:

```modelica
parameter Real d(fixed = false);
parameter Real m(fixed = false);
...
initial equation
  d = 0.5; m = 0.5; z = y;
equation
  x = time;
  y = if abs(x) < d then x else x^m;   // d = 0 selects x^m; der(y) = m*x^(m-1)
  der(z) = der(y);
```

The refresh validated every value and failed the whole evaluation on the
first non-finite one, so the rows that solve `d` and `m` never ran.

## Fix

In the initialization's residual view a `NonFiniteValue` from the refresh is
a value, not a failure: the partially refreshed view is returned, the rows
that read the non-finite coordinate are non-finite, and the projection
rejects that iterate row by row while the other rows solve. The
complete-residual certificate evaluates the same view, so a coordinate that
stays non-finite still fails initialization.

The model above initializes (`z` tracks `y`), and fails without the change
with the original error; both IBPSA `PowerLaw_dp_DerivativeCheck` models run.

## Still open

When a refresh row's conservative incidence (all unknowns, TOOLBUG-051)
couples it into one block with a parameter's own row, an infinite Jacobian
entry at the seed still blocks that block's Newton step
(`y = 1/(time - c)` with `c = 5` and `z = y` initial). Splitting it needs the
precise incidence through the algebraic refresh.
