# TOOLBUG-044 — bitcode v2 refused five DAE owner tables and three coordinates

**Status:** fixed (this change).
**Severity:** high for interchange — any model using `when`-algorithm
transactions, `previous`, `delay`, `terminal()`, structured roots or
`interval(c)` could not be exported at all.

## What

`export/profile.rs` refused a model outright when any of these DAE owner
tables was non-empty:

| Table | Source construct |
|---|---|
| `model_event_transactions` | discrete variables defined together by an event-guarded algorithm or `when` clause (MLS §8.3.5, §11.1.2) |
| `previous_values` | `previous(v)` on a clocked variable (MLS §16.5) |
| `delays` | `delay(e, T)` and `delay(e, T, Tmax)` (MLS §3.7.4.1) |
| `terminals` | `terminal()` (MLS §8.3.6) |
| `structured_roots` | tensor-native root families over a domain |

and the coordinates that read them -- `Delay`, `Previous`, `Terminal`, and
`ClockInterval` (`interval(c)`) -- had no encoding.

Routing the test suite through `DAE -> RBC -> DAE` (`--pass none`) showed
about thirty test models refused for these reasons.

## Fix

Each table is carried in `RbcModel` (additive, defaulted), with id newtypes
(`PreviousId`, `TerminalId`, `DelayId`) the linker relocates, validation of
every reference, and import through the DAE's own constructors in the order
its wire replay uses: `previous`/`terminal()` owners before the arena, each
delay when the arena reaches the one coordinate that reads it, structured
roots after conditions, transactions last. `round-trip` compares all five.

With this, every DAE expression form and owner table has an encoding, and
export's `Unsupported` fallback for expressions is unreachable.
