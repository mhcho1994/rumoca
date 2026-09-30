# TOOLBUG-092 — package constants in conditional-component conditions

**Status:** fixed (this change).
**Severity:** high — `EI006 conditional component … requires parameter
expression` for every condition that reads a package constant, e.g. the
Buildings/IBPSA/AixLib fluid sources (`x_pTphi if not singleSubstance` with
`singleSubstance = Medium.nX == 1`) and the FMI adaptors (`X_w if Medium.nXi > 0`).

## What

Even `X x if PM.k == 2;` with `package PM constant Integer k = 1; end PM;`
failed. Three gaps in the instantiate-time Integer evaluator
(`rumoca-eval-ast/src/eval_instantiate`):

1. The class prefix of `PM.k` was looked up only as a fully qualified name,
   although resolve had already recorded the lexical target on the segment.
2. `size(A, k)` was not evaluated at all, so `nS = size(substanceNames, 1)`
   was unknown.
3. A forwarding `redeclare package Medium = Medium` was never followed, even
   when the right-hand `Medium` (looked up where the modifier is written)
   is a non-replaceable package alias such as `package Medium = AixLib.Media.Air`.

A prefix that the instance redeclares (an active modification-environment
entry) is not answered from the declared default; an unredeclared replaceable
prefix denotes its default. A forwarding redeclaration is followed only when
its target is not itself replaceable.

Affected cluster-C models (5): `AixLib…PressurizationData`,
`Buildings.Fluid.Sources.Examples.MassFlowSource_WeatherData`,
`IBPSA.Fluid.FMI.…AirToOutletFlowReversal`, `…HVACThermalZoneSimpleAir2/3`.

## Fix

- `eval_instantiate/mod.rs`: `static_class_prefix` (lexical `def_id`, no
  redeclared prefix) for class-constant references;
  `forwarded_class` for a forwarding redeclaration whose target is final.
- `eval_instantiate/size_eval.rs` (new): `size(A, k)` for a component of the
  scope, a record field (`size(layers.material, 1)` is `layers.nLay`), or a
  class constant; a `:` extent is read from the `{...}` or scalar `[...; ...]`
  constructor that binds it.

## Test

`structural_evaluation_regressions.rs`:
`package_constant_conditions_follow_lexical_and_forwarded_packages` (checks
both the forwarded `Air` package and the `PM` default).
