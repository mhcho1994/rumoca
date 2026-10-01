# TOOLBUG-152 — `.+`/`.-`/`./`/`.^` with a scalar operand refused by the constant evaluator (ED019)

**Status:** fixed.
**Severity:** medium.

## What

MLS §10.6.2–§10.6.6 define the element-wise operators for one scalar and one
array operand (`{65, 95, 110} .- 10`, `2*pi .+ {a, b}`). The Flat constant
evaluator mapped `.+`/`.-` onto `+`/`-`, which only accept matching shapes, so
a parameter binding using them failed with
`ED019 parameter binding: cannot be evaluated: type mismatch: expected numeric or array, got Array - Integer`.

Affected (cluster R2-fnbody): IBPSA/AixLib
`Electrical.Transmission.Functions.Validation.SelectCable_low` (`I_nominal`),
IBPSA `Airflow.Multizone.BaseClasses.Examples.WindPressureProfile`
(`incAngExt`) — 3 models move past this error (they then stop on function
assertion owners, ED019, not fixed here).

## Fix

`crates/rumoca-eval-flat/src/constant/operators.rs`: `broadcast_elementwise`
applies the scalar to every element for the element-wise operators; the plain
operators keep their stricter rule.

## Test

`crates/rumoca-eval-flat/src/constant/tests.rs::elementwise_add_sub_broadcast_a_scalar_operand`,
`function_body_and_binding_regressions.rs::parameter_bindings_broadcast_and_defer_division_by_zero`.
