# TOOLBUG-132 — flexible-size protected function local rejected (ED019 "no call-site equality")

**Status:** fixed.
**Severity:** medium.

## What

```modelica
function pressureSatVap_T              // IBPSA/Buildings Media.Refrigerants.R410A
  ...
protected
  final Real a[:] = {-1.440004, -6.865265, -0.5354309, -3.749023, -3.521484, -7.75};
```

```
[ED019] unsupported Flat semantic owner `function shape proof`: `a`: axis 1 is
variable-size but has no call-site equality
```

The function shape proof only knew a `:` extent from a call-site actual, which
a protected local does not have. MLS §12.4.4 makes the declaration equation the
local's value on entry, so its extents are the declaration equation's.

Affected (cluster G): IBPSA and Buildings `ReciprocatingCompressor`, IDEAS
`ScrollCompressor` (next error after TOOLBUG-130) — 3 models.

## Fix

`rumoca-phase-dae/src/construction/function_shapes/mod.rs::local_entry_shape`:
for a local with a `:` axis that the body never assigns, the shape of its
call-free declaration equation stands in for the call-site shape (rank and
fixed extents are still checked).

## Test

`crates/rumoca-contracts/tests/func_contracts.rs::flexible_local_array_sized_by_declaration_equation`
(simulates).
