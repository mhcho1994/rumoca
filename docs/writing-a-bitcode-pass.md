# Writing an external pass

An artifact carries two representations: **equation IR** (`RbcFile.model`,
what must hold) and **execution IR** (`RbcFile.execution`, what the host does
and when). The numerical program a run needs is derived from both at load and
is not in the file. Which one your pass should target, and the rules that
keep them consistent, are in
[SPEC_RUMOCA_BITCODE.md §2a](SPEC_RUMOCA_BITCODE.md#2a-two-irs-equation-and-execution).
**This page focuses on equation IR.**

For an end-to-end composition example and the distinction between equation
passes, execution passes and explicit replay, see
[Combining two models](combining-models.md).

A pass is a program that reads Rumoca Bitcode. That is the whole contract.

You do not need a Rumoca checkout, Rust, a matching compiler version, or any
linkage against Rumoca's crates. If your language can read a file, it can write
a pass.

---

## The shortest possible pass

```bash
rumoca compile Motor.mo --model Motor --emit-bitcode motor.rbc
```

```python
from rumoca_bitcode import Model

model = Model.load("motor.rbc")
print(model)                       # <Model 'Motor': 23 variables, 20 equations, 4 connections>

for variable in model.states:
    print(variable.name, variable.unit, variable.source.span)
```

```
$ PYTHONPATH=packages/rumoca-bitcode python3 my_pass.py
cap.v None Circuit.mo:40:3
```

That is an analysis pass. It reads and reports.

## The two kinds of pass

```
ANALYSIS                       TRANSFORMATION

model.rbc                      model.rbc
   |                              |
   v                              v
 analysis                    transformation
   |                              |
   v                              v
 report                       model2.rbc
                                  |
                                  v
                               rumoca
```

An analysis pass produces information. A transformation pass produces another
artifact, which Rumoca validates and rebuilds.

## Installing the SDK

```bash
pip install -e packages/rumoca-bitcode
```

or just put it on your path:

```bash
export PYTHONPATH=/path/to/rumoca/packages/rumoca-bitcode
```

The SDK has **no dependencies**. A pure-Python CBOR decoder ships with it, so
reading a `.rbc` needs nothing but the standard library. Install the optional
`cbor2` extra if you want the faster codec.

## The object model

```python
model.variables      # every variable, any role
model.states         # role == "state"
model.parameters     # role in ("parameter", "constant")
model.inputs         # role == "input"
model.outputs
model.algebraics
model.equations      # continuous residuals
model.initial_equations
model.events         # reinit / assert / terminate
model.connections    # connect(...) relationships
model.components     # component instances
model.trace_points   # observation requests
```

Every other table the compiler exports has a typed view too
(`rumoca_bitcode.dynamics`), each list indexed by the id the artifact uses:

```python
model.relations             # Relation: .expression, watched for sign changes
model.conditions            # Condition: .kind, .relation, .expression, .operands, .clock
model.roots                 # Root: .relation, .activation (a Condition)
model.clocks                # Clock: .kind, .period / .phase (exact Fraction), .anchor, .condition
model.clock_ownerships      # ClockOwnership: .variable, .clock, .sampled
model.time_events           # TimeEvent: .time (Fraction) or .deadline (Expression)
model.discrete_definitions  # DiscreteDefinition: .targets, .branches (.trigger, .guard, .values)
model.event_transactions    # EventTransaction: .targets, .steps (.trigger, .guard, .clock, .definitions)
model.previous_values       # PreviousValue: .variable, .clock
model.delays                # Delay: .kind, .expression, .delay_time, .delay_time_value, .maximum
model.terminals             # Terminal
model.structured_roots      # StructuredRoot: .domain, .expression
model.connector_types       # ConnectorType: .name, .members, .flow_convention
```

An event also names what fires it and what it reports: `event.trigger` and
`event.guard` (Conditions), and for `assert`/`terminate` `event.message`
and `event.level` (Expressions; no level means `AssertionLevel.error`).
`view.raw` is the entry as stored, for a field a view does not name yet.

A variable:

```python
variable.name            # "motor.flange.tau", the flattened path
variable.role            # "state" | "parameter" | "algebraic" | ...
variable.causality       # "input" | "output" | "local" | ...
variable.unit            # "N.m" or None
variable.type            # ValueType: scalar kind + dimensions
variable.source          # Provenance
variable.component       # Component or None
variable.is_state
variable.is_connector_member
variable.quantity        # "potential" | "flow" | "stream" | None
variable.start           # Expression or None
```

An equation:

```python
equation.residual            # Expression; the model asserts residual == 0
equation.reads               # [Variable]  <- use this
equation.reads_derivative    # [Variable] whose der() is read
equation.source              # Provenance
```

## Use `reads`, do not walk the tree

`equation.reads` is computed by the compiler's own dependency projection, which
resolves function calls, structured equation families, and runtime array
selection correctly.

```python
# right
for variable in equation.reads:
    ...

# wrong: misses function-call dependencies and dynamic subscripts
for node in equation.residual.walk():
    ...
```

Walking the tree is fine when you care about *syntax* — which operator was
used, whether a literal appears. It is the wrong tool for *dependency*.

## Expressions

```python
Literal(value)
VariableRef(kind, variable)        # kind: "state" | "derivative" | "pre_state" | ...
TimeRef()
UnaryOp(op, operand)
BinaryOp(op, lhs, rhs)
Conditional(branches, fallback)
Unsupported(detail)
```

`der(x)`, `x` and `pre(x)` are all `VariableRef` with different `kind`s, on the
same variable. They are different coordinates, not different variables.

```python
for node in equation.residual.walk():
    if isinstance(node, VariableRef) and node.is_derivative:
        print("differentiates", node.variable.name)
```

**Handle `Unsupported`.** It means bitcode v2 could not represent part of the
model. Refuse, or narrow your claim — never treat it as a default:

```python
if any(isinstance(n, Unsupported) for e in model.equations for n in e.residual.walk()):
    raise SystemExit("this model uses constructs bitcode v2 cannot represent")
```

## Provenance

```python
equation.source.span            # Circuit.mo:13:3
equation.source.span.text()     # "v = p.v - n.v"   (if sources are embedded)
equation.source.is_generated    # True for compiler-produced equations
equation.source.generation      # "connection_equation", "flow_balance_equation", ...
```

Report `Motor.mo:52`, not `equation 17`. The line and column are precomputed,
so this costs nothing.

Two traps:

- **A span is not unique.** An equation inherited through `extends` appears
  once per instance with the same span. Use `id` for identity.
- **Generated equations point at the nearest responsible declaration**, not at
  the statement you might expect. A connection equation's span is the connector
  member's declaration. Check `source.generation` before reporting a span as if
  the user wrote it there.

## Connections are not messages

A Modelica connector carries *potential* quantities, equated across the
connection, and *flow* quantities, whose signed sum is zero. Modelling a
connection as "a sends to b" is physically wrong.

```python
for connection in model.connections:
    print(connection.left_connector, "<->", connection.right_connector, connection.quantity)
    if connection.is_flow:
        ...  # conservation law, not a transfer
```

## Writing a transformation

```python
model = Model.load("motor.rbc")

for connection in model.connections:
    for variable in model.connector_members(connection.left_connector):
        model.add_trace_point(variable, connection=connection, added_by="my_pass.py")

model.save("motor-traced.rbc")
```

`save()` recomputes the summary the validator checks, so the artifact you write
is accepted.

Then hand it back:

```bash
rumoca bitcode check motor-traced.rbc
rumoca compile-bitcode motor-traced.rbc --simulate --t-end 1.0 --trace-out traces.csv
```

```
time,trace_id,connection,variable,quantity,unit,value
0.1,1,inertia.b <-> spring.a,inertia.b.tau,flow,N.m,4.272051
0.1,2,inertia.b <-> spring.a,spring.a.tau,flow,N.m,-4.272051
```

The two torques are equal and opposite because they are *flow* quantities
either side of one connection. Your pass asked for them by name; the solver
reported them.

## Adding computation, not just observation

The objection to answer first, because it is a good one:

> LLVM IR is Turing complete, so an instrumentation pass can insert arbitrary
> code — a counter, a check, a call into a runtime. Rumoca Bitcode is
> deliberately total ([§9a](SPEC_RUMOCA_BITCODE.md#9a-computational-power)):
> no loops, no recursion, no statements at all. So an arbitrary pass cannot be
> written against it.

The premise is right and the conclusion does not follow. **An LLVM pass inserts
instructions into a control-flow graph; a pass here inserts equations into a
system.** The substrate is different, not weaker:

| you want | you add |
|---|---|
| an accumulator | a state and a residual `der(E) - f(...) = 0` |
| a register | a discrete variable and a `when` definition over `pre()` |
| a branch | a relation and a root, so the solver locates the crossing |
| a trap | an event action: `assert` or `terminate` |
| a probe | a trace point |

The computation that gets you is not limited in power. A hybrid DAE with
discrete state *is* Turing complete —
[`Minsky.mo`](../crates/rumoca-bitcode/examples/Minsky.mo) is a two-counter
machine in six variables and one equation — so a monitor can be an arbitrary
program. It is limited in *shape*: declarative, and evaluated on the solver's
clock rather than at a program point.

And the artifact stays total, which is the part worth keeping. Instrumenting an
LLVM module can make it undecidable to analyse. Instrumenting one of these
cannot: every static analysis in `modelsan` still terminates on the result.

### The builder

`rumoca_bitcode.Builder` enforces the invariants at the point of the mistake
rather than at validation time. It lives in the SDK, not in the sanitizer:
writing an artifact is part of the *format's* contract, and a consumer with no
interest in sanitizers should not have to install one to produce a model.

```python
builder = model.builder("my_pass")

current = builder.variable("R1.i")                  # an existing variable
charge = builder.add_state("charge_R1", start=0.0)  # one the pass owns

builder.add_derivative_equation(charge, builder.coordinate(current))
builder.add_trace_point(charge, "charge through R1")

builder.finish()          # refresh the typed views, recompute the summary
model.save("instrumented.rbc")
```

`add_derivative_equation` is not a convenience. The runtime rejects a state
equation that is not a *subtractive* derivative residual, so the algebraically
identical `der(x) + k*x` fails at simulation with a message about a constraint
nothing else states.

### Starting from nothing

The same builder produces an artifact where there was none, so a tool that is
not the Rumoca compiler can emit one:

```python
import rumoca_bitcode as rb

model = rb.Model.empty("Decay")
builder = model.builder("my_generator")
k = builder.add_parameter("k", 2.0)
x = builder.add_state("x", start=1.0)
builder.add_derivative_equation(
    x, builder.unary("negate", builder.multiply(builder.coordinate(k),
                                                builder.coordinate(x))))
builder.add_trace_point(x, "x")
builder.finish()
model.save("decay.rbc")
```

```console
$ rumoca bitcode check decay.rbc --strict
decay.rbc: valid bitcode v2 (strict)
$ rumoca compile-bitcode decay.rbc --simulate --trace-out decay.csv
  x(0) = 1.000000   x(1) = 0.135337      # exp(-2) = 0.135335
```

In Rust, `rumoca_bitcode::build::Builder` does the same and is what this
crate's own test fixtures are built from — a struct literal naming every field
of `RbcModel` broke on every schema addition, four times in one week.

`modelsan/passes/energy.py` is the worked version: it adds
`der(E) = sum of port powers` for every component whose ports give a complete
power expression, and the result validates, simulates and reports a quantity
the model never contained.

### Two rules that bite silently

**The expression arena is topologically ordered.** A new node may reference
earlier ones; nothing earlier may reference it. Appending is free; *splicing*
into the middle of an existing tree is impossible in place, so
`rewrite_operand` rebuilds the path from that tree's root and shares
everything off the path. An equation is not in the arena, so
`rewrite_equation` can point it at a node appended afterwards — which is how a
fault-injection pass replaces a term.

**Everything is dense and everything is counted.** Ids are positions, and
`finish()` recomputes the summary the validator checks. Skip it and the
artifact is rejected for a count that no longer matches.

### What you genuinely cannot do

- **Unbounded iteration within a step.** There is no `while`. A recurrence
  across time steps is what `pre()` is for; a fixpoint inside one evaluation is
  not expressible, and making it so would cost the termination property every
  analysis here relies on.
- **Call into a runtime.** There is no linkage. An external function can be
  *named* (`RbcFunctionBody::External`), not written by a pass.
- **Dynamic allocation.** Arrays have fixed extents; a monitor needing an
  unbounded history has to encode it in state the way `Minsky.mo` does.

### A finding from actually running one

The energy pass on `ChuaCircuit` reports `energy_C1 = -1.06 J`: the capacitor
delivers net energy over the window. That is correct — `C1` starts at 4 V and
discharges — and it means the obvious runtime check, *a passive component must
not source power*, fires on ordinary physics.

A storage element obeys `E_in(t) = E_stored(t) - E_stored(0)`, which needs the
constitutive law and not just the ports. A resistor stores nothing, so
`E_in >= 0` holds unconditionally. `NetworkSan` therefore checks components a
contract establishes as **dissipative**, and excludes storage explicitly. The
distinction was not visible from reading the model; it came out of running the
instrumented one.

## Your pass is untrusted, and that is fine

Rumoca does not trust what you write. Import validates references, ids,
ordering and counts, then rebuilds through the DAE's own checked constructors —
the same ones used when compiling from source.

That means a bug in your pass produces a **clean rejection naming the offending
record**, not a corrupt model:

```
$ rumoca compile-bitcode broken.rbc
bitcode failed validation:
  - equation 0 references expression 999, which does not exist (6 defined)
  - variable 1 references type 900, which does not exist (1 defined)
```

Validation reports every problem it finds, not just the first.

Prefer trace points to equation edits when you only need to observe something:
a trace point cannot change what the model computes, so it cannot introduce a
physics bug.

### Observing a value from an execution pass

`lower-execution` has no `--observe` option, and the SDK's `lower(model)` takes
no observation list. Observations are *demanded* by the program: a
`snapshot.value` instruction names a `TracePointId`, and the numerical program
is derived from exactly the trace points the program references. A model with
no trace points and a program with no instructions is a valid artifact with
zero observations, not an error.

Registering a trace point is an equation-IR edit, so the helper that needs one
creates it. `rumoca_bitcode.execution.observe_connector_members(model)`
registers one per connector member and returns `{path: trace_point_id}`; it is
idempotent by `(label, variable)`, so replaying a logging pass reuses the
points it registered the first time. If you observe something else, call
`model.add_trace_point(variable, label=...)` yourself and reference the id it
returns.

Run the pass inside its builder context:

```python
with program.builder("my.logger") as b:
    points = observe_connector_members(model)
    sink = b.declare_csv_sink(
        key="my-sink", filename="trace.csv",
        columns=[{"name": "t", "ty": "real"}, {"name": "T", "ty": "real"}],
        # `members` is always present. `connector` is one optional record, so
        # a connector without an orientation is not representable; leave it
        # out entirely when the sink is not connector instrumentation.
        metadata={"connector": {"connector": 0, "orientation": "outside"},
                  "members": [{"trace_point": points["hot.port.T"]}]})
    with b.before_return("run_start") as ir:
        ir.emit("csv.open", sink=sink)
    with b.before_return("publish") as ir:
        ir.emit("csv.write_row", sink=sink, values=[
            ir.emit("snapshot.time"),
            ir.emit("snapshot.value", trace_point=points["hot.port.T"])])
    with b.before_return("run_finish") as ir:
        ir.emit("csv.close", sink=sink)
```

`with` matters: leaving the builder is the pass boundary, and that is where the
`dependency_digest` is recomputed against the identities the program now names.
`emit` declares each result it produces, so a pass cannot leave an undeclared
local behind; `compute` is the exception and must state its `ty`, because only
the pass knows what it computed.

## Worked examples

In `examples/bitcode-passes/`:

| Pass | Kind | Shows |
|---|---|---|
| `model_summary.py` | analysis | the minimum shape of a pass |
| `dependency_graph.py` | analysis | `reads` edges, reachability, Graphviz as *a* renderer |
| `connector_graph.py` | analysis | connectors, members, flow vs potential |
| `connector_logger.py` | transformation | instrumenting every cross-component quantity |

Run them:

```bash
export PYTHONPATH=packages/rumoca-bitcode
python3 examples/bitcode-passes/model_summary.py motor.rbc
python3 examples/bitcode-passes/dependency_graph.py motor.rbc --influences throttle
python3 examples/bitcode-passes/connector_graph.py motor.rbc --dot | dot -Tsvg -o topology.svg
python3 examples/bitcode-passes/connector_logger.py motor.rbc -o motor-traced.rbc
```

## Running your pass inside the compiler

A pass that takes `INPUT.rbc -o OUTPUT.rbc`, like every example here, can run
as a step of `rumoca compile` itself:

```bash
rumoca compile motor.mo --model Motor \
    --pass "default,exec:python3 examples/bitcode-passes/trace_all.py" \
    --pass "fixpoint(fold-constants,fold-pure-calls)" \
    --emit-bitcode motor-traced.rbc
```

The compiler writes the model to a temporary CBOR file, runs
`COMMAND IN.rbc -o OUT.rbc` (the command split on whitespace), and reads the
output back. Your program's stdout and stderr both go to the compiler's
stderr. The output then gets exactly what a built-in pass's result gets:
dead expressions are dropped, the summary is recomputed, the model is
validated (a failure names your pass), and it is rebuilt into the DAE that
simulation and code generation consume. Trace points and the execution
section a pass adds are kept in the artifact `--emit-bitcode` writes.

`exec:` runs to the end of its `--pass` value, so a command may contain
commas; put anything after it in another `--pass`.

The pipeline grammar and scheduling:

| Element | Meaning |
|---|---|
| `NAME` | a built-in pass (`rumoca compile --pass nope` lists them) |
| `default`, `O1` | every built-in pass, in catalog order (what a plain compile runs) |
| `none`, `O0` | nothing; alone, the model is exactly what the frontend lowered |
| `round-trip` | nothing, but still export and rebuild |
| `exec:COMMAND` | an external pass |
| `fixpoint(PIPELINE)` | repeat until a round changes nothing (at most 16 rounds) |

Steps run in the order written. A built-in pass is skipped when the model has
not changed since it last ran and changed nothing, so `default,default` or a
converged `fixpoint` round costs nothing. External passes always run.

## Writing a pass in another language

The SDK is one reader, not the interface. To write a pass in any language:

1. Read the file. If the first non-whitespace byte is `{` it is JSON;
   otherwise it is CBOR. `rumoca bitcode convert --format json` gets you JSON if
   your language has no CBOR library.
2. Check `magic == "RUMOCA-RBC"` and `bitcode_version == 2`. Refuse otherwise.
3. Read `model`. Every enum is tagged by an explicit `kind` string.
4. To write: preserve fields you do not understand, recompute `summary`, and
   keep expression operands referencing strictly lower ids.

`docs/SPEC_RUMOCA_BITCODE.md` is the normative description.

## Debugging

```bash
rumoca bitcode inspect motor.rbc       # one-screen summary
rumoca bitcode dump motor.rbc | less   # the whole artifact as JSON
rumoca bitcode check motor.rbc         # validate, listing every problem
rumoca bitcode round-trip motor.rbc    # prove import/export fidelity
```

JSON artifacts are ordinary text: `diff`, `jq` and an editor all work.

```bash
rumoca compile Motor.mo --model Motor --emit-bitcode motor.json --bitcode-format json
jq '.model.connections' motor.json
```
