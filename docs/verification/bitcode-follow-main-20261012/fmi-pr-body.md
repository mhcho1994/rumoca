# Rumoca PR Review Template

## Branch Naming

- `fix-fmi-typed-reductions`, based on `CogniPilot/rumoca` main at `0b249f0f7`.

## Summary

- FMI 2/3 C generation now supports typed binary64 `sum`, `product`, `min`, and `max`, plus Boolean `all`, inside pure functions. A dependent parameter such as `gain = reduce(scale)` previously compiled but failed export with `unsupported-feature:fmi.c.pure-call: tensor reduce`.
- Extracts the remaining reproduced FMI-generation fix from `rumoca-bitcode-v1` (`4b4a31c0e`). Current upstream already implements the earlier scalar-fold and typed-map emitters and passes the original nested-parameter-fold/FastDyn cases.
- Addresses SPEC_0007's executable target contract and SPEC_0044 ME-PARAM-001. The implementation follows the existing typed evaluator's first-element accumulation order and shared Real extrema rules.

## Spec / MLS Alignment

- Relevant active specs checked: SPEC_0007 / SPEC_0040 (including SOLVE-C66), SPEC_0021, SPEC_0022, SPEC_0025, SPEC_0029 / SPEC_0041, and SPEC_0033. FMI design context: draft SPEC_0038 and its SPEC_0044 catalog, particularly ME-PARAM-001.
- MLS: §10.3.4, reduction functions/operators; §11.2.2 for the retained nested-loop regression; §8.6 for dependent parameter initialization. Existing language semantics are preserved. See https://specification.modelica.org/maint/3.6/arrays.html#reduction-functions-and-operators.
- Crate/phase owner: `rumoca-phase-codegen` owns the shared FMI C templates; `rumoca/tests/suite_core` owns the source-to-target regressions.

## Risk and Design Notes

- Main correctness risk: reduction initialization or reassociation can change NaNs, signed zeros, and rounding. Generated C seeds from the first element and combines subsequent elements in order, exactly as the typed evaluator does. Min/max reuse the existing shared Real arithmetic macro.
- Main maintenance risk: the emitter must track the checked typed-operation profile. Integer reductions, Boolean min/max, and binary32 reductions still fail closed; regression tests pin that boundary.
- The change belongs in codegen because the source model and checked program already compile successfully; the first failure is the missing `reduce` branch in the shared FMI tensor renderer.
- No new public API, dependency, abstraction, migration, or alternate compiler path.

## Testing

All Cargo verification used `CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 RAYON_NUM_THREADS=4`.

- Passed: `cargo fmt --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo test --workspace`; `cargo doc --no-deps`.
- The first workspace invocation stopped at four unrelated eFMI schema checks because `xmllint` was missing. Extracted Ubuntu `libxml2-utils` under `/tmp/rumoca-fmi-tools`, added its bin directory to PATH, reran the 105-test eFMI suite successfully, and then completed the entire workspace suite successfully.
- Passed: `cargo test -p rumoca-phase-codegen -p rumoca-ir-solve` (263 codegen and 593 Solve unit tests, plus integration/doc tests).
- Supplemental local FMI3 validation: built four FMUs and changed `scale` from 2 to 5 at initialization; after 100 Co-Simulation steps of 0.01 s, sum/product/min/max cases reached 30/750/5/15 respectively. All four passed through the existing FastDyn importer/build harness.
- Tier 1: ran `cargo xtask verify msl-parity --sim-targets-file infra/verification/msl-canary-20.json` on both `0b249f0f7` and `e92faa513`, with separate result directories. Model-by-model band/channel delta is empty. Each partial canary has 14 compared/strict-high, 0 near/deviation, 1 skipped/excluded, 0 missing traces, 3 not attempted, and 2 simulation failures; all statuses are identical. This is not a full-cohort parity claim.
- Full 566-model MSL gate executed with `cargo xtask verify msl-parity --results-dir target/verification/fmi-follow-main-20261012/msl-full`, at `e92faa513`. The comparator completed, but the quality gate failed closed: the resolved baseline requires OMC `a96aa1a-cmake`, while this host runs `OpenModelica 1.26.1`. **A no-regression claim against the promoted baseline cannot be made; the MSL checkbox remains open.** No baseline was changed or promoted. A run in the baseline-matching OMC environment is required before merge. The official xtask wrapper invokes the same Cargo-native `test_msl_all` using the repository `msl-fast` profile; the literal `--release` invocation was not separately repeated.
- Remote CI has not run on a PR yet.

- Regression coverage: source-to-FMI 2/3 rendering for all four Real reductions and nested parameter folds; compiled C execution for ordinary values, cancellation order, signed zeros, NaNs, and Boolean `all`; unsupported arithmetic profiles remain rejected.
- Before/after: the new source-level reduction regression fails at FMI C rendering on `0b249f0f7` and passes with this change. The original three FastDyn FMI3 package/Co-Simulation tests already pass on upstream and are not claimed as newly fixed.
- `cargo xtask verify gate --rev e92faa513 --coverage` has not been run; its isolated pre-landing/coverage gate and remote CI remain required before merge. This PR does not claim a `GATE_OK` result.
- The separate ModelicaTest/pinned modelica_models semantic gates were not run: this change renders existing checked operations and changes no frontend, Solve lowering, or native simulation semantics.

## Code Size Budget (required)

- production_lines_added: 10
- production_lines_deleted: 2
- test_lines_added: 244
- test_lines_deleted: 0
- public_items_added: 0
- public_items_removed: 0
- files_touched: 5
- net_added_lines: 252

Measured against the actual PR base with `git diff --numstat upstream/main...HEAD` (`origin/main` is an older fork revision). The two `#[cfg(test)]` module-registration lines and the suite registration are counted as test lines. A declaration diff over the changed files finds no added or removed public items.

- Growth is required for the missing renderer dispatch and executable regression evidence: production grows by 8 lines; tests account for 244 lines.
- First compression pass: consolidated the source branch's separate reduction implementations into one operator table and loop, reusing the existing binary arithmetic macro. Avoided duplicating the upstream fold/map emitters or importing the old scalar dependency checker.
- Follow-up cleanup: none planned for this bounded change.

## Reviewer Checklist

- [x] Relevant active specs were checked.
- [x] MLS-sensitive changes cite the right MLS section.
- [x] Crate boundaries and phase ownership preserved (SPEC_0029).
- [x] Tests prove behavior or explain the remaining gap.
- [ ] Standard CI gates pass (`fmt`, `clippy -D warnings`, `cargo test`, `cargo doc`) — all local commands passed; remote CI is pending.
- [ ] MSL gate run for compiler/simulator changes; no regression vs baseline.
- [x] Size-budget section completed.
- [x] Positive net diff has explicit compression justification.
- [x] New APIs are required and minimal (none added).
- [x] Old/new parallel paths removed unless explicitly migrating.
- [x] No `#[allow(clippy::...)]` added outside generated code.
- [x] Every commit signed off (`git commit -s`); no named AI assistant/session references or AI `Co-Authored-By` trailers.
- [x] External material (if any) attributed and Apache-2.0 compatible (no new third-party material).
