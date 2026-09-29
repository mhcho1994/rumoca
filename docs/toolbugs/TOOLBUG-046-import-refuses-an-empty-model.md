# TOOLBUG-046 — import refuses an empty model

**Status:** fixed (this change).
**Severity:** low.

`model Empty end Empty;` exports fine and failed to import:

```
cannot rebuild a checked DAE from this bitcode: bitcode has no object to
anchor type provenance
```

Import anchors the provenance of rebuilt value types at the first variable or
expression. An empty model has neither, and nothing that could name a type
either, so there are no types to rebuild. Import now also anchors at a
function declaration, and with no anchor at all rebuilds no types.
