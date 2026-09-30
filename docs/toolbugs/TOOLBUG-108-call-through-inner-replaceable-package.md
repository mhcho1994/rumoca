# TOOLBUG-108 — call through a replaceable package inside a qualified name

**Status:** fixed.
**Severity:** high — ThermoSysPro calls every water/steam property function as
`ThermoSysPro.Properties.WaterSteam.IF97.Water_Ph(...)`, where `WaterSteam`
declares `replaceable package IF97 = IF97_packages.IF97_wAJ`. Once TOOLBUG-105
let ThermoSysPro load MSL, these calls failed with `ER002 unresolved function
call` (TestFresnelField, TestVolumes2, TestStodolaTurbine2 in cluster D, and
most of ThermoSysPro's fluid models).

## What

```modelica
package Props
  package Impl function f input Real x; output Real y; algorithm y := 2*x; end f; end Impl;
  replaceable package P = Impl;
end Props;
model M Real y = Props.P.f(time); end M;
```

Resolve correctly defers the tail after the replaceable class `P` (MLS §7.3),
but two later steps assumed the replaceable class is the *root* of the
reference:
1. the post-resolution validator only excused deferred callees whose root was
   a component or replaceable class, so it reported `Props.P.f` unresolved;
2. instantiation's deferred-reference resolver only proved members after the
   root segment, so the tail kept no identity (EF024).

## Fix

- `crates/rumoca-phase-resolve/src/validation.rs`: a callee whose resolved
  prefix passes through a replaceable class and whose tail is unresolved is
  deferred, not unresolved.
- `crates/rumoca-phase-instantiate/src/type_overrides/deferred_references.rs`:
  the deferred edge is the last resolved segment; the selected class (a
  redeclaration, a selected component type, or the declared default per
  TOOLBUG-102) proves the members after it.

## Test

`crates/rumoca/tests/suite_core/frontend_functions_names.rs`:
`call_through_an_inner_replaceable_package_uses_its_default`.
