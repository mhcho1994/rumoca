# ThermoSysPro — findings report

Review date: 2026-10-01. **No maintainer-ready issue established.** No upstream issue has been posted by this review.

Repository: [ThermoSysPro/ThermoSysPro](https://github.com/ThermoSysPro/ThermoSysPro)

Evaluated revision: [`a896ab75ce22162369e7fa4ce79927eeca44ad4a`](https://github.com/ThermoSysPro/ThermoSysPro/tree/a896ab75ce22162369e7fa4ce79927eeca44ad4a). Source checkout was unmodified when inventoried.

## Findings and reporting decision

- **Hold initialization failure:** `Examples.Control.Drum_LevelControl` has a retained initialization failure involving the reduced initialization projection. The log does not establish that the source library is responsible.
- **Hold non-finite result:** `InstrumentationAndControl.Blocks.Math.Div` reports a non-finite output in the historical run. Validate the supplied inputs and reproduce under a legal configured input contract before reporting division behavior as a library defect.
- The remaining routed rows concern failed execution or held-input validation; no independent source-level defect has been established.

Next action: reproduce one configured control example and compare the first failing stage with an independent compiler. No maintainer-ready issue has been established.

## Retained campaign coverage

| Measure | Count |
| --- | ---: |
| Static model records | Not available in retained static results |
| Static finding instances | 0 |
| Runtime model attempts | 50 |
| Runtime records marked ran | 36 |
| Runtime records marked no-dynamics | 0 |
| Runtime records marked did-not-run | 14 |
| Runtime finding instances (including failed runs) | 70 |
| Total finding instances | 70 |

Counts describe retained historical results, not today's full repository coverage. A completed execution is not a correctness or parity result. An unavailable static result is not zero static defects. Findings can repeat across instances and phases; these counts are not unique bug counts.

## Disposition of every finding

The [repository-only CSV](findings.csv) retains all 70 instances, including model, phase, severity, held-input flag, evidence, original result path and disposition. Its rows are an exact partition of the [combined inventory](../../finding-routing.csv).

| Disposition | Instances | Meaning |
| --- | ---: | --- |
| `execution-not-completed` | 54 | Execution failed; identify toolchain versus model responsibility. |
| `hold-input-contract-validation` | 16 | Held inputs need validation against the model contract. |

## Representative unresolved evidence

These examples identify follow-up work; they are not additional confirmed defects. Full evidence and all remaining models are in the repository CSV.

- `ThermoSysPro.Examples.Control.Drum_LevelControl` — `initialization-failure` (runtime-v3); `execution-not-completed`.
- `ThermoSysPro.InstrumentationAndControl.Blocks.Math.Div` — `simulation-failure` (runtime-v3); `execution-not-completed`.
- `ThermoSysPro.Examples.Control.Drum_LevelControl` — `network-boundary-port` (runtime-v3); `execution-not-completed`.
- `ThermoSysPro.Examples.Control.Drum_LevelControl` — `network-observation-incomplete` (runtime-v3); `execution-not-completed`.
- `ThermoSysPro.Examples.Control.Condenser_LevelControl` — `network-boundary-port` (runtime-v3); `hold-input-contract-validation`.

## Evidence boundaries

This per-repository report reorganizes the existing review; it does not add new compiler runs or independently validate every row. The named shortlist above defines the source-review and fresh-reproduction scope. Input hashes, all pinned revisions and aggregate counts are in [inventory.json](../../inventory.json). Current-source reads and targeted duplicate searches are in [upstream-evidence.json](../../upstream-evidence.json); absence from those searches is not an exhaustive novelty claim.

[All repository reports](../../README.md#per-repository-reports)
