# TOOLBUG-039 — bitcode import refuses `div`/`mod`/`rem` with a varying operand

**Status:** fixed (this change).
**Severity:** medium — every model with a time-varying quotient.

## What

```
IDEAS.BoundaryConditions.Climate.Time.Elements.SimulationTime
  y = rem(time, 31536000)

cannot rebuild a checked DAE from this bitcode: discontinuous builtin `rem`
requires statically computable operands until it has a checked event owner
```

MLS §3.7.2 makes such a quotient discontinuous, so the DAE admits it only
with an owner. In an equation the compiler builds the owner as one batch:
the quotient, six generated nodes of the indicator `sin(pi*lhs/rhs) >= 0`
(stamped `RuntimeDiscontinuity`), a relation, an `Always` activation and a
root. In a function body the owner is the function.

The artifact carries all of that as ordinary nodes, relations, conditions and
roots, and import rebuilt them one by one. The bare quotient is exactly what
the DAE refuses.

## Fix

- A quotient node followed by `RuntimeDiscontinuity` nodes is re-issued with
  `begin_quotient_replay`, which regenerates the seven-node batch; the
  owner's relation, activation and root are then routed through the token
  (`replay_quotient_relation`, `_activation`, `_root`, `finish_`).
- Inside a function body a quotient the DAE refuses as a bare builtin is
  re-issued with `function_runtime_quotient`.

Both use the DAE's public replay protocol, so every check a compiler-built
owner passes applies to an imported one.
