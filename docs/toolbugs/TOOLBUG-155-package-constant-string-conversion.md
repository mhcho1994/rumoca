# TOOLBUG-155 — `String(x)` in a package-constant modifier left as a call named `String` (ED008)

**Status:** fixed.
**Severity:** low.

## What

Package constants given by an `extends` modifier are folded by the
constant-injection evaluator in flatten. Its call evaluator turned every
non-builtin call into a plain `FunctionCall`, including the predefined
`String` conversion (MLS §3.7.1.2), which everywhere else is lowered to
`StringConversion`. The DAE then had no owner for a call named `String`:
`ED008 unresolved Flat reference String`.

```modelica
package Med
  constant Real X_a = 0.4;
  extends Base(mediumName = "PG(X_a = " + String(X_a) + ")");
end Med;
```

Affected (cluster R2-fnbody): IDEAS and AixLib
`Media.Examples.PropyleneGlycolWaterDerivativeCheck` — 2 models (they then
stop on `polynomialProperty`, a loop whose inner range depends on the outer
index, ED019, not fixed here).

## Fix

`crates/rumoca-phase-flatten/src/pipeline/constant_injection.rs`: a one-argument
call whose callee is the predefined `String` declaration becomes
`StringConversion` with default formatting.

## Test

`function_body_and_binding_regressions.rs::package_constant_string_conversion_is_the_predefined_operator`.
