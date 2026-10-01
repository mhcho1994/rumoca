# Frontend coverage on third-party libraries — 2026-10-01

**Status: in progress.** Two rounds of fixes are merged and pushed
(`rumoca-bitcode-v1` at `4b9e9fb7`). Most models OpenModelica accepts still
do not compile in Rumoca; the remaining blockers are listed at the end.

## What was measured

The evaluation set is eleven Modelica libraries (AixLib, Buildings, IBPSA,
IDEAS, TRANSFORM, OpenIPSL, ThermoPower, ThermoSysPro, PowerGrids,
VehicleInterfaces, Modelica_DeviceDrivers): 13,028 compile targets, each
compiled against its library and the MSL its `uses` annotation asks for.

The v2 run (before this work) failed **7,892 models that OpenModelica's
`checkModel` accepts**. Every one of those is a Rumoca failure, so they are
the frontend backlog. Re-running all 7,892 takes about ten hours, so progress
is tracked on a **stratified sample of 595**: up to three models for every
(library, first error code) pair. The sample over-weights rare errors, so its
rate is not the dataset rate; it is the same set every time, so the trend is
exact.

Harness: `/data/mrumoca/eval/fe/` (`run.sh` for the sample, `repro.sh` for one
model, `clusters/` and `clusters2/` for the per-round case lists).

## Results

| | Start | After round 1 | After round 2 |
|---|---:|---:|---:|
| Compile | 14 (2.4%) | 96 (16.1%) | **131 (22.0%)** |
| Fail with a diagnostic | 566 | 496 | 455 |
| Compiler crash (panic, stack overflow) | 12 | 0 | 0 |
| Timeout (>300 s) | 3 | 3 | 3 |

The round-2 sample run also recorded six Buildings timeouts; all six were
host disk stalls. Re-run, they finish in 16–73 s with ordinary diagnostics.
The three real timeouts are the OpenIPSL Nordic44 models.

| Library | Sample | Start | Round 2 |
|---|---:|---:|---:|
| PowerGrids | 18 | 0 | 8 (44%) |
| OpenIPSL | 39 | 2 | 13 (33%) |
| ThermoSysPro | 21 | 3 | 6 (29%) |
| TRANSFORM | 67 | 3 | 17 (25%) |
| Buildings | 89 | 2 | 22 (25%) |
| IBPSA | 78 | 3 | 19 (24%) |
| AixLib | 91 | 0 | 22 (24%) |
| VehicleInterfaces | 17 | 0 | 4 (24%) |
| IDEAS | 94 | 1 | 14 (15%) |
| ThermoPower | 54 | 0 | 5 (9%) |
| Modelica_DeviceDrivers | 27 | 0 | 1 (4%) |

Every merge passed the full workspace suite (7,509 tests at the end; the only
failures are the four known environment ones: galec C toolchain, two
OpenModelica Jacobian batteries, CUDA), workspace clippy with `-D warnings`,
and the thermal golden CSVs, which stayed byte-identical throughout.

## How the work was done

Failures were grouped by first error code into clusters of shared cause, and
each cluster was given to one agent in its own git worktree with a common
brief (`/data/mrumoca/eval/fe/AGENT_BRIEF.md`): reduce each failure to a
minimal model, fix the cause, add a regression test (most simulate and check
values), write a `docs/toolbugs/` record, and match OpenModelica where the
specification is stricter than tools in practice. Each branch was merged,
checked (clippy, contracts, architecture gates, full suite, goldens) and only
then pushed.

- **Round 1 (seven clusters):** replaceable packages and redeclarations,
  over-strict checks, structural evaluation, functions and name lookup,
  events and algorithms, connections and balance, crashes.
- **Round 2 (five clusters, built on round 1's new first errors):** media
  package selection, function bodies, array dimensions, events, balance
  counting.

## What was fixed

About 95 root causes, each with a test and a record in `docs/toolbugs/`
(TOOLBUG-067, 068, 070–078, 080–098, 100–108, 110–123, 130–134, 140–157,
165–173, 180–183, 195–196). Grouped by area:

**Loading and parsing.** A UTF-8 byte-order mark is no longer a parse error
(033). A stray directory without `package.mo` no longer fails its library
(035). An `outer` element with a modification is a warning, not an
unreadable file (036). Libraries used only by a loaded library are now loaded
(105) — the cause of about 611 ThermoSysPro failures in the full run.

**Replaceable packages and redeclarations** (the `Medium` machinery of every
fluid model). Redeclarations are checked against `constrainedby` (070);
outer-extends and repeated redeclarations, constraining-clause modifiers and
forwarded `redeclare package Medium = Medium` work (072, 075, 076, 146);
members, constants and functions are looked up in the selected package rather
than the partial default (073, 102, 108, 130, 140, 143, 144, 148); inherited
package constants take each extending package's value (148).

**Checks that were stricter than OpenModelica**, now warnings (WR006–WR013)
with the model compiled correctly: `Evaluate` on non-parameters, `each` on a
scalar, `parameter input`, external objects in records, undeclared purity
calling impure, empty icon bases, equivalent enumeration types (083, 085, 088,
089, 142, 149). Several were real resolution bugs behind a check (080, 081,
084, 086, 087).

**Structural evaluation and dimensions.** `size()` of declared, bound and
comprehension arrays; package constants, Real modifiers, reductions and
if-expressions in conditions and dimensions; `:` dimensions inferred through
slices, element-wise builtins and record function outputs (090–096, 122,
165–173).

**Functions.** Calls inside array constructors, short function aliases,
predefined assertion levels, `array(...)`, element-wise operators in
bindings, package constants inside bodies, flexible local arrays
(100–107, 132, 145, 150–157).

**Events and algorithms.** Relational `when` statements, multi-output and
looping algorithms, initial algorithms with loops and partial assignments,
if-guards inside when-branches, `terminate`/`reinit` in algorithm `when`,
`delay(u, 0)` (067, 110–119, 180–182). A silent wrong-result bug:
`floor`/`ceil`/`integer` of a time-varying value generated no events, so
e.g. `integer(floor(time/0.3))` stayed 0 (183).

**Connections and balance.** Expandable-bus source counting and read-only
members, discrete inputs, parameters in expandable connectors (120–123);
`Complex` (operator-record) arithmetic lowered to the operator functions, so
record equations are counted per field (195); matrix-literal equations
counted per element (196).

**Crashes.** Five panics and a stack overflow, each now a correct compile or
a proper diagnostic (098, 130, 131, 133, 134, 180).

## Behaviour changes to review

- **TOOLBUG-168:** a whole-number parameter whose binding evaluates at
  translation time is lowered as a literal, even under
  `--no-fold-parameter-bindings` (the DAE needs literal array extents).
  Narrowing it to parameters used as extents would keep that flag's meaning.
- **TOOLBUG-183:** `floor`, `ceil` and `integer` of continuous arguments now
  generate events; event timing changes for models that use them that way.
- **New warnings** WR006–WR013 replace errors listed above.
- **ED001 policy:** a model OpenModelica also counts as unbalanced stays an
  error (22 such models in the balance cluster). Only Rumoca's own
  mis-counts were fixed.
- **Contract changes:** several contract tests that pinned an over-strict
  rejection now expect acceptance with a warning (ANN-008, INST-012, INST-040,
  PKG-006, a CONN expandable case); each change is in the merge that made it.

## What still blocks the remaining 455 sample models

First error after round 2:

| Code | Models | What it mostly is |
|---|---:|---|
| ED019 | 105 | function bodies and values the DAE cannot represent: external objects (MSL tables, device handles), `while` loops, loops inside runtime `if` branches, inner loop ranges that depend on the outer index |
| EF004 | 65 | `inStream` on indexed ports, `Connections.branch` without a root, side-effect calls (`print`) in when-equations |
| ED008 | 47 | media functions and constants still evaluated in the partial package (`Medium.h_default`, `setState_pTX`) and other unresolved Flat references |
| ED001 | 26 | mostly genuinely unbalanced (OpenModelica agrees); TOOLBUG-197 and two VehicleInterfaces chassis models are Rumoca's |
| ED013 | 21 | algorithms that read a variable after writing it in the same section |
| ED018 | 18 | `sample(t0, T)` with `t0` known only after initialization (CDL) |
| others | 173 | long tail (ED020, EI006, EF025, EF015, ER002, …) |

Open, analysed but not fixed: TOOLBUG-184 (runtime sample start), 185
(Integer/Boolean initial equations need discrete values evaluated during
initialization — a solver change), 197 (bus-to-bus-only expandable members),
and the inline `sample(u, Clock(...))` half of 067. Also noted, not fixed: a
binding resolved in the wrong scope for `replaceable model Load =
Resistor(R = R)`; `per = per` inside a redeclare losing to nested defaults;
the Nordic44 compile time.

## Next

The remaining blockers are features rather than isolated bugs, so the next
round is design work: an external-object value in the DAE with
constructor/destructor lifecycle, `while` and nested-range loops in function
bodies, a sample start read after initialization, and discrete-valued
initialization. Those cover roughly 150 of the 455 sample failures.
After that, re-run the full 7,892-model backlog for a dataset-level number.
The full list, with approaches, is in [../future-work.md](../future-work.md).

## Also delivered in this session

- **Modular API completed:** typed SDK views for every bitcode section;
  external passes inside the compiler (`--pass exec:COMMAND`); pass pipelines
  with groups, `fixpoint(...)` and skip-when-unchanged scheduling; the
  bitcode version contract (`docs/design/modular-api.md`).
- **Sanitizers from the command line:** `modelsan check Model.mo --model M
  -fsanitize=...` (`docs/using-the-sanitizers.md`).
- **Lint gate green:** the branch had accumulated 91 denied clippy lints;
  all fixed by refactoring (TOOLBUG-068).
