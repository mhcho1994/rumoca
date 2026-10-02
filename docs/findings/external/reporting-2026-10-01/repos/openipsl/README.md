# OpenIPSL — findings report

Review date: 2026-10-01. **No maintainer-ready issue established.** No upstream issue has been posted by this review.

Repository: [OpenIPSL/OpenIPSL](https://github.com/OpenIPSL/OpenIPSL)

Evaluated revision: [`cad8ddf4a6fd7196bb5ff6257b57bb2e2e7e6a75`](https://github.com/OpenIPSL/OpenIPSL/tree/cad8ddf4a6fd7196bb5ff6257b57bb2e2e7e6a75). Source checkout was unmodified when inventoried.

## Findings and reporting decision

- **Do not report as a division defect:** `Tests.NonElectrical.Nonlinear.Div0Block` deliberately tests protected division at zero. The large finite result follows `div0protect` and is not proof of an unhandled division by zero.
- **Analyzer follow-up:** `Electrical.Solar.PSAT.ConstantPQPV.PQ1` has a reported conservation residual of about 3.58e-18. Establish an appropriate absolute tolerance and connected-system witness before attributing this to the library.
- **Hold:** free-input configurations and failed initialization/simulation rows need valid input contracts and independent reproduction. A recovered solver trial-domain message does not itself prove a library defect.

Next action: address the tolerance/input-contract questions, then select a configured failing model for independent reproduction. No maintainer-ready library defect has been established here.

## Retained campaign coverage

| Measure | Count |
| --- | ---: |
| Static model records | Not available in retained static results |
| Static finding instances | 0 |
| Runtime model attempts | 255 |
| Runtime records marked ran | 115 |
| Runtime records marked no-dynamics | 14 |
| Runtime records marked did-not-run | 126 |
| Runtime finding instances (including failed runs) | 578 |
| Total finding instances | 578 |

Counts describe retained historical results, not today's full repository coverage. A completed execution is not a correctness or parity result. An unavailable static result is not zero static defects. Findings can repeat across instances and phases; these counts are not unique bug counts.

## Disposition of every finding

The [repository-only CSV](findings.csv) retains all 578 instances, including model, phase, severity, held-input flag, evidence, original result path and disposition. Its rows are an exact partition of the [combined inventory](../../finding-routing.csv).

| Disposition | Instances | Meaning |
| --- | ---: | --- |
| `execution-not-completed` | 429 | Execution failed; identify toolchain versus model responsibility. |
| `hold-input-contract-validation` | 100 | Held inputs need validation against the model contract. |
| `standalone-topology-not-defect-proof` | 47 | Needs intended containing model and connection context. |
| `analyzer-tolerance-review-near-zero-residual` | 1 | Review analyzer absolute tolerance for a near-zero residual. |
| `source-sentinel-or-protection-not-defect-proof` | 1 | Source sentinel or protection explains the alarm. |

## Representative unresolved evidence

These examples identify follow-up work; they are not additional confirmed defects. Full evidence and all remaining models are in the repository CSV.

- `OpenIPSL.Electrical.Banks.PSSE.SVC` — `initialization-failure` (runtime-v3); `execution-not-completed`.
- `OpenIPSL.Electrical.Banks.PwCapacitorBank` — `simulation-failure` (runtime-v3); `execution-not-completed`.
- `OpenIPSL.Electrical.Banks.PwCapacitorBank` — `division-out-of-domain` (runtime-v3); `execution-not-completed`.
- `OpenIPSL.Electrical.Banks.PSSE.SVC` — `network-boundary-port` (runtime-v3); `execution-not-completed`.
- `OpenIPSL.Electrical.Banks.PwCapacitorBank` — `network-component-isolated` (runtime-v3); `execution-not-completed`.

## Evidence boundaries

This per-repository report reorganizes the existing review; it does not add new compiler runs or independently validate every row. The named shortlist above defines the source-review and fresh-reproduction scope. Input hashes, all pinned revisions and aggregate counts are in [inventory.json](../../inventory.json). Current-source reads and targeted duplicate searches are in [upstream-evidence.json](../../upstream-evidence.json); absence from those searches is not an exhaustive novelty claim.

[All repository reports](../../README.md#per-repository-reports)
