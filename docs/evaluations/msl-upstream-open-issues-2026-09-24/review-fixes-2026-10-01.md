# Frontend and bitcode review fixes — October 1, 2026

This validates local changes on `rumoca-bitcode-v1`, based on
`73c6a7d588cc2d8af04764089edc6910f26e0aa0`. It closes the four reproduced
review issues below. It does not establish that the entire frontend or runtime
is bug-free, and it does not rerun the historical issue census.

## Repairs and regression evidence

| Reproduced issue | Repair | Regression |
|---|---|---|
| Nonstructural Integer parameter bindings lost dependencies even with `--no-fold-parameter-bindings --pass none` | DAE construction preserves the binding; only proven structural array extents are specialized | Save RBC with `k=if a[2]>0 then 3 else 1`; repeatedly override `a[2]` as 2, -1, 2 and observe `y=k+time` as 3.2, 1.2, 3.2 at 0.2 s |
| Runtime failure could return success without SolverSan; static selections unnecessarily simulated | Execution failures contribute coverage gaps independently of sanitizer selection; static-only checks bypass backend preparation/execution; static findings survive preparation failure | Unbound input with NumericSan returns 2/incomplete; dimension-only checking completes without execution; static findings persist across failed preparation |
| Native MoistAir control failed checked lowering | Explicit `--freeze-parameters` specializes pure declaration bindings and numeric attributes using the existing bounded evaluator before DAE construction | Native source and saved-RBC control traces stay at 300 K; reduced composition produces EF032 with source locations |
| Initial compiler process ignored `modelsan check --timeout` | Compilation uses the shared process-group deadline helper | A fake compiler and its child are terminated on timeout; check returns 2 |

Frozen specialization is deliberately an explicit fixed-input profile. Default
compilation retains function bodies and tunable dependencies. Unknown runtime
inputs and `fixed=false` parameters remain unresolved. Evaluator refusals leave
the original expression for checked lowering. This does not implement general
runtime `while` support or permit overriding frozen inputs without recompiling.

## Validation

All commands ran locally with `RAYON_NUM_THREADS=4`; Rust checks also used
`CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4`.

- Compiler, DAE, and flatten library suites: **1,265 passed**.
- Core integration suite: **488 passed, 2 failed** in OpenModelica C compilation
  because the host toolchain could not find `stddef.h`. The affected tests are
  `jacobian_admission_battery::pairs_this_compiler_cannot_run_agree_with_central_differences_under_omc`
  and `jacobian_standard_modelica::the_expanded_battery_agrees_with_finite_differences_under_omc`.
  These comparisons remain unverified in this environment.
- CLI parameter-profile regressions: **4 passed**; array-dimension regressions:
  **10 passed**.
- Full ModelSan suite: **352 passed, 3 opt-in gates skipped**. The opt-in gates
  were run separately against actual MSL 4.1.0.
- Final known-issue gates and campaign tests: **10 passed**, including the
  strengthened control/bounds assertions. Final current-bitcode regressions:
  **22 passed**, including the subsequently added frozen-artifact override test.
- Native bitcode SDK acceptance suite (`new_inst`): **50 passed**.
- Changed-owner library Clippy with `-D warnings`, workspace formatting, and
  `git diff --check` passed.

The native source, native saved-artifact, and OpenModelica known-issue gates each
check six faults and five controls. Native runs use `freeze_parameters=True`,
a 1 s stop time, 0.0025 s output spacing, and a 90 s subprocess timeout. The
MoistAir regression checks every returned control value against 300 K and
requires the native defect's typed EF032 bounds diagnostic and source location.
Other finite wrong results use declared behavioral contracts. This is known-case
coverage, not automatic discovery or general recall.

## Fixed 20-model canary

Before and after used the same command, changing only `--results-dir`:

```sh
CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 RAYON_NUM_THREADS=4 cargo xtask verify msl-parity \
  --sim-targets-file infra/verification/msl-canary-20.json \
  --results-dir /tmp/modelsan-fixes-after --omc-script-mode --no-plots \
  --no-remote-quality-baseline --stage-parallelism 4 --sim-parallelism 4
```

Both runs: **20 selected, 11 compiled, 9 DAE rejections; 9 simulations succeeded,
2 solver failures**. All 20 per-model compile outcomes and diagnostic ownership
were unchanged. Both compared eight traces, all in the high-agreement band,
with one missing OpenModelica reference (`TransformerYD`). These are existing
limitations, not passing models. No full 566-model or cohort parity claim is made.

The [retained before/after evidence](review-fixes-2026-10-01.json) contains
per-model outcomes, trace comparison measurements, source/build hashes, and
hashes of the original local reports. Original full run directories are
`/tmp/modelsan-fixes-before` and `/tmp/modelsan-fixes-after`.
