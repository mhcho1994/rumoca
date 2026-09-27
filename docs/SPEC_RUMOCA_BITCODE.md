# Rumoca Bitcode v2

**Status:** implemented, v2.
**Scope:** the public interchange format for a compiled Modelica model.

Rumoca Bitcode (`.rbc`) is a versioned, machine-readable representation of a
compiled Modelica model, intended for analysis and transformation by tools
outside the Rumoca source tree.

A tool that reads or writes `.rbc` needs no Rumoca checkout, no Rust, no
matching compiler version, and no linkage against Rumoca's crates. **The file
format is the interface.**

---

## 1. What this is not

Bitcode is deliberately **not** Rumoca's internal DAE serialization.

| | internal wire | Rumoca Bitcode |
|---|---|---|
| Produced by | `Serialize for Dae` (`--emit dae-json`) | `rumoca-bitcode::export` |
| Version | `DAE_SCHEMA_VERSION`, currently 33 | `RBC_VERSION`, currently 2 |
| Compatibility | single version only; SPEC_0033 requires the old reader be deleted on every change | versioned public contract |
| Audience | the compiler | external tools |
| Stability | changes freely | changes only when the public contract changes |

This separation is the point. Rumoca may reorganise its Rust structs, rename
internal types, renumber internal ids, or restructure its crates without
touching `RBC_VERSION`. Conversely, a bitcode version change is a statement
about the public contract, not about compiler internals.

The two must not be conflated: publishing the internal schema would freeze
Rumoca's representation and create a plugin-versus-compiler upgrade deadlock.

## 2. Container

```
RbcFile
  magic            "RUMOCA-RBC"
  bitcode_version  2
  producer         e.g. "rumoca 0.10.0"   (informational only)
  model            RbcModel                 -- equation IR, always present
  execution        Option<ExecutionArtifact> -- execution IR, optional
```

An artifact therefore carries **two** representations. They are described in
[§2a](#2a-two-irs-equation-and-execution).

A reader **must** reject a file whose `magic` differs, and **must** reject a
`bitcode_version` it does not implement. Both checks happen before any other
field is interpreted.

### Equation-artifact linking

`rumoca-bitcode::link` owns namespaced linking of this public projection;
the CLI and Python SDK delegate to it. This is not internal DAE structural
lowering (SPEC_0007) or runtime execution (SPEC_0029). See
[bitcode linking](bitcode-linking.md) for commands and the supported boundary.

A link must preserve each module's equations and explicit boundary conditions,
remap every typed identity/reference in its own table's ID space, namespace
module-local symbols, retain diagnostic provenance, and validate the result.
It must not infer wiring from names, merge equal-looking variables, or interpret
literal numbers/ordinals as IDs. Inputs remain unchanged. Existing executable
projections must be rejected unless the caller explicitly authorizes discarding
them; linking equations cannot preserve arbitrary numerical/execution edits.

Artifacts with explicit scalar connector declarations must validate complete
member/set coverage, compatible contracts, exclusive equation ownership and
the actual potential/flow equations at the native boundary (MLS §9.2;
SPEC_0022 CONN-001/002/003/005/008/026). Authoring-helper checks alone are not
sufficient. Complete-contract checking must reject legacy annotations that
lack the information needed for this proof. The bounded proof profile and its
explicit refusal conditions are documented in [connector validation](connector-validation.md).

`producer` is informational. A consumer must not change behaviour based on it.


## 2a. Two IRs: equation and execution

An artifact holds up to two representations of the same model, and they answer
different questions:

| | Equation IR | Execution IR |
|---|---|---|
| Field | `RbcFile.model` (`RbcModel`) | `RbcFile.execution` (`ExecutionArtifact`) |
| Answers | *what must hold* | *what the host does, and when* |
| Content | variables, typed expressions, residual equations, events, connection sets | ordered instructions: snapshots, typed `compute`, CSV lifecycle, `call`, `if`, `assert` |
| Presence | always | only after lowering |
| Versioning | `bitcode_version` | its own `version`, plus `dependency_digest`, `lowering`, `revision` |
| Shape | declarative and unordered; equations are a set | imperative and ordered; instructions run in sequence |

### What the execution IR is *not*

It is **not** a numerical program. Storage slots, residual rows, derivative
rows, projection blocks and observation instruction lists are **derived**: they
are rebuilt from the equation IR and the recorded `lowering` profile every time
an artifact is loaded, and none of them is on the wire.

This is the v2 correction. v1 serialized the derived program beside the
authored one, which put one lowering's output, for one backend, into a public
artifact — and let a caller edit a register program that the equations no
longer implied. v2 carries only what a pass author wrote:

```text
ExecutionArtifact
  version             wire version of this section
  lowering            a compiler-known named profile, not a free-text string
  dependency_digest   over the identities the program references
  revision            counts edits to the program
  passes              one receipt per pass that edited it
  program             HostProgram: functions and the sinks their effects name
```

Everything else a run needs is a cache, and is treated as one.

### Why two rather than one

**They are not orderings of each other.** A residual equation states a relation
that must hold at a solution. An instruction states a step that runs at a point
in time. Deriving the second from the first requires choosing an evaluation
order and a solver profile — choices the equations deliberately do not make,
because a different solver makes them differently. Collapsing the two would
bake one solver's choices into the model.

**One is authored, the other is derived.** Equation IR is the public model
representation. The host program is authored on top of it by passes. The
numerical program is derived from both and stored nowhere.

**They fail differently, and both failures matter.** An equation-level defect is
a statement about the model — an unmatched equation, a dimensional conflict, a
connector with no source. An execution-level defect is a statement about a run
— a divisor that reaches zero, an assertion that fires, a trace that stops. A
tool that had only one of these would report the other in the wrong vocabulary.

**The execution IR is statically bounded.** Not "loop-free" as a slogan: the
instruction set has no iteration, and `call` targets a call graph the validator
proves acyclic (`EX2-030`), so every program halts in a number of steps bounded
by its own text. That is what makes an externally-authored program safe to
execute — a host needs no termination proof from a pass author. Equation IR is
bounded for the same reason and by the same means (§9a).

### How they interoperate

```text
Modelica source
  -> equation IR            (RbcFile.model)          authored, canonical
  -> equation passes        (read/write RbcModel)
  -> lowering               (bitcode lower-execution) records a profile
  -> execution IR           (RbcFile.execution)      authored by passes
  -> execution passes       (read/write instructions)
  -> derivation             (at load, every load)    numerical program
  -> native run
```

The rules a consumer must observe:

1. **Staleness is about referenced identity, not about the equations.** The
   `dependency_digest` covers the wire and execution versions, the expanded
   lowering profile, and — for each trace point the program *references* — its
   id, component path, value type, causality and unit. Rewriting a residual the
   program does not observe leaves the program runnable; retyping or removing a
   variable it does observe does not. A mismatch is an error, not a re-lowering
   trigger: re-lowering is explicit, because replay discards execution-level
   edits that no pass reproduces.
2. **Linking is an equation-IR operation.** `rumoca bitcode link` must refuse an
   input that carries an execution program unless the caller explicitly
   authorizes discarding it (§2). Equations can be combined; two authored
   programs cannot.
3. **Execution IR never rewrites equations.** An execution pass may add host
   effects such as CSV logging, add instructions, or reference trace points. If
   a change would alter what must hold, it belongs in an equation pass. The two
   expression arenas are separate for this reason: `RbcModel.expressions` is the
   equation IR's, `Function.expressions` is one program function's, and a
   program expression's only non-literal leaf is a **declared local**. There is
   no `VariableId` leaf, so a program cannot read a model variable except
   through `snapshot.value`.
4. **Observations are demanded by the program, not requested at lowering.**
   `lower-execution` takes no observation list. A `snapshot.value` instruction
   names a `TracePointId`, and the numerical program is derived from exactly
   the trace points the program references. A model with no trace points and a
   program with no instructions is a **valid** artifact with zero observations.
   Registering a trace point is an equation-IR edit, so the logging helper that
   needs one creates it.
5. **A reader that understands only equation IR is valid.** `execution` is
   optional, and a consumer that ignores it still sees the whole model. A
   consumer that reads *only* execution IR does not, and must not present itself
   as having read the model.
6. **The wire names identities; the emitted trace resolves them.** A `CsvSink`
   is an opaque capability handle: it carries its member trace points, its
   connector id and orientation when it is connector instrumentation, and no
   paths, units or roles. Those are the equation IR's to state, and the runtime
   joins them in when it writes `manifest.json`. A second copy on the wire
   could diverge from the first.
7. **Structure, never flattened text.** A consumer that needs a component path
   or a member name takes it from `RbcConnectorInstance` and `RbcComponent`,
   not by splitting a variable's display name on `.`: a component named `a.b`
   and a component `a` owning `b` flatten to the same text. The digest encodes
   the path as two length-prefixed records for the same reason.

### Scoping and ordering

**Locals are function-scoped.** A `Function` declares every local it uses;
`if` carries no declarations of its own. A branch may assign a local declared
by its function, but a local assigned on only one arm is not assigned after the
join, and reading it there is `EX2-020`. Because an inner declaration is
unrepresentable, a write from outside to an inner-scoped local cannot occur;
the code reserved for it is documented and left unallocated rather than
checked.

**The program owns order, and nothing normalizes it.**

| Ordering | Owned by |
|---|---|
| CSV column order within a row | the value order of the `csv.write_row` that writes it |
| CSV row order within a file | publication sequence |
| Effect order within a function | instruction order, as written |

No validator, optimizer or writer may permute any of these.

### Which one a pass should target

Target equation IR to change what the model means: adding a component, wiring a
port, rewriting a residual, constraining a declaration. Target execution IR to
change what a run does without changing what it means: logging a signal,
inserting an assertion, computing a derived column from observed values.

If a pass finds itself encoding an equation into instructions to work around a
missing equation-level capability, that is a gap in the equation IR and should
be reported as one. The derived representation is the wrong place to introduce a
fact the canonical one does not hold.

## 3. Encodings

The same schema is carried by two encodings:

| Encoding | Use | Detection |
|---|---|---|
| **CBOR** | production, default | any first non-whitespace byte other than `{` |
| **JSON** | debugging, diffing, hand-editing | first non-whitespace byte is `{` |

Encoding is detected on read, so a consumer is never told which it was handed.
`rumoca bitcode convert` moves between them; `rumoca bitcode dump` prints any
artifact as JSON.

CBOR is map-keyed and self-describing, so a field a reader does not recognise
is skipped rather than shifting every field after it. This is a deliberate
contrast with an ordinal-tagged codec, where inserting an enum variant silently
changes how older payloads decode — a class of bug Rumoca's own wire tests
exist to catch.

## 4. Versioning

`RBC_VERSION` is the public contract version. It is independent of every
internal schema.

**Compatible** (no version bump):
- adding an optional field
- adding a new enum variant with an explicit `kind` tag
- adding a new collection

**Incompatible** (bump `RBC_VERSION`):
- removing a field
- renaming a `kind` tag
- changing a field's meaning

Every enum is externally tagged by an explicit `kind` string, never by
declaration order, so appending or reordering variants in the producer cannot
change how an existing file decodes.

The implementation reads and writes exactly version 2. Version 1 is rejected;
recompile the source to create a current artifact. Rewriting the header is not
a migration: the former payload-free `clock` condition lost the schedule,
identity and ownership information required for faithful replay. Version 2
replaces it with `clock_activation` referencing the exact `clocks` table and
preserves `clock_ownerships`, including sampled ownership. No superseded
payload-free clock reader is retained.

`execution.version = 1` is a separate numerical-program contract. It remains
unchanged and event-free. Equation artifacts containing supported clocks can
be rebuilt and simulated with `compile-bitcode --simulate`; they do not acquire
support in the numerical `bitcode run` adapter.

## 5. Identity

Every object carries an explicit `id` field. IDs are dense and ordered within
each collection, but a consumer reads the `id` rather than assuming array
position is identity.

| Property | Guarantee |
|---|---|
| Unique within one artifact | **yes** |
| Stable across two compilations of the same source | **no** |
| Stable across compiler versions | **no** |
| Derived from a rendered name | **never** |

Expressions reference variables through typed *coordinates*
(`state`, `derivative`, `parameter`, `pre_state`, …), never by string. A pass
must not parse pretty-printed names to recover identity.

For cross-artifact comparison — diffing two versions of a model — match on
`name` + `role` + `type` + `unit`, not on ids. Two independent compilations
allocate ids independently.

## 6. Provenance

Every variable, expression, equation, relation, condition, root, event and
connection carries:

```
provenance
  origin   "source" | { "generated", generation }
  span     { source, start, end, line, column }
```

`line` and `column` are 1-based and precomputed at export time, so a pass can
report `Motor.mo:52` without the source text. `start`/`end` are byte offsets
and are authoritative.

`generation` names the lowering kind for generated objects —
`connection_equation`, `flow_balance_equation`, `binding_equation`,
`index_reduction`, and so on — so a consumer distinguishes them without
guessing from shape.

**A span is not a unique key.** An equation inherited through `extends` appears
once per instance with the same span. Use `id` for identity and provenance for
explanation.

Sources may embed their full text (`--emit-bitcode` does by default), which
makes the artifact self-contained and lets a pass print the exact source of any
object.

## 7. Model contents

> **The per-type reference is [bitcode-reference.md](bitcode-reference.md)**,
> generated from `schema.rs` by `tools/bitcode/gen_reference.py` and checked
> for staleness in CI. This section is the narrative: what the major tables
> are for and how they relate. It stopped tracking the schema once — 51 of 56
> types had never appeared in it by name — which is why the reference half is
> now a build product.


```
RbcModel
  name                model name as compiled
  sources[]           files, optionally with text
  types[]             scalar kind + array dimensions
  variables[]         every variable, in every role
  expressions[]       flat arena, topologically ordered
  equations[]         continuous residuals: residual == 0
  initial_equations[] initialization residuals
  relations[]         primitive comparisons that can generate events
  conditions[]        boolean activation algebra
  roots[]             zero-crossing surfaces
  events[]            reinitialize / assert / terminate
  time_events[]       scheduled events
  components[]        component instances
  connections[]       connector-level provenance
  trace_points[]      observation requests (added by passes)
  summary             denormalised counts
```

### Variables

Carry `role` (the Appendix B partition: state, parameter, constant, input,
algebraic, output, discrete real, discrete value) and `causality` (the
interface annotation) **separately**, because they are orthogonal. Also carry
type, scalar count, unit, description, `start`/`binding`/`min`/`max`/`nominal`
expressions, `fixed`, `tunable`, and connector semantics when the variable is a
connector member.

### Expressions

A flat arena in topological order: **every operand references a strictly lower
`ExprId`**. This makes the arena a DAG by construction, lets a consumer build
the tree in one forward pass, and makes a cycle unrepresentable rather than
something to detect.

The current schema represents literals, coordinates, unary and binary operators, and
conditionals. Anything else is recorded as `unsupported` with a detail string.
A consumer that requires completeness must treat an `unsupported` node as "this
artifact does not fully describe the model", never as a default value.

### Equations

Residual form: the model asserts `residual == 0`. Each equation carries
`reads` and `reads_derivative` — the variables it depends on, computed by the
compiler's own dependency projection, which resolves function calls and runtime
array selection correctly. **A consumer should use these rather than walking
the expression tree**, which would produce a weaker answer.

### Connections

A connection is an equality between two connector endpoints plus the
conservation law over the flow members of its connection set. It is **not** a
directional message: `left` and `right` are symmetric.

`quantity` is `potential` (equated across the connection),
`flow` (signed sum is zero) or `stream`. Preserving this distinction is the
reason connections are modelled at all rather than left as raw equations.

### Trace points

Observation requests: a variable, a label, optionally the connection and
physical quantity it belongs to, and the tool that added it.

Trace points are **observation metadata, deliberately separate from the
physical equations**. Adding one cannot change what the model computes, which
is why instrumentation does not require a pass to rewrite equations. A
transformation pass that genuinely needs to change behaviour edits equations
instead; trace points are the narrow, safe path for the common case.

### Trace output

`--simulate --trace-out FILE` writes one row per trace point per output time:

```
time,trace_id,connection,variable,quantity,unit,value
0.1,1,inertia.b <-> spring.a,inertia.b.tau,flow,N.m,4.272051
0.1,2,inertia.b <-> spring.a,spring.a.tau,flow,N.m,-4.272051
```

The structured form is the contract; the table `--simulate` prints without
`--trace-out` is one presentation of it. A trace point naming a variable the
solver does not report is warned about, never silently dropped.

### Summary

Denormalised counts. A reader checks them against the actual collections, so a
truncated or carelessly hand-edited artifact fails loudly. A pass that edits a
model must recompute the summary before writing; the Python SDK's `save()` does
this automatically.

## 8. Validation

An artifact that has been through an external pass is **untrusted**. Import
runs two gates:

```
.rbc
  ↓ decode          rejects malformed bytes, foreign magic, unknown version
  ↓ validate        references, dense ids, duplicate names, topological order,
  ↓                 summary agreement, unsupported nodes
  ↓ construct       the DAE's own checked constructors
checked DAE
```

The second gate is the important one. Import does not fill in structs; it
issues the same checked construction operations the compiler uses when
compiling from source. All 70 of the DAE's construction invariants therefore
apply to imported bitcode exactly as they apply to a freshly compiled model.

**An invalid external pass cannot produce an invalid DAE — only a rejection.**

Validation returns *every* problem it finds, not the first, so a pass author
fixes one round of errors rather than playing whack-a-mole.

## 9. Command-line interface

```bash
# export
rumoca compile Model.mo --model Model --emit-bitcode model.rbc
rumoca compile Model.mo --model Model --emit-bitcode model.json --bitcode-format json

# examine
rumoca bitcode inspect model.rbc
rumoca bitcode dump model.rbc                 # JSON, whatever the on-disk encoding
rumoca bitcode check model.rbc                # validate without rebuilding
rumoca bitcode convert model.rbc -o model.json --format json   # lossless
rumoca bitcode round-trip model.rbc           # prove import/export fidelity

# import
rumoca compile-bitcode model.rbc --summary

# import, simulate, and emit the trace points an external pass requested
rumoca compile-bitcode model-traced.rbc --simulate --t-end 1.0
rumoca compile-bitcode model-traced.rbc --simulate --t-end 1.0 --trace-out traces.csv
```

A full external transformation:

```bash
rumoca compile Motor.mo --model Motor --emit-bitcode motor.rbc
python connector_logger.py motor.rbc -o motor-traced.rbc
rumoca compile-bitcode motor-traced.rbc --simulate --t-end 1.0 --trace-out traces.csv
```

## 8a. Producing an artifact

A consumer that is not the Rumoca compiler can write one.
`rumoca_bitcode::build::Builder` in Rust and `rumoca_bitcode.Builder` in
Python append to a model, or start from `Model.empty(name)`, and hold the
three invariants that are cheap to break and expensive to find: the arena's
topological order, dense ids, and generated provenance on everything a tool
adds. `docs/writing-a-bitcode-pass.md` has the worked examples.

Until they existed the only supported write operation was `add_trace_point`,
which made this an export format that called itself an interchange format.

## 9a. Computational power

**Rumoca Bitcode is not Turing complete, and the analyses built on it depend
on that.** A witness search, an interval propagation and a maximum-flow
matching all terminate without a step budget because the artifact they read
cannot express unbounded iteration. Three properties give that, each enforced
rather than conventional:

**The expression arena is a DAG, and a cycle is unrepresentable.** Validation
requires every operand id to be strictly less than its node's id, so an
artifact holding a cycle is rejected before reconstruction and evaluation is
one forward pass. Not "no cycle has been seen": a cycle cannot be written down
and be valid.

**There is no control flow.** The node set has no assignment, no jump and no
loop. The only iteration is `Comprehension` over a `DomainId` and
`RbcEquationFamily` over extents the artifact carries as constants, so every
trip count is known before evaluation starts.

**Function bodies are not carried.** `RbcFunctionBody` is `ElidedModelica` or
`External`; neither holds a body, so recursion is not expressible.

Every artifact therefore denotes a finite system of equations over a finite
index space, and every quantity it can express is computable in bounded steps.

The **execution IR** is bounded separately and by a fourth means: it has no
iteration at all, and its `call` graph is checked acyclic (`EX2-030`). So a
host program halts in a number of steps bounded by its own text, and a host
needs no termination proof from the pass author who wrote it (§2a).

### The system it denotes is another matter

The artifact describes a residual function and an event structure. *Running*
it — integrating over time with `pre` state, events and `reinit` — is a hybrid
dynamical system, and those encode Turing machines.

[`Minsky.mo`](../crates/rumoca-bitcode/examples/Minsky.mo) is a two-counter machine written in Modelica and compiled to a
valid strict artifact of **6 variables, 1 equation and 52 expressions**. Its
step count grows with its input: `seed = 1` halts after 4 steps, `seed = 7`
after 22, `seed = 20` after 61, and `seed = 40` has not halted at the horizon.
A fixed, finite, total artifact; an unbounded computation.

So:

| Question | Status |
|---|---|
| does evaluating this expression terminate | yes, always, in one pass |
| does this static analysis terminate | yes, and that is why none carries a budget |
| does this model ever divide by zero when run | **undecidable** |

The third is why the divisor analysis is three-valued rather than a predicate,
and why `divisor-zero-unresolved` exists. It is forced by the semantics, not a
convenience.

### Three holes in the totality claim, named

- `RbcFunctionBody::External { language, symbol }` — the artifact can call
  arbitrary foreign code. Totality is a property of what the IR carries, not
  of what running it does.
- `RbcFunctionBody::ElidedModelica` — the body exists and is not here, so any
  analysis that needs to look inside a function is working with a hole, and a
  `Call` node is opaque to it.
- `RbcExprNode::Unsupported` — explicitly "this schema version cannot
  represent it"; a consumer must treat the model as not fully understood.

The argument is machine-checked in `crates/rumoca-bitcode/src/tests.rs`: an
exhaustive `match` with no wildcard classifies every node kind, so adding one
fails to compile until somebody classifies it, and removing an arm was
confirmed to produce `E0004` rather than a silent default. Two further tests
assert that a self-referencing operand and a forward reference are both
rejected.

## 10. Equation transport support and limits

Version 2 carries exact periodic schedules (including negative phase and the
absolute/simulation-start anchor), triggered clocks, activation identities and
discrete-variable ownership. Decimal strings preserve each clock rational's
128-bit numerator and denominator in JSON and CBOR. Checked import validates
positive periods, references, ownership roles and sampled ownership.

The predefined `String` conversion is a closed semantic expression tag. Its
value, formatting options and format-string operand remain typed expression
references. Import registers a fresh local predefined declaration and uses the
checked DAE constructor; it does not infer builtins from user function names or
transport a source compiler's declaration IDs.

| Limit | Effect |
|---|---|
| Modelica function bodies remain elided | Signatures are inspectable, but calls cannot be rebuilt without the body. |
| Previous/clock-transfer/delay/terminal operations are not represented | Unsupported expressions refuse on import; unrepresented semantic owner tables refuse export. |
| Model-event transactions and structured roots are not represented | Export refuses rather than discarding their semantic ownership. |
| Type aliases collapse on import | `Voltage` and `Current` both become `Real`; alias identity is not retained. |
| Native numerical `execution.version = 1` remains scalar-real and event-free | Transporting an equation artifact does not expand the separate execution adapter. |
| `--target` from bitcode covers `ir = "dae"` only | Flat/AST/Algorithm-Code and FMI packaging need artifacts or session state this projection does not carry. |

Arrays, records and enumerations have typed representations. Every unsupported
expression remains explicit, and import refuses it. An artifact's successful
export alone does not prove that it can be imported or executed.

## 10a. Unknown-field preservation

Raw `dump` and `convert` preserve unknown fields in the document. The Python
SDK preserves unknown fields within the current version when saving edits.
Typed import rebuilds only the current schema and rejects unknown variants;
all semantic readers reject unsupported version headers. Unknown-field
preservation does not authorize interpreting another contract version.

## 11. Connector provenance: what was recovered, and what was not

The DAE retains **no** connector structure. Of the original `connect(...)`, only
a `connection_equation` / `flow_balance_equation` tag survives, anchored to the
connector member declaration.

The exporter recovers connections at export time by joining the DAE with the Flat model,
which retains per-equation `EquationOrigin::Connection { lhs, rhs }` and
per-variable `flow` / `stream` / `connected` flags. That is enough for
endpoints, flow-versus-potential classification, connector membership, and
component grouping — enough for the connector-logging application.

Connection spans now point at the `connect(...)` statement. That required one
fix in the compiler: `build_connection_sets` recorded the connect span only for
*direct* endpoints, so a connector-level `connect(a.p, b.p)` left its expanded
members (`a.p.v`, `a.p.i`) to fall back to their own declaration — the connector
*type*. Recording the span for each expanded member closes it, and the struct's
own doc comment already said this was the intent.

What is still lost: the connection-set grouping for sets with more than two
members, the inside/outside sign, and the connector type's `DefId`.
`ConnectionSet` and `ConnectionGraph` exist in `rumoca-ir-flat` but are built
and discarded inside `rumoca-phase-flatten/src/vcg.rs`; they are not fields of
`flat::Model`. The schema is already shaped for the richer data, so extending it
would be additive.
