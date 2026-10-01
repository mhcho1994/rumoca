# TOOLBUG-153 — Parameter binding that divides by a defaulted zero parameter rejected (ED019)

**Status:** fixed.
**Severity:** medium.

## What

Library base classes declare parameters without a binding (`parameter Length
length;`), so their value is the `start` default 0 until a user sets it, and
derived bindings such as `conUM = 4/rho/dh/dh/pi/length` divide by it. The DAE's
translation-time parameter evaluation treated the division by zero as fatal:
`ED019 parameter binding: conUM cannot be evaluated: division by zero`.
OpenModelica checks these models; the binding is a runtime parameter
equation, and the value only matters if it is used structurally.

Affected (cluster R2-fnbody): IBPSA `PlugFlowTransportDelay`, OpenIPSL
`PSAT.Order6`, OpenIPSL `PV_Plant`, TRANSFORM `PumpCharacteristics` (`d_curve`),
TRANSFORM `Initial_FissionProducts_Test` (`mCs_start2`) — 5 models
(`PV_Plant` now compiles; the others reach later errors).

## Fix

`crates/rumoca-phase-dae/src/construction/analysis.rs`: a `DivisionByZero`
evaluation error leaves the parameter unevaluated (runtime binding), like a
binding whose value is not known at translation time. A consumer that needs
the value structurally still reports its own error.

## Test

`function_body_and_binding_regressions.rs::parameter_bindings_broadcast_and_defer_division_by_zero`.
