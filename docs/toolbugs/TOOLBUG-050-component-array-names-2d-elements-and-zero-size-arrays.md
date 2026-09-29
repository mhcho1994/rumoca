# TOOLBUG-050 — nested array names, 2-D elements and zero-size arrays broke runtime runs

**Status:** fixed (this change).
**Severity:** medium — each refused a whole model at runtime; found in
OpenIPSL and ThermoPower once TOOLBUG-049 let array models run at all.

Three defects in the same family as TOOLBUG-049:

1. **A variable inside an array of components** is named
   `rampTrackingFilter.TF1[1].x`, and its elements `...TF1[1].x[2]`. The
   helpers that map an element column back to its variable cut at the
   *first* `[` and produced `rampTrackingFilter.TF1`, so the backend reported
   the variable missing (`OpenIPSL...PSS2A`, `PSS2B`). They now strip only the
   trailing subscript (modelsan `variable_of`, RangeSan, `--check`).

2. **A two-dimensional array element** is named `Y[1,2]`. The trace CSV
   writer did not quote fields, so the comma split the row and the backend
   refused the trace as malformed (`OpenIPSL...Line_1Ph`, `Line_3Ph`). Fields
   are quoted per RFC 4180 now.

3. **A zero-size array with a scalar attribute** -- `Xi_outflow[nXi]`, `nXi =
   0`, with a scalar `nominal` -- was refused: `nominal for ... must contain 0
   finite positive values`. A single value was spread over the elements only
   when there were more than one; it now spreads over any count including
   zero, in both Solve lowering and the DAE evaluator. The backend also no
   longer asks to observe a zero-size variable, which has no element to
   publish (`ThermoPower...TestThroughMassFlow`).

All five models named run after the fix.
