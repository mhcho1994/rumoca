# TOOLBUG-066 — the exported call graph missed calls with literal arguments

**Status:** fixed (this change).
**Severity:** medium — `prune-functions` removed a function still in use
("functions 0 was removed but is still referenced"), so `--pass default`
refused the model.

## What

```modelica
function choose
  output Real selected;
protected
  Candidate candidates[2];
algorithm
  candidates[1] := candidate(2.0);
  ...
```

Export built each function's `calls` from the function scope of every call
node. A call whose arguments are all literals (`candidate(2.0)`) depends on
nothing in the body, so the checked DAE gives it no function scope, and the
sweep attributed it to no function. `choose` listed no callees, the model's
own call sites reach only `choose`, and `prune-functions` removed
`candidate`.

## Fix

The call graph also walks each function body from its definitions and loop
values (`ExpressionTraversal`), recording every call reached. `choose` now
lists `candidate`, and `FunctionRecordArray` compiles under `--pass default`.
