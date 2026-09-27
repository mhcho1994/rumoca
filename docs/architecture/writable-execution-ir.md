# Writable RBC execution programs

This milestone implements the contract in `new_inst/STUDENT_INSTRUCTIONS.md`.
The starting inventory is `new_inst/IMPLEMENTATION_INVENTORY.md`.

This is the **execution IR** half of the two-IR design; the division of
responsibility, and the rules that keep a derived program consistent with the
equations it came from, are specified in
[SPEC_RUMOCA_BITCODE.md §2a](../SPEC_RUMOCA_BITCODE.md#2a-two-irs-equation-and-execution).

Equation RBC remains the canonical public model representation. The execution
extension has its own version, dependency digest, lowering profile and revision.
An edit to an identity the program *references* invalidates it; an edit
elsewhere in the equations does not. Verification and execution recompute the
digest, including edits made through `raw_model`. Re-lowering and replay are
explicit operations.

Public executable programs project existing Solve operations. Rust checked
construction and capability checks remain authoritative; Python authors records
and invokes that validator. The SDK does not implement a numerical solver.
Lifecycle functions add ordered host effects to the same executable artifact.
CSV resources contain declarations, never live file handles. Numerical operations
continue to execute through Rumoca's Solve evaluator and native integration.

Connector construction owns both semantic identity and connection equations.
A finalized connection set emits potential equalities and one signed sum per
flow member. Adding overlapping finalized sets is rejected rather than producing
pairwise flow equations. Scalar connector instances have artifact-local IDs,
component owners, ordered typed members, orientation and provenance. Observation
targets are registered before lowering; inability to reconstruct one is an error.

Publication occurs at initialized and settled runtime output points. A sample is
an observation, never a request to alter the numerical trajectory. Event-left
diagnostic rows are not default logger publications. Settled events replace a
coincident periodic publication. A publication sequence ID identifies each row.
I/O failures abort execution with cleanup of every opened resource. A nonempty
trace destination is rejected; earlier runs are never overwritten implicitly.

This extends the public RBC contract and the executable ownership of
SPEC_0007 stage 4; it does not create a new canonical numerical IR stage.
Representation checks do not require preservation of the original equations'
physical behavior. Explicit execution edits may intentionally change it.

## Public execution v2 profile

`RbcFile.execution` is optional, alongside (not inside) the equation model. It
carries `version`, `lowering`, `dependency_digest`, `revision`, replayable
`passes` and one `program`. Old equation-only RBC remains supported. A v1
artifact is rejected with `EX2-001`: there is no adapter for a superseded wire
version (SPEC_0007).

**The numerical program is not on the wire.** Storage declarations,
residual/derivative/initialization rows, projection blocks and observation
programs are *derived* at every load from the equation IR and the recorded
`lowering` profile. v1 serialized them, which published one lowering's output
for one backend and let a caller edit a register program the equations no
longer implied. They are now a cache, rebuilt by
`rumoca::bitcode_execution::derive`, and an artifact that mentions them does
not parse.

`lowering` is a compiler-known **named** profile, not a free-text string: the
only value is `solve-scalar-v1`, and it expands to normalized fields
(`name`, `backend_family`, `scalar_only`) that the digest hashes. A profile
cannot be invented on the wire, and adding a field to the expansion
invalidates dependent programs, which is the intended behaviour.

`passes` is a list of receipts — `{id, version, options}` — with typed options
(`real`, `integer`, `boolean`, `text`); an unknown option kind is rejected at
parse rather than carried as an opaque blob. A receipt identifies a pass by a
canonical path, never by a `VariableId` or `TracePointId`, because it must stay
readable across a recompilation that renumbers ids.

The first derivation profile accepts scalar real parameters, states and
algebraics, identity mass matrices, and explicitly solved Y initialization.

Tensor storage, streams, external/pure-call frames, tearing, manifold projection,
discrete/clock/event owners and unsupported initialization forms are rejected.
This is an explicit profile limit, not a claim that Rumoca's existing backends
lack those features. Runtime publication itself is event-aware and is tested on
an actual ME scheduled event, separately from this event-free public adapter.
Other executable backends/codegen targets fail explicitly; RK45 is the delivered
native backend. Ordinary equation-only workflows keep their existing support.

## Ownership and effects

| Owner | Responsibility |
|---|---|
| `rumoca-ir-solve::execution` | Public profile, operation vocabulary, scalar and lifecycle validation |
| `rumoca-phase-solve::execution` | Projection from Solve and checked reconstruction, including derived refresh/AD artifacts |
| `rumoca-eval-solve::execution` | Lifecycle interpreter and generic CSV resources; arithmetic delegates to the existing evaluator |
| `rumoca-solver::fmi_me` | Proves publication coordinates/roles and performs same-time deduplication |
| `rumoca-sim::execution` | Composes checked FMI component, native effect interpreter and existing RK45 master |
| RBC CLI / Python SDK | Container freshness, editable records, insertion/replacement/removal and explicit replay |

Lifecycle functions are ordered instruction regions. Snapshot loads produce
real values, integer sequence IDs or text phase values. `snapshot.value` names
a `TracePointId` — never a `VariableId` and never a storage index.

`compute` evaluates a node of the function's **own** expression arena into a
declared local. The arena is program-local and separate from
`RbcModel.expressions`, because an execution pass must never write into the
equation IR; one vocabulary, two arenas. Operands name earlier nodes, so the
arena is acyclic by construction, and the only non-literal leaf is a declared
local. There is no `VariableId` leaf, so the code reserved for "a program
expression names a model variable" is documented and left unallocated: the
shape is unrepresentable rather than rejected.

Operators are the Solve enums (`UnaryOp`, `BinaryOp`, `CompareOp`) on the wire,
not strings, so a misspelling is a deserialization error rather than a runtime
one.

`if` selects one region; `call` invokes a named helper such as
`publish:check`. Locals are **function-scoped**: a function declares every
local it uses, and `if` carries no declarations of its own. A branch may assign
a local its function declared, but a local assigned on only one arm is not
assigned after the join.

Helpers declare their lifecycle in the prefix; cross-lifecycle calls, undefined
or redefined values, wrong types and inconsistent branch resource states are
rejected. `assert` is an ordered check. There are no loops and no returning
helper-call values: termination is by acyclicity of the call graph
(`EX2-030`), so every program halts in a number of steps bounded by its own
text.

### Diagnostic codes

| Code | Meaning |
|---|---|
| `EX2-001` | Artifact declares a version this reader does not implement |
| `EX2-010` | A `csv.*` effect names a sink the program does not declare |
| `EX2-011` | The profile expands to a target that performs no file effects (*currently unreachable*: every profile sets `file_effects`) |
| `EX2-012` | An effect is in a lifecycle phase that cannot perform it, or names a resource not open there |
| `EX2-013` | A written row does not have one value per declared column |
| `EX2-014` | Two sinks share a key or a filename |
| `EX2-015` | A sink filename is not one relative, non-reserved path component |
| `EX2-016` | A sink declares no columns |
| `EX2-020` | Read of a local that is not declared, or not yet assigned |
| `EX2-021` | Value of the wrong type for its position |
| `EX2-022` | `Text` used outside a CSV column or an assert message |
| `EX2-023` | *Reserved, unallocated* — inner-scope declarations are unrepresentable |
| `EX2-024` | *Reserved, unallocated* — a `VariableId` expression leaf is unrepresentable |
| `EX2-030` | The call graph contains a cycle |
| `EX2-031` | A helper's name carries no lifecycle prefix the reader knows |
| `EX2-032` | A call reaches a helper in a different lifecycle phase |
| `EX2-033` | A snapshot read outside the publish phase, where there is no snapshot |
| `EX2-034` | The two arms of an `if` leave different resources open |
| `EX2-040` | An instruction or sink references a trace point the model does not declare |

A caller matches on the code, never on message text.

`csv.open`, `csv.write_row`, `csv.close` are ordered effects. Open belongs to
`run_start`, write to `publish`, close to `run_finish`. Every declared sink must
be opened and closed exactly once on each normal path. There is no lifecycle
arithmetic optimizer that could erase/move those instructions. Numerical
reconstruction never rewrites lifecycle regions. Intentional pass edits may
change effects; physical equivalence is not a validation condition.

A `CsvSink` is an **opaque capability handle**, not an operation. It declares a
key, a relative filename, ordered `{name, ty}` columns — paired, so a count
mismatch between two parallel lists is unrepresentable — and identity-only
metadata: `{connector, orientation, members: [{trace_point}]}`. No path, unit,
role or causality appears on the wire; those belong to the equation IR, and a
second copy could diverge from the first.

The validator reports an undeclared sink id (`EX2-010`) separately from a
target that cannot write files at all (`EX2-011`): they have different fixes.
`EX2-011` is answered by the lowering profile's `file_effects` expansion
field, not by counting declared sinks — "the program declares no sinks" is
the `EX2-010` question in other words.

The runtime does not enumerate connectors or infer sink membership. It executes
only the serialized instructions. Filenames are single path components, opened
exclusively inside a newly created trace directory. CSV uses escaping and
locale-independent round-trip real formatting.

`manifest.json` is an **emitted** artifact, not a wire one, so it is the place
where identities are resolved: it carries the `dependency_digest`, the
execution revision, and for each sink its columns plus a `connector_path` and
per-member `{trace_point, name, unit, kind}` joined in from the derived
program. That split is what lets an independent oracle such as
`new_inst/check_thermal_csv.py` check paths and units without importing Rumoca.

## Publication and failure behavior

The host retains one pending row until the next distinct coordinate, allowing a
settled event value to replace initialization, an event-left limit or a nominal
sample. It never sends event-left rows to the executable interpreter. Normal
completion drains the final pending row exactly once. Phases are `initial`,
`sample` (consistent periodic output) and `settled` (after event iteration).
Sequence IDs start at zero and are shared by all sinks in one publication.
Callbacks exist only between native owners; there are no runtime Python hooks.

Publication uses existing ME getter/reconstruction transactions and does not
set the integrator's history or force additional physical events. On failure,
unpublished pending evidence is not promoted to a valid snapshot. The effect
owner runs `run_finish` and flushes/closes remaining handles even if a finish
instruction itself fails. Normal I/O errors are returned; secondary cleanup
errors are reported on stderr. Process termination/power failure is not covered.

## Derivation and edits

`dependency_digest` is a SHA-1 change detector, not a security signature, and
it covers exactly what the program depends on:

| Hashed | Why |
|---|---|
| `RBC_VERSION`, `EXECUTION_VERSION` | a wire change invalidates every program |
| the expanded `lowering` profile | a different derivation is a different program |
| per referenced trace point: id, component path, type **contents**, causality tag, unit | the identities `snapshot.value` resolves against |

A type is hashed by its contents — scalar kind, dimensions and, for a record,
its name and fields, recursively — never by its `TypeId`. Type ids are
per-compilation, so hashing the number would make the digest depend on how
one run laid out its type table, and would miss a retype that lands on the
same id. Causality is hashed as an explicit per-variant byte tag rather than
`Debug` output, which is not a contract anyone promised to keep.

It does **not** hash the equations. Rewriting a residual the program does not
observe leaves it runnable, which is the point: v1 hashed the whole equation
model, so any unrelated edit forced a re-lower and threw away execution-level
work. The component path is encoded as two length-prefixed records rather than
a dotted string, so a component named `a.b` cannot collide with `a` owning `b`.

Because the digest names the identities a program *references*, it is only
knowable once a pass has emitted its instructions. It is derived, so it is
recomputed by re-lowering rather than hand-maintained: `lower-execution` is
idempotent on an artifact that already carries a program, and a pass refreshes
the digest at its own boundary — which is what `with program.builder(...)`
does in the SDK. Neither `validate` nor `save` refreshes it, because refreshing
there would re-derive against whatever the model has since become, and nothing
would ever be stale.

Every execute/check boundary recomputes it. Public raw writes are included.
Unknown public execution fields and operations fail decode. `compile-bitcode`
refuses executable containers so it cannot accidentally discard edits. The
text profile refuses an artifact whose program it cannot represent, rather than
writing a lossy listing that would assemble back into a different artifact;
`bitcode disasm` prints the program as a human listing, and JSON/CBOR container
conversion remains lossless in both directions.

`Program.relower(replay={id: implementation})` requires an explicit recipe
implementation for every applied pass and rechecks that every trace point the
program references still exists. Removed or reassigned targets, and missing
recipes, fail rather than losing coverage. Replay rebuilds the program from
the passes; the previous program is not copied first, or every pass would
append a second time. Raw program edits are not replay recipes: keep them in a
named pass if they must survive re-lowering. Saving changed execution data
advances its revision.

Run-local `bitcode run --param NAME=VALUE` and `--initial NAME=VALUE` overrides
resolve through the derived program's `variable_id -> storage slot` map, not by
matching storage by name. Parameters must be tunable retained storage with no
frozen dependent binding/start/nominal; initial values may only change
unconstrained state seeds, never bypass explicit initialization rows. Refusals
require intentional authoring or re-lowering, not silent equation edits.

`--domain-diagnostics` selects the native interpreter and writes bounded
internal numerical evidence separately from publications. The evaluator owns
reached operand faults; the solver owns step, event and projection telemetry.
These internal coordinates are not valid physical trace samples. See the
[ModelSan integration guide](../modelsan-bitcode-integration.md#native-diagnostics)
for identity, coverage, replay and diagnostic limits.

The scalar equation removal transaction rejects retained references and
unsupported structured/event owners, compacts all affected IDs and dead
expressions, then commits only after checked import. Returned remapping tables
let later equation passes update their handles. Existing derived Programs
continue to point at the changed model and consequently become stale.
