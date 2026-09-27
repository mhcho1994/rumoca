# Writable equation/execution IR — start here

The supplied thermal example now builds its equations from scratch, rewrites
the conductor law, saves editable execution programs, and runs their CSV
instructions in a fresh native Rumoca process. No Modelica/MSL input, Python
simulation, runtime Python callback or trajectory-to-CSV postprocessor is used.

## Run the demonstration

From the repository root:

```bash
CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 RAYON_NUM_THREADS=4 cargo build -p rumoca --bin rumoca --offline
PYTHONPATH=packages/rumoca-bitcode python3 new_inst/synthesize_connector_csv.py \
  --rumoca "$PWD/target/debug/rumoca" --out build/thermal-demo --run
python3 new_inst/check_thermal_csv.py build/thermal-demo/traces --conductance-scale 2
```

Use a new output directory each time. The example saves the original equations,
rewritten equations, uninstrumented executable and instrumented executable.
The trace directory contains a manifest and one CSV for each of the four ports.
Each CSV has 51 records, from 0 through 5 seconds. The independent checker
tests temperatures, all flow signs, both connection balances and stored energy.
See [saved native runs and CSV index](results/README.md) for both scales.

## APIs and implementation

| Work | Entry point |
|---|---|
| Construct/rewrite equations | `Model.empty`, `model.builder`, `add_derivative_equation`, `rewrite_equation` |
| Remove scalar states/equations atomically | `builder.remove(variables=..., equations=..., initial_equations=...)` |
| Construct semantic connectors and n-ary equations | `add_component`, `add_connector_type`, `add_connector`, `add_connection_set` |
| Record a lowering profile | `rumoca_bitcode.execution.lower(model)` — no observation list |
| Register observations | `rumoca_bitcode.execution.observe_connector_members(model)`, `model.add_trace_point` |
| Edit instructions | `program.builder(...)` as a context manager; `.at/replace/remove` |
| Build program expressions | `program.expressions(function).local/real/binary/compare` |
| Edit lifecycle regions | `program.function`, `add_function`, `builder.before_return/at` |
| Check/save/load | `program.validate`, `program.save`, `Program.load` |
| Explicit re-lowering and pass replay | `program.relower(replay={pass_name: implementation})` |
| Native execution, no equation re-lowering | `rumoca bitcode run artifact.rbc --execution=require --trace-root NEW_DIR` |

See [architecture and profile limits](../docs/architecture/writable-execution-ir.md)
for the public schema, native ownership, lifecycle rules, freshness and failure
semantics. [IMPLEMENTATION_INVENTORY.md](IMPLEMENTATION_INVENTORY.md) records the
historical starting gaps, not the final implementation state.

## Insert a computation/check with the same builder

```python
with program.builder("my.time-check") as b:
    helper = program.add_function("publish:check-time")
    arena = program.expressions(helper)
    with b.before_return(helper) as ir:
        t = ir.emit("snapshot.time")
        ok = ir.emit("compute", ty="boolean",
                     expr=arena.compare("Ge", arena.local(t), arena.real(0.0)))
        message = ir.emit("snapshot.phase")
        ir.emit("assert", condition=ok, message=message)
    with b.before_return("publish") as ir:
        ir.emit("call", function="publish:check-time")
program.validate()
```

`compute` evaluates a node of the function's own expression arena, whose only
non-literal leaf is a declared local — a program cannot read a model variable
except through `snapshot.value`. `emit` declares each result, so an undeclared
local is not something a pass can leave behind; `compute` must state its `ty`
because only the pass knows what it computed.

`builder.at(function, index)` inserts anywhere; `replace`/`remove` modify
existing instructions. Rust, not Python, checks operation types and effect
order. There is no numerical program to edit: it is derived at load from the
equations and the recorded profile. Leaving the `with` block is the pass
boundary, and that is where the dependency digest is recomputed. Register the
transformation as a replay implementation if it must survive equation
changes.

## Bitcode linking

Start with [Combining two models](../docs/combining-models.md) and its runnable
`compose_models.py` example for a full linker → connector → equation pass →
execution pass → native simulation workflow.

Independent equation artifacts can now be combined with `rumoca bitcode link`
or `Model.link`. See [the linking guide](../docs/bitcode-linking.md) for CLI/SDK
examples, explicit connection handling, execution-discard rules, and tests.
The [linker verification record](LINK_VERIFICATION.md) records focused results
and the broader validation boundaries.

## Acceptance tests

```bash
PYTHONPATH=packages/rumoca-bitcode:new_inst RUMOCA="$PWD/target/debug/rumoca" \
  python3 -m unittest discover -s new_inst -p test_execution.py -v
CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 RAYON_NUM_THREADS=4 \
  cargo test -p rumoca-solver --lib --offline
```

The native integration suite covers scales 1/2, exact instrumentation neutrality,
fresh-process execution, all ports and publication counts, raw and builder
staleness, explicit replay, missing targets, duplicate passes, register/type/
lifecycle errors, unsupported backends/shapes, initialization failure, signed
alias reconstruction, ordinary computation/calls/branches, ordered effects, CSV
quoting, safe output creation, and atomic state/equation removal. Solver tests
add same-time replacement, publication failure propagation and an actual
scheduled-event/periodic coincidence.

The delivered public numerical adapter is intentionally scalar-real and
event-free. The underlying native publication path is event-aware. Unsupported
numerical owners and alternative executable backends are diagnosed explicitly,
not silently omitted. This does not claim arbitrary MSL models fit this first
executable profile.

## Verification record (2026-09-23)

| Check | Result |
|---|---|
| Native Python acceptance suite | 15 passed |
| Existing SDK builder regression suite | 8 passed |
| Rust bitcode / evaluator / Solve IR / Solve lowering unit suites | 46 / 159 / 241 / 110 passed |
| Rust solver unit suite, including native event CSV publication | 420 passed |
| Independent oracle on both saved runs | 204 rows per run; all checks passed |
| Targeted strict lint: Solve IR, evaluator, lowering, solver and simulation | Passed |
| Broader CLI / bitcode strict lint | Not clean: pre-existing failures in the existing instantiation, DAE analysis, bitcode builder/import/export/text/validation code |

The broad lint failures were left untouched because they are outside this
milestone and overlap the existing dirty working tree. Examples include nesting
in `rumoca-phase-instantiate/src/attributes.rs`, function length in
`rumoca-phase-dae/src/construction/analysis.rs`, and existing bitcode text/parser
complexity. This is not a claim of a clean repository-wide lint gate. No commits
or external publication were made.
