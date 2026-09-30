# TOOLBUG-065 — bitcode export ran out of memory on a function with nested loops

**Status:** fixed (this change).
**Severity:** high — `--emit-bitcode` (and any compile through the pass
stage) exhausted memory on a 25-line function; found when routing the whole
suite through the pass stage, where the galec suite grew past 20 GB.

## What

```modelica
function crossedScratch
  input Real m[3, 3]; input Real b[3, 3]; input Integer mode;
  output Real y[3, 3]; output Real z[3, 3];
protected
  Real fromZ; Real fromY;
algorithm
  y := m;
  z := b + m;
  for column in 1:2 loop
    if mode == 2 then
      for row in 2:3 loop
        if row > column then
          fromZ := z[row, column] / z[column, column];
          fromY := y[row, column] / y[column, column];
          y[row, :] := y[row, :] - fromZ * y[column, :];
          z[row, :] := z[row, :] - fromY * z[column, :];
        end if;
      end for;
    end if;
  end for;
end crossedScratch;
```

Export records, per equation, exactly which variables it reads, projecting
each scalar through function bodies (`rumoca_eval_dae::for_each_scalar_coordinate`).
Projecting a loop-carried value projects every iteration's update, and each
update reads the previous iteration's carried value -- of another scalar --
so the projection re-entered the same loop from inside one of its own
iterations, pushing another point of the same loop domain each time. The
cycle guard only blocked exact ancestors, so the recursion explored
orderings of the carried scalars (18 here): the domain-point stack reached
17 entries of one domain and memory grew without bound.

The normal compile never exports, so only the pass stage and
`--emit-bitcode` hit it.

## Fix

- Projecting a loop-carried value drops the enclosing points of that loop's
  own domain while it runs (they are shadowed: the loop's complete projection
  does not depend on which of its iterations asked) and restores them after.
- Completed loop-carried projections are memoized, keyed by the dependency,
  the call it is projected in (summary, with a per-capture serial, or the
  concrete arguments) and the remaining enclosing loop points -- everything
  the projection can visit is a function of these.

`--emit-bitcode` of the model: 32 MB and 0.01 s, where it exhausted a 4 GB
cap; the artifact round-trips, and each equation reads `m`, `b` and `mode`.
