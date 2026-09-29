# TOOLBUG-052 — static evaluation refused every input, and never said which

**Status:** fixed (this change).
**Severity:** medium — refused models the compiler accepts; found in
TRANSFORM's geometry models.

## What

```modelica
model InputStart
  input Real u = 2;
  Real x(start = 3 * u, fixed = true);
  ...
```

```
numeric evaluation depends on a runtime coordinate
```

The DAE evaluator resolves an input's own value from its binding or the
host-supplied value (`input_value`), but a *coordinate* naming an input inside
another expression -- a `start` or `nominal` written over an input -- fell into
the branch that refuses every non-parameter coordinate. The message did not
say which coordinate, so the failure could not be diagnosed from its output.

## Fix

- An input coordinate is evaluated through `input_value`: its binding, or the
  host value (including one `--free-inputs start` holds). An input with
  neither is still refused, with the existing message naming it.
- A cycle guard, as for parameters: TRANSFORM's `GenericPipe` gives its
  inputs circular defaults (`dimensions` from `crossAreas`, `crossAreas` from
  `dimensions`), meant to be broken by whoever instantiates it, and now
  reports `cyclic static-value dependency includes 'crossAreas'` instead of
  overflowing the stack.
- The refusal names the coordinate: `depends on 'algebraic crossAreaNew_shell'`,
  `depends on 'time'`, and so on.

`InputStart` initializes to `x(0) = 6`; TRANSFORM's `GenericAnnulus`,
`GenericDuct` and `StandardPipe` now run.
