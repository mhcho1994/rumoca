# TOOLBUG-056 — `assert(..., AssertionLevel.warning)` refused

**Status:** fixed (this change).
**Severity:** medium — refused models the compiler accepts ("assertion levels
do not yet have checked Solve lowering"); found in IBPSA's CDL blocks
(`Line`, `BooleanExtractor`, `IntegerExtractor`, their validations).

## What

Any `assert` that passed a `level` argument was refused by the Solve lowering,
including `AssertionLevel.error`, which is the default.

## Fix

- A new Solve event-action kind, `Warning`: a violation is reported through
  `tracing` (target `rumoca_eval_solve::assert`) and the simulation continues,
  as MLS §8.3.7 specifies.
- The level must be a translation-time value, since it decides whether a
  violation stops the run. The predefined type is
  `enumeration(warning, error)`, so ordinal 1 lowers to `Warning` and 2 to an
  ordinary `Assert`; anything else is refused with its own message.
- Consumers that only support plain assertions (FMI static assertions, event
  transactions) reject the new kind as they reject `Terminate`.

`BooleanExtractor` and `IntegerExtractor` run with an out-of-range index held
at its start value, reporting the warning instead of stopping.
