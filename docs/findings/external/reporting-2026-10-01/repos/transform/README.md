# TRANSFORM — findings report

Review date: 2026-10-01. **No maintainer-ready issue established.** No upstream issue has been posted by this review.

Repository: [ORNL-Modelica/TRANSFORM-Library](https://github.com/ORNL-Modelica/TRANSFORM-Library)

Evaluated revision: [`2903b9c7b5ad03a6299dcd054dedc893a01b2054`](https://github.com/ORNL-Modelica/TRANSFORM-Library/tree/2903b9c7b5ad03a6299dcd054dedc893a01b2054). Source checkout was unmodified when inventoried.

## Findings and reporting decision

- **Explained alarms:** 123 unheld extreme-value instances stem from `Utilities.ErrorAnalysis.UnitTests` using `Modelica.Constants.inf` when `errorCalcs=false`. This is an intentional disabled-error sentinel, not 123 numerical defects.
- **Hold solver-trial evidence:** the `Sizing_CrossflowHX` trial-domain message is not a demonstrated source-model failure; solver recovery and completed-run status matter.
- **Hold other failures:** legal held inputs, initialized configurations and independent reproduction remain necessary for execution failures and the remaining candidates.

Next action: select a remaining non-sentinel candidate with a legal input contract and a reproducible observable failure. No maintainer-ready issue has been established.

## Retained campaign coverage

| Measure | Count |
| --- | ---: |
| Static model records | Not available in retained static results |
| Static finding instances | 0 |
| Runtime model attempts | 397 |
| Runtime records marked ran | 223 |
| Runtime records marked no-dynamics | 90 |
| Runtime records marked did-not-run | 84 |
| Runtime finding instances (including failed runs) | 462 |
| Total finding instances | 462 |

Counts describe retained historical results, not today's full repository coverage. A completed execution is not a correctness or parity result. An unavailable static result is not zero static defects. Findings can repeat across instances and phases; these counts are not unique bug counts.

## Disposition of every finding

The [repository-only CSV](findings.csv) retains all 462 instances, including model, phase, severity, held-input flag, evidence, original result path and disposition. Its rows are an exact partition of the [combined inventory](../../finding-routing.csv).

| Disposition | Instances | Meaning |
| --- | ---: | --- |
| `execution-not-completed` | 253 | Execution failed; identify toolchain versus model responsibility. |
| `source-sentinel-or-protection-not-defect-proof` | 123 | Source sentinel or protection explains the alarm. |
| `hold-input-contract-validation` | 46 | Held inputs need validation against the model contract. |
| `standalone-topology-not-defect-proof` | 26 | Needs intended containing model and connection context. |
| `advisory-or-explicit-non-defect` | 10 | Advisory or explicitly non-defect evidence; not a confirmed bug. |
| `coverage-gap` | 2 | Missing observations/coverage; not affirmative defect evidence. |
| `hold-needs-independent-evidence` | 1 | Independent evidence still required. |
| `solver-trial-not-source-model-verdict` | 1 | Internal solver trial alone is not a source-model verdict. |

## Representative unresolved evidence

These examples identify follow-up work; they are not additional confirmed defects. Full evidence and all remaining models are in the repository CSV.

- `TRANSFORM.Controls.TankLevelControl` — `initialization-failure` (runtime-v3); `execution-not-completed`.
- `TRANSFORM.Blocks.Examples.EasingRamp_Test` — `simulation-failure` (runtime-v3); `execution-not-completed`.
- `TRANSFORM.HeatExchangers.UAdT_lm` — `division-out-of-domain` (runtime-v3); `execution-not-completed`.
- `TRANSFORM.Controls.TankLevelControl` — `network-boundary-port` (runtime-v3); `execution-not-completed`.
- `TRANSFORM.Electrical.Grid` — `network-component-isolated` (runtime-v3); `execution-not-completed`.

## Evidence boundaries

This per-repository report reorganizes the existing review; it does not add new compiler runs or independently validate every row. The named shortlist above defines the source-review and fresh-reproduction scope. Input hashes, all pinned revisions and aggregate counts are in [inventory.json](../../inventory.json). Current-source reads and targeted duplicate searches are in [upstream-evidence.json](../../upstream-evidence.json); absence from those searches is not an exhaustive novelty claim.

[All repository reports](../../README.md#per-repository-reports)
