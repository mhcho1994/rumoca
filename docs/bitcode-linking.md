# Linking bitcode modules

For a runnable two-model walkthrough covering open ports, the IR builders and
pass ordering/replay, start with [Combining two models](combining-models.md).

`rumoca bitcode link` is the equation-artifact counterpart of `llvm-link`:
it combines independently produced artifacts into one namespaced artifact.
It does **not** infer physical connections, resolve missing function bodies,
or concatenate executable solver programs.

## Command line

```bash
rumoca bitcode link motor=motor.rbc load=load.rbc \
  --name MotorAndLoad -o combined.rbc
rumoca bitcode check combined.rbc --strict
rumoca compile-bitcode combined.rbc --summary
```

Inputs may be CBOR or JSON, including a mixture. `--format json` selects JSON
output. Specify one or more `NAMESPACE=FILE` inputs in the desired order. Each
namespace must be a distinct simple identifier: letters, digits and `_`, with
no leading digit. The same file can be instantiated multiple times under
different namespaces. Output must not already exist, including any input path.

## Python SDK

```python
from rumoca_bitcode import Model
from rumoca_bitcode.execution import lower

combined = Model.link(
    {"motor": Model.load("motor.rbc"), "load": "load.rbc"},
    name="MotorAndLoad",
)
combined.save("combined.rbc")

# Add explicit coupling equations/connections here when appropriate.
# These calls require a model within the native execution profile:
program = lower(combined)
program.save("combined-execution.rbc")
```

The installed `rumoca` executable performs linking; set `RUMOCA` to select it.
The SDK does not maintain a second relocation implementation. Input Model
objects and files remain unchanged. Raw edits must have consistent summaries;
call `refresh()` after authoring edits before linking.

## What is preserved and renamed

| Data | Link behavior |
|---|---|
| Variable, component, connector, function, record-type names and trace labels | Prefixed with the module namespace |
| Local numeric identities/references | Remapped into distinct dense output tables |
| Continuous, initial, discrete and structured equations | Preserved in their own partitions, including incidence information |
| Conditions, roots, event actions, schedules, discrete definitions | Preserved and remapped; no event behavior inferred |
| Contracts, starts, bounds, units, connector signs/orientation | Preserved; referenced IDs relocated |
| Source paths/text/spans and declaring class names | Original diagnostic/declaration information retained; source IDs relocated |
| Function parameter names, binder/field/result ordinals, external symbols | Unchanged (not module-global object references) |
| Simulation time | One shared time coordinate |

No optimization, symbol deduplication or equation elimination occurs. Two
modules with a parameter named `k` retain separate `motor.k` and `load.k`.
Artifacts linked again acquire another namespace prefix. Input order determines
table order; identical inputs/options produce deterministic output.

## Merging is not connecting

Linking two closed models yields two independent systems in one artifact.
All existing boundary equations—including zero-flow equations for unconnected
ports—are retained. Deleting such constraints implicitly would change physics.

For components intentionally authored with open scalar potential/flow ports,
find their connector IDs in `combined.connectors` and call
`combined.builder("wire").add_connection_set([...])`, then `combined.refresh()`
and `combined.validate()`. Independently linked connector type IDs can differ;
the helper requires matching ordered member names, scalar types, potential/flow
roles, units, quantities and flow convention. It rejects overlapping finalized
connection sets and incompatible contracts without modifying them.

This helper does not reopen ports already closed by compilation, infer which
boundary equations to remove, or reconnect existing connection sets. Those
cases need an explicit interface/boundary transformation. Successful linking
alone does not prove equation balance, initialization consistency, or simulation
support; reconstruction and lowering remain their own gates.

Connection construction is transactional and requires the native checker.
For an explicit complete-interface audit, run
`rumoca bitcode check combined.rbc --strict --connections` or
`combined.validate(connections=True)`. See [native connection validation](connector-validation.md)
for mandatory raw-artifact checks, equation-proof syntax, legacy refusal and
the distinction between a declared interface and arbitrary boundary equations.

## Executable inputs

An input containing an execution projection is rejected by default. To retain
only its equations, explicitly authorize loss of the execution program:

```bash
rumoca bitcode link a=instrumented.rbc b=other.rbc \
  --discard-execution -o combined-equations.rbc
```

The SDK equivalent is `Model.link(..., discard_execution=True)`. This discards
**all** authored host instructions, sinks and pass receipts. Equation-level
trace points remain, but linking renumbers ids, so a discarded program's
`TracePointId` references would not survive anyway. The result has no
execution section; lower it again and reapply instrumentation, letting the
logging helper register trace points against the linked model. Never reuse
input solver/storage ids or execution programs.

The linker handles the known public RBC v2 equation schema, not internal DAE or
Solve serialization. Opaque unsupported expressions/conditions are rejected.
Clock schedules, activation references and sampled ownership are relocated
with their typed IDs. Preserving arrays, events or function signatures does not add support for them
to the bounded scalar-real, event-free RK45 execution adapter. Elided Modelica
function bodies remain elided; this is not external-function symbol resolution.

## Regression tests

```bash
CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 RAYON_NUM_THREADS=4 \
  cargo test -p rumoca-bitcode --lib link:: --offline
PYTHONPATH=packages/rumoca-bitcode:new_inst RUMOCA="$PWD/target/debug/rumoca" \
  python3 -m unittest discover -s new_inst -p test_link.py -v
```

Coverage includes two native ODEs versus independent runs and analytic solutions,
Modelica-compiled inputs, same-module instantiation, nested linking, JSON/CBOR,
source/contract/array/domain/event relocation, separate equation ID spaces,
thermal connector metadata and eight native CSV sinks, open-port wiring,
incompatible ports, explicit execution discard, and malformed-input/overwrite
rejection. Pure explicit ODE replay also regresses the residual-target sizing
bug uncovered by these tests: target count follows the checked residual block's
outputs, not the number of state/algebraic variables.
