# TOOLBUG-102 — members of an un-redeclared replaceable package had no identity

**Status:** fixed.
**Severity:** high — `EF024 flat variable is missing structured identity`
for Medium constants (`nX`, `nXi`, `X`, `p_default`, `substanceNames`, ...)
whenever a model is compiled with its `replaceable package Medium` left at the
declared default: IBPSA/AixLib `LumpedVolumeDeclarations`, psychrometric
blocks, ThermoPower gas tests (EF024 is the first error of 20 cases in
cluster D).

## What

```modelica
package P
  partial package PartialMedium
    constant Integer nX = 2;
  end PartialMedium;
  model Decl
    replaceable package Medium = PartialMedium;
    parameter Real X_start[Medium.nX];
  end Decl;
end P;
```

Resolve defers a reference rooted at a replaceable class (`Medium.nX`),
because the selected class is instance dependent; instantiation then proves
the member in the class it selected. Post-materialization only did so for
source scopes that had at least one redeclaration or selected component type
and skipped the rest, and the deferred-reference resolver had no notion of
the declared default. With no redeclaration the member reached the Flat
boundary without a DefId. With `redeclare package Medium = Air` it worked.

## Fix

`crates/rumoca-phase-instantiate/src/type_overrides/`:
- `deferred_references.rs`: when no modification selects a class for a
  replaceable class alias, the alias denotes its declared default (MLS §7.3),
  so the member is proved in that declaration. A default that does not name
  the member leaves the reference deferred instead of failing.
- `post_materialization.rs`: every source scope is resolved, not only scopes
  with redeclarations (the now-unused `TypeOverrideMap::is_empty` is gone).

## Test

`crates/rumoca/tests/suite_core/frontend_functions_names.rs`:
`unredeclared_replaceable_package_dimension_uses_the_default` simulates the
default (`nX = 2`) and a redeclared (`nX = 3`) instance and checks each uses
its own constant.

## Remaining

`PartialMedium`'s own `nX = size(substanceNames, 1)` over
`constant String substanceNames[:]` is not evaluable as a dimension
(`ET004`/`ED020`), so a block that declares `X_start[Medium.nX]` with the MSL
`PartialMedium` default still fails, one step later.
