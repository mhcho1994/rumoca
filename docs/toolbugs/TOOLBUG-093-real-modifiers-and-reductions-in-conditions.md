# TOOLBUG-093 — Real modifiers and array reductions in conditions

**Status:** fixed (this change).
**Severity:** high — `EI006` for conditions derived from Real parameters.

## What

```modelica
model FirstOrder
  parameter Real T;
  parameter Boolean noDynamics = not (T > 0);
  X x if not noDynamics;
end FirstOrder;
model Top  parameter Real T1 = 0.1;  FirstOrder f(T = T1);  end Top;
```

MLS §7.2.4 evaluates the modifier `T = T1` in `Top`. Integer and Boolean
modifiers are decided there when the modification environment is built, but a
Real modifier keeps its symbolic form (so that parameter propagation survives
into the flat model), and was later evaluated in `FirstOrder`, where `T1` does
not exist. Likewise `solRad[n] if sum(ATransparent) > 0` failed because no
Real reduction (`sum`, `product`, `min`, `max`) or `abs`/`sqrt` was evaluated.

Affected cluster-C models: PowerGrids `IEEE_PSS2A`, `GovHydro4GeneralPurpose`,
`GovHydro4Kaplan` (`not noDynamics`); OpenIPSL DIgSILENT `Controller`,
`PV_Plant` (`with_I = if T > 0 …`); the ReducedOrder `solRad` validations in
AixLib, IDEAS and Buildings (`sum(ATransparent) > 0`).

## Fix

- `rumoca-ir-ast/src/instance.rs`: `ModificationValue::structural_real`, the
  Real value of a modifier decided in the scope that wrote it.
- `rumoca-phase-instantiate/src/mod_env.rs`: fill it when a component
  modifier is inserted.
- `rumoca-eval-ast/src/eval_instantiate/mod.rs`: the Real lookup reads it.
- `rumoca-eval-ast/src/eval_instantiate/real_builtins.rs` (new): Real
  reductions and `abs`/`sqrt`; a modifier array is reduced only when all its
  elements are literals (they were written in another scope).

## Test

`structural_evaluation_regressions.rs`:
`real_modifiers_and_array_reductions_decide_conditions`.
