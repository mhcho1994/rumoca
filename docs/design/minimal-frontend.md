# A minimal frontend: lowering in the compiler, optimization in passes

**Goal.** The frontend should lower Modelica to canonical form and stop.
Anything that *changes* a model to make it smaller, cheaper or more folded
belongs in a bitcode pass, where it can be tested, reordered, skipped, and
blamed individually.

**Why now.** Every serious breakage this project has recorded in the compiler
came from an optimization living inside lowering, not from lowering itself:

| Defect | What it was |
|---|---|
| [TOOLBUG-029](../toolbugs/TOOLBUG-029-folding-deleted-functions-from-the-artifact.md) | `fold_pure_constant_calls` deleted declared functions from the DAE; 25 tests |
| [TOOLBUG-028](../toolbugs/TOOLBUG-028-constant-folding-discards-the-unit.md) | constant folding dropped `mu_0`'s unit, making dimensional analysis unsound |
| [TOOLBUG-030](../toolbugs/TOOLBUG-030-evidence-defaulted-to-empty.md) | a new diagnostic defaulted missing evidence to "clean" |

The pipeline itself shows the cost. `finalize_flat_model` runs
`mark_record_constructor_calls` three times, and `collect_functions`,
`canonicalize_collected_function_calls`, `materialize_flat_function_call_args`,
`inject_referenced_qualified_class_constants` and
`substitute_known_constants_in_flat` twice each. It iterates because each
optimization invalidates lowering that already ran. A lowering-only frontend
runs each step once.

## The enabling fact

`rumoca_bitcode::import()` already reconstructs a full DAE from an artifact,
and both simulation and DAE/FMI code generation already run from it
(`bitcode_cli::run_compile_bitcode`). So this path works today:

```
bitcode --> DAE --> simulate / codegen
```

What is missing is a stage, not a capability:

```
AST -> Instance -> Flat(lowering) -> RBC -> [pass pipeline] -> DAE -> Solve/codegen
                                            ^^^^^^^^^^^^^^^
                                            does not exist yet
```

Passes today are external programs reading `.rbc`
([writing-a-bitcode-pass.md](../writing-a-bitcode-pass.md)). Moving an
optimization out of flatten without an in-compiler pass stage would simply
lose it for `rumoca compile`.

## Inventory

Classified by one question: *does the step make the model canonical, or does
it make it cheaper?* Only the second kind moves.

### Stays — lowering

| Step | Why it is lowering |
|---|---|
| `collect_functions`, `canonicalize_collected_function_calls` | resolves what a call names |
| `mark_record_constructor_calls`, `canonicalize_varrefs_via_record_aliases` | record construction is a different node kind, not a cheaper one |
| `lower_record_function_params`, `materialize_flat_function_call_args` | the executable ABI; a call has positional slots or it cannot run |
| `normalize_record_array_field_access_bindings`, `drop_invalid_field_access_bindings`, `propagate_unexpanded_record_array_dims` | record/array shape normalization |
| `collapse_index_refs_to_known_varrefs` | MLS §10.5: an array of components denotes the array of its members. Semantics, not a saving |
| `recover_indexed_lhs_dimensions` | recovers a declared shape the earlier IR lost |
| `canonicalize_flat_enum_literals`, `redirect_outer_refs` | identity resolution |
| connection expansion | `connect` becomes equations, MLS §9.2 |

### Moves — optimization

| Step | Needs function bodies? |
|---|---|
| `inject_referenced_qualified_class_constants` | no |
| `substitute_known_constants_in_flat` | no |
| `fold_structural_initial_asserts` | no |
| `postprocess/function_shape_constants` | partly — parameters only |
| `prune_unreachable_functions` | no, but needs call edges |
| `fold_pure_constant_calls` | **yes** |
| `specialize_function_inputs` (higher-order) | **yes** |
| `package_constants::specialize_package_constants` | **yes** |

## The constraint that shapes the plan

`RbcFunctionBody` has exactly two forms: `ElidedModelica` and `External`. **A
Modelica function body is never carried in the artifact.** That is deliberate
and load-bearing: it is what makes the IR total. With no bodies there is no
recursion and no unbounded iteration to represent, which is the property the
totality argument rests on.

So the last three rows above cannot become bitcode passes as things stand.
They need the bodies they evaluate.

## Staging

**Stage 1 — build the stage, move what already fits.** Add an in-compiler pass
pipeline between export and import, and move constant injection, constant
substitution and structural assert folding into it. No schema change. This
alone removes both of the doubled constant steps from `finalize_flat_model`.

**Stage 2 — call edges.** Add a `calls: Vec<FunctionId>` to `RbcFunction`: the
call graph without the bodies. Moves `prune_unreachable_functions`, and pays
for itself elsewhere — reachability over callables is something every consumer
currently has to rebuild.

**Stage 3 — a decision, not a task.** Whether to carry Modelica function
bodies. Carrying them unlocks call folding, higher-order specialization and
package-constant specialization as passes, and makes the frontend genuinely
minimal. It also makes recursion representable, which ends the IR's totality
property as currently argued. The alternatives are to keep those three in the
frontend as an explicitly-marked optimization block, or to carry bodies in a
separate optional section whose presence is what makes an artifact non-total.

Stage 3 is the only part that trades away something real, and it should be
decided deliberately rather than arrived at.

## Status and measured reclassification (2026-09-29)

**The stage exists.** `rumoca compile --pass NAME` routes the frontend's DAE
through `DAE -> RBC -> [passes] -> DAE` (`crates/rumoca/src/pass_stage.rs`,
`crates/rumoca-bitcode/src/passes.rs`). Passes today: `prune-functions`,
`fold-constants`, `dead-expressions`; `default` runs all, `none` round-trips
with no rewrite. The pipeline removes dead expressions, revalidates and
rebuilds through the checked constructors after every pass. Stage 2 (call
edges) and Stage 3 (bodies, including import) are done; see
[carry-function-bodies.md](carry-function-bodies.md).

**The stage is opt-in, for a measured reason.** Routing the whole test suite
through it (`--pass none`, every compile) left ~50 tests failing beyond the 4
known, and every one is an explicit refusal, not a wrong result: owner tables
bitcode v2 does not carry (`model_event_transactions`, `previous_values`,
`delays`, `structured_roots`), plus element-wise operators and
`clock_transfer`, both since added (TOOLBUG-042 and this change). Until the
four owner tables are carried, a default-on stage would refuse models the
frontend compiles.

**The inventory above was wrong about three of its seven rows.** Each step
was switched off in turn and 1338 tests (flatten, compile and DAE crates;
MSL-simulation, example, heavy-solve and template suites) were run:

| Step | Failures when off | What it actually is |
|---|---:|---|
| `fold_pure_constant_calls` | 0 | optimization -- **moved**: now the `fold-pure-calls` pass; its one diagnostic (EF032, a settled binding indexing out of bounds) stays in the frontend as `check_settled_binding_bounds` |
| `inject_referenced_qualified_class_constants` (late) | 2 | lowering: package constants must be materialized |
| `fold_structural_initial_asserts` | 4 | the EF030 translation diagnostic, plus folding of proven-true asserts |
| `substitute_known_constants_in_flat` | 7 | **lowering**: without it DAE construction fails, `ED008 unresolved Flat reference P.Constants.pi` |
| `specialize_function_inputs` | 9 | **lowering**: a function-typed input cannot reach a DAE ("reachable function apply retains a functional input") |
| `prune_unreachable_functions` | 17 | **lowering**: the functions it drops are the higher-order and package-constant templates specialization superseded, which the DAE cannot hold; the `prune-functions` pass repeats it after folding, where it is an optimization |

So the minimal frontend keeps constant materialization, constant
substitution, higher-order specialization and template pruning: they make
the model *representable*, which is the frontend's job by this document's
own test. Pure-call folding has moved. What remains in the frontend that is
purely an optimization is the proven-true half of structural assert folding,
which is entangled with the EF030 diagnostic that must stay.

`fold-pure-calls` is narrower than the frontend step was: it folds scalar
results of calls whose arguments are literals, where the frontend also
folded array results and used frozen parameter values. The model is the same
either way -- an unfolded binding is evaluated at initialization -- and the
evaluator (`passes/evaluate.rs`) fails closed on anything it does not model.
