# TOOLBUG-097 — `homotopy(actual=…, simplified=0)` rejected

**Status:** fixed (this change).
**Severity:** medium — `ED020 … expression shape mismatch` for the IBPSA/IDEAS
mover `PowerInterface` and every model that writes an Integer literal as
the simplified expression.

## What

MLS §3.7.4.4 takes two Real scalar expressions; an Integer argument is
promoted (MLS §10.6.13). The DAE type rule required the two operand types to
be identical, so `simplified = 0` (Integer) failed against a Real `actual`.

## Fix

`rumoca-ir-dae/src/expression/type_rules.rs` (`homotopy_result`): both
operands must be scalar; their common numeric type must be Real.

## Test

`structural_evaluation_regressions.rs`: `homotopy_promotes_integer_simplified`.
