# VehicleInterfaces — findings report

Review date: 2026-10-01. **No maintainer-ready issue established.** No upstream issue has been posted by this review.

Repository: [modelica/VehicleInterfaces](https://github.com/modelica/VehicleInterfaces)

Evaluated revision: [`ae1fe06d82c4edccdc2685895aa49bf4d8bed772`](https://github.com/modelica/VehicleInterfaces/tree/ae1fe06d82c4edccdc2685895aa49bf4d8bed772). Source checkout was unmodified when inventoried.

## Findings and reporting decision

- **Hold structural candidates:** `Accessories.NoAccessories` has static unmatched-equation/variable findings and a runtime structural failure. A complete connected-system reproduction and independent compiler comparison are still needed to assign responsibility.
- **Standalone topology:** sensor and interface models naturally expose unconnected ports when evaluated alone. Those warnings cannot be promoted to library defects without the intended containing model.
- **Hold parameter witnesses:** e.g. `Atmospheres.ConstantAtmosphere` has a zero-temperature divisor witness. Establish a valid intended configuration before using such a witness in a report. Zero derivative coefficients may also be intentional algebraic limits.

Next action: investigate a configured `NoAccessories` use first, preserving the exact compiler stage and independent outcome. No maintainer-ready issue has been established.

## Retained campaign coverage

| Measure | Count |
| --- | ---: |
| Static model records | 26 |
| Static finding instances | 141 |
| Runtime model attempts | 26 |
| Runtime records marked ran | 6 |
| Runtime records marked no-dynamics | 6 |
| Runtime records marked did-not-run | 14 |
| Runtime finding instances (including failed runs) | 71 |
| Total finding instances | 212 |

Counts describe retained historical results, not today's full repository coverage. A completed execution is not a correctness or parity result. An unavailable static result is not zero static defects. Findings can repeat across instances and phases; these counts are not unique bug counts.

## Disposition of every finding

The [repository-only CSV](findings.csv) retains all 212 instances, including model, phase, severity, held-input flag, evidence, original result path and disposition. Its rows are an exact partition of the [combined inventory](../../finding-routing.csv).

| Disposition | Instances | Meaning |
| --- | ---: | --- |
| `hold-needs-source-and-witness-review` | 91 | Needs source review and a valid concrete witness. |
| `execution-not-completed` | 53 | Execution failed; identify toolchain versus model responsibility. |
| `standalone-topology-not-defect-proof` | 50 | Needs intended containing model and connection context. |
| `advisory-or-explicit-non-defect` | 13 | Advisory or explicitly non-defect evidence; not a confirmed bug. |
| `hold-input-contract-validation` | 5 | Held inputs need validation against the model contract. |

## Representative unresolved evidence

These examples identify follow-up work; they are not additional confirmed defects. Full evidence and all remaining models are in the repository CSV.

- `VehicleInterfaces.Accessories.NoAccessories` — `initialization-failure` (runtime-v3); `execution-not-completed`.
- `VehicleInterfaces.Chassis.MinimalChassis` — `network-boundary-port` (runtime-v3); `execution-not-completed`.
- `VehicleInterfaces.Mechanics.NormalisedRotational.AngleSensor` — `network-component-isolated` (runtime-v3); `execution-not-completed`.
- `VehicleInterfaces.Accessories.NoAccessories` — `network-observation-incomplete` (runtime-v3); `execution-not-completed`.
- `VehicleInterfaces.Mechanics.NormalisedRotational.AngleSensor` — `network-port-unconnected` (runtime-v3); `execution-not-completed`.

## Evidence boundaries

This per-repository report reorganizes the existing review; it does not add new compiler runs or independently validate every row. The named shortlist above defines the source-review and fresh-reproduction scope. Input hashes, all pinned revisions and aggregate counts are in [inventory.json](../../inventory.json). Current-source reads and targeted duplicate searches are in [upstream-evidence.json](../../upstream-evidence.json); absence from those searches is not an exhaustive novelty claim.

[All repository reports](../../README.md#per-repository-reports)
