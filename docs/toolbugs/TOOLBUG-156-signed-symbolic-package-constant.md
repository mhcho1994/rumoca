# TOOLBUG-156 — Package constant bound by `+expr`/`-expr` not folded into modifiers (ED008)

**Status:** fixed.
**Severity:** low.

## What

The constant-injection evaluator in flatten keeps a symbolic `Binary` for a
package constant such as `pi/2`, but negation only handled literal operands
and unary `+` was not handled at all, so `W = +Modelica.Constants.pi/2` and
`E = -Modelica.Constants.pi/2` (IDEAS `Types.Azimuth`) could not be evaluated
and a modifier `k = {Azimuth.S, Azimuth.W, Azimuth.E}` reached the DAE as an
unresolved reference: `ED008 unresolved Flat reference IDEAS.Types.Azimuth.W`.

Affected (cluster R2-fnbody): IDEAS
`Buildings.Components.BaseClasses.RadiativeHeatTransfer.Examples.ZoneLwGainDistribution`
— 1 model (IDEAS uses `Types.Azimuth` in 29 places).

## Fix

`crates/rumoca-phase-flatten/src/pipeline/constant_injection.rs`: unary `+` is
the identity; unary `-` over a symbolic operand stays a symbolic `Unary`, like
`Binary` already does. Integer negation is checked for overflow.

## Test

`function_body_and_binding_regressions.rs::signed_symbolic_package_constants_fold_into_modifiers`.
