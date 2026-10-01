# TOOLBUG-145 — short function alias called only by a sibling has no Flat instance

**Status:** fixed.
**Severity:** high — every water/steam model using MSL `WaterIF97_*`
`setState_phX` failed with `EF005 internal flatten error: function reference
Modelica.Media.Water.IF97_Utilities.hl_p has declaration identity but no Flat
function instance` (also `rhol_T`, `thermalConductivity`).

## What

```modelica
package P
  package B function f input Real x; output Real y; algorithm y := 2*x; end f; end B;
  function f = B.f;
  function g input Real x; output Real y; algorithm y := if x < 0 then 0 else f(x); end g;
  model M Real w(start=0, fixed=true); equation der(w) = g(1.5); end M;
end P;
```

TOOLBUG-106 restates the sibling call as `P.f` (the alias). When nothing calls
`P.f` by that name, only the target `P.B.f` is collected (plus its short-name
copy, so a declaration-only match is ambiguous), and call canonicalization
found no function for `P.f`.

Affected in cluster R2-media: 17 models that reached EF005 once
TOOLBUG-140/143 let them past function selection (ThermoPower water
components and turbines, AixLib `TwoPhaseSeparator`, TRANSFORM pipes/valves).

## Fix

`crates/rumoca-phase-flatten/src/functions/call_canonicalization.rs`:
`by_pure_alias_target` matches a call whose rendered name is a pure short
function alias (one unmodified base, no components, classes, body or external
clause) whose base is exactly the call's target declaration, to the collected
function of that declaration.

## Test

`crates/rumoca-contracts/tests/func_contracts.rs::short_function_alias_called_only_by_a_sibling`
(simulates).
