# TOOLBUG-169 — `size()` of a literal array left symbolic (ED020)

**Status:** fixed.
**Severity:** medium (every model with the default `PartialMedium`).

## What

MSL `PartialMedium`: `nX = size(substanceNames, 1)`,
`X_default = fill(1/nX, nX)`. After constant substitution the Flat binding of
`X_start[Medium.nX] = Medium.X_default` reads
`fill(1/size({"unusablePartialMedium"}, 1), size({"unusablePartialMedium"}, 1))`;
the checked DAE needs the `fill` extent as a literal:
`ED020 array extent must be a nonnegative literal Integer`.

Affected in cluster R2-dims: IBPSA/AixLib `Fluid.Interfaces.LumpedVolumeDeclarations`
(and every model instantiating it with the default medium).

## Fix

`rumoca-phase-flatten/src/ast_lower/static_size.rs`: `size(e, k)` of an array
literal or a `fill`/`zeros`/`ones` call with literal extents folds to the
literal extent, both when lowering and in the late Flat rewrite
(`postprocess/array_domain_comprehensions.rs`) that runs after constant
substitution.

## Test

`array_dimension_regressions.rs::size_of_literal_array_is_a_literal_extent`.
