# TOOLBUG-040 — `rumoca_bitcode::build::operands` skips subscripts and call heads

**Status:** fixed (this change).
**Severity:** medium — silent: wrong read sets, missed override dependencies.

## What

`build::operands` is documented as "every expression a node references" and
is the walk behind three consumers:

- `Builder::expr`'s topological-order assertion,
- `Builder`'s equation read sets (`reads_of`),
- `bitcode run --param`'s check of which bindings an override affects
  (`bitcode_execution/overrides.rs::depends`).

It returned only `base` for `Index` and `base, value` for `ArrayUpdate`,
never the subscript expressions. So in `x[i]`, `i` was invisible: an equation
built with the builder did not list `i` among its reads, and overriding a
parameter used only as an index was not seen as affecting anything that
indexes with it.

A call *projection* (a further result of a multi-output call) carries no
arguments; it depends on the call's head. That edge was also missing, so a
projection's reads were empty.

## Fix

- `operands` includes subscript expressions.
- New `build::references(id, node)` adds the head of a call projection (the
  node's own id is needed to tell a head, which names itself as owner, from
  a projection). The builder's order check, its read sets and the override
  walk use it.
