# Future work

What is known to be missing, in the order it should be done. Each item
says what fails today, how many evaluation models it blocks (from the
595-model frontend sample, see
[evaluations/frontend-coverage-2026-10-01.md](evaluations/frontend-coverage-2026-10-01.md)),
and the intended approach. Items marked **design** need an IR or runtime
change, not just a bug fix.

## 1. Frontend coverage (round 3 and later)

The sample stands at 131 / 595 compiling (22%). The remaining failures are
dominated by a few missing features.

### 1.1 External objects as values — design (part of ED019, ~11+ models)
MSL `CombiTimeTable`/`CombiTable1Ds` and Modelica_DeviceDrivers handles pass
an `ExternalObject` between external functions. The DAE has no value type
for it, so construction stops with ED019 "function value type ... has
unsupported type". The backends already have table runtime support
(`ExternalTableData`, `table_runtime.rs`); nothing in DAE construction
produces it.
*Approach:* an opaque handle type in the DAE and Solve IR, a constructor call
at initialization and a destructor at termination (MLS §12.9.7), and
codegen/runtime that keep the handle alive between calls.

### 1.2 Loops in function bodies — design (most of ED019, ~90 models)
The DAE function IR cannot represent `while` loops (MSL 3.2.3 `TimeTable`),
loops inside runtime `if` branches (`besselJ0` and similar), inner loop
ranges that depend on the outer loop index (IDEAS/AixLib antifreeze
`polynomialProperty`), asserts inside loops, and warning-level asserts in
functions.
*Approach:* extend the function-body IR with a general loop and a
runtime-bounded range, with lowering in every backend; keep the existing
unrolled forms as the fast path.

### 1.3 `sample(t0, T)` with a start known only after initialization — design (ED018, 18 models, TOOLBUG-184)
All CDL pulse/sampler blocks compute `t0` as a `fixed = false` parameter.
Rumoca resolves every sample schedule at translation time.
*Approach:* a sample whose start is a parameter read after initialization
(touches core, DAE, bitcode, Solve IR and the solvers), or a rewrite of
`sample` into generated discrete variables. Guessing `t0` is not acceptable.

### 1.4 Discrete-valued initial equations — design (TOOLBUG-185)
CDL `Latch`, `TrueFalseHold` and MSL StateGraph have Integer/Boolean initial
equations. The initialization system takes only numeric residuals, and
discrete values defined by ordinary equations are evaluated only after
initialization, so accepting them today would read start values silently.
*Approach:* evaluate discrete definitions inside initialization; this
changes every model's initialization and needs the full simulation suite.

### 1.5 Connection-stream and graph lowering (EF004, 65 models)
`inStream` on indexed or vectorized ports (e.g. `PlugFlowPipeDiscretized`),
`Connections.branch` in a component with no root, and side-effect calls
(`print`, `writeLine`) inside when-equations.

### 1.6 Media still evaluated in the partial package (ED008, ~47 models)
`Medium.h_default`, `PartialMedium.setState_pTX`, `cp_const` and
`kappa_const` are evaluated in `PartialMedium`'s scope instead of the
selected medium's; where the medium really is partial at top level, the
error should say so instead of ED008.

### 1.7 Smaller open items
- Algorithms that read a variable after writing it in the same section
  (ED013 "SSA event transition", ~21 models).
- Expandable-connector members connected only bus-to-bus should be dropped,
  as OpenModelica does (TOOLBUG-197).
- VehicleInterfaces `MinimalChassis2/3`: equation count off by 2 against
  OpenModelica, cause not found.
- `replaceable model Load = Resistor(R = R)` resolves the binding in the
  wrong scope (`load.R = load.R`).
- `per = per` inside a redeclaration loses to the nested defaults (IDEAS
  `Pump_stratos`, ET009).
- `Complex` arguments to functions are copied once per field, so deeply
  nested `Complex` expressions grow exponentially (TOOLBUG-195 note).
- CalendarTime's month-search initial algorithm grows past 50,000 nodes and
  is refused (ED013); needs generated temporaries (TOOLBUG-181 note).
- Inline `sample(u, Clock(...))` (the other half of TOOLBUG-067): hoist the
  constructor into a synthesized clock coordinate. No uses in the evaluation
  libraries.
- OpenIPSL Nordic44: compile exceeds 300 s, then fails with ED008 on
  `G1_bus7000.gENROU.S`. Profile with a release build.
- Flatten has no warning channel, so EF002 quantity mismatches (`Temp_C` vs
  `ThermodynamicTemperature`) and the TOOLBUG-123 acceptance cannot be
  reported as warnings. Add one, then downgrade those to warnings.

### 1.8 Measure on the whole backlog
After round 3, re-run all 7,892 OpenModelica-accepted models (about ten
hours at six parallel compiles) for a dataset-level number, instead of the
stratified sample.

## 2. Decisions to revisit

- **TOOLBUG-168:** whole-number parameters with translation-time bindings
  are lowered as literals even under `--no-fold-parameter-bindings`.
  Narrow it to parameters actually used as array extents so the flag keeps
  its meaning.
- **ED001:** models OpenModelica also counts as unbalanced stay an error.
  OpenModelica's `checkModel` never rejects on balance; if coverage against
  it matters more than refusing unrunnable models, this becomes a warning.

## 3. Compiler and tooling

- **Pass scheduling:** pipelines run in the order written (with groups,
  `fixpoint(...)` and skip-when-unchanged). Dependency-ordered scheduling
  (passes declaring what they require and invalidate) is not implemented.
- **`modelsan check -fsanitize=static`** still runs one simulation; a
  static-only selection should skip execution.
- **Sanitizer coverage gaps** reported as "not checked": runtime expression
  observation for DomainSan (`observe_expression`) and connector observation
  for NetworkSan are not provided by the Rumoca backend.
- **Unit analysis after passes:** the default `inline-constants` pass
  replaces package-constant references with literals, so unit analyses must
  read `--pass none` output (TOOLBUG-028). A pass that keeps units on
  inlined literals would remove that caveat.

## 4. Environment

- The machine's disk stalls under Microsoft Defender when several builds
  run; evaluation timeouts during such windows are I/O artefacts. Copying
  library sources to `/dev/shm` avoids them.
- Four suite tests fail for environment reasons only (galec C toolchain,
  two OpenModelica Jacobian batteries, CUDA). Fixing the toolchain on the
  host, or marking them as requiring it, would make the suite fully green.
