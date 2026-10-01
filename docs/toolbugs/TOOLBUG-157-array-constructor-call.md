# TOOLBUG-157 — `array(...)` constructor call left as a call to an unknown function (ED008)

**Status:** fixed.
**Severity:** medium.

## What

MLS §10.4.1 defines `array(A, B, C, ...)` as the array constructor that the
`{A, B, C, ...}` syntax abbreviates. Resolve binds it to the predefined
`array` declaration, but AST lowering had no rule for it and emitted a plain
`FunctionCall` named `array`, which the DAE cannot own:
`ED008 unresolved Flat reference array`. It failed in model bindings
(`parameter Real p[2] = array(4, 5)`) and in function constants alike.

Affected (cluster R2-fnbody): ThermoSysPro
`Examples.SimpleExamples.TestFresnelField` (`BaseIF97`:
`constant Real[42] nn = array(...)`) — 1 model; `BaseIF97` is the water/steam
property base of ThermoSysPro, so more of that library is reached.

## Fix

`crates/rumoca-phase-flatten/src/ast_lower.rs`: `PredefinedIntrinsicIds`
carries the predefined `array` identity, and a call of exactly that
declaration lowers to `Expression::Array` (a user function named `array`
keeps its own identity and is unaffected).

## Test

`function_body_and_binding_regressions.rs::array_constructor_call_is_an_array`.
