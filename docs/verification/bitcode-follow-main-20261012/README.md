# Bitcode integration with upstream main

`bitcode-follow-main-20261012` is the full integration checkpoint branch. It merges
`rumoca-bitcode-v1` at `4b4a31c0e` with upstream main at `0b249f0f7` and the
FMI reduction fix at `e92faa513`. Both parent histories are retained.
The user authorized committing and pushing this checkpoint on 2026-10-12 after
reviewing the unresolved gate results. This publication preserves those failures;
it does not certify the integration for release or clear the upstream merge gate.
The existing `rumoca-bitcode-follow-main`, `rumoca-bitcode-v1`, and
`fix-fmi-typed-reductions` branch tips remain unchanged by this publication.

`fix-fmi-typed-reductions` is the separate upstream PR branch: one signed-off
commit on upstream main, touching five files. Its PR must target
`CogniPilot/rumoca:main`. Do not use the full integration branch as that PR's
head. The completed [PR review template](fmi-pr-body.md) is committed here. Local
validation evidence is under `target/verification/fmi-follow-main-20261012/`.
The branch is pushed; opening the GitHub PR still requires an authenticated
GitHub session. [Open the comparison](https://github.com/CogniPilot/rumoca/compare/main...mhcho1994:fix-fmi-typed-reductions?expand=1).

## Retained functionality

The merge retains the public bitcode v2 crate and Python SDK, bitcode CLI,
equation/execution passes, artifact linking and replay, ModelSan, solver
numerical diagnostics, parameter profiles, and the source branch's compiler
regression tests. New upstream checked IR and lowering implementations own
fold continuation, tensor slices, operator records, native bodies, and compact
Jacobians. Older equivalent operator-record and FMI renderers were removed.

The public bitcode schema is unchanged. Upstream terminal-print event actions
and bounded-while continuations cannot yet be represented by v2: export
explicitly rejects them instead of losing their effects. The ordinary source
compiler and simulation paths retain upstream support. These constructs were
not supported by the original v1 branch's compiler.

## Compiler policy

The requested conflict policy is **upstream main semantics first**. Bitcode and
ModelSan remain available, but this branch does not retain v1's compatibility
exceptions to current compiler checks. In particular it restores main's checks
for scalar `each` (ER104), invalid Evaluate scope (ER070), function parameter
inputs, partial-class instantiation, incompatible class inheritance, clocked
reads, outer modifications, and cyclic parameter bindings. The corresponding
upstream negative tests are retained. Library models that depended on those
exceptions can now be rejected; that is the explicitly selected policy.

## Merge adaptations

- Preserve loop assertions while adopting upstream fold continuation and
  per-call-owner lowering. Preserve both the lexical fold parent query needed
  by bitcode and the new continuation query.
- Extend the existing FMI dependency proof to tensor slices and continuation
  predicates; both receive focused regression tests.
- Keep upstream's shared fold/map emitters and the corrected first-element
  reduction emitter from the isolated PR.
- Route v1 assertion/when comprehensions through upstream's common expression
  visitor; retain replayed initial-algorithm comprehension planning.
- Carry Flat structural-parameter evidence into model-loop normalization, so
  a parameter fixed by an array dimension remains a valid loop bound.
- Keep continuous/discrete runtime values and their initial guesses out of the
  typecheck structural-value environment (MLS 10.1 and 8.6).
- Preserve opt-in parameter freezing while respecting inherited Evaluate=false.
- Adapt bounded solver diagnostics to compact Jacobian storage without
  densifying matrices when diagnostics are disabled or the size limit is exceeded.
- Reserve EF036 for upstream's foreign-state diagnostic and assign EF038 to
  v1's constant-index-out-of-bounds diagnostic. Keep its negative regression.
- Reserve EF033 and ED022/ED023 for upstream diagnostics; allocate EF037 for
  expandable source counts and ED024/ED025 for v1 derivative/cyclic-binding errors.
- Rebuild dependency inputs from the fully resolved source-root graph before
  closure selection. Per-root cached summaries cannot resolve cross-root names;
  the warm-cache contract still has a populated graph after resolution.
- Keep scalar and empty reductions at upstream's checked reduction owner; only
  nonempty array-valued reductions take the existing array expansion path.
- Handle conditional operator-record equations and operands inside upstream's
  operator-record resolver, preserving field counts without a second renderer.
- Split oversized modules by their existing responsibilities: component shapes,
  DAE parameter values, simple-equation expansion, and contract test groups.
- Renumber the v1 proposed analysis-pass document to SPEC_0055 because upstream
  owns SPEC_0052; its status and content are unchanged.
- Update instance-name assertions for upstream's folded String literals; the
  expected instance names and modifier scope are unchanged.

## DAE construction size review

The measured IR is 18,830 core + 5,572 wire = 24,402 physical production lines.
The combined ceiling remains 24,500; the core sub-budget is adjusted from
18,750 to 19,000 to accommodate the retained exporter/replay queries. Module
review against upstream main:

| Module | Added / deleted | Retained purpose |
| --- | ---: | --- |
| `expression/type_rules.rs` | 57 / 20 | v1 shaped integer conversion and Boolean extrema; one shared builtin dispatch |
| `model.rs` | 17 / 0 | current-definition readback for ordered bitcode replay |
| `model/view.rs` | 23 / 0 | complete domain enumeration and lexical fold parent ordinal for export |
| `model_event_transactions.rs` | 16 / 1 | read-only event target/trigger/guard/definition queries for passes |
| `expression/call_nodes.rs` | 1 / 1 | retain declared function purity metadata |
| `model/function_conditionals.rs` | 2 / 2 | retain the v1 replay helper contract |

The net increase is 92 lines. There is no parallel wire format or decode path.
The first compression pass reuses upstream continuation and slice owners and
removes the superseded operator-record implementation. Further DAE shrinkage
remains part of the existing Solve Algorithm Block cutover, not this FMI PR.

## Continuing to follow upstream

Fetch and merge upstream main into the integration branch, resolve at the
current IR owners, then run the retained Rust, Python, FMI and MSL checks.
Keep the original v1 branch as the historical source.

For each upstream contribution, start a descriptive branch from upstream/main
and cherry-pick only its reviewed fix and tests. Check the resulting three-dot
diff against upstream/main before pushing. A merge of the integration branch
into a PR branch would import unrelated bitcode and ModelSan history.

## Validation

The final verification record is maintained alongside this document and in
`target/verification/bitcode-integration-20261012/`. Cargo commands use
`CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 RAYON_NUM_THREADS=4`.


- `cargo fmt --check`, workspace/all-targets/all-features Clippy with warnings
  denied, and `cargo doc --no-deps`: passed.
- Final workspace: **10432 passed, 13 failed, 23 ignored** across
  230 result groups. Every remaining failure also reproduces on original v1;
  all 13 are v1-added tests absent from upstream. No new failing test remains.
  [The exact failure inventory](preexisting-failures.json) and the local baseline
  logs preserve that distinction. The suite is **not** claimed green.
- ModelSan: 347 passed, 3 skipped; bitcode Python/link/execution: 50 passed.
  Two existing ModelSan pytest collection warnings remain.
- Actual FMI3 package/build/Co-Simulation execution: 7 passed, including the
  four parameter reduction cases from the isolated PR.
- Fixed canary: same classifications and channel counts as upstream for all
  20 models: 14 strict-high, 1 excluded, 3 not attempted, 2 simulation failures.
  One high-band model's mean normalized L1 differs by approximately 9.28e-19;
  maximum error and all 45 channel classifications are unchanged.
  [Exact comparison](canary-delta.json).
- ModelicaTest's selected `ModelicaTest.Blocks.MuxDemux` target: compiled,
  simulated, and compared strict-high (1/1). This is the repository's selected
  semantic gate, not a claim about the entire ModelicaTest library.
- Pinned `modelica_models`: `Tests.All` compile and SO2/SE2 assertion smoke
  simulations passed after the final compiler policy changes.

The source-level failures inherited from v1 include matrix-equation scalar
counting, a redeclared short function alias, nominal-subtype checking, captured
array extents, four inherited package-constant cases, two structural-assert
expectations, and three derivative/index-reduction cases. They remain enabled
and require separate fixes. The standalone FMI PR does not carry this code or
these failures; its workspace run is green.

[Verification summary](verification.json) records the exact source revisions.
[The FMI upstream audit](fmi-upstream-audit.json) explains what was ported and
what upstream already provides.


## Unresolved integration gate

The complete 566-model MSL gate **failed**. It reports 318 strict-high, 1 near,
53 excepted/excluded, 131 not attempted, 1 missing reference, and 62 simulation
failures. These terms classify the full cohort: strict-high means every
compared channel clears the strict criterion; excepted rows are outside that
claim; near is a completed but unclassified simulation, not a verified result.

Two blockers remain:

1. The promoted baseline uses OMC `a96aa1a-cmake`, while this host uses
   `OpenModelica 1.26.1`. No baseline was changed or promoted.
2. `Modelica.Electrical.Machines.Examples.ControlledDCDrives.SpeedControlledDCPM`
   completes but is near: 348/352 channels are high and four deviate. All 352
   initial-condition channels are high. Upstream/FMI-only rejects this model
   in Flatten (`tauMax` has no structured identity); integration admits it.
   Two anti-windup channels have mean absolute error about 1.006e-6. Their
   reference 5th-to-95th percentile range is zero, so the comparator uses its
   1e-6 scale floor. This does not mean the raw reference is identically zero:
   both traces contain brief pulses of about 19.617. Two inverter-feedback
   aliases have mean absolute error about 6.263e-4 over a reference percentile
   range of 7.999e-4. Small absolute residuals
   do not establish correctness under the current trace contract. Their root
   cause is not established, and no exception or tolerance change was made.

[Full gate facts and affected channels](msl-full-review.json) retain this open
counterexample. SPEC_0033 §6a requires a general fix or typed profile rejection
and blocks merging while the counterexample remains actionable. Following the
user's explicit instruction after reviewing these failures, the integration merge
is recorded and published only as a non-release checkpoint. The counterexample
remains open and the quality gate remains failed. The independent FMI PR
retains its earlier green workspace and unchanged 20-model canary; its only
full-gate blocker is the OMC baseline context mismatch.


The same full run also loses upstream's strict-high `CauerLowPassSC` result to
its unchanged 12-second simulation timeout. Thirteen other models become
strict-high. Neither aggregate growth nor a retry is used to clear the open
counterexample or to claim full no-regression.

Focused reproduction of the open numerical case:

```bash
CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 RAYON_NUM_THREADS=4 \
  cargo xtask verify msl-parity \
  --results-dir target/verification/speed-controlled-dcpm \
  --sim-targets-file docs/verification/bitcode-follow-main-20261012/counterexample-target.json
```

The failing aliases are anti-windup correction and inverter power-feedback
signals. The comparator uses the reference 5th-to-95th percentile spread, so short
pulses can be absent from the normalization range while still contributing to
the error integral. This mechanism is confirmed in `sim_trace_compare/normalization.rs`;
it is a diagnostic lead, not proof that the numerical result is correct. No
model-specific exception has been requested or installed. Raw overlays and
alias checks are in the local verification directory as
`speed-controlled-dcpm-review.png` and `speed-controlled-dcpm-review-stats.json`.
