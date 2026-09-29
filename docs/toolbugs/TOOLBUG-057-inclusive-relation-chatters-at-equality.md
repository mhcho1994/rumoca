# TOOLBUG-057 — `a >= b` exactly at equality never settles its event iteration

**Status:** fixed (this change).
**Severity:** high — `y = 0 >= time` failed at t = 0 with "event update
iteration did not converge"; found in IBPSA's CDL Integer comparisons
(`GreaterEqual`, `LessEqualThreshold`, ...) and OpenIPSL `LimitCheck`, whose
held inputs make both sides equal.

## What

A relation's root is `a - b` for `a < b`/`a <= b` (and `b - a` for `>`/`>=`),
and the runtime refreshes the relation memory as `root < 0`. That is exact for
the strict relations but, at equality, says false for `<=`/`>=`, where the
relation is true. At an event the discrete rows evaluate the relation
literally (true) while the memory refreshed from the root says false, so each
pass of the event iteration undid the other.

```modelica
model Inclusive
  Boolean y;
equation
  y = 0 >= time;   // event update iteration did not converge at t=0
end Inclusive;
```

## Fix

The root of an inclusive relation is `select(root == 0, -MIN_POSITIVE, root)`:
negative exactly when the relation holds, so the memory and the literal
evaluation agree. Crossing detection is unchanged (a sign change or a value
within tolerance still triggers). `Inclusive` gives `y = true` at t = 0 and
false after it; the five IBPSA Integer comparisons and `LimitCheck` run.
