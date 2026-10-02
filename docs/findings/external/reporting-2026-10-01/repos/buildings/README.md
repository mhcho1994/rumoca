# Buildings — findings report

Review date: 2026-10-01. **One reproduced fixture issue; one unit clarification candidate.** No upstream issue has been posted by this review.

Repository: [lbl-srg/modelica-buildings](https://github.com/lbl-srg/modelica-buildings)

Evaluated revision: [`a3cfdde4e2fa1605f351875c2199b6aafaee7fe0`](https://github.com/lbl-srg/modelica-buildings/tree/a3cfdde4e2fa1605f351875c2199b6aafaee7fe0). Source checkout was unmodified when inventoried.

## Findings and reporting decision

- **Ready for review:** [humidity validation draft](../../buildings-humidity-draft.md). The two psychrometric validation examples reach 1.001 against the input maximum of 1. Fresh OpenModelica runs reproduce the warnings; controls with ramp height 0.999 reach 1.000 without the warnings. This is a low-severity fixture mismatch, not a simulation crash.
- **Clarification candidate:** [CDL time-constant draft](../../buildings-td-units-draft.md). `Reals.LimitSlewRate` and `Reals.Ramp` bind time-typed `Td` to rate-typed expressions. The intended coefficient units/scaling need clarification; independent OpenModelica builds emitted no unit warning.
- **Hold:** derivative-check fixtures outside physical bounds and heat-capacity shade conductance expressions with implicit coefficient units need intent/context review. They are not additional confirmed reports.

Next reporting action: review and submit the humidity draft to Buildings first; coordinate the copied fixtures in IBPSA, IDEAS and AixLib. No issue has been submitted by this review.

## Retained campaign coverage

| Measure | Count |
| --- | ---: |
| Static model records | 626 |
| Static finding instances | 2050 |
| Runtime model attempts | 530 |
| Runtime records marked ran | 420 |
| Runtime records marked no-dynamics | 11 |
| Runtime records marked did-not-run | 99 |
| Runtime finding instances (including failed runs) | 647 |
| Total finding instances | 2697 |

Counts describe retained historical results, not today's full repository coverage. A completed execution is not a correctness or parity result. An unavailable static result is not zero static defects. Findings can repeat across instances and phases; these counts are not unique bug counts.

## Disposition of every finding

The [repository-only CSV](findings.csv) retains all 2697 instances, including model, phase, severity, held-input flag, evidence, original result path and disposition. Its rows are an exact partition of the [combined inventory](../../finding-routing.csv).

| Disposition | Instances | Meaning |
| --- | ---: | --- |
| `hold-needs-source-and-witness-review` | 1219 | Needs source review and a valid concrete witness. |
| `standalone-topology-not-defect-proof` | 674 | Needs intended containing model and connection context. |
| `advisory-or-explicit-non-defect` | 423 | Advisory or explicitly non-defect evidence; not a confirmed bug. |
| `execution-not-completed` | 258 | Execution failed; identify toolchain versus model responsibility. |
| `hold-input-contract-validation` | 83 | Held inputs need validation against the model contract. |
| `hold-check-unset-parameters-and-inputs` | 22 | Check required parameter bindings and legal inputs. |
| `source-unit-review` | 8 | Source unit/metadata candidate; assess dimensions and intended behavior. |
| `coverage-gap` | 4 | Missing observations/coverage; not affirmative defect evidence. |
| `source-validation-input-range-review` | 3 | Review fixture input range and test intent. |
| `hold-needs-independent-evidence` | 3 | Independent evidence still required. |

## Representative unresolved evidence

These examples identify follow-up work; they are not additional confirmed defects. Full evidence and all remaining models are in the repository CSV.

- `Buildings.BoundaryConditions.SolarGeometry.BaseClasses.IncidenceAngle` — `initialization-failure` (runtime-v3); `execution-not-completed`.
- `Buildings.BoundaryConditions.SkyTemperature.BlackBody` — `simulation-failure` (runtime-v3); `execution-not-completed`.
- `Buildings.Controls.OBC.ASHRAE.G36.Plants.Chillers.Towers.WaterLevel` — `network-boundary-port` (runtime-v3); `execution-not-completed`.
- `Buildings.Fluid.HeatExchangers.RadiantSlabs.BaseClasses.HeatFlowRateMultiplier` — `network-component-isolated` (runtime-v3); `execution-not-completed`.
- `Buildings.Controls.OBC.ASHRAE.G36.Plants.Chillers.Towers.WaterLevel` — `network-observation-incomplete` (runtime-v3); `execution-not-completed`.

## Evidence boundaries

This per-repository report reorganizes the existing review; it does not add new compiler runs or independently validate every row. The named shortlist above defines the source-review and fresh-reproduction scope. Input hashes, all pinned revisions and aggregate counts are in [inventory.json](../../inventory.json). Current-source reads and targeted duplicate searches are in [upstream-evidence.json](../../upstream-evidence.json); absence from those searches is not an exhaustive novelty claim.

[All repository reports](../../README.md#per-repository-reports)
