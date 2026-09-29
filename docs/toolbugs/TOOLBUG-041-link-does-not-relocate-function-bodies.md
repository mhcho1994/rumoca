# TOOLBUG-041 — `bitcode link` does not relocate carried function bodies

**Status:** fixed (this change).
**Severity:** high for linking — silent corruption that validation accepts.

## What

`link` relocates every table of each input into the combined id space. Its
`RbcFunction` relocation moved the function id, result types and declaration
span, and nothing else. Since function bodies are carried, a function also
names arena expressions (statements, fold definitions, external arguments),
types (value table), domains (folds), sources (every body span) and other
functions (`calls`). None moved.

The second instance's body therefore pointed at the *first* instance's
expressions. Every index was in range, so `validate` accepted it; import
rejected it only while rebuilding:

```
rumoca bitcode link -o L.rbc a=F.rbc b=F.rbc     # succeeds
rumoca compile-bitcode L.rbc --summary
  cannot rebuild a checked DAE from this bitcode: parameter from function 0
  cannot be used in function 2
```

## Fix

`Shift` for `RbcFunction` relocates `calls`, the value table, folds, the body
(statements recursively, conditional correlations, external arguments) and
parameter declarations. Owner-local ordinals (values, folds, definitions)
are addresses within the function and are deliberately left alone.

Pinned by `bitcode_function_import::linking_two_artifacts_relocates_their_function_bodies`.
