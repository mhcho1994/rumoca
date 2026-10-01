# TOOLBUG-144 — inherited media function keeps the partial default's scope under a component redeclaration

**Status:** fixed.
**Severity:** high — a non-redeclared function inherited from the partial
package (`PartialMedium.specificEnthalpy_pTX`, `density_pTX`, ...) that calls
a sibling (`setState_pTX`, `density`, ...) reached the DAE as a call to the
partial sibling (`ED008 unresolved Flat reference L.PM.f`) when the medium was
selected by `Q q(redeclare package Medium = W)`. In the same calls, the
argument `time` was rewritten to the package member `q.Medium.time`.

## What

```modelica
partial package PM
  replaceable partial function f input Real x; output Real y; end f;
  replaceable function twice input Real x; output Real y; algorithm y := 2*f(x); end twice;
end PM;
package W extends PM; redeclare function extends f algorithm y := 3*x; end f; end W;
model Q
  replaceable package Medium = PM;
  Real w(start=0, fixed=true);
equation
  der(w) = Medium.twice(time);
end Q;
model ByComponent Q q(redeclare package Medium = W); end ByComponent;
```

`twice` is the same declaration in `PM` and `W`, so the override projection
found "nothing to retarget" and left the call exposed as `Q.Medium.twice`,
whose body then resolved `f` in `PM` (MLS §5.3 requires the package it is an
element of, `W`). `extends Q(redeclare ...)` worked only because the root
scope's override applied to function bodies. Separately, the member-reference
rewrite captured any reference whose declaration has no enclosing class,
including the predefined `time`, into the scope's unique replaceable package.

## Fix

`crates/rumoca-phase-flatten/src/pipeline/function_overrides_and_dims/`:
- `function_selection.rs` (`exact_package_function_rewrite`): when the call is
  written through the replaceable alias the package was selected for, and the
  selected package is not the declaring package or a pure alias of it, the
  call is re-exposed through the selected package even though the
  implementation is unchanged, so the body is converted in that scope.
- `member_references.rs`: the predefined `time` is never a package member.

## Test

`crates/rumoca-contracts/tests/func_contracts.rs::inherited_package_function_reaches_selected_package_members`
(component and extends redeclaration; simulates both a constant and a `time`
argument).
