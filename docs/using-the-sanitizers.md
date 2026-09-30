# Using the model sanitizers (ModelSan)

ModelSan finds defects in Modelica models the way AddressSanitizer finds
defects in C: it looks at the model's structure before it runs, then watches
real simulations for things that should never happen. Each problem it finds is
a **finding** that names the sanitizer, the kind of defect, the evidence, and
where it is in the source.

It is a Python package (`packages/modelsan`) built on the bitcode SDK
(`packages/rumoca-bitcode`). It uses Rumoca to compile and simulate; it does
not have its own compiler or solver.

## Setup

From the repository root, with a built compiler (`cargo build -p rumoca --bin rumoca`):

```sh
python3 -m pip install -e packages/rumoca-bitcode -e packages/modelsan
```

or, without installing, prefix commands with
`PYTHONPATH=packages/rumoca-bitcode:packages/modelsan`.

## A first check

`Tank.mo` has two defects: a drain that empties the tank until `sqrt(h)` sees a
negative level, and an area parameter that would remove the only equation for
`h` if someone set it to zero.

```modelica
model Tank
  parameter Real A = 1 "Tank area";
  parameter Real k = 0.5 "Outflow coefficient";
  Real h(start = 1, fixed = true, min = 0) "Level";
  Real q "Outflow";
equation
  q = k * sqrt(h);
  A * der(h) = 0.2 - q - 0.6;
end Tank;
```

`check.py` compiles it, runs the default sanitizers, and prints what they found:

```python
import subprocess
from rumoca_bitcode import Model
from modelsan.backends.rumoca import RumocaBackend
from modelsan.pipeline import Pipeline
from modelsan.sanitizers import DEFAULT, SanitizerRegistry

rumoca = "./target/debug/rumoca"
subprocess.run([rumoca, "compile", "Tank.mo", "--model", "Tank",
                "--emit-bitcode", "Tank.rbc"], check=True)
model = Model.load("Tank.rbc")

registry = SanitizerRegistry()
for sanitizer in DEFAULT:
    registry.register(sanitizer())

backend = RumocaBackend(rumoca, t_end=5.0)
outcome = Pipeline(registry, backend).run(model, "Tank.rbc", model.name)

for bug in outcome.database.bugs:
    for finding in bug.findings:
        print(finding.summary())
print("not checked:", outcome.coverage)
```

Output:

```
[singularity] vanishing-coefficient at Tank.mo:2
[solver] simulation-failure [simulation]
not checked: {'domain.runtime': 'observe_expression unavailable', ...}
```

- `singularity` found the `A` problem without running anything.
- `solver` reports that the run itself failed. Its evidence
  (`finding.evidence`) says a NaN was computed for `q`, which is the negative
  `sqrt`.
- `not checked` lists what could not be examined, and why. **An empty finding
  list only means "clean" for the sanitizers that actually ran**; always read
  the coverage.

## How it works

A campaign has three stages (`modelsan/pipeline.py`):

1. **Static analysis.** Sanitizers read the compiled model (its equations,
   variables, units and connections) through the bitcode SDK. They find
   defects that need no simulation: dimensional mismatches, structurally
   singular systems, a parameter whose zero value removes an equation, and so
   on. They also propose *hints*: parameter values or inputs worth trying.
2. **Planning.** Each runtime sanitizer asks for the observations it needs
   (variable traces, event times, solver statistics). The planner checks what
   the chosen backend can provide and records every request it cannot meet as
   a coverage gap, so a missing check is never silent.
3. **Execution and judgement.** The backend instruments the model (it adds
   trace points to the bitcode; the model's equations are untouched),
   simulates it, and hands the observations to the runtime sanitizers. A
   failed run is judged too: the failure is itself evidence.

Findings with the same signature are grouped into one **bug**
(`outcome.database.bugs`), so a defect seen in ten runs is reported once.

### Backends

| Backend | What it runs |
|---|---|
| `RumocaBackend` | compiles to bitcode, instruments it, simulates with `rumoca compile-bitcode --simulate` |
| `rumoca-source` (contracts CLI) | runs `rumoca sim` on the source directly; nominal runs only |
| `openmodelica` | runs OpenModelica (`omc`) for comparison |

Useful `RumocaBackend` options: `t_end`, `t_start`, `dt` (output interval),
`timeout`, `source_roots=[...]` for library models (e.g. the MSL), and
`freeze_parameters=True` to fix declared parameter values at compile time.

## The sanitizers

`DEFAULT` holds the sanitizers that need nothing a current backend lacks.

| Sanitizer | Finds |
|---|---|
| **DomainSan** (`domain`) | operations evaluated outside their mathematical domain: `sqrt`/`log` of a negative, `asin` outside [-1, 1] |
| **NumericSan** (`numeric`) | NaN, infinity and extreme values in a run that did not report failure |
| **RangeSan** (`range`) | a variable leaving the `min`/`max` its declaration promised |
| **SolverSan** (`solver`) | runs that failed: no convergence, step size collapse, non-finite values |
| **AssertSan** (`assert`) | violations of the model's own `assert` statements |
| **DiscontinuitySan** (`discontinuity`) | the thresholds a model switches on, used to aim test inputs at them |
| **SingularitySan** (`singularity`) | structure that cannot be solved, found before running |
| **InitSan** (`init`) | initialization problems, treated as a problem of their own |
| **EventSan** (`event`) | pathological event behaviour that a completed run still hides |
| **ZenoSan** (`zeno`) | event intervals collapsing toward zero |
| **PhysicalSan** (`physical`) | violated physical invariants (e.g. conservation) |
| **DivisorSan** (`divisor`) | a denominator that a permitted configuration drives to zero |

Opt-in sanitizers, registered the same way:

| Sanitizer | Finds |
|---|---|
| **QuantitySan** (`quantity`) | a declaration whose `unit` disagrees with its `quantity` |
| **DimensionSan** (`dimension`) | an equation adding quantities of different dimensions |
| **StructureSan** (`structure`) | defects in the shape of the equation system (unmatched equations, non-square blocks) |
| **NetworkSan** (`network`) | defects in the connection graph |
| **InitStaticSan** (`init-static`) | invariants evaluated in the initialization context |
| **BehaviorSan** | user-declared behavioural contracts (see below) |
| **DeterminismSan**, **DifferentialSan** (`COMPARATIVE`) | the same run giving different answers; two tools disagreeing. These schedule extra runs; use `Pipeline.run_comparative` |

Enable a subset by registering only those classes, or pass
`registry.register(SomeSan(), enabled=False)` to keep one registered but off.

## Checking declared behaviour: the contracts CLI

For "this output must equal that one" or "this signal must be a pulse of
period 2 s", write a campaign file and use the command-line tool:

```json
{
  "schema": 1,
  "source": "MyModel.mo",
  "cases": [{
    "model": "MyModel",
    "contracts": [{
      "kind": "equality",
      "contract_id": "state-round-trip",
      "origin": "inverse(forward(x)) restores x",
      "actual": "recovered",
      "expected": "original",
      "atol": 1e-8,
      "rtol": 1e-7
    }]
  }]
}
```

```sh
PYTHONPATH=packages/rumoca-bitcode:packages/modelsan python3 -m modelsan.cli contracts \
  campaign.json --backend rumoca-source --output report.json
```

Contract kinds: `equality`, `periodic_pulse`, `sample_delay`, `cardinality`.
Exit status: **0** no violation, **1** a violation, a proven model defect or a
failed run, **2** something could not be checked (blocked, inconclusive or
unobserved). The JSON report holds the evidence, traces and coverage.
[`packages/modelsan/README.md`](../packages/modelsan/README.md) has the full
contract semantics and a worked MSL campaign.

## Reading a finding

Each `Finding` carries:

- `sanitizer` and `kind`, e.g. `domain` / `sqrt-out-of-domain`;
- `severity`: `low`, `medium`, `high`;
- `source_locations`: file and line in the Modelica source;
- `canonical_anchors`: the variables, parameters or equations involved, by
  their stable bitcode identity;
- `evidence`: the observed values, time, failing input and so on;
- `test_case`: the parameters and inputs of the run that showed it, so it can
  be replayed.

A finding is evidence, not a verdict. Some are the model's intended behaviour
(a controller that deliberately saturates), so review them before reporting
them upstream.

## Further reading

- [`packages/modelsan/README.md`](../packages/modelsan/README.md): contracts,
  backends, validation tests.
- [`docs/modelsan-bitcode-integration.md`](modelsan-bitcode-integration.md):
  how ModelSan prepares and runs saved bitcode.
- [`docs/writing-a-bitcode-pass.md`](writing-a-bitcode-pass.md): the SDK the
  sanitizers are built on, for writing your own analysis.
