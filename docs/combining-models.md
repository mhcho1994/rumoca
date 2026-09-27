# Combining two models: linker, IR builders, and passes

Start here for an end-to-end example. The [linker reference](bitcode-linking.md)
describes relocation details; this guide shows how to build two components,
save them separately, link them, connect their ports, instrument and run them.

The runnable source is [compose_models.py](../new_inst/compose_models.py).
It uses the current public Python SDK and native Rumoca executable, not a
Python numerical solver. No Modelica source or MSL download is needed.

## Who does what?

| Part | Responsibility | Does not do |
|---|---|---|
| Linker: `Model.link` / `rumoca bitcode link` | Combine equation artifacts; prefix namespaces and relocate IDs/references | Choose connections, remove boundaries, or merge solver programs |
| Equation IR builder: `model.builder(...)` | Author variables, expressions, residual equations, ports and connection sets | Solve the model or establish arbitrary physical correctness |
| Native lowering: `execution.lower(...)` | Record a lowering profile and seed the lifecycle functions | Take an observation list, or serialize a numerical program |
| Execution IR builder: `program.builder(...)` | Author host instructions and ordered lifecycle effects such as CSV logging | Edit the derived numerical program, or run Python callbacks during simulation |
| Pass management | Explicit equation-pass ordering, plus named execution-pass recipes and explicit replay | Automatic dependency scheduling or LLVM-style cached-analysis invalidation |

The two builders operate at different levels. Equation IR describes **what must
hold**; execution IR describes **how numerical evaluation and host effects run**.
A pass is authoring code that reads or transforms one of those representations.

```text
hot.rbc + cold.rbc
        -> link (independent equations, relocated IDs)
        -> wire open ports (connection equations)
        -> equation passes
        -> validate + lower, preserving requested observations
        -> execution passes
        -> save executable RBC -> native run
```

## 1. Run the complete example

From the repository root, with the current CLI built:

```bash
CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 RAYON_NUM_THREADS=4 \
  cargo build -p rumoca --bin rumoca --offline
export PYTHONPATH="$PWD/packages/rumoca-bitcode:$PWD/new_inst"
export RUMOCA="$PWD/target/debug/rumoca"
python3 new_inst/compose_models.py --out build/composition-demo --run
```

Use a **new output directory** on each invocation. The example refuses an
existing directory. Omit `--run` to author/check the artifacts without simulation.

| Output | Meaning |
|---|---|
| `hot.rbc`, `cold.rbc` | Independent, intentionally open equation modules |
| `01-linked-open.rbc` | Namespaced union; not yet a closed simulation problem |
| `02-connected.rbc` | Ports connected and native connection contracts checked |
| `03-equation-pass.rbc` | Added heat-integral monitor |
| `04-executable.rbc` | Recorded lowering profile plus the authored CSV instructions |
| `traces/manifest.json`, `traces/connector-*.csv` | Native runtime metadata and one CSV per port |
| `result.json` | Native simulation result, including both body temperatures and heat integral |

The physical example is two heat capacities, initially at 350 K and 300 K.
The hot module includes a thermal contact of conductance 1 W/K between its
internal temperature and its public port. The cold module's port temperature
equals its internal temperature. Capacities are 2 and 3 J/K respectively.

**A connector is not a resistor/conductor.** Connecting ports imposes ideal
equality/conservation laws. The finite thermal resistance here is explicitly
part of the hot module's constitutive equation. Directly equating two body
temperatures fixed at different initial values would be inconsistent.

## 2. Build each module with an explicit, open interface

`thermal_body` in the runnable source starts with `Model.empty`, then adds a
component owner, state, parameters, port declaration and constitutive equations.
The common port contract is:

```python
port_type = b.add_connector_type("HeatPort", members=[
    {"name": "T", "scalar_type": "real", "kind": "potential",
     "unit": "K", "quantity": "ThermodynamicTemperature"},
    {"name": "Q_flow", "scalar_type": "real", "kind": "flow",
     "unit": "W", "quantity": "HeatFlowRate"},
])
port = b.add_connector("port", owner=owner, type_id=port_type)
tp = b.member(port, "T")
q = b.member(port, "Q_flow")
```

`member` returns an artifact-local **variable ID**. `b.ref(q)` creates an
expression referring to it; `b.real(1.0)` creates a literal expression.
`add_equation(residual)` means `residual == 0`.
`add_derivative_equation(T, rhs)` adds `der(T) - rhs == 0`.
`add_state(..., start=...)` fixes the initial value by default; a start on a
general `add_variable` is not necessarily an initialization constraint.

In this example the equations are:

```text
hot:  C_h * der(T_h) = Q_h;  Q_h = G * (T_h_port - T_h)
cold: C_c * der(T_c) = Q_c;  T_c_port = T_c
```

Do **not** call `add_connection_set([port])` before exporting an open component:
a singleton set closes that port with zero flow. These open modules need their
environment to supply the remaining equations, so structural validation does
not mean each is ready for standalone simulation.

Call `b.finish()` (or `model.refresh()`) after equation edits to refresh views
and counts, and `model.validate(connections=True)` for native validation. A
refresh or ordinary `Model.save()` alone is not semantic validation.

## 3. Link the files — this does not connect them

```python
from rumoca_bitcode import Model

combined = Model.link(
    {"hot": "build/composition-demo/hot.rbc",
     "cold": "build/composition-demo/cold.rbc"},
    name="ConnectedThermalBodies",
)
```

Equivalent CLI (use a fresh output filename):

```bash
"$RUMOCA" bitcode link \
  hot=build/composition-demo/hot.rbc cold=build/composition-demo/cold.rbc \
  --name ConnectedThermalBodies -o build/linked-only.rbc
```

Both inputs contain `body.T` and `body.port`; the result contains
`hot.body.T`, `cold.body.T`, `hot.body.port`, and `cold.body.port`.
Numeric IDs are relocated. **Never reuse an input module's numeric IDs after
linking.** Look up the corresponding object in the linked artifact.

The Python linker delegates to the native implementation. It accepts paths or
`Model` objects, preserves inputs, supports JSON/CBOR, and keeps source provenance.
Each namespace must be unique and a simple identifier. Linking the same module
twice under different namespaces creates independent instances, not shared state.

## 4. Connect the linked ports with the equation builder

```python
ports = {p.path: p.id for p in combined.connectors}
b = combined.builder("tutorial.connect")
b.add_connection_set([ports["hot.body.port"], ports["cold.body.port"]])
b.finish()
combined.validate(connections=True)
combined.save("build/connected.rbc")
```

With both ports using this example's default `outside` orientation, this adds:

```text
hot.body.port.T - cold.body.port.T = 0
hot.body.port.Q_flow + cold.body.port.Q_flow = 0
```

The helper adds **both equations and semantic metadata**. Do not add the same
equations again yourself. Flow is positive into each owner, so the hot body's
flow is negative while it cools. Orientation metadata determines signs; do not
choose orientation from whether a component happens to be hot or cold.

For three or more ports, pass the **whole set once**. Separate overlapping
pairwise sets are rejected: pairwise zero-sum equations are not the same as one
multiway conservation law. Independently declared type IDs may differ, but
ordered member names, scalar types, potential/flow kinds, units, quantities and
flow conventions must match.

Construction is transactional: it builds a candidate and invokes native
validation before committing. Invalid wiring or an unavailable compiler leaves
the model unchanged. Native validation also checks raw-file edits against the
declared connection equations. See [connection checks](connector-validation.md).

## 5. Add an equation-IR pass

The example's `add_heat_integral` is a normal Python transformation:

```python
def add_heat_integral(model):
    b = model.builder("tutorial.heat-integral")
    q = b.variable("hot.body.port.Q_flow")
    energy = b.add_state("heat_into_hot", 0.0, unit="J", causality="output")
    b.add_derivative_equation(energy, b.ref(q))
    b.finish()
    model.validate(connections=True)

add_heat_integral(combined)
```

This adds `der(heat_into_hot) = hot.body.port.Q_flow`, initially zero, without
feeding back into the original equations. Negative accumulated heat is expected
because the hot body loses energy. Execute equation passes explicitly in the
desired order, before lowering. Builder names provide authorship provenance;
they do not register an automatically scheduled equation-pass pipeline.

IDs are dense table positions, not permanent cross-transformation handles.
Use builder operations instead of manually splicing expression nodes. After an
ID-compacting removal, use the returned remapping or resolve targets again.

## 6. Lower, then run an execution-IR pass

Lowering takes no observation list. It records a lowering profile; the
*program* says what it needs, and the logging pass registers the trace points
it references:

```python
from rumoca_bitcode.execution import lower
from synthesize_connector_csv import instrument_all_connectors

program = lower(combined)
instrument_all_connectors(program, combined)
program.save("build/connected-executable.rbc")
```

`synthesize_connector_csv` is the repository example module in `new_inst`, not
an SDK built-in. Its logger runs inside `with program.builder(...)`, calls
`observe_connector_members(model)` to register one trace point per connector
member, declares CSV sinks, then inserts these serialized lifecycle
operations:

| Region | Inserted work |
|---|---|
| `run_start` | `csv.open` |
| `publish` | Read snapshot time/sequence/phase and member values; `csv.write_row` |
| `run_finish` | `csv.close` |

`before_return` appends to a lifecycle region; `at`, `replace` and `remove`
support explicit instruction edits. Native checking validates operation types,
references, resource lifetimes and equation freshness. Python runs at authoring
time only: a fresh native process executes the saved instructions.

```bash
"$RUMOCA" bitcode check build/connected-executable.rbc --strict --connections
"$RUMOCA" bitcode run build/connected-executable.rbc --execution require \
  --stop 5 --publish-interval 0.1 --trace-root build/connected-traces
```

## 7. What our pass management currently provides

There is **no public standalone `PassManager` class** in the bitcode SDK today.
The implemented execution-pass mechanism is `Program.builder` plus
`Program.relower`:

- `program.builder(id, version=..., options=...)` records a receipt — id,
  version and typed options — in the artifact's ordered `passes` list;
  duplicate ids fail. Used as a context manager, leaving it is the pass
  boundary, and that is where the `dependency_digest` is recomputed.
- The authoring script chooses invocation order. There is no automatic
  dependency resolver, optimization scheduling, or cached analysis manager.
- Changing an identity the program *references* makes it stale;
  checking/running it rejects the mismatch instead of silently regenerating and
  dropping edits. Rewriting a residual the program does not observe does not
  make it stale, because the digest does not cover the equations.
- Explicit re-lowering uses a caller-supplied implementation for **every**
  recorded execution pass, in recorded order, with its saved options.

For the example's logger:

```python
# After an edit that changed an identity the program references:
fresh = program.relower(replay={
    "example.connector-csv": instrument_all_connectors,
})
fresh.save("build/replayed-executable.rbc")
```

Each callback has the shape `pass_fn(fresh_program, model, **saved_options)`.
Missing recipes, or a referenced trace point the model no longer declares,
fail. Receipts store ids, versions and typed options, **not Python
implementations**. Replay rebuilds the program from its passes; raw program
edits are not replayable, so express them in a named pass if they must survive
re-lowering. General pass mutations are not automatically transactional like
`add_connection_set`; validate after each pass and keep a known-good artifact.

After **linking**, lower a new program and resolve instrumentation targets anew;
do not replay old register or variable IDs blindly. Executable inputs are
rejected by the linker unless `discard_execution=True` / `--discard-execution`
explicitly authorizes discarding saved programs and instrumentation.

Separately, ModelSan has a [sanitizer registry](../packages/modelsan/modelsan/sanitizers/registry.py)
and [campaign `Pipeline`](../packages/modelsan/modelsan/pipeline.py). They
coordinate capability planning, static analyses, execution and runtime oracles.
That is sanitizer orchestration, not the equation/execution transformation
scheduler, and is not needed to compose these models.

## Limits and verification

Ordinary compiled Modelica artifacts may have legacy connection annotations or
already-closed ports. The linker does not infer open interfaces or delete
zero-flow equations. To wire a module, provide complete explicit port contracts
and handle its boundaries intentionally. Renaming an artifact is insufficient.

The connector builder currently supports scalar Real potential/flow members,
not stream/array/expandable/overconstrained or causal signal connections. Native
contract validation proves declared connection laws, not the whole model's
physical intent or solvability. Lowering has a separate supported execution
profile: [scalar-real, event-free native RK45](architecture/writable-execution-ir.md).
For example, a formulation needing algebraic tearing is rejected by that profile
even when the port contract is valid.

The tutorial regression runs the saved artifact in a native subprocess and
checks all 51 samples against:

```text
f(t) = exp(-5*t/6)
T_hot(t) = 320 + 30*f(t)        T_cold(t) = 320 - 20*f(t)
Q_hot(t) = -50*f(t)            Q_cold(t) = +50*f(t)
heat_into_hot(t) = 60*(f(t) - 1)
```

It also checks both port temperatures equal `T_cold`, opposite flow signs,
CSV row counts, and explicit logger replay. This is analytic evidence for this
fixture, not an MSL/OMC parity claim.

Verified on 2026-09-23: the command-line example completed, and the native
acceptance suite passed all 39 tests, including this tutorial regression. No
compiler/runtime behavior was changed for this documentation example.

```bash
PYTHONPATH=packages/rumoca-bitcode:new_inst RUMOCA="$PWD/target/debug/rumoca" \
  python3 -m unittest discover -s new_inst -p test_composition_tutorial.py -v
```

Further reading: [linker reference](bitcode-linking.md),
[native connection validation](connector-validation.md),
[writing equation passes](writing-a-bitcode-pass.md),
and [execution IR architecture](architecture/writable-execution-ir.md).
