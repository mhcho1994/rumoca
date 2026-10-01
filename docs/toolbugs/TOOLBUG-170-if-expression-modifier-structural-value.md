# TOOLBUG-170 — if-expression modifiers invisible to nested conditional components (EI006)

**Status:** fixed.
**Severity:** medium.

## What

```modelica
block PID
  parameter Reset reset = Reset.Disabled;
  IntegratorWithReset I(final reset = if reset == Reset.Disabled then reset else Reset.Input);
end PID;
block IntegratorWithReset
  parameter Reset reset;
  RealInput y_reset_in if reset == Reset.Input;
end IntegratorWithReset;
```

The modifier value reached `I`'s modification environment unevaluated. Inside
`I` the branch `reset` names `PID.reset`, which `I`'s environment cannot see,
so `y_reset_in`'s condition stayed undecided:
`EI006 conditional component y_reset_in requires parameter expression`.

Affected in cluster R2-dims: the 7 `y_reset_in` cases (IBPSA/IDEAS/TRANSFORM
`LimPID` users: `WaterCooler_T`, `Adsolair58`, `Case660`, `Case920`,
`LimPID_withReset_Test`, `CS_LoadFollow`); `LimPID_withReset_Test` now
compiles, the others stop at later errors.

## Fix

`crates/rumoca-phase-instantiate/src/mod_env.rs` (`decided_if_branch`):
while resolving a modifier in the scope that writes it, an if-expression whose
conditions all evaluate there is replaced by its selected branch (MLS §3.6.5),
which is then resolved as usual.

## Test

`array_dimension_regressions.rs::if_expression_modifier_decides_nested_conditional_component`.
