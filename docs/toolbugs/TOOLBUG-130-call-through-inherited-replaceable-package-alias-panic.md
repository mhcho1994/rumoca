# TOOLBUG-130 — call through an inherited replaceable package alias panics in function collection

**Status:** fixed.
**Severity:** high (compiler panic).

## What

```modelica
partial model PartialCompressor
  replaceable package ref = IBPSA.Media.Refrigerants.R410A;
equation
  vSuc = ref.specificVolumeVap_pT(pSuc, TSuc);
end PartialCompressor;
model ReciprocatingCompressor
  extends PartialCompressor;            // ref is not redeclared
  ...
  k = ref.isentropicExponentVap_Tv(TSuc, vSuc);
end ReciprocatingCompressor;
```

```
panicked at crates/rumoca-phase-flatten/src/functions/function_requests.rs:29:22:
function collection receives only resolved references
```

Resolve defers `ref.f` across the replaceable edge (only `ref` gets a
`DefId`). Instantiation completes the member identity from the
`TypeOverrideMap` of the instantiated class, but that map only held the
class's *own* nested classes, its enclosing package's, and redeclarations in
`extends` modifications. A replaceable package declared in a base class and
not redeclared (it selects its default) was absent, so `f` kept no identity
and function collection hit its `expect`.

Minimal: `model Base replaceable package ref = P.R; end Base;
model M extends Base; Real w; equation w = ref.f(time); end M;`.

Affected (cluster G): IBPSA/Buildings `HeatPumps.Compressors.ReciprocatingCompressor`,
IDEAS `ScrollCompressor`, AixLib `Compressors...Buck_..._Scroll`,
`...Poly_R134a_170_Reciprocating`, TRANSFORM `TableBasedInterpolation_old`
— 6 models. The panic is gone in all six; each now reaches its next
diagnostic (see TOOLBUG-132 for the IBPSA/Buildings/IDEAS one; AixLib and
TRANSFORM stop at EF025 on a `Medium.*` call against the partial default
`PartialMedium`, a proper diagnostic).

## Fix

`rumoca-phase-instantiate/src/type_overrides/override_collection.rs::build_type_override_map`:
also collect the nested classes inherited through the extends chain (MLS
§7.1), after the class's own classes and extends-redeclarations and before
the enclosing package's (members shadow enclosing declarations, MLS §5.3.1).

## Test

`crates/rumoca-contracts/tests/func_contracts.rs::call_through_inherited_replaceable_package_alias`
(simulates; checks the integrated value).
