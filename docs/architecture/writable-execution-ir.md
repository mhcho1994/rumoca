# Writable RBC execution programs

This milestone implements the contract in `new_inst/STUDENT_INSTRUCTIONS.md`.
The starting inventory is `new_inst/IMPLEMENTATION_INVENTORY.md`.

This is the **execution IR** half of the two-IR design; the division of
responsibility, and the rules that keep a derived program consistent with the
equations it came from, are specified in
[SPEC_RUMOCA_BITCODE.md §2a](../SPEC_RUMOCA_BITCODE.md#2a-two-irs-equation-and-execution).

Equation RBC remains the canonical public model representation. The executable
extension has its own version, derivation digest, lowering profile and revision.
An equation edit invalidates all derived execution. Verification and execution
recompute the equation digest, including edits through `raw_model`. Re-lowering
and replay are explicit operations.

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

## Public execution v1 profile

`RbcFile.execution` is optional, alongside (not inside) the equation model.
It carries `version`, `equation_digest`, `lowering`, `revision`, replayable
`passes`, `numerical`, `functions` and `sinks`. Old equation-only RBC remains
supported. The public profile is `solve-scalar-v1`, independent of the private
Solve schema version. The source owner `numerical.source_name` identifies the
generated executable instructions; original connector provenance stays in the
equation model's source table.

Numerical data consists of scalar storage declarations, residual/derivative/
initialization register programs, projection blocks and observation programs.
Supported canonical scalar operations are constants, time/Y/P loads, unary and
binary operations, comparisons, selection and a final output store. Operator
names and arithmetic semantics are the existing Solve vocabulary. A row has
one output. Scalar constructors check registers and output ownership; the
adapter additionally checks storage bounds, projection coverage and supported
types. Refresh certificates, Jacobians and other solver artifacts are rebuilt
from these **edited programs**, not from equations. No private wire is embedded.

The first numerical profile accepts scalar real parameters, states and
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

Lifecycle functions are ordered instruction regions. Snapshot loads produce real
values, integer sequence IDs or string phase values. `compute` calls the same
checked Solve scalar evaluator with explicit numeric arguments. `if` selects one
region; `call` invokes a named acyclic helper such as `publish:check`. Values are
function-local and branch-local. Helpers declare their lifecycle in the prefix;
cross-lifecycle calls, recursion, undefined/redefined values, wrong types and
inconsistent branch resource states are rejected. `assert` is an ordered check.
This first profile has no loops or returning helper-call values.

`csv.open`, `csv.write_row`, `csv.close` are ordered effects. Open belongs to
`run_start`, write to `publish`, close to `run_finish`. Every declared sink must
be opened and closed exactly once on each normal path. There is no lifecycle
arithmetic optimizer that could erase/move those instructions. Numerical
reconstruction never rewrites lifecycle regions. Intentional pass edits may
change effects; physical equivalence is not a validation condition.

CSV sinks declare ordered column types, relative filenames and semantic
metadata. The runtime does not enumerate connectors or infer sink membership.
It executes only the serialized instructions. Filenames are single path
components, opened exclusively inside a newly created trace directory. CSV
uses escaping and locale-independent round-trip real formatting. The manifest
identifies the equation digest, execution revision and all sink declarations.

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

The CLI hashes canonical typed equation-model serialization, not container
encoding, so JSON/CBOR conversion preserves freshness. This SHA-1 change detector
is not a security signature. Every execute/check boundary recomputes it. Public
raw writes are included. Unknown public execution fields/operations fail decode.
`compile-bitcode` refuses executable containers so it cannot accidentally
discard edits. Text/DAE round-trip commands refuse metadata they cannot preserve;
JSON/CBOR container conversion remains lossless.

`Program.relower(replay={name: implementation})` requires an explicit recipe
implementation for every applied pass and rechecks observation ID/name identity.
Removed/reassigned targets or missing recipes fail rather than losing coverage.
Raw numerical edits are not replay recipes: keep them in a named pass if they
must survive re-lowering. Saving changed execution data advances its revision.
Editing numerical programs does not pretend to invert them back into equations.

Run-local `bitcode run --param NAME=VALUE` and `--initial NAME=VALUE` overrides
preserve the saved numerical program. Parameters must be tunable retained
storage with no frozen dependent binding/start/nominal; initial values may only
change unconstrained state seeds, never bypass explicit initialization rows.
Refusals require intentional authoring/re-lowering, not silent equation edits.

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
