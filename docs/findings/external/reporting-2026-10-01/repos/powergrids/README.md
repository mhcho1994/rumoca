# PowerGrids — findings report

Review date: 2026-10-01. **No maintainer-ready issue established.** No upstream issue has been posted by this review.

Repository: [PowerGrids/PowerGrids](https://github.com/PowerGrids/PowerGrids)

Evaluated revision: [`1858e8b1d5234a3d9506e89da89c3c1df793b866`](https://github.com/PowerGrids/PowerGrids/tree/1858e8b1d5234a3d9506e89da89c3c1df793b866). Source checkout was unmodified when inventoried.

## Findings and reporting decision

- **Explained alarms:** two large timer values in tap-changer logic represent explicit time-infinity sentinels. They are not evidence of numerical overflow or a new library defect.
- **Hold unit candidates:** three unit-review instances involving integrator gain metadata require dependency-version and unit-modifier context. No wrong trajectory has been established.
- **Hold static candidates:** zero-denominator witnesses and vanishing derivative coefficients dominate the static rows. Prove that the parameter assignment is allowed and distinguish an intentional algebraic limit from a defect before reporting.

Next action: choose a concrete allowed parameter configuration for the highest-impact static candidate and reproduce it in a connected model with an independent compiler. No maintainer-ready issue has been established.

## Retained campaign coverage

| Measure | Count |
| --- | ---: |
| Static model records | 25 |
| Static finding instances | 866 |
| Runtime model attempts | 26 |
| Runtime records marked ran | 17 |
| Runtime records marked no-dynamics | 8 |
| Runtime records marked did-not-run | 1 |
| Runtime finding instances (including failed runs) | 17 |
| Total finding instances | 883 |

Counts describe retained historical results, not today's full repository coverage. A completed execution is not a correctness or parity result. An unavailable static result is not zero static defects. Findings can repeat across instances and phases; these counts are not unique bug counts.

## Disposition of every finding

The [repository-only CSV](findings.csv) retains all 883 instances, including model, phase, severity, held-input flag, evidence, original result path and disposition. Its rows are an exact partition of the [combined inventory](../../finding-routing.csv).

| Disposition | Instances | Meaning |
| --- | ---: | --- |
| `hold-needs-source-and-witness-review` | 847 | Needs source review and a valid concrete witness. |
| `standalone-topology-not-defect-proof` | 21 | Needs intended containing model and connection context. |
| `source-unit-review` | 3 | Source unit/metadata candidate; assess dimensions and intended behavior. |
| `execution-not-completed` | 3 | Execution failed; identify toolchain versus model responsibility. |
| `hold-input-contract-validation` | 3 | Held inputs need validation against the model contract. |
| `hold-check-unset-parameters-and-inputs` | 2 | Check required parameter bindings and legal inputs. |
| `advisory-or-explicit-non-defect` | 2 | Advisory or explicitly non-defect evidence; not a confirmed bug. |
| `source-sentinel-or-protection-not-defect-proof` | 2 | Source sentinel or protection explains the alarm. |

## Representative unresolved evidence

These examples identify follow-up work; they are not additional confirmed defects. Full evidence and all remaining models are in the repository CSV.

- `PowerGrids.Controls.FirstOrderWithNonWindupLimiter` — `initialization-failure` (runtime-v3); `execution-not-completed`.
- `PowerGrids.Controls.FirstOrderWithNonWindupLimiter` — `network-boundary-port` (runtime-v3); `execution-not-completed`.
- `PowerGrids.Controls.FirstOrderWithNonWindupLimiter` — `network-observation-incomplete` (runtime-v3); `execution-not-completed`.
- `PowerGrids.Controls.Test.TestDerivativeLag` — `divisor-zero-at-declared-values` (historical-static); `hold-check-unset-parameters-and-inputs`.
- `PowerGrids.Controls.IntegratorWithNonWindupLimiter` — `network-boundary-port` (runtime-v3); `hold-input-contract-validation`.

## Evidence boundaries

This per-repository report reorganizes the existing review; it does not add new compiler runs or independently validate every row. The named shortlist above defines the source-review and fresh-reproduction scope. Input hashes, all pinned revisions and aggregate counts are in [inventory.json](../../inventory.json). Current-source reads and targeted duplicate searches are in [upstream-evidence.json](../../upstream-evidence.json); absence from those searches is not an exhaustive novelty claim.

[All repository reports](../../README.md#per-repository-reports)
