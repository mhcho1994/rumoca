# ModelSan and current bitcode

ModelSan now distinguishes equation-only RBC from saved executable RBC. It
uses the shared `rumoca-bitcode` SDK; it does not own a second format reader or
numerical solver. These changes implement the consumer boundaries described in
[SPEC_0007](../spec/SPEC_0007_IR_PIPELINE.md),
[writable execution IR](architecture/writable-execution-ir.md), and
[native connector contracts](connector-validation.md) (MLS §9.2).

Use the SDK and ModelSan from this same checkout (for example,
`python3 -m pip install -e packages/rumoca-bitcode -e packages/modelsan`), or use
the `PYTHONPATH` in the tests below. An older installed SDK is not updated just
because the source tree changed.

## Runtime paths

| Input | Preparation | Execution |
|---|---|---|
| Modelica source | Compile to equations, then prepare that artifact | Existing equation path |
| Equation-only `.rbc` / RBC `.json` | Add variable trace points and native-check the result | `compile-bitcode --simulate --trace-out` |
| Saved executable `.rbc` / RBC `.json` | Check freshness/capabilities, append an execution observation pass, check again | `bitcode run --execution require` |

The executable path never changes the equation model and never implicitly re-lowers a
saved numerical program. Existing numerical edits, pass recipes, sinks and
lifecycle instructions are preserved. The new `modelsan.observe-variables`
pass appends ordered open/write/close effects for a private observation CSV.
All non-parameter variables must already have valid observation mappings in
the saved program; missing mappings are an explicit preparation error. They
are not guessed from solver registers or silently omitted.

Every run uses a distinct trace directory. Preparation works on a separate
artifact and leaves the input bytes unchanged. `close()` cleans the backend's
temporary artifacts and traces; copy wanted evidence before calling it.

```python
from pathlib import Path
from modelsan.backends.rumoca import RumocaBackend
from modelsan.fuzz.testcase import NOMINAL

backend = RumocaBackend(executable="./target/debug/rumoca", t_end=5)
try:
    failure = backend.prepare_from_artifact(Path("build/composition-demo/04-executable.rbc"))
    if failure is not None:
        raise RuntimeError(failure.failure)
    result = backend.run(NOMINAL)
    print(result.status, result.failure)
    if result.trace is not None:
        print(result.trace.final_state)
finally:
    backend.close()
```

`Pipeline.run(model, artifact_path, model.name)` also accepts RBC paths through
the backend's `prepare` entry point. The pipeline refreshes its capability
plan after preparation, when actual connector/observation coverage is known.
As before, explicitly register optional sanitizers such as `NetworkSan` if you
want them to run; this change does not silently enable all optional detectors.

## Passes, replay and limits

The observation pass is replayable:

```python
from modelsan.backends.rumoca_execution import PASS_NAME, instrument

# Supply implementations for every other recorded pass too.
fresh = program.relower(replay={PASS_NAME: instrument})
```

The default backend does **not** replay automatically. Explicitly constructing
`RumocaBackend(..., replay={pass_id: implementation, ...})` authorizes a
re-lower of a program whose referenced identities have changed. Every recorded
receipt needs an implementation. A stale program is still rejected. Raw program
edits are not recipes and cannot be recovered by replay: register them as named
passes first. `relower` takes no `observe` argument — observations are demanded
by the program, and `modelsan.observe-variables` registers the trace points it
references.
See [composition and pass management](combining-models.md).

Saved executable fuzz cases now use native `bitcode run --param name=value`
and `--initial name=value` flags. Overrides change only run-local storage; they
preserve saved instructions, effects and original artifact bytes. Parameters
must be tunable and retained in execution storage. Frozen dependent bindings,
start or nominal expressions cause a refusal, not stale simulation. Initial
overrides change unconstrained scalar state start seeds; states owned by explicit
initialization rows are refused. Structural parameters require recompilation.
Equation-only parameter fuzzing still uses `--param`. Equation-only initial
overrides, input trajectories and solver-option overrides remain explicitly
unsupported rather than silently ignored.

## Native diagnostics

ModelSan requests native interpreter diagnostics on both execution paths.
`bitcode run --domain-diagnostics` writes `domain-diagnostics.json` in the new
trace directory; `compile-bitcode --simulate --domain-diagnostics FILE` writes
the same evidence for equation input. These diagnostics survive ordinary solver
failure before the first physical sample. They use the existing evaluator and
solver, not Python reconstruction or a second solver.

| Evidence | Producer and interpretation |
|---|---|
| Zero divisors, sqrt/log/inverse-trig domain violations | Reached scalar SSA operations, with actual operand, internal time, row fingerprint and instruction index. Inactive Select branches are not evaluated. |
| Accepted step proposals | Native ME integrator host; may subsequently be truncated at an event. Not publication-grid spacing or rejected-step telemetry. |
| Events / discrete iteration counts | Native event-mode host, with actual time. Initialization iteration records do not invent triggered events. |
| Projection residuals and Jacobians | Initial, algebraic, reduced tearing and manifold projection owners; raw residuals/matrices already used by the solve. These are trial coordinates, not settled physical equations. No guessed rank or condition number. |

Runtime domain findings use backend instruction identities, **not invented DAE
expression IDs**. If the run completes after an internal violation, the finding
is informational. A failed run carries high-severity operation evidence, but
that alone is not a verdict that the source model is wrong. Newton trial
residuals do not become conservation failures. General canonical-expression,
settled-equation-residual and rank/conditioning capabilities remain unclaimed.

Evidence is bounded: 64 distinct domain faults and 4096 solver records;
projection payloads larger than 64 rows/columns omit values explicitly.
Unsupported tensor/call/assignment-prefix/non-SSA evaluations are counted.
`RunOutcome.coverage` reports truncation and omissions; no findings does not
mean complete coverage. Internal diagnostic records retain their native order
inside metadata; they are distinct from time-major physical variable samples.

Event regression testing also found and fixed a bitcode importer defect:
exported static/dynamic time schedules were dropped. They now rebuild through
the checked DAE event owner. A `when time >= 0.1` regression checks both the
event timestamp and the resulting state. Clocked conditions absent from the v1
import profile still fail explicitly as backend coverage errors.

The saved-program profile remains scalar-real, event-free native RK45. Stale
programs, unsupported operations, missing observations, sink/pass collisions,
unavailable tools and malformed observation files are backend coverage errors,
not findings that the physical model is broken. Native runtime assertion or
solver failures still reach runtime sanitizers. NaN/Inf **values** survive
transport for NumericSan; invalid publication coordinates are rejected.

`Program.validate` / `Program.save` accept optional `executable` and `timeout`
keywords so the backend's configured compiler is used consistently, without
temporarily changing process-wide `RUMOCA` settings.
`Model.has_execution` exposes container presence without consumers parsing its
raw fields; native checking, not that property, establishes validity.

## Connector identities and NetworkSan

When explicit connector declarations exist, network construction uses their
component-owner IDs and member-variable IDs. A name such as `a.nested.port`
does not imply that `a.nested` is its owner. Member variables need not share
the port's textual prefix. Linked modules are read using their relocated IDs.

`Network.identity_source` distinguishes `declared-ids` from `legacy-paths`.
Legacy annotation-only artifacts retain their existing inspection path;
malformed explicit declarations never fall back to that path. This projection
does not replace native connection-law validation or certify legacy ports for
new wiring.

Declared open ports remain visible with `port.node is None`. They are not
invented singleton connection sets and no zero-flow law is assumed. A finalized
singleton remains a real zero-flow boundary. `Port.connector_id`,
`Port.owner_id`, and `Node.owners` carry the declared identities/owner paths.
The SDK now also exposes `ConnectionSet.potential_equations` directly.

NetworkSan now implements the registry's `requests(model, context)` and
`observe(stream, model, context, testcase)` protocols. It consumes canonical
`VariableObservation` records on synchronized publication times, not a nonexistent
`stream.series()` API. Missing, duplicate-time, nonfinite or unsynchronized
member evidence produces an informational `network-observation-incomplete`
coverage note, not a clean conservation verdict or a physical defect.

Each flow balance is tested separately, including singleton zero-flow laws.
Equal-and-opposite violations of different fields cannot cancel each other.
Network findings retain the triggering test case.

## Reproductions and verification

Before the fix, the new regression tests showed:

1. A saved decay program deliberately edited to `der(x) = -3` failed in ModelSan
   with “compile-bitcode would discard executable edits.” It now runs unchanged:
   `x(0.2) = 1.4` from `x(0) = 2`, rather than the original exponential law.
2. Valid ports named `a.nested.port` / `b.nested.port`, owned by components
   `a` / `b` and using arbitrarily named member variables, produced wrong owners
   and lost members. They now retain both declared owners and all members.
3. An injected flow imbalance produced no NetworkSan finding because the stream
   accessor did not exist. It now produces `network-conservation-violated`.

The native regression file is
[test_current_bitcode.py](../packages/modelsan/tests/test_current_bitcode.py).
It also covers existing CSV-pass preservation, repeated runs, unchanged input
bytes, executable replay, selected-compiler isolation, stale/unsupported
program refusal, missing observations, source preparation lifetime, equation
parameter fuzzing, assertion delivery to AssertSan, linked connector observations
through the pipeline, legacy inspection, open/singleton ports, separate
multi-field violations and malformed/nonfinite trace handling.

```bash
PYTHONPATH=packages/rumoca-bitcode:packages/modelsan \
RUMOCA="$PWD/target/debug/rumoca" \
  python3 -m pytest packages/modelsan/tests -q

PYTHONPATH=packages/rumoca-bitcode:new_inst RUMOCA="$PWD/target/debug/rumoca" \
  python3 -m unittest discover -s new_inst -p 'test_*.py' -q
```

The stale generated schema reference was also regenerated with
`python3 tools/bitcode/gen_reference.py`. This fixes the two documentation
failures observed during the original compatibility check; the generator still
reports existing undocumented schema fields, which this consumer migration does
not fill in.

### Recorded results — 2026-09-23

Base commit: `b38af3a3`, plus this working-tree migration. Ordinary execution
remains unchanged except restoration of dropped imported time-event schedules.
Diagnostic mode explicitly selects the native interpreter and records reached
operations and numerical-owner telemetry.

| Check | Result |
|---|---|
| Focused current-bitcode, overrides, domain, solver telemetry, transport and architecture regressions | 54 passed |
| Native `new_inst/test_*.py` acceptance suite | 39 passed |
| Generated reference check and working diff whitespace | Passed |
| Full ModelSan suite, including all new regressions | 236 passed in 399.28 seconds; 0 failed, 0 skipped |
| Solve evaluator unit tests | 164 passed |
| Solver unit tests including telemetry | 422 passed |
| Simulation unit tests | 123 passed |
| Bitcode unit tests including static/dynamic time schedules | 54 passed |
| OMC reference unit tests | 38 passed |
| Explicit xtask reference-mode forwarding regression | 1 passed |

The early broad attempt exposed two stale-reference failures, now corrected.
New regression files also cover native domain faults, run-local overrides,
explicit replay, strict trace transport, solver diagnostics and time schedules.
The final full-suite report is `/tmp/modelsan-final-complete.xml`. Its single
warning is the existing pytest collection warning for the imported `TestCase`
dataclass, not a failed test. An earlier full run also passed 233 tests before
the last three regressions were added.

Socket-free, plot-free fixed 20-model Tier 1 canary command:

```bash
CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 RAYON_NUM_THREADS=4 CARGO_NET_OFFLINE=true \
  CC=gcc cargo xtask verify msl-parity --no-remote-quality-baseline \
  --sim-targets-file infra/verification/msl-canary-20.json \
  --omc-script-mode --no-plots
```

The initial run had 11 compiled, 9 ToDae failures, 8 completed simulations and
3 solve-IR failures. OMC's persistent socket transport was prohibited here;
the report also attempted unavailable npm downloads. Explicit `--script-mode`
on `rumoca-msl-tools omc-simulation-reference` now runs ordinary `.mos` files
with per-model timeout/process-group cleanup; `--no-plots` keeps comparison
JSON without fetching browser assets. The xtask flags above forward these modes.
`CC=gcc` avoids the installed clang configuration's missing standard headers.
No simulator failure is silently retried with a different numerical policy.

The first repaired OMC reference run successfully compared all 8 available
Rumoca traces: 8 strict-high agreement, 0 near/deviation, 0 bad channels,
100% initial-value match and exact state sets. The other 12 fixed targets
remain visible as unmeasured. This is not a 20/20 simulation pass, and not the
566-model cohort. Artifacts live in `target/msl/results/`.

The full xtask command above was then rerun on the updated compiler and exited
0: 11 compiled, 9 ToDae failures, 8 completed simulations, 3 solve failures;
8 high-agreement comparisons across 129 channels, no bad/severe channels,
100% initial-condition and exact-state-set agreement. No target was dropped
from the 20-model denominator. This canary gates regressions; exit 0 does not
mean every target is supported.

Strict clippy passes for the evaluator/solver crates. Broader clippy remains
blocked by existing lint violations in bitcode parsing/disassembly and compiler
phases (including `bitcode_disasm.rs`, `text.rs`, `attributes.rs`, and
`component_instance.rs`); no lint suppressions were added. No clean
whole-workspace lint result is claimed.

No baseline was promoted. A full 566-model sweep and a clean whole-workspace
gate are not claimed; these consumer tests establish the specific behaviors
listed above, not universal sanitizer correctness.
