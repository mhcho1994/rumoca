# TOOLBUG-172 — `size(x, 1)` of a `:` array bound to another array (EF002)

**Status:** fixed.
**Severity:** medium.

## What

```modelica
parameter Real stageInputs[:] = massFlowRates;          // via modifier
parameter Real massFlowRates[:] = m_flow_nominal*{s[i]/s[end] for i in 1:size(s, 1)};
Constant[size(stageInputs, 1)] stageValues(final k = stageInputs);
```

Instantiation sizes component arrays itself. Its `size()` evaluator (round 1,
`size_eval.rs`) read a `:` extent only from an array constructor or an
Integer-range comprehension binding, so `stageValues` was not expanded and
`connect(stageValues.y, extractor.u)` failed:
`EF002 incompatible connector types ... (dims: []) and ... (dims: [3])`.

Affected in cluster R2-dims: IBPSA `Movers.Examples.MoverStages` and the other
`stageInputs` mover cases (after TOOLBUG-171).

## Fix

`crates/rumoca-eval-ast/src/eval_instantiate/size_eval.rs` (`binding_extent`):
a `:` binding that references another array takes that array's extent, and an
element-wise scaling by a scalar (`k * v`, `v / k`) keeps the array operand's
extent (MLS §10.6.4).

## Test

`array_dimension_regressions.rs::extends_attribute_modifier_keeps_binding_and_sizes_component_array`
(simulates; `extractor.y = 19`).
