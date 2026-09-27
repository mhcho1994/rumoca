# Execution IR v2 — design note

Fork commit `c8f0254f` (`rumoca-bitcode-v1`). `RBC_VERSION = 2`
(`crates/rumoca-bitcode/src/schema.rs:26`). `ExecutionArtifact.version` is
currently `1` (`crates/rumoca-ir-solve/src/execution.rs:12`); v2 sets it to `2`
and rejects `1` with no adapter (SPEC_0007: no superseded readers).

## 1. Defects, changes, proofs

| # | Carrier | Change | Test |
|---|---|---|---|
| D1 | `NumericalProgram` field on `ExecutionArtifact` (`ir-solve/src/execution.rs:24-34`); `lower`/`run` in `rumoca/src/bitcode_execution.rs` | Drop the field. `HostProgram` is the wire; the numerical program is derived at load by `rumoca_phase_solve::execution::export` from equation IR + recorded `lowering`. | Serialized artifact has no `storage`/`residual`/`derivatives`/`initialization`/`algebraic_blocks`/`initial_blocks`/`observations`; JSON↔CBOR↔text round-trip; `run --backend rk45` matches pre-milestone to solver tolerance; `--backend <other>` gives a capability diagnostic naming the backend; `version == 1` rejected with a stable code. |
| D1b | `apply` in `rumoca/src/bitcode_execution/overrides.rs:8-26`: mutates `execution.numerical.storage.start`, matching by `v.name == name && v.role == "parameter"` | No serialized program to mutate. Overrides apply to the *derived* program in memory at `run`, after derivation and before execution, and are never persisted. Resolve the name to a `VariableId` through the model, then map that to a slot through the derived program's **variable->storage** map (not the trace-point observation map -- an override targets `p`, not an observation); the name match violates the post-resolution identity rule (and the string `role` goes with `Storage`, D4). | `--param` on a v2 artifact reproduces today's result; an override naming a variable absent from the model fails naming it, not silently. |
| D2 | `RbcTracePoint` (`bitcode/src/schema.rs`) vs `NumericalProgram.observations` (`phase-solve/src/execution.rs:241`) | `trace_points` becomes the sole request. `snapshot.value { result, trace_point }` references a `TracePointId`. `observations` ceases to be a wire concept. | Unknown `TracePointId` fails validation naming the ID; grep-based architecture test that `rumoca-phase-solve` and `rumoca-sim` read no *wire* `observations` field (the derived internal map is exempt and named in the test); SDK helpers create a trace point. |
| D3 | `digest` (`rumoca/src/bitcode_execution.rs:53-58`): `Sha1(serde_json::to_vec(&file.model))` | `dependency_digest` over a hand-written canonical encoding of referenced trace points (component path, `TypeId` contents, causality, unit), the `lowering` profile, `RBC_VERSION`, and artifact version. The component path is encoded as the structured `RbcComponent` chain resolved to names *at encoding time*, never as a dotted display string that a later reader re-splits (SPEC_0007 no-tokenizing rule). | Digest stable under field reorder and `skip_serializing_if` toggles; stable when an unreferenced residual is rewritten (and `run` succeeds without re-lower); changes and `run` names the trace point when a referenced variable is removed or retyped. |
| D4 | `BTreeMap<String, Value>` env, `operator: String`, `Storage.role`/`causality: String`, `passes: Vec<serde_json::Value>`, `compute { instructions: Vec<ScalarOp> }`, **`CsvSink.column_types: Vec<String>`**, **`CsvSink.metadata: serde_json::Value`** (both found while sizing §2; §9 fails on them) | Declared typed locals; `serde` forms of Solve's `UnaryOp`/`BinaryOp`/`CompareOp`; `Vec<PassRecord>`; `compute { result, expr: ExprId }` over one expression vocabulary; lexical scoping. **Scope rule:** a local declared outside an `if` may be assigned inside it; one declared inside is not visible outside; the validator rejects the reverse. This is the one place v2 changes what a *valid* v1 program means (v1 cloned the environment, so an inner write was silently discarded), so it is stated here and in §2a. **Leaf rule:** `compute` expressions live in a program-local arena keyed by a distinct `ProgramExprId`, and may reference **declared locals only**. `ProgramExpr` has **no** `VariableId` variant, so this is unrepresentable rather than validated -- stronger than the error-with-a-code originally specified, and the reserved code is left unallocated. Reusing the model's node type verbatim would let a program read a variable directly and reopen the second observation path D2 closes. The by-construction fix (node type generic over its leaf reference) is the SPEC_0036 answer if `compute` gains real callers; not now. | Undeclared local, type mismatch, `Text` in arithmetic, and out-of-scope write are four distinct stable codes; operator enums round-trip without a string, misspelling is a deserialization error. |
| D4c | `compute` has **zero** production callers (thermal mix: 3 snapshots, 8 `snapshot.value`, 12 `csv.*`, no `compute`); its only users are three hand-written `ScalarOp` programs in `new_inst/test_execution.py` | Keep it, typed. But the rewrite of those three is not sufficient evidence a public construct works: add one realistic end-to-end use -- the thermal logging pass computing a derived column such as `hot.port.T * hot.port.Q_flow` -- through the typed form. | Derived column appears in the thermal CSV with the expected values; a `VariableId` leaf in a program expression is rejected with its own code. |
| D4b | `CsvSink` (`ir-solve/src/execution.rs:113-126`) | `column_types` + `columns` collapse to `Vec<SinkColumn { name, ty: ValueType }>`, making a length mismatch unrepresentable. `metadata` becomes `SinkMetadata { connector: ConnectorId, orientation: Orientation, members: Vec<SinkMember { trace_point: TracePointId }> }` -- identity only. `ConnectorId` is admissible here because a sink lives in the same file as its model; the canonical-path rule binds `PassRecord`, which must survive a recompilation. **Cross-reference D2:** `unit`, `kind` and `variable_id` are all reachable from the trace point, so the sink restates none of them and stops being a second, weaker owner of connector identity. | Sink with mismatched column/type counts is unconstructible; a sink member naming an unknown `TracePointId` fails validation. |
| D5 | §2a "loop-free" in `docs/SPEC_RUMOCA_BITCODE.md` | Restate as *statically bounded*: acyclic checked call graph, and any iteration has a compile-time constant trip count. | Recursive `call` cycle rejected by termination analysis. |

### D2 clarification — observations are demanded, not requested

Established by the §5 pre-check: the thermal artifact carries **8** serialized
observations and **0** trace points. The request today is the `observe=`
argument to `execution.lower`, and `lower-execution` with no `--observe`
falls back to *every* variable (`bitcode_execution.rs:94-96`). With `functions`
empty those are observations nothing reads -- dead data on the wire.

In v2 the derived observation map is built from the trace points the
`HostProgram` actually references through `snapshot.value`. The program says
what it needs, so "observe everything" has no meaning:

- **No trace points and an empty program is valid**, with zero observations.
  `lower-execution` on it yields a program-less v2 artifact with the profile
  recorded. It is not an error.
- **The only error is D2's:** a program referencing a `TracePointId` the model
  does not have.
- **`--observe` is removed from `lower-execution`.** It was doing two jobs:
  editing the model to add requests, which is an equation-IR edit and belongs
  to a pass or the SDK, and selecting observations, which now belongs to the
  program. Neither is the CLI's.
- **The SDK logging helper is the producer.** `execution.lower(observe=...)`
  becomes: create a trace point per connector member on the model, carrying
  instrumentation provenance that names the helper, then emit the
  `snapshot.value` / `csv.write_row` instructions that reference them. This is
  §3's "helpers create trace points", and it is the only such helper.

Ordering consequence: because §5 gives `csv.write_row` ownership of column
order, the order in which the helper creates trace points does not affect the
golden. Create them in connector-member order for readable artifacts, but the
test asserts against `write_row` order, never against creation order.

**Wire versus emitted.** The wire `CsvSink` carries identity only (D4b). The
runtime's manifest writer resolves `unit`, `kind` and `variable_id` from the
model at `run` and emits the denormalized manifest as before. An earlier
proposal here claimed a typed identity-only struct could serialize to the same
JSON as one restating those fields; it cannot, and the resolution is this
split. The §8 golden is the **CSV output**; `manifest.json` is a run-trace side
artifact and not a golden. Two manifest changes are wanted: `variable_id`
becomes an integer rather than `"5"`, and `check_thermal_csv.py` is updated in
the same PR.


## 2. Consumers — every file this touches

Obtained by `grep -rln`, not memory.

**`ExecutionArtifact`** — `bitcode/src/schema.rs`, `eval-solve/src/execution.rs`,
`ir-solve/src/execution.rs`, `ir-solve/src/execution/validate.rs`,
`sim/src/execution.rs`, `solver/src/fmi_me/tests/publication.rs`,
`rumoca/src/bitcode_execution.rs`.

**`NumericalProgram`** — `ir-solve/src/execution.rs`,
`phase-solve/src/execution.rs`, `sim/src/execution.rs`,
`rumoca/src/bitcode_execution/overrides.rs`. Wire-`observations` readers:
`eval-solve/src/execution.rs:126`, `ir-solve/src/execution/validate.rs:114`.

`solver/src/fmi_me/kernel/component.rs:1553` is **not** one: it reads
`outcome.observations`, an internal event-outcome field. The derived internal
`TracePointId -> observation` map stays legitimate for the kernel to read.

`solver/src/fmi_me/tests/publication.rs` constructs an `ExecutionArtifact`,
so the solver crate's tests depend on the public wire type. Not blocking. After
§2 that test should build the derived program directly; if it cannot, that is a
SPEC_0029 boundary question to raise separately rather than to settle here.

**Python SDK** (`packages/rumoca-bitcode/rumoca_bitcode/`) — `execution.py`,
`connectors.py`, `__init__.py`, `compiler.py`, `linker.py`, `removal.py`,
`builder.py` (trace points).

**Text form** — untouched. `emit-text` already refuses an artifact carrying an
execution section, so the writer and parser needed no change; see §6.

**`new_inst/`** — `test_execution.py`, `test_composition_tutorial.py`,
`check_thermal_csv.py`, `test_link.py`, `test_connection_validation.py`,
`test_clock_transport.py`, `compose_models.py`, `synthesize_connector_csv.py`.
`IMPLEMENTATION_INVENTORY.md` is deliberately **not** updated: it records the
pre-implementation state as inspected on 2026-09-23 and says so in its first
paragraph, so editing it would destroy the record rather than correct it.

**Docs** — `SPEC_RUMOCA_BITCODE.md` §2a, `architecture/writable-execution-ir.md`,
`bitcode-reference.md` (regenerated; unchanged, since `schema.rs` did not
change), `writing-a-bitcode-pass.md`, `combining-models.md`,
`modelsan-bitcode-integration.md`, `new_inst/README.md`.
`writing-a-bitcode-pass.md` additionally documents the removal of `--observe`
and the helper's new duty to create trace points. `bitcode-linking.md` needed
no change.

### Files the note did not predict

The Python SDK's `connectors.py`, `linker.py`, `removal.py` and `builder.py`
needed no change: they touch the equation IR, and D2 moved observation
registration onto an existing equation-IR API (`model.add_trace_point`).

Added instead:

- `crates/rumoca-ir-solve/src/execution/derived.rs` — the derived program,
  moved out of the wire module so §9's grep holds by construction.
- `crates/rumoca/src/bitcode_disasm/execution.rs` and a hook in
  `bitcode_disasm.rs` — the human listing of the program, which is what makes
  the text-profile refusal in §6 acceptable.
- `crates/rumoca/tests/architecture_hardening_test/execution_wire_boundary.rs`
  — the §8 grep-based architecture test.
- `crates/rumoca-bitcode/src/validate.rs` — `validate_execution_references`
  (analysis 5). It lives here, not in `ir-solve`, because resolving a trace
  point needs the equation IR, and the other direction would invert the
  SPEC_0029 tier.
- ModelSan: `backends/rumoca_execution.py` is a *producer* of observation
  requests under D2, so it registers its own trace points; its three test
  modules move off the derived program.

## 3. SPEC_0045 factor mapping

Constraint, not build order: v2 must not take a shape SEV-001's grammar cannot
absorb. SEV-002 forbids a second wire form, so every row below is the *same*
grammar's factor, never a parallel vocabulary.

| v2 construct | SPEC_0045 factor | Note |
|---|---|---|
| `compute { result, expr }` | `ValueOp` | Expression nodes are the existing arena type; no second expression language (TRP-002). |
| `snapshot.time` / `sequence` / `phase` | `ValueOp` | Reads of runtime-provided values; pure, no effect. |
| `snapshot.value { trace_point }` | `ValueOp` | Resolves through the derived observation map; the reference is canonical (`TracePointId`), not backend-local (TRP-003). |
| `csv.open` / `write_row` / `close` | `EffectOp` | Ordered, never deleted as unused, never speculated, never moved across a publication point. |
| `assert` | `EffectOp` | Diagnostic effect; ordering is observable. |
| `if` | `Terminator` + two `RegionId`s | Lexical scope is the region boundary; this is why §5 replaces environment-cloning with declared scope. |
| `for` (only if §6 picks it) | `Terminator` + one `RegionId` | Literal trip count only. |
| `call` | `InvokeOp` | Parameter-free in v2; acyclic call graph is the termination proof. |
| function body | `RegionId` | A function is a named region, not a new container kind. |
| `PassRecord` | **no factor** | A *receipt* in the TRP-046 sense: digest plus canonical path. SEV-002's provenance model covers ops, not the edit log. **Consequence:** a receipt never stores a `VariableId` or `TracePointId` as its target identity -- canonical path only, so a receipt stays readable across a recompilation that renumbers ids. |
| `LoweringProfile` | **no factor** | A compiler-known *named* profile that expands to normalized fields (TRP-033), selecting the root profile (SEV-003) the derived program is built with. **Consequence:** in v2 it is an enum with one variant, not an open struct a pass can fill in, and `dependency_digest` hashes the *expansion* rather than the name -- so adding a field to the expansion invalidates dependent programs, which is correct. |
| `CsvSink` | **no factor** | An opaque capability handle (SEV-014), not an op. **Consequence:** two distinct validator codes, never merged -- (a) *unresolved sink reference*: the id a `csv.*` effect names is not declared; (b) *missing file-effect capability* (TRP-042): the target cannot write files at all. Both fail before run time, and they have different fixes. SEV-016's observable-access-count rule binds the effects, not the declaration. |

## 4. Open decision for review

§6 offers two bounds. **Recommendation: keep no loops** — the termination
analysis is call-graph acyclicity alone, and the §2a wording change is the whole
task. Array-valued connector members were deferred by the original milestone §6
and nothing in the current thermal example iterates. Adding `for` costs a
literal-bound check, a `RegionId`, and validator surface for a capability with
no caller today; it is cheap to add when the first array member lands, and the
wording change above already makes room for it without a further spec edit.

## 5. Ordering ownership, and the one-off check

v2 **owns** the CSV ordering rather than inheriting it: column order is the
order of values in `csv.write_row`, and row order is publication sequence.
`CsvExecution::Write` already behaves this way. **Nothing may depend on
`export`'s row order**; once that holds, the thermal golden holds by definition
rather than by coincidence. This rule goes in §2a alongside the
effect-ordering rules.

That reduces the risk to a single one-off: confirm today's serialized program
and `export` agree on observation order before the digest changes. If they
disagree, that is a pre-existing discrepancy and lands as its own commit with
its own test *before* this milestone, so a golden diff here is never
ambiguous about which change caused it.

## 6. Decisions taken during implementation

Five points the note did not settle, resolved in code. Each changes something a
reviewer would otherwise expect from §3 or §8.

**EX2-023 is unallocated, not checked.** The milestone asked for a distinct code
for "write to a local declared in an inner scope from outside". Locals are
declared per `Function` and `if` carries no declarations of its own, so an
inner declaration is unrepresentable — the same argument that left EX2-024
unallocated. The first implementation carried a `depth` field that was always
`0`, so the check could never fire; that dead machinery is gone. What a branch
can actually do is leave a local unassigned on one arm, and reading it after
the join is EX2-020. The test asserts that, and both codes stay documented and
unallocated so they are not reused.

**Two effect codes were split out.** `csv.open`/`write_row`/`close` shared one
untyped string covering lifecycle, resource state and row width. They are now
`EX2-012` (wrong phase, or a resource not open in it) and `EX2-013` (row width
disagrees with the declared columns). Three conditions with three different
fixes should not share one message.

**Text round-trip is a refusal, not a conversion.** §8 asks that `program`
round-trip JSON ↔ CBOR ↔ text. JSON ↔ CBOR round-trips exactly, and the test
asserts it. The **text profile is a declared subset** that already refuses any
artifact carrying an execution or connector section, and extending it would
create a third hand-editable representation of the program — which is what
TRP-002 argues against. The test therefore asserts the refusal is explicit, and
`bitcode disasm` gained an execution listing so the program is still readable
by a person. This is a deviation from that §8 row and is flagged as one.

**The digest is refreshed at the pass boundary.** The digest covers the
identities the program references, so it is unknowable until a pass has emitted
its instructions. Refreshing it inside `validate`/`save` would re-derive
against whatever the model has since become, and nothing would ever be stale —
the first implementation did exactly that and silently defeated the staleness
test. `Builder` is now a context manager whose exit refreshes it, which makes
the pass boundary explicit. `lower-execution` is idempotent precisely so this
is possible.

**The emitted manifest resolves what the wire leaves as identity.** The wire
`CsvSink` is identity-only, so the runtime joins `connector_path` and per-member
`{name, unit, kind}` in from the derived program when it writes
`manifest.json`. `Observation` gained a `quantity` field, borrowed from the
trace point at derivation, so a reader does not infer "flow" from a member's
name. This is the wire/emitted split the review asked for, and it is why
`check_thermal_csv.py` still runs without importing Rumoca.

**`SinkMetadata.connector` and `.orientation` are optional.** D4b specified
both as required. Not every sink is connector instrumentation — ModelSan's
variable logger and the tests' plain sinks are not — and writing `connector: 0`
on one states a fact a reader could act on. `members` stays required, which is
what preserves the D2 unification: a sink's trace-point references are visible
without reading its instructions, and `referenced_trace_points` walks them. The
emitted manifest resolves `connector_path` only when `connector` is present.

**`manifest.json` no longer carries `variable_id` at all.** The review asked
that it become an integer rather than `"5"`. Under D2 the identity a sink
member carries is its `trace_point`, which is an integer, and the resolved
member record is `{trace_point, name, unit, kind}`. `check_thermal_csv.py`
needed no change beyond its docstring: it reads `connector_path`, `name`,
`unit` and `kind`, all of which the runtime still resolves.

### Verified

- All eight thermal golden CSVs (scale 1 and scale 2) byte-identical to
  `new_inst/results/`, and `check_thermal_csv.py` passes at both scales.
- `new_inst/`: `test_execution.py` 23, `test_connection_validation.py` 14,
  `test_link.py` 9, `test_clock_transport.py` 3,
  `test_composition_tutorial.py` 1 — all passing.
- `packages/modelsan/tests`: 322 passed, 3 skipped.
- `cargo test --workspace --no-fail-fast`: 6723 passed, 4 failed. All four
  fail identically before this milestone and none is in its scope:
  `jacobian_admission_battery::…under_omc` and
  `jacobian_standard_modelica::…under_omc` report shape-mismatch rejections
  for mixed scalar/array builtins (`atan2`, `fill`, `zeros`, `identity`, `.+`)
  that OpenModelica accepts — real front-end gaps, not missing tools, since
  `omc` is installed here; `galec_equivalence::embedded_c_serves_a_conditional
  _branch…` fails in the generated C; and
  `target_manifest::tests::cuda_ode_builtin_target_nvcc_smoke…` fails inside
  `nvcc`, which is also installed.
- Gates: `cargo xtask verify architecture` passes (246 + 17). `cargo fmt --all
  --check` passes. Rustdoc passes; `verify docs` then stops because `mdbook`
  is not installed here. `verify lint`'s clippy stage has 77 findings, all in
  `rumoca-bitcode` and all present at HEAD `c8f0254f` with an identical
  per-file distribution — see
  `docs/toolbugs/TOOLBUG-032-the-lint-gate-has-never-been-green.md`. Ten
  findings attributable to earlier commits in this session were fixed.
- §9 greps: `observations` absent from `crates/rumoca-bitcode/src/schema.rs`
  and `crates/rumoca-ir-solve/src/execution.rs`; no `serde_json::Value`, free
  text operator, role or causality in a wire type. The derived types carry no
  `Serialize`/`Deserialize` at all, so D1 holds by construction rather than by
  inspection.
