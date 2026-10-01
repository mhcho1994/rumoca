# TOOLBUG-134 — `size()` of a flexible parameter array not structural; synthesized-inner failure reported as "needs inner"

**Status:** fixed.
**Severity:** medium.

## What

```modelica
partial model PartialSimInfoManager           // IDEAS.BoundaryConditions.Interfaces
  parameter SI.Angle incAndAziInBus[:,:] = {{...}, ...};
  final parameter Integer numIncAndAziInBus = size(incAndAziInBus,1);
equation
  for i in 1:numIncAndAziInBus loop
    connect(solTim.y, radSol[i].solTim);
  end for;
```

1. Instantiation evaluates structural integers with the integer values it
   knows, but never with array extents, and a `:` extent is not settled when
   the component is registered. `size(incAndAziInBus, 1)` therefore did not
   evaluate and the connect for-range failed (EI004).
2. IDEAS `ShadingControl` has `outer SimInfoManager sim` and no inner. MLS §5.4
   lets the tool synthesize a default inner, and Rumoca retries with one; but
   when that retry failed (because of 1) the error was dropped and the user saw
   only "model needs inner declarations: sim".

Minimal: the contract test below (fails with EI004 before the fix).

Affected (cluster G): IDEAS `Buildings.Components.Shading.ShadingControl` — 1
model.

## Fix

- `rumoca-phase-instantiate/src/lib.rs`: `InstantiateContext` records the
  settled extents of each instantiated component (`known_dims`; a flexible `:`
  extent is taken from the shape of its `{...}` constructor binding,
  `array_constructor_shape`), and structural integer evaluation sees them.
- `inner_outer.rs` / `entry.rs`: `SyntheticInnerError::InstantiationFailed`
  carries the retry's `InstantiateError`, which is reported instead of the
  missing-inner summary.

## Test

`crates/rumoca-contracts/tests/decl_contracts.rs::connect_for_range_over_size_of_flexible_parameter_array`
(simulates).

## Known follow-up (not fixed)

With a `flow` member, a connector array whose extent is this kind of symbolic
structural integer gets its unconnected-flow equation `0 = c.f` sized from
`Variable::dims` before `recompute_symbolic_component_dimensions` settles them
(`connections/equation_generation.rs::generate_unconnected_flow_equations`), so
balance counts it as one scalar. A literal extent is unaffected.

After this fix IDEAS `ShadingControl` stops at EF025 (a `Medium.*` call through
a partial package), a proper diagnostic.
