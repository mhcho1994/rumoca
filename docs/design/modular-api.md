# The modular API: typed SDK coverage, external passes in the compiler, pass scheduling

**Status:** implemented.

The bitcode is meant to be the one interface through which anything outside
the compiler reads or rewrites a model. Three pieces were missing:

1. **The SDK did not read the whole artifact.** Thirteen sections -- relations,
   conditions, roots, clocks, clock ownerships, time events, discrete
   definitions, event transactions, `previous` / `delay` / `terminal` owners,
   structured roots and connector types -- had no typed view, and `Event`
   exposed neither its trigger nor its message. A pass that needed them read
   `Model.raw_model`, an undocumented shape.
2. **External passes ran only on files.** An optimization written against
   the SDK could not be a step of `rumoca compile`; the user had to export,
   run it, and `compile-bitcode` the result.
3. **The pipeline ran a fixed list.** `--pass` took names in order, with
   `default` as the only group; nothing repeated passes to a fixed point or
   avoided rerunning a pass on a model it had already left unchanged.

## 1. Typed SDK coverage

`packages/rumoca-bitcode/rumoca_bitcode/dynamics.py` holds a view class per
section; `Model` builds each list, indexed by the artifact's dense ids.
Views resolve ids through the model (`root.relation.expression`,
`clock.period` as an exact `Fraction`, `delay.delay_time_value`), and every
view keeps `.raw` for a field it does not name yet. `Event` gains `trigger`,
`guard`, `message` and `level`.

`packages/modelsan/tests/test_sdk_typed_tables.py` compiles a model with
relations, `when`, `reinit`, `assert` with a level, `terminate` and `delay`,
and a clocked model with `previous`, then checks that every raw section has a
view of the same length and touches every accessor, so a shape the SDK
mis-reads fails the test.

## 2. External passes inside the compiler

`--pass exec:COMMAND` runs `COMMAND IN.rbc -o OUT.rbc`, the convention every
example in `examples/bitcode-passes` already follows
(`crates/rumoca-bitcode/src/passes/external.rs`). The output is untrusted:
it is decoded with the header check, then goes through exactly what a
built-in pass's result goes through -- dead-expression removal, summary
recompute, validation naming the step, and the rebuild through the DAE's
checked constructors.

A DAE has no place for trace points or an execution section. The first
attempt had `--emit-bitcode` write the pass stage's artifact instead of
re-exporting the rebuilt DAE; on 18 models, 4 came out different beyond type
numbering: the artifact's derived fields (`reads`, `binding_depends_on`,
`effective_value`, some expression types) still described the model *before*
`inline-constants` and folding rewrote it. A pass is not required to keep
derived fields current, so the emitted artifact must stay a fresh export.

Instead the compile result keeps the pass stage's artifact only when it holds
trace points or an execution section (`CompilationResult::bitcode`), and
`--emit-bitcode` exports the DAE as before, then carries those two over
(`crates/rumoca-bitcode/src/observations.rs`): verbatim when the variable and
connection tables line up, otherwise trace points remapped by variable name
and connector names; an execution section laid out over renumbered variables
is refused. A compile without such a pass emits exactly what it did before.

## 3. Pass scheduling

`crates/rumoca-bitcode/src/passes/pipeline.rs`:

| Element | Meaning |
|---|---|
| `NAME` | a built-in pass |
| `default`, `O1` | every built-in pass, in catalog order |
| `none`, `O0` | nothing; alone, the stage is skipped |
| `round-trip` | nothing, but still export and rebuild |
| `exec:COMMAND` | an external pass (runs to the end of its `--pass` value) |
| `fixpoint(PIPELINE)` | repeat until a round changes nothing, at most 16 rounds |

Elements are comma-separated and repeated `--pass` flags concatenate. Steps
run in the order written, like LLVM's `-passes=`; the user, not a dependency
solver, decides order.

The scheduler keeps a model generation that every changing step bumps, and
records for each built-in pass the generation at which it last ran and
changed nothing. Requested again at that same generation, it is skipped:
built-in passes are deterministic, so the rerun could only report nothing.
That makes `default,default` and a converged `fixpoint` round free, and is
what lets `fixpoint` detect convergence. External passes are never skipped.

## 4. The version contract

`docs/SPEC_RUMOCA_BITCODE.md` §4.1: every reader -- the compiler's decoder,
an in-compiler external pass's output, the SDK's `Model` -- implements
exactly one `RBC_VERSION` and refuses any other before reading the payload.
The SDK's package version (`rumoca_bitcode.__version__`, now 0.2.0) is
separate from the bitcode version it reads (`rumoca_bitcode.VERSION`).
`packages/modelsan/tests/test_sdk_version_contract.py` pins the SDK and the
compiler to the same version.

## Tests

- `crates/rumoca-bitcode/src/passes/tests.rs`: grammar, skip-on-unchanged,
  fixpoint convergence, an external pass in the pipeline, a failing or silent
  external pass, one that writes another version.
- `packages/modelsan/tests/test_example_passes.py`: `trace_all.py` as a step
  of `rumoca compile`, its trace points in the emitted artifact; a pass that
  corrupts the model is refused naming it.
