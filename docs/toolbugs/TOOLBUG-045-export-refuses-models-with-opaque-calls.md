# TOOLBUG-045 — export refuses a model whose residual calls an external body

**Status:** fixed (this change).
**Severity:** medium — models the compiler accepts could not be exported.

## What

Export computes each equation's read sets with `rumoca_eval_dae`'s scalar
coordinate projection. That projection resolves a call by following the
callee's result definition, and an external body (MLS §12.9) has none, so it
returns `ProjectionError::ExternalFunction`; a record field computed by an
operation it does not project returns `UnsupportedRecordOperation`. Export
turned either into a hard error:

```
export: dependency projection failed: external C function `extcomp` calls
`ext_sum`, which this runtime cannot execute
```

## Fix

For exactly those two errors, the read sets fall back to a syntactic walk of
the residual (`export/syntactic_reads.rs`): every variable coordinate the
expression reaches. For an opaque call that is the honest incidence -- the
result depends on whatever its arguments read -- and it is a superset of the
scalar incidence, never a subset. Every other projection error still refuses.
