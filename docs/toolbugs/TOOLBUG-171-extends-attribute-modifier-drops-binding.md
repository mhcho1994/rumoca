# TOOLBUG-171 — `extends B(x(attr = v) = e)` dropped the binding `e` (ET004/EF010)

**Status:** fixed.
**Severity:** high (silently lost a binding).

## What

```modelica
model FlowControlled_m_flow
  extends PartialFlowMachine(final stageInputs(each final unit="kg/s") = massFlowRates);
```

The parser represents a modifier with both attribute modifiers and a binding
as `Binary(Assign, ClassModification, e)`. Component modifiers handled that
form, but the extends-clause inheritance helpers (value modifications, target
names, nested attribute merging) recognised only `Modification`,
`ClassModification` and named arguments, so `stageInputs` lost both its
binding and its `unit`. With no binding its `:` extent was unknown:
`ET004`/`EF010 unresolved component dimension for ...stageInputs: :`.

Affected in cluster R2-dims: the `stageInputs` cases (IBPSA `MoverStages`,
IDEAS `FlowControlled_dp`, `FlowControlled_m_flow`, `ComparePowerTotal`,
`ControlledFlowMachinePreconfigured` ×2 incl. AixLib) and every model using
those movers.

## Fix

`crates/rumoca-phase-instantiate/src/inheritance/redeclaration.rs`
(`extract_extend_modification_target`, `extract_modification_value`) and
`inheritance.rs` (`extend_nested_target_modifications`) accept the
`x(attr = v) = e` form.

## Test

`array_dimension_regressions.rs::extends_attribute_modifier_keeps_binding_and_sizes_component_array`.
