# Modelica_DeviceDrivers — findings report

Review date: 2026-10-01. **No maintainer-ready issue established.** No upstream issue has been posted by this review.

Repository: [modelica-3rdparty/Modelica_DeviceDrivers](https://github.com/modelica-3rdparty/Modelica_DeviceDrivers)

Evaluated revision: [`f0ea4af79f50c7bd1e9560c88b3fb4d82f0f8e98`](https://github.com/modelica-3rdparty/Modelica_DeviceDrivers/tree/f0ea4af79f50c7bd1e9560c88b3fb4d82f0f8e98). Source checkout was unmodified when inventoried.

## Findings and reporting decision

- **Tool limitation:** the retained `Utilities.StringExpression` failure explicitly reports unsupported checked DAE semantics for String values. This does not establish a library bug.
- **Coverage/topology:** `Blocks.Communication.Internal.PartialSampleTrigger` contributes standalone boundary-port and incomplete-observation findings. Neither proves incorrect library behavior.
- This campaign attempted only six models; its results cannot support a library-wide correctness conclusion.

Next action: classify the other failed executions and use configured, supported examples before seeking a maintainer report. No maintainer-ready issue has been established.

## Retained campaign coverage

| Measure | Count |
| --- | ---: |
| Static model records | Not available in retained static results |
| Static finding instances | 0 |
| Runtime model attempts | 6 |
| Runtime records marked ran | 2 |
| Runtime records marked no-dynamics | 1 |
| Runtime records marked did-not-run | 3 |
| Runtime finding instances (including failed runs) | 7 |
| Total finding instances | 7 |

Counts describe retained historical results, not today's full repository coverage. A completed execution is not a correctness or parity result. An unavailable static result is not zero static defects. Findings can repeat across instances and phases; these counts are not unique bug counts.

## Disposition of every finding

The [repository-only CSV](findings.csv) retains all 7 instances, including model, phase, severity, held-input flag, evidence, original result path and disposition. Its rows are an exact partition of the [combined inventory](../../finding-routing.csv).

| Disposition | Instances | Meaning |
| --- | ---: | --- |
| `execution-not-completed` | 3 | Execution failed; identify toolchain versus model responsibility. |
| `standalone-topology-not-defect-proof` | 3 | Needs intended containing model and connection context. |
| `coverage-gap` | 1 | Missing observations/coverage; not affirmative defect evidence. |

## Representative unresolved evidence

These examples identify follow-up work; they are not additional confirmed defects. Full evidence and all remaining models are in the repository CSV.

- `Modelica_DeviceDrivers.Blocks.InputDevices.KeyboardInput` — `simulation-failure` (runtime-v3); `execution-not-completed`.

## Evidence boundaries

This per-repository report reorganizes the existing review; it does not add new compiler runs or independently validate every row. The named shortlist above defines the source-review and fresh-reproduction scope. Input hashes, all pinned revisions and aggregate counts are in [inventory.json](../../inventory.json). Current-source reads and targeted duplicate searches are in [upstream-evidence.json](../../upstream-evidence.json); absence from those searches is not an exhaustive novelty claim.

[All repository reports](../../README.md#per-repository-reports)
