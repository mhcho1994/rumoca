# TOOLBUG-037 — bitcode export snapshots the source table before every span is seen

**Status:** fixed (this change).
**Severity:** medium — produced artifacts that fail import on a valid model.

## What

`export_model` built `RbcModel::sources` from the export context's source
registry *before* exporting functions, discrete-real equations and initial
discrete values. Each of those registers the sources its spans name as it
is exported, so a span first seen there named a source index the artifact
did not contain.

The validator only checked variable spans against the source table, so the
artifact passed `bitcode check` and failed at import:

```
cannot rebuild a checked DAE from this bitcode: DAE provenance references
an unknown source: Span { source: SourceId(0), start: BytePos(16330), ... }
```

Found on AixLib's `FiniteLineSource_Erfint`, whose function lives in a file
no equation references. It predates carried function bodies: a function's
*declaration* span already had the same exposure; carrying bodies (values,
statements, folds, each with a span) only made it common.

## Fix

- Export every span-bearing table first; collect `sources` last.
- `validate` checks every function span (declaration, parameters, values,
  statements, folds) against the source table, so an artifact written by an
  older exporter is rejected at validation with a precise message instead of
  partway through import.
