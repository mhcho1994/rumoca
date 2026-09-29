# TOOLBUG-038 — bitcode import refuses a model whose array sizes are parameters

**Status:** fixed (this change).
**Severity:** medium — any `fill(u, n)` with a parameter `n` was unimportable.

## What

```
AixLib.Controls.OBC.CDL.Routing.IntegerScalarReplicator
  y = fill(u, nout)          // parameter Integer nout = 1

cannot rebuild a checked DAE from this bitcode: array extent must be a
nonnegative literal Integer
```

The DAE resolves a structural extent through the parameter's *binding*
(`Storage::static_integer` follows `Coordinate::Parameter` to its bound
expression). Import reserved every variable up front but defined their
attributes only after rebuilding the whole expression arena, so when
`fill(u, nout)` was built `nout` had no binding yet and the extent was not
static.

The compiler never hits this because it completes a parameter before
building expressions that depend on it. Import reordered that.

## Fix

Parameters and constants are defined during the arena pass, as soon as every
attribute expression they name (binding, start, min, max, nominal) exists.
Everything else is still defined after the arena, where the discrete-value
topology it may need is open.
