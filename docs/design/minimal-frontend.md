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

## Status (2026-09-30)

**The stage exists and is on by default.** A plain `rumoca compile` routes the
frontend's DAE through `DAE -> RBC -> [default group] -> DAE`
(`crates/rumoca/src/pass_stage.rs`, `crates/rumoca-bitcode/src/passes.rs`).
`--pass NAME` (repeatable, in order) replaces the group; `--pass none`
compiles the model exactly as the frontend lowered it, skipping the stage;
`--pass round-trip` exports and rebuilds with no rewrite, which is how the
stage's fidelity is measured. Each pass is followed by dead-expression
removal, revalidation and a rebuild through the checked constructors.

| Pass (default order) | What it does |
|---|---|
| `inline-constants` | a reference to a scalar constant with a literal binding becomes the literal (structural parameters stay parameters: the galec projection folds calls over them itself) |
| `fold-constants` | scalar operators over literals; relation roots stay relational |
| `fold-pure-calls` | scalar results of calls with literal arguments, by the fail-closed evaluator |
| `fold-asserts` | an `assert` whose condition is true from literals, constants and structural (`Evaluate`/`final`) parameters is removed |
| `prune-functions` | functions nothing reaches, over the exported call graph |
| `dead-expressions` | expressions nothing references |

**Why default-on is now safe, measured.** Every compile in the suite was
forced through the stage, first as a bare round trip, then with the whole
default group. Only the known environment failures remained once two
defects it exposed were fixed: bitcode export was exponential on nested loops
(TOOLBUG-065) and the exported call graph missed calls with literal arguments,
so `prune-functions` deleted a live function (TOOLBUG-066). The four owner
tables that kept the stage opt-in are carried. Compiling
`Modelica.Mechanics.MultiBody.Examples.Elementary.DoublePendulum` and
`Modelica.Electrical.Machines.Examples.InductionMachines.IMC_DOL` (warm
cache, two runs each), the default pipeline took 7.68/7.85 s and 7.71/7.67 s
against 7.59/7.56 s and 7.88/7.24 s for `--pass none`, at the same 2.2 GB
peak: within run-to-run noise. (An earlier figure on
`Modelica.Fluid.Examples.HeatingSystem` timed a compile that fails before the
stage, EI012, and is withdrawn.)

**What moved out of the frontend.**

- *Pure-call folding* is `fold-pure-calls`; its one diagnostic (EF032, a
  settled binding indexing out of bounds) stays as
  `check_settled_binding_bounds`.
- *Package-constant inlining.* A Real scalar package constant owned by its
  declaration is declared once as a model constant under its qualified name
  (`constant Real Modelica.Constants.pi = ...`) and the reference is kept;
  `inline-constants` inlines it. Integer, Boolean and enumeration constants
  are still substituted by the frontend -- they select branches and size and
  index arrays, where lowering needs the literal and dead branches must not
  survive -- as are Real arrays and constants specialized per instance (a
  replaceable `Medium`), which have no single qualified name yet.
- *Proven-true assertion folding.* The frontend keeps the assertion and only
  reports one proven false (EF030, `check_structural_initial_asserts`);
  `fold-asserts` removes the proven-true ones.

**What stays in the frontend, and why.** Switching each step off in turn and
running 1338 tests (flatten, compile and DAE crates; MSL-simulation, example,
heavy-solve and template suites) gave:

| Step | Failures when off | Why it stays |
|---|---:|---|
| `inject_referenced_qualified_class_constants` (late) | 2 | lowering: package constants must be materialized |
| `substitute_known_constants_in_flat` | 7 | lowering for the structural constants above and loop indices; the Real-constant inlining it did is now declaration plus `inline-constants` |
| `specialize_function_inputs` | 9 | monomorphization: a function-typed input cannot reach a DAE ("reachable function apply retains a functional input"), and the solver cannot execute a function value, so as a pass it would be mandatory on every compile |
| `prune_unreachable_functions` | 17 | drops the templates specialization superseded, which the DAE cannot hold; `prune-functions` repeats it after folding, where it is an optimization |

**Still open.**

- `finalize_flat_model` still runs some lowering steps more than once
  (`mark_record_constructor_calls` four times; `collect_functions`,
  `canonicalize_collected_function_calls` and
  `materialize_flat_function_call_args` twice each). The repeats come from the
  ordering of lowering steps, not from optimizations.
- `fold-pure-calls` folds only scalar results with literal arguments, where
  the frontend step also folded arrays and used frozen parameter values.
- Declaring Real array constants and per-instance constants.
