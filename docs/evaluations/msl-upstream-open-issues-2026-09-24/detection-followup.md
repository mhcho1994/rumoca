# ModelSan detection follow-up — 2026-09-24

Historical results below are superseded for the six focused cases by the
[October 1 regression follow-up](review-fixes-2026-10-01.md), which passes all
six faults and five controls in both native execution profiles.

ModelSan now detects **five of the six reproduced issue patterns with native
Rumoca**, and **all six across explicitly selected Rumoca and OpenModelica
backends**. These are actual sanitizer findings from the original MSL APIs,
using declared behavioral expectations and captured execution evidence.

This implementation follow-up supersedes the initial evaluation's zero credited
detections for these six patterns. The original census and initial results remain
historical records. This is not a recall score over the 352 independent issue
entries, automatic discovery of arbitrary semantic bugs, or a compiler parity
claim.

## Observed detections

Both campaigns used the cached MSL 4.1.0 source tree, a 1 s stop time, 400 output
intervals, and a 90 s timeout per subprocess. The native source profile used
`rk-like`. The [native report](detection-native.json) and
[OpenModelica report](detection-openmodelica.json) retain all cases, observations,
findings, execution statuses, raw failure evidence and source/executable hashes.

| Upstream issue | Contract or failure witness | Native Rumoca | OpenModelica |
|---|---|---|---|
| [#4749](https://github.com/modelica/ModelicaStandardLibrary/issues/4749) | Gas entropy/state round trip: requested 300 K, recovered 5204.0536 K | Contract violation | Contract violation |
| [#4750](https://github.com/modelica/ModelicaStandardLibrary/issues/4750) | Water entropy/state round trip: requested 300 K, recovered 327.4574 K | Contract violation | Contract violation |
| [#3624](https://github.com/modelica/ModelicaStandardLibrary/issues/3624) | BooleanPulse with a past start: output 0 where the phase requires 1 | Contract violation | Contract violation |
| [#4451](https://github.com/modelica/ModelicaStandardLibrary/issues/4451) | UnitDelay at 0.0525 s: output 0.309017 instead of the previous sample, 0 | Contract violation | Contract violation |
| [#4459](https://github.com/modelica/ModelicaStandardLibrary/issues/4459) | Two-bit quantizer requires five distinct levels, exceeding four | Contract violation | Trace transport blocked |
| [#4771](https://github.com/modelica/ModelicaStandardLibrary/issues/4771) | Reduced MoistAir composition attempts array index 2 in a dimension of length 1 | Compiler blocked | Initialization assertion failure |

The first five findings come from `BehaviorSan`. The MoistAir finding comes from
`SolverSan`; the regression checks the specific array-bounds assertion text,
rather than accepting any failed execution as reproduction of #4771.

Four native controls complete without findings: gas at reference pressure,
pulse with a non-triggering start, an explicitly implemented previous-sample
delay, and the quantizer with a smaller input amplitude. OpenModelica also
completes the full-composition MoistAir control. Its quantizer control has the
same trace-transport blocker as the triggering case. These controls test detector
discrimination; the non-triggering configurations do not repair the MSL library.

## What changed

**Reusable semantic checks.** `BehaviorSan` checks signal equality, periodic
pulse phase, a one-sample delay of a continuous input, and an upper bound on
output levels. Callers declare exact signal bindings, tolerances, and the origin
of each expectation. The sanitizer has no issue-number, model-name or MSL-name
special cases. Finite wrong values therefore become reportable defects once a
meaningful property has been supplied.

Checks require completed executions and usable observations. Missing signals,
ambiguous identities, nonfinite values, partial traces and unsuitable event
ordering produce coverage gaps. Pulse and delay checks skip event boundaries
where timestamps alone cannot establish event side. Level counting uses the
minimum number of levels compatible with the declared uncertainty intervals,
so small numeric jitter does not invent extra levels. Distinct contracts remain
distinct findings even when they bind the same signal.

**A runnable campaign and regression gate.** The new CLI loads the
[campaign](../../../examples/modelsan/known-msl-issues.json), executes the
[MSL fixtures](../../../examples/modelsan/KnownMSLIssues.mo), and reports typed
findings, coverage and provenance. Expectations live outside the MSL code.
The opt-in integration gate checks specific findings and control results.
See the [package instructions](../../../packages/modelsan/README.md) for commands.

**Native access to the entropy defects.** Flatten now specializes inherited
function package constants using resolved declaration identities and effective
package modifiers. This repairs the concrete `cp_const` blocker without weakening
DAE validation. The [compiler note](compiler-specialization.md) documents the
reduced cause, regression tests and before/after canary.

**Explicit, stricter execution profiles.** `RumocaSourceBackend` executes the
existing native source simulator and records observations with backend identity.
It is selected explicitly and does not silently replace editable-bitcode
execution. Unsupported overrides and canonical instrumentation are rejected.
The OpenModelica backend now honors caller timeouts, bounds process groups,
checks library loading, avoids stale artifacts and validates CSV transport.
Externally interrupted processes are inconclusive, not credited model defects.

## Remaining coverage gaps

Native MoistAir still stops before execution; both triggering and control
cases are blocked and no native detection is claimed. The blocker has moved as
the frontend advanced: it was EF015, missing resolved class metadata for a
`ThermodynamicState` record constructor, and is now ED019, an unsupported
function shape proof -- `X`: axis 1 requires extent 2 where the call site
proves 1, at `Modelica/Media/package.mo:5133`. The reduced-composition case is
exactly the shape disagreement upstream #4771 reports, so the compiler is
meeting the defect and declining to compile rather than reporting it.

OpenModelica's quantizer CSV contains decreasing timestamps, including tiny
floating-point reversals around clock/output events. The transport rejects it
instead of sorting observations or guessing event order. Neither OpenModelica
quantizer case earns detection or control credit. Native traces establish the
quantizer result independently.

Saved-bitcode clock transport and string conversion remain separate limitations.
The source execution profile does not claim those features are repaired. The next
coverage work is native record metadata, explicit event-order transport, and
additional contracts and executable fixtures for the wider issue inventory.

## Validation scope

The full ModelSan Python suite passes **326 of 327 tests**, with the two opt-in
actual-library tests skipped in that command. Those tests pass separately with
both backends, including a subsequent expanded nine-case native gate. The compiler
repair passes 682 Flatten tests, including six new general regression cases.
The fixed 20-model SPEC_0033 canary has identical before/after per-model statuses:
eight simulations complete with high-agreement comparisons, while twelve retained
compiler/solve/runtime blockers remain visible. No full 566-model parity result
is claimed.

Full Rust Clippy is blocked by existing complexity errors in unchanged Flatten,
DAE and Instantiate functions; see the compiler note for exact locations.
The [validation record](detection-validation.json) records commands, test outcomes,
case counts and evidence hashes. The broader ModelSan Python suite is recorded
there separately from the opt-in actual-library gate.

No upstream library files were changed and no changes were published to GitHub.
