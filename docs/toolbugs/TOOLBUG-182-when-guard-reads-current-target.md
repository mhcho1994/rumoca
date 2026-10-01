# TOOLBUG-182 — if-equation guard in a when-branch reading a target of the same branch

**Status:** fixed.
**Severity:** medium.

## What

```modelica
when daySampleTrigger then
  if t - timeStampsNewYear[pre(yearIndex) + 1] > -eps then
    yearIndex = pre(yearIndex) + 1;
  else
    yearIndex = pre(yearIndex);
  end if;
  if pre(month) == 2 and isLeapYear[yearIndex] then   // current yearIndex
    month = ...;
  else
    month = pre(month);
  end if;
end when;
```

When-equations are simultaneous, so the second guard reads the `yearIndex`
the first if-equation defines. Rumoca made every target of a when chain
depend on *every* if-guard in the chain. That turned this acyclic system into
the error "`yearIndex` participates in an internal current-value dependency
cycle". Even with the dependency fixed, lowering if-guards as DAE branch
activations still failed: an owner's branch activation may not read its own
targets.

Affected: every `CalendarTime` instance (IBPSA/IDEAS/AixLib/Buildings). It
showed up once TOOLBUG-181 let their initial algorithms through.

## Fix

- `analysis/discrete_values.rs`: a when target depends on the chain's
  activation conditions plus the guards of the if-equations that assign it,
  not on every guard in the chain.
- `model_events.rs::conditional_target_values`: an if-equation whose guard
  reads a current target of its own chain, and whose arms (`else` included)
  all assign the same targets with plain assignments, is lowered as one
  conditional value per target (`m = if g then a else b`). This is exact
  because one arm's values hold whenever the branch is active. The DAE
  owner's prefix-order check then accepts the read of the earlier target.
  Any other shape keeps the guard lowering and its typed rejection.

## Test

`suite_core/frontend_event_lowering.rs::a_when_branch_guard_reads_the_current_value_of_an_earlier_target`
(month updates use the current `yearIndex`; with `pre(yearIndex)` the
trajectory would differ at t = 1.5).
