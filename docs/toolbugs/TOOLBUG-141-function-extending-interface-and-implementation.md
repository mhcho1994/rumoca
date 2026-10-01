# TOOLBUG-141 — function extending both its interface and an implementation has no selection

**Status:** fixed.
**Severity:** high — every call of MSL 4 `Modelica.Utilities.Files.loadResource`
failed in Flatten with `EF025 missing exact function-selection identity ...
exposed function has no unique exact extends implementation`. Weather files,
CSV readers, table files and EnergyPlus inputs all use it.

## What

```modelica
partial function PI input Real x; output Real y; end PI;
function Impl extends PI; algorithm y := 2*x; end Impl;
function F extends PI; extends Impl; end F;     // MSL Files.loadResource
model M Real w(start=0, fixed=true); equation der(w) = F(1.5); end M;
```

`resolve_function_extends_target_def_id` walked the function-extends chain
and gave up when a body-less function had two distinct function bases. A
function has at most one algorithm or external body (MLS §12.2); the
interface-only base contributes none, so `Impl` is the unique implementation.

Affected in cluster R2-media: 18 models (the `loadResource` EF025 group:
IBPSA/IDEAS/AixLib/Buildings weather-data and solar examples, CSV/schedule
readers, TRANSFORM `Interpolation_2D_Test_*`, Modelica_DeviceDrivers
`TestLoadRealParameter`). Most then hit TOOLBUG-142 and, past it, the
unsupported external-object tables (`ExternalCombiTimeTable`, ED019), which
are outside this cluster.

## Fix

`crates/rumoca-phase-flatten/src/pipeline/function_overrides_and_dims/exact_identity_queries.rs`:
`unique_function_body` searches the function bases for the one body they
supply (interface-only bases contribute nothing, a diamond reaching the same
body is unique, two distinct bodies stay ambiguous). The old single-chain walk
remains the fallback for interface-only chains.

## Test

`crates/rumoca-contracts/tests/func_contracts.rs::function_extending_interface_and_implementation_selects_implementation`
(simulates).
