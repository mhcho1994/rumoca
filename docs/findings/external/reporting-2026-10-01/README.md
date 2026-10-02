# External repository finding review — 2026-10-01

This review covers the newer third-party repository campaign, not the MSL issue corpus. It produces one independently reproduced validation-fixture issue and two source-supported unit/metadata clarification drafts. No upstream issue has been posted.

## Per-repository reports

Each repository has a standalone summary and a complete repository-only findings CSV. These reports exclude the already-reported MSL issue corpus. Shared issues remain cross-linked so separate repository summaries do not imply separate root causes.

| Repository report | Finding instances | Reporting status |
| --- | ---: | --- |
| [VehicleInterfaces](repos/vehicleinterfaces/README.md) | 212 | No maintainer-ready issue established |
| [PowerGrids](repos/powergrids/README.md) | 883 | No maintainer-ready issue established |
| [Modelica_DeviceDrivers](repos/devicedrivers/README.md) | 7 | No maintainer-ready issue established |
| [ThermoPower](repos/thermopower/README.md) | 1000 | One unit/unused-parameter clarification candidate |
| [OpenIPSL](repos/openipsl/README.md) | 578 | No maintainer-ready issue established |
| [ThermoSysPro](repos/thermosyspro/README.md) | 70 | No maintainer-ready issue established |
| [TRANSFORM](repos/transform/README.md) | 462 | No maintainer-ready issue established |
| [IBPSA](repos/ibpsa/README.md) | 957 | Shared humidity fixture and unit candidates; local reproduction pending |
| [IDEAS](repos/ideas/README.md) | 409 | Shared humidity fixture and unit candidates; local reproduction pending |
| [AixLib](repos/aixlib/README.md) | 942 | Shared humidity fixture and unit candidates; local reproduction pending |
| [Buildings](repos/buildings/README.md) | 2697 | One reproduced fixture issue; one unit clarification candidate |

## Reporting shortlist

| Draft | Evidence and disposition |
| --- | --- |
| [Buildings humidity validation](buildings-humidity-draft.md) | Ready for maintainer review: original examples reach 1.001 against max=1; independent OpenModelica simulation reproduces the warnings; height=0.999 controls reach 1.000 without those warnings. Low severity. |
| [Buildings CDL time-constant units](buildings-td-units-draft.md) | Clarification candidate: rate-typed defaults assigned to a time-typed parameter in two definitions. No independent unit-check diagnostic or incorrect trajectory demonstrated. |
| [ThermoPower unused controller gain](thermopower-mu-draft.md) | Cleanup/clarification candidate: unused parameter has inconsistent dimensional metadata. No dynamic effect demonstrated. |

The Buildings patterns also occur in IBPSA, IDEAS and AixLib copies. Coordinate the shared source lineage before opening duplicate tickets. Counts of instances are not counts of independent defects. Live upstream source reads and targeted issue searches are retained in [upstream-evidence.json](upstream-evidence.json); those searches found no exact matching report but do not establish exhaustive novelty.

## Coverage and limits

[Inventory](inventory.json) pins all 11 source revisions, source checkout modification state, and input-result hashes. [Finding routing](finding-routing.csv) assigns every retained finding instance a disposition: **4,494 historical static rows plus 3,723 runtime-v3 rows = 8,217 rows**. Static results cover only five repositories and 1,106 model records. Runtime-v3 covers all eleven repositories and 2,691 model records: 1,817 ran, 167 reported no dynamics, and 707 did not run.

This is a complete routing inventory of those retained records, not individual proof or dismissal of every alarm. Most rows remain evidence holds. It does not claim that every repository model was covered or that the campaign was rerun with today's compiler fixes. The retained static/runtime campaign predates this review; fresh runs are restricted to the named shortlist.

| Repository | Runtime attempts | Ran | No dynamics | Did not run |
| --- | ---: | ---: | ---: | ---: |
| aixlib | 553 | 385 | 9 | 159 |
| buildings | 530 | 420 | 11 | 99 |
| devicedrivers | 6 | 2 | 1 | 3 |
| ibpsa | 345 | 285 | 4 | 56 |
| ideas | 426 | 316 | 13 | 97 |
| openipsl | 255 | 115 | 14 | 126 |
| powergrids | 26 | 17 | 8 | 1 |
| thermopower | 77 | 12 | 11 | 54 |
| thermosyspro | 50 | 36 | 0 | 14 |
| transform | 397 | 223 | 90 | 84 |
| vehicleinterfaces | 26 | 6 | 6 | 14 |

## Alarms that should not become library bug reports yet

- 2,151 finding rows accompany executions that did not complete. Investigate the toolchain/backend failure before assigning library fault.
- 537 rows require held-input contract validation. The campaign holds free inputs at their starts; some resulting temperatures are 0 K against explicit positive bounds. This is not a valid nominal witness.
- 43 static rows need unset-parameter/input review. Required parameters with no binding must not be treated as intentional zero-valued configurations.
- 126 unheld extreme-value rows have explicit sentinel/protection explanations: TRANSFORM disabled error calculations, PowerGrids timer sentinels, and OpenIPSL's intentionally tested protected division.
- The OpenIPSL ConstantPQPV conservation residual is approximately 3.58e-18. Review the analyzer's absolute tolerance before alleging a conservation defect.
- Native solver trial-domain messages do not establish a source-model failure. Preserve whether the solver recovered and whether the run completed.
- Standalone connector topology alarms, derivative fixtures outside physical bounds, and implicit-unit numeric coefficients need context and configured witnesses.

These are reporting dispositions, not permanent analyzer suppressions. Full route counts are in the inventory, including 3,124 rows held for source/witness review.

## Fresh verification

OpenModelica 1.27.1~2-g6db4671, Buildings with Modelica 4.1.0, and ThermoPower with Modelica 3.2.3. GCC/G++ were selected in the parent process. The Buildings simulations finish successfully in both original and corrected cases; the original issue is a range warning, not a crash.

[Humidity measurements](evidence/humidity-results.json), [original output](evidence/humidity-output.txt), [control output](evidence/humidity-controls-output.txt), source wrappers, exact local MOS scripts, and full CSV traces are retained under `evidence/`. The scripts contain the original local library paths, which must be adjusted on another machine. Unit-check/build transcripts are retained in [unit-check-results.json](unit-check-results.json) and [build-results.json](build-results.json). OpenModelica with `--unitChecking` did not emit unit warnings for the two unit candidates; that negative result is part of the evidence.
