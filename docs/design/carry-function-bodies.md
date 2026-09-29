# Stage 3: carry function bodies, and keep the totality property

Follow-on to [minimal-frontend.md](minimal-frontend.md), which left this as
"a decision, not a task" on the grounds that carrying bodies "makes recursion
representable, which ends the IR's totality property as currently argued."

**That reasoning does not survive contact with the IR.** The decision is
smaller than the doc assumed, and the totality property is recoverable with
machinery that already exists.

## What a function body actually is by the time it reaches the DAE

Not Modelica statements. `FunctionStatementWire`
(`rumoca-ir-dae/src/model.rs:410`) has exactly four forms:

```rust
enum FunctionStatementWire {
    Assignment      { definition }
    AssignmentGroup { definitions, conditional }
    Assertion       { condition, message, provenance }
    For             { fold, statements, provenance }
}
```

There is no `while`, no `break`, no `return`, and no general recursion. `For`
is a *fold over a compact domain* — the same bounded shape `Comprehension`
already has in the artifact, with a trip count fixed before evaluation.

So "carry function bodies" means carrying four statement kinds and the three
entities they name: an SSA definition, a branch correlation, and a fold.

## Why the totality property survives

§9a of SPEC_RUMOCA_BITCODE rests on three claims. Carrying bodies in this
form touches one of them, and the gap closes:

| Claim | Effect of carrying bodies |
|---|---|
| The expression arena is a DAG, cycles unrepresentable | unchanged; bodies reference the same arena under the same rule |
| There is no control flow; every trip count is known | unchanged; `For` is a compact fold, `AssignmentGroup` a branch correlation |
| Function bodies are not carried, so recursion is not expressible | **this one changes** |

Only the third gives way, and only for *call* recursion — the body form
itself cannot express unbounded iteration. `RbcFunction.calls` already
carries the call graph (Stage 2, commit `36716d4d`), so acyclicity is
checkable directly from the artifact. That is the same guard Execution IR v2
uses for the same purpose (`EX2-030`), proven there.

§9a therefore changes from "recursion is not expressible because bodies are
absent" to "recursion is not expressible because the call graph is checked
acyclic." The property is the same; the argument for it is better, because
it no longer depends on withholding information a consumer needs.

## Why it is worth doing

Measured over the eleven-library evaluation corpus:

- **401 of 2381 artifacts (17%)** carry at least one `RbcExprNode::Unsupported`.
- **3607 of 3911** of those unsupported nodes are `function_value` — a
  reference to a value inside a function body. A further 226 are
  `function_fold_parameter` and `function_fold_output`. **97% are
  function-internal.**
- **43 of 120 artifacts failed `compile-bitcode`.** After the domain-export
  fix (`8f86b8fe`) the remaining cause is `function_value`.

An artifact carrying an `Unsupported` node cannot be imported, so it cannot
be simulated or instrumented through the public path. That is the
interchange promise the format exists for, and function-body lowering is
what breaks it.

It is also the prerequisite for minimal-frontend Stage 1 in the form the
design doc describes. Routing `compile` through
`Flat -> RBC -> [passes] -> DAE` requires a lossless round trip, and the
round trip is not lossless while bodies are elided.

## Shape

Additive to `RbcModel`; no existing field changes meaning.

```
RbcFunctionBody::Modelica { statements: Vec<RbcFunctionStatement> }
    // joins ElidedModelica and External; an artifact may still elide.

RbcFunctionStatement   Assignment | AssignmentGroup | Assertion | For
RbcFunctionDefinition  { ordinal, value, expression }
RbcFunctionConditional { conditions, branches, fallback }
RbcFunctionFold        { ordinal, domain, carried, statements }
```

`RbcExprNode` gains `FunctionValue`, `FunctionFoldParameter` and
`FunctionFoldOutput`, whose operands are owner-local ordinals into the
function that declares them — the same addressing the DAE uses, so export is
a projection rather than a translation.

`ElidedModelica` stays. An artifact that elides bodies is still valid and
still total by the old argument; one that carries them is total by the call
graph check. Which applies is readable from the artifact.

## Staging

1. Schema, export, import for `Assignment` and `Assertion` only — the two
   non-looping forms. Round-trip tests on artifacts that contain no fold.
2. `AssignmentGroup` with its branch correlation.
3. `For` and the fold entities.
4. The acyclic call-graph check, and the §9a rewrite that rests on it.
5. Text writer/parser, and regenerate `bitcode-reference.md`.

Each step is measurable against the corpus: the count of artifacts carrying
an `Unsupported` node, and the count that survive `compile-bitcode`, should
fall monotonically to zero for the forms handled so far.

## Import: done, by interleaving

Import reconstructs carried bodies (`crates/rumoca-bitcode/src/import/functions.rs`).
The obstacle was import's phase order, not the DAE's API: `rebuild` built
the whole expression arena in one pass, but a `FunctionValue` node names a
definition that exists only while its function is under construction, and
that function's statements name arena expressions. So the two interleave:

- functions are opened in id order, which is callee-first (the DAE assigned
  the ids that way, and `check_call_graph_acyclic` proves it consistent);
- the arena is rebuilt forward through the last node that function needs;
- a statement is issued as soon as every expression it names exists;
- a `FunctionValue` is a *read*, so statements are replayed until the value
  holds exactly the definition the artifact names;
- `FunctionFoldParameter` / `FunctionFoldOutput` nodes are *issued* by
  opening and closing a loop (`begin_loop*`, `finish_*loop`), so reaching one
  means the next statement must be that loop's opening or closing.

Every step goes through the checked constructor a compiler would call; the
DAE gained one read-only accessor (`current_definition_rhs`, to learn which
ids a loop transition's nodes took) and three `pub(crate)` replay
constructors became public. The same work lands `Call` and call projections,
function parameters, external bodies (with the full ABI, now carried:
arguments, result, purity, linkage) and function-owned quotients.

Carrying enough to rebuild also meant carrying provenance for values,
statements and folds, and a value table (`RbcFunction::values`) naming what
each owner-local ordinal is.

Pinned by `crates/rumoca/tests/suite_core/bitcode_function_import.rs`: nested
folds, a conditional join, an assertion, a multi-output call, quotients and a
parameter extent all round-trip function for function and simulate.

Measuring it over the corpus surfaced five defects outside the body code
itself, each fixed and recorded: TOOLBUG-037 (export snapshotted sources too
early), 038 (parameters defined too late for a parameter extent), 039
(dynamic quotient owners), 040 (`build::operands` skipped subscripts) and 041
(`link` did not relocate bodies).

## What this does not do

It does not make the frontend minimal by itself. It removes the reason the
frontend's function-body lowering cannot cross the interchange boundary,
which is what lets `fold_pure_constant_calls`, `specialize_function_inputs`
and `specialize_package_constants` become passes instead of frontend steps —
the three rows in minimal-frontend.md's inventory marked "needs function
bodies: **yes**".
