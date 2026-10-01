# TOOLBUG-166 — literal array extents lost under a redeclared component (ET004)

**Status:** fixed.
**Severity:** medium.

## What

```modelica
model Impl
  final parameter PW inter(V_flow = {1, 2, 3});   // PW: Real V_flow[3]
  final parameter PP powEu(V_flow = inter.V_flow); // PP: V_flow[:], P[size(V_flow,1)]
end Impl;
model M  extends Base(redeclare Impl floMacSta); end M;
```

Constant collection clears the evaluation-context scope of every redeclared
alias before seeding the override's class constants (MLS §7.3). For a
redeclared *component* (`redeclare Impl floMacSta`) that scope also held the
instance values collected from the instance tree, including the extents of
literal-size arrays (`floMacSta.inter.V_flow`). Only declarations with a
dimension *expression* were re-registered by the dimension passes, so
`powEu.V_flow`'s shape (from `inter.V_flow`) and then `size(V_flow, 1)` stayed
unevaluable: `ET004 unevaluable array dimensions for 'floMacSta.eff.powEu.P'`.

Affected in cluster R2-dims: IDEAS `Movers.Validation.Pump_stratos`,
`FlowControlled_dp`, `FlowControlled_m_flow`,
`Preconfigured.Validation.ControlledFlowMachinePreconfigured`, AixLib
`ControlledFlowMachinePreconfigured` (revealed after TOOLBUG-165).

## Fix

`rumoca-phase-typecheck/src/typechecker/late_methods/dimensions.rs`: the
explicit-dimension pass restores a missing context entry from the extents
instantiation recorded (never overwriting an existing entry, so alias
propagation keeps precedence), and registers evaluated explicit extents
when the context lacks them even if the instance already carried them.

## Test

`array_dimension_regressions.rs::literal_extents_survive_component_redeclaration`.
