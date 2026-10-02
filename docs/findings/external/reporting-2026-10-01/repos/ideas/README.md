# IDEAS — findings report

Review date: 2026-10-01. **Shared humidity fixture and unit candidates; local reproduction pending.** No upstream issue has been posted by this review.

Repository: [open-ideas/IDEAS](https://github.com/open-ideas/IDEAS)

Evaluated revision: [`3c12883555885bcd5f17979d9028eadf94b21d92`](https://github.com/open-ideas/IDEAS/tree/3c12883555885bcd5f17979d9028eadf94b21d92). Source checkout was unmodified when inventoried.

## Findings and reporting decision

- **Shared fixture candidate:** `IDEAS.Controls.OBC.CDL.Psychrometrics.Validation.DewPoint_TDryBulPhi` and `SpecificEnthalpy_TDryBulPhi` contain the same humidity ramp pattern: offset 0.001 plus height 1 reaches 1.001. The historical runtime findings and source review identify these copies; the fresh independent reproduction and corrected controls were run against Buildings, not IDEAS.
- **Shared unit clarification candidate:** the copied CDL `Reals.LimitSlewRate` and `Reals.Ramp` declarations use rate-typed expressions for time-typed `Td`. This is source-reviewed copy evidence; it is not an independent demonstration of incorrect IDEAS dynamics.
- **Hold:** derivative-validation range excursions and held-input violations need intended-domain review before becoming reports.

Next reporting action: track the [shared humidity draft](../../buildings-humidity-draft.md) and [shared time-constant draft](../../buildings-td-units-draft.md), then independently reproduce the local fixtures and coordinate the source sync with this repository's maintainers. Do not present the Buildings control run as a local verification. No issue has been submitted by this review.

## Retained campaign coverage

| Measure | Count |
| --- | ---: |
| Static model records | Not available in retained static results |
| Static finding instances | 0 |
| Runtime model attempts | 426 |
| Runtime records marked ran | 316 |
| Runtime records marked no-dynamics | 13 |
| Runtime records marked did-not-run | 97 |
| Runtime finding instances (including failed runs) | 409 |
| Total finding instances | 409 |

Counts describe retained historical results, not today's full repository coverage. A completed execution is not a correctness or parity result. An unavailable static result is not zero static defects. Findings can repeat across instances and phases; these counts are not unique bug counts.

## Disposition of every finding

The [repository-only CSV](findings.csv) retains all 409 instances, including model, phase, severity, held-input flag, evidence, original result path and disposition. Its rows are an exact partition of the [combined inventory](../../finding-routing.csv).

| Disposition | Instances | Meaning |
| --- | ---: | --- |
| `execution-not-completed` | 228 | Execution failed; identify toolchain versus model responsibility. |
| `standalone-topology-not-defect-proof` | 122 | Needs intended containing model and connection context. |
| `hold-input-contract-validation` | 49 | Held inputs need validation against the model contract. |
| `advisory-or-explicit-non-defect` | 4 | Advisory or explicitly non-defect evidence; not a confirmed bug. |
| `source-validation-input-range-review` | 3 | Review fixture input range and test intent. |
| `coverage-gap` | 3 | Missing observations/coverage; not affirmative defect evidence. |

## Representative unresolved evidence

These examples identify follow-up work; they are not additional confirmed defects. Full evidence and all remaining models are in the repository CSV.

- `IDEAS.BoundaryConditions.Climate.Time.BaseClasses.LocalTime` — `initialization-failure` (runtime-v3); `execution-not-completed`.
- `IDEAS.BoundaryConditions.SkyTemperature.BlackBody` — `simulation-failure` (runtime-v3); `execution-not-completed`.
- `IDEAS.Experimental.Electric.BaseClasses.Converters.Examples.converterPower` — `division-out-of-domain` (runtime-v3); `execution-not-completed`.
- `IDEAS.BoundaryConditions.SolarIrradiation.RadiationConvertorSimplified` — `network-boundary-port` (runtime-v3); `execution-not-completed`.
- `IDEAS.Buildings.Components.BaseClasses.ConservationOfEnergy.PrescribedEnergy` — `network-component-isolated` (runtime-v3); `execution-not-completed`.

## Evidence boundaries

This per-repository report reorganizes the existing review; it does not add new compiler runs or independently validate every row. The named shortlist above defines the source-review and fresh-reproduction scope. Input hashes, all pinned revisions and aggregate counts are in [inventory.json](../../inventory.json). Current-source reads and targeted duplicate searches are in [upstream-evidence.json](../../upstream-evidence.json); absence from those searches is not an exhaustive novelty claim.

[All repository reports](../../README.md#per-repository-reports)
