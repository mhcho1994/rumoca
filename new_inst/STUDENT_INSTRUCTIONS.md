# Writable Equation and Execution IRs with Automatic Connector CSV Logging

## Objective

Extend the existing Rumoca/RBC SDK so that **equation IR and procedural execution IR are both first-class writable pass targets**. Keep the existing ability to construct models from nothing and to rewrite their equations. Add the ability to inspect and transform their lowered executable programs.

The end-to-end demonstration must build a complete model using the public Python IR builder, rewrite one of its equations, lower it, and insert executable logging instructions for every connector. A fresh Rumoca runtime process must execute the saved artifact and produce one CSV per connector.

This is not a new ABI/plugin project. RBC remains the interchange contract; Python remains the first pass-authoring SDK. Do not add Rust dynamic-library plugins, RPC, WASM, FastDyn, or Tempo dependencies.

## 1. Start from what already exists

The reported implementation already provides `Model.empty(name)`, `model.builder(pass_name)`, a public Python builder, equation/operand rewriting, public raw access and refresh, validation, and a Rust builder. Reuse these. Do not restart the bitcode project or create a sanitizer-private builder.

Inventory the actual fork's Solve and runtime representations first. Upstream describes Solve as typed scalar/tensor programs and execution roots [1]. Extend/expose the appropriate existing vocabulary; do not build a competing numerical instruction set merely to obtain an LLVM-like appearance.

Separate three things in the design note: existing supported APIs, missing public access to existing machinery, and genuinely missing execution semantics. Record the fork commit and supported RBC version.

## 2. Preserve both transformation surfaces

The pipeline is:

```text
Model.empty()
    -> equation builder
    -> equation transformation passes
    -> checked equation IR
    -> lowering
    -> writable execution IR
    -> execution transformation passes
    -> checked executable artifact
    -> runtime/code generation
```

Equation passes must retain the ability to add/remove states and equations, rewrite expressions, change initialization, and alter physical relationships within the supported IR vocabulary. Execution passes must be able to insert, replace, and remove instructions and modify supported control flow. They are not limited to returning trace requests.

Reuse checked reconstruction on import. Verification checks representation validity and backend capabilities; it does not require every transformation to preserve the original model's physical behavior. Deliberate equation changes and fault injection are valid uses.

Do not promise inverse translation from arbitrary execution edits back to equations. An executable can consist of a base mathematical model plus explicit observational or behavior-changing execution transformations.

## 3. Make the procedural representation genuinely writable

Expose existing numerical programs through public SDK objects. Add or expose functions, blocks/regions, typed values, ordinary computational operations, calls, supported branching, and ordered effects. Public builders need insertion points and operand/instruction replacement, not just a specialized `add_logger` method.

Keep one authoritative operation/type/effect registry. Convenience helpers should build the canonical operations and use the shared validator, not reproduce type rules in a second Python implementation. The existing low-level untyped builder can remain available.

For this milestone, expose serializable lifecycle functions equivalent to:

| Function | Meaning |
|---|---|
| `run_start` | Per-instance execution setup, before the first publication |
| `publish(snapshot)` | Observe a consistent state at a declared publication point |
| `run_finish` | Normal completion and controlled-failure cleanup |

These are executable IR functions, not retained Python callbacks. Their blocks must be editable with the same instruction builder used elsewhere. A single-exit layout makes `before_return` a useful convenience in the example; it does not limit the general execution representation to three hooks.

For the example, provide operations equivalent to `snapshot.time`, `snapshot.sequence`, `snapshot.phase`, `snapshot.value`, `csv.open`, `csv.write_row`, and `csv.close`. CSV resources are per run/model instance. Their declarations carry relative filenames, typed columns and connector metadata; live host file handles are never serialized.

CSV operations have observable effects. They must not be deleted as unused arithmetic, speculated into branches, or moved across publication points. Preserve ordering with explicit effect information and the execution model's ordering rules. MLIR's effect model is a useful reference for this distinction [2]. Do not force LLVM or MLIR adoption.

## 4. Keep derived programs coherent with equation changes

Record the equation artifact digest, lowering configuration/version, execution revision, and applied execution-pass identities/options. Keep equation data and executable data separate even when they share one RBC container. Do not expose private Rust serialization as the new public execution schema.

For V1, any equation transformation invalidates the derived execution program and dependent analyses. Re-lower, then explicitly replay compatible execution passes. Analysis invalidation is a standard pass-management responsibility [3]. Fine-grained regeneration is a later optimization.

A stale artifact must fail with a useful diagnostic. The runtime must never silently execute old code against changed equations or silently regenerate code and discard execution edits. Use a mode such as `--execution=require` for the demonstration.

Public `raw_model` writes must not bypass this rule. Recompute the relevant digest at verification/run boundaries or enforce an equivalent transaction mechanism. `refresh()` rebuilding cached SDK views is not, by itself, validation or proof of executable freshness.

Retain replayable execution-pass recipes. When a rewrite removes or changes an instrumented target, replay must either resolve it through supported semantic provenance or report the missing target. Never silently omit its instrumentation.

## 5. Define exactly when logging occurs

Default connector logs describe **published, consistent model snapshots**, not every RHS call, residual evaluation, trial update, or nonlinear solver iteration. Numerical solvers distinguish internal evaluation/step behavior from requested output values [4], and Modelica event processing can iterate at one simulation time [5].

For this example, request publication at `0, 0.1, ..., 5.0` seconds. Publish the initialized state once, then one settled snapshot per requested time. Reconstruct all needed algebraic/alias values from the same snapshot. Do not mutate the integrator's history merely to produce output. Use the existing backend's valid output reconstruction; reject an unsupported observation rather than returning stale values.

Publication is an observation operation, not necessarily an internal accepted-step boundary. Do not force extra physical events merely to write CSV. For future eventful tests, the default publication at an event is after its discrete iteration settles. If event and periodic publications coincide, define deduplication explicitly. Use publication IDs and phases so equal timestamps never become ambiguous.

Runtime sanitization may later require pre-operation insertion during trial evaluations. Do not confuse that separate capability with the publication semantics of this logger.

The snapshot available to this logging pass is read-only. Future state reset/intervention operations must go through explicit solver-consistency handling, not arbitrary writes into solver buffers.

## 6. Preserve connector identity and observability

A connector is a semantic object with component ownership and typed members, not a display-name label. Preserve member roles, units, connection sets, inside/outside orientation, stable artifact-local IDs, generated/source provenance, and mappings to executable values.

Connection construction must create actual mathematical constraints as well as metadata. Modelica forms connection sets, then generates potential equalities and signed flow sums [6]. Do not generate a separate pairwise flow equation for each edge of a multiway connection. The simple example uses two disjoint two-port connection sets.

For original variables removed by alias elimination, retain a checked reconstruction mapping. Register requested connector observations before destructive optimization, or provide an equivalent complete value map. A connector member may map to a state slot, constant, signed alias or computed expression. Never guess storage slots from names or IDs.

The example logs **one file per connector instance**, not per edge, connection set, or member. Connected endpoints therefore each receive a file. Equal potential values and opposite endpoint flows are intentional and useful for validation.

V1 must support the scalar connector members in the example. Unsupported shapes/types or streams must yield an explicit capability diagnostic; no silent skipping. Later array support should define a deterministic column layout, and stream logging must distinguish the declared stream value from derived transport quantities.

## 7. Required synthesized model

Build three components from scratch: hot thermal storage, a storage-free thermal conductor, and cold thermal storage. Do not read Modelica source, import MSL components, or load a prebuilt fixture.

```text
hot.port <-> link.a [thermal conductor] link.b <-> cold.port
```

There are four connectors. Define a HeatPort-like type with `T` in K as a potential member and `Q_flow` in W as a flow member, positive into its owning component. This matches MSL's documented heat-port sign convention [7].

Use two temperature states, two heat capacities and one conductance:

```text
C_hot = 2 J/K       T_hot(0) = 350 K
C_cold = 3 J/K      T_cold(0) = 300 K
G = 1 W/K
```

Build the component equations and actual connection equations:

```text
hot.port.T = T_hot
cold.port.T = T_cold

der(T_hot) = hot.port.Q_flow / C_hot
der(T_cold) = cold.port.Q_flow / C_cold

link.a.Q_flow = G * (link.a.T - link.b.T)
link.a.Q_flow + link.b.Q_flow = 0

hot.port.T = link.a.T
hot.port.Q_flow + link.a.Q_flow = 0
link.b.T = cold.port.T
link.b.Q_flow + cold.port.Q_flow = 0
```

Use explicit initial equations for the two temperatures. Expose them as outputs without changing their state roles so the runtime has legitimate observables. Generated provenance must have a real source-table owner.

The connection-set helper emits the last four equations exactly once. Reuse the existing connection machinery or provide a documented restricted builder helper that produces canonical RBC; do not make each pass author implement connection lowering.

Use `add_derivative_equation(state, rhs)` instead of requiring callers to remember a solver-specific residual form. More general supported residual forms belong in normalization/lowering, not in ad hoc caller conventions.

## 8. Demonstrate an equation transformation, not only construction

After synthesis, an independent equation pass rewrites the conductor law to:

```text
link.a.Q_flow = scale * G * (link.a.T - link.b.T)
```

The example defaults to `scale=2`. This is an expression/equation rewrite, not merely an input override. Save the original and rewritten equation artifacts. Lower only after the equation pass and validation.

Also test the reverse sequence: lower and instrument, modify an equation, reject the stale executable, explicitly re-lower, replay the logger, and execute the new program. If this fails, equation transforms have not remained first-class.

> **Amendment, 2026-09-27 — Execution IR v2, D3.** The sentence above is the
> original contract and states the v1 rule. It is superseded, not corrected:
> the v2 staleness digest covers the identities a program *references*, not
> the equations, so rewriting the conductor law — which no trace point
> observes — now leaves the instrumented program runnable. That is the
> intended change; v1 rejected the edit and forced a re-lower that discarded
> execution-level work the edit did not affect.
>
> The rejection path is still required, and is now provoked by changing a
> referenced identity: retype or remove a variable a trace point names, see
> the executable rejected as stale, explicitly re-lower, replay the logger,
> and execute the new program. Both halves are tested —
> `test_unreferenced_equation_edit_is_not_stale` and
> `test_referenced_identity_change_is_stale_and_names_the_point`. Rationale
> and the full amended test list are in
> [EXECUTION_IR_V2_DESIGN.md](EXECUTION_IR_V2_DESIGN.md) §6–§7.

## 9. Implement the connector CSV pass

The pass enumerates all connectors from semantic IR; it contains no thermal-specific names or formulas. It registers a CSV sink for each connector, loads its member values from the publication snapshot, and inserts ordinary executable `csv.write_row` instructions.

The runtime provides generic CSV operations. It must not enumerate model connectors itself or reconstruct pass decisions from hidden metadata. After serialization, the generated executable functions contain the logging behavior.

Use relative, collision-free filenames inside an explicit runtime trace directory. Do not overwrite earlier runs by default. Write a `manifest.json` mapping filenames to connector IDs/paths, member units/roles, and artifact identity. Member order must be deterministic. Use correct CSV quoting, locale-independent numbers and round-trip numeric precision. Surface I/O errors as structured execution failures. Flush/close on normal and controlled-failure exits; do not promise recovery from process termination.

The demo header is:

```csv
time_s,publish_id,phase,T,Q_flow
```

At `t=0` and `scale=2`, the expected values are:

| Connector | T (K) | Q_flow (W) |
|---|---:|---:|
| hot.port | 350 | -100 |
| link.a | 350 | 100 |
| link.b | 300 | -100 |
| cold.port | 300 | 100 |

These are analytic expectations, not measurements from an executed Rumoca run.

No Python `csv.writer` in the synthesis pass, runtime callbacks into the pass, or postprocessing of a returned trajectory qualifies as implementing this feature. Python may read CSV afterward in an independent test oracle.

## 10. Example scripts and target API status

`synthesize_connector_csv.py` contains the full synthesis, equation rewrite, lowering, execution instrumentation and optional fresh-process invocation. The execution/convenience method signatures specify the **target API for this milestone**, not verified APIs of the current fork. Map helpers to the existing SDK and document final names. Do not create a toy backend or second IR implementation to make the example appear to pass.

The known starting entry points are `Model.empty`, `model.builder`, and the reported derivative-equation helper. The proposed `rumoca_bitcode.execution.lower`, lifecycle builders, sink operations, program serialization and `rumoca bitcode run` switches are implementation requirements.

Suggested execution after implementing these surfaces:

```bash
python synthesize_connector_csv.py --out build/thermal --run
python check_thermal_csv.py build/thermal/traces --conductance-scale 2
```

The CLI should load the saved executable RBC and execute it without the Python pass module. It must not silently re-lower equations. A runtime that lacks a required execution operation must reject the artifact rather than omit it.

`check_thermal_csv.py` is a standalone standard-library verifier. For the model constructed above, let `a = scale * (1/2 + 1/3)`. The analytic reference is:

```text
T_hot(t)  = 320 + 30 exp(-a t)
T_cold(t) = 320 - 20 exp(-a t)
Q(t)      = 50 scale exp(-a t)
```

Endpoint flows are `-Q, +Q, -Q, +Q` in the table's order. The idealized stored energy `2*T_hot + 3*T_cold` stays at `1600 J`.

The verifier expects four CSVs with 51 records each, checks every temperature/flow against the reference at its recorded time, validates publication order/phase, checks endpoint equalities and flow sums, and checks energy. Its fixture tests exercise only the verifier, not Rumoca.

## 11. Validation and acceptance tests

Require these tests, in addition to existing SDK and Rust regressions:

| Test | Required outcome |
|---|---|
| Empty-model synthesis | Complete valid RBC without `.mo` input or prior fixture |
| Equation rewrite | `scale=1` and `scale=2` produce their distinct analytic trajectories |
| Serialization | A fresh runtime executes inserted CSV instructions without the pass installed |
| Every connector | Four files; all requested members covered; correct endpoint signs |
| Publication policy | 51 rows per file; no trial/RHS/Newton-iteration records |
| Instrumentation neutrality | Baseline and logged model observations agree within declared tolerances |
| Stale derivation | Any equation change invalidates old execution and dependent analyses |
| Raw mutation | Public raw edits cannot bypass stale-execution detection |
| Replay | Re-lowering followed by explicit logger replay preserves requested coverage |
| Alias elimination | Observed members remain reconstructible or compilation fails explicitly |
| Optimization effects | Optimizations cannot delete, duplicate or reorder CSV writes illegally |
| Duplicate pass | Deterministic deduplication or explicit duplicate-key rejection, not doubled rows |
| Invalid execution IR | Bad references, wrong types, undefined values and invalid lifecycle usage rejected |
| Unsupported backend | Fail explicitly when required executable operations are unsupported |
| Failed initialization | No rows labelled as valid snapshots; structured failure and resource cleanup |
| Event regression | Later eventful test publishes settled states with unambiguous IDs/phases |

Do not count a successful bitcode check as proof that the executable program is runnable. Validate the equation IR, execution IR, derivation link, and selected backend capabilities separately.

## 12. Delivery order

1. Document current representations and missing public execution surfaces.
2. Expose writable execution programs and builders over existing Solve machinery.
3. Implement versioned serialization, effect validation and stale-artifact checks.
4. Define consistent snapshot publication and semantic value reconstruction.
5. Implement generic executable CSV operations in one real Rumoca backend.
6. Run the complete synthesized model and equation rewrite example.
7. Run the fresh-process, CSV, neutrality, invalidation and replay tests.
8. Document how another Python pass inserts an ordinary computation/check using the same builder.

Do not wait for every backend or a full runtime-sanitizer suite. The proof of modularity is one real backend running a model synthesized and transformed through both public IR layers, with logging entirely represented in the saved executable artifact.

**Final acceptance statement:** A researcher using installed tools, without modifying Rumoca, can synthesize equations and connectors, rewrite the model, transform its lowered executable program, serialize it, and obtain correct connector CSVs from that executable program in a fresh runtime process.

## Primary design references

These describe upstream concepts; they do not establish which new methods exist in the private fork.

1. Rumoca IR pipeline: https://raw.githubusercontent.com/CogniPilot/rumoca/main/spec/SPEC_0007_IR_PIPELINE.md
2. MLIR side effects and speculation: https://mlir.llvm.org/docs/Rationale/SideEffectsAndSpeculation/
3. MLIR pass and analysis management: https://mlir.llvm.org/docs/PassManagement/
4. SUNDIALS CVODE usage and output/evaluation behavior: https://sundials.readthedocs.io/en/latest/cvode/Usage/index.html
5. Modelica DAE and event iteration: https://specification.modelica.org/maint/3.6/modelica-dae-representation.html
6. Modelica connection equations and sign conventions: https://specification.modelica.org/maint/3.6/connectors-and-connections.html
7. MSL 4.1.0 thermal connectors: https://doc.modelica.org/Modelica%204.1.0/Resources/helpDymola/Modelica_Thermal_HeatTransfer_Interfaces.html

## Validation performed when preparing this handoff

The Python files were syntax-checked. The standalone CSV verifier passed six tests using temporary analytic fixtures, including rejection of a wrong flow sign, missing connector and incorrect equation scale. The synthesis/instrumentation example has not been executed against the private Rumoca fork. No generated CSV is presented as real Rumoca simulation output.
