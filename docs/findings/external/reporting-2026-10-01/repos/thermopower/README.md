# ThermoPower — findings report

Review date: 2026-10-01. **One unit/unused-parameter clarification candidate.** No upstream issue has been posted by this review.

Repository: [casella/ThermoPower](https://github.com/casella/ThermoPower)

Evaluated revision: [`e2b011ac7fd90f9cf5771f29f1aefa160550b6ee`](https://github.com/casella/ThermoPower/tree/e2b011ac7fd90f9cf5771f29f1aefa160550b6ee). Source checkout was unmodified when inventoried.

## Findings and reporting decision

- **Cleanup/clarification candidate:** [SecondaryController draft](../../thermopower-mu-draft.md). `Test.mo` declares unused angular-frequency parameter `mu=5*Ts*Pnom/(f0*droop)`, whose expression has different dimensions. Its expression also differs from the active controller coefficient. Two static instances correspond to this one declaration.
- Independent OpenModelica builds with a configured wrapper emitted no unit warning. No effect on the simulated trajectory was demonstrated; do not report this as a broken controller.
- **Hold:** the many initialization/execution failures, standalone connector warnings and parameter-zero witnesses need configured models and independent outcomes.

Next reporting action: review the narrow unused-parameter cleanup question with the maintainer; do not propose changing the active differential equation on this evidence. No issue has been submitted by this review.

## Retained campaign coverage

| Measure | Count |
| --- | ---: |
| Static model records | 77 |
| Static finding instances | 700 |
| Runtime model attempts | 77 |
| Runtime records marked ran | 12 |
| Runtime records marked no-dynamics | 11 |
| Runtime records marked did-not-run | 54 |
| Runtime finding instances (including failed runs) | 300 |
| Total finding instances | 1000 |

Counts describe retained historical results, not today's full repository coverage. A completed execution is not a correctness or parity result. An unavailable static result is not zero static defects. Findings can repeat across instances and phases; these counts are not unique bug counts.

## Disposition of every finding

The [repository-only CSV](findings.csv) retains all 1000 instances, including model, phase, severity, held-input flag, evidence, original result path and disposition. Its rows are an exact partition of the [combined inventory](../../finding-routing.csv).

| Disposition | Instances | Meaning |
| --- | ---: | --- |
| `hold-needs-source-and-witness-review` | 487 | Needs source review and a valid concrete witness. |
| `execution-not-completed` | 262 | Execution failed; identify toolchain versus model responsibility. |
| `standalone-topology-not-defect-proof` | 175 | Needs intended containing model and connection context. |
| `advisory-or-explicit-non-defect` | 43 | Advisory or explicitly non-defect evidence; not a confirmed bug. |
| `hold-input-contract-validation` | 23 | Held inputs need validation against the model contract. |
| `hold-check-unset-parameters-and-inputs` | 8 | Check required parameter bindings and legal inputs. |
| `source-unit-review` | 2 | Source unit/metadata candidate; assess dimensions and intended behavior. |

## Representative unresolved evidence

These examples identify follow-up work; they are not additional confirmed defects. Full evidence and all remaining models are in the repository CSV.

- `ThermoPower.Electrical.Breaker` — `initialization-failure` (runtime-v3); `execution-not-completed`.
- `ThermoPower.PowerPlants.Control.PID` — `simulation-failure` (runtime-v3); `execution-not-completed`.
- `ThermoPower.Test.ElectricalComponents.TestElectrical4` — `network-boundary-port` (runtime-v3); `execution-not-completed`.
- `ThermoPower.Electrical.Breaker` — `network-component-isolated` (runtime-v3); `execution-not-completed`.
- `ThermoPower.Electrical.Breaker` — `network-observation-incomplete` (runtime-v3); `execution-not-completed`.

## Evidence boundaries

This per-repository report reorganizes the existing review; it does not add new compiler runs or independently validate every row. The named shortlist above defines the source-review and fresh-reproduction scope. Input hashes, all pinned revisions and aggregate counts are in [inventory.json](../../inventory.json). Current-source reads and targeted duplicate searches are in [upstream-evidence.json](../../upstream-evidence.json); absence from those searches is not an exhaustive novelty claim.

[All repository reports](../../README.md#per-repository-reports)
