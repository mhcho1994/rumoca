# TOOLBUG-067 — `terminate` in an algorithm `when`, and `sample(u, Clock(...))`, are refused

**Status:** open.
**Severity:** low — each has an equivalent spelling that compiles; found while
writing `packages/modelsan/tests/test_sdk_typed_tables.py`.

## What

```modelica
algorithm
  when time > 0.25 then
    terminate("done");      // ED013: function-call assignment must retain at least one output
  end when;
```

Model algorithms accept assignments, assertions, function-call assignments
with outputs and conditional discrete updates
(`rumoca-phase-dae/src/construction/analysis/model_algorithm_statements.rs`).
`terminate(...)` (MLS §8.3.8) and `reinit(...)` (§8.3.6) inside an algorithm
`when` reach the function-call arm with no outputs and are refused. The same
statements in an equation `when` lower to `terminate` / `reinitialize` event
actions.

```modelica
  s = sample(x, Clock(1, 10));   // ED009: a clock equation must be an exact
                                 // whole-coordinate constructor or alias
```

MLS §16.5.1 allows any Clock expression as the second argument of `sample`.
The clock schedule resolver accepts only a Clock *coordinate* bound by a
constructor, alias or sub/super/shift/back-sample composition, so an inline
constructor has nothing to resolve to.

## Workaround

Put `terminate` in an equation `when`; declare `Clock c = Clock(1, 10);` and
use `when c then s = sample(x); ... end when;` or `sample(x, c)`.

## Fix direction

- Lower an algorithm-`when` `terminate`/`reinit` to the same event actions the
  equation form produces, guarded by the block's condition.
- Hoist an inline Clock constructor argument of `sample`/`subSample`/... to a
  synthesized Clock coordinate before schedule resolution.
