# ModelSan documentation

| | |
|---|---|
| [**v2/bugs/**](v2/bugs/README.md) | **One file per instance, all three tiers.** Each names the sanitizer that found it and carries the commands to check it. Start here. |
| [Local-report regression evaluation](evaluations/msl-issues-2026-09-24/README.md) | Current-tool coverage of reviewed local MSL reports; not the upstream GitHub issue benchmark. |
| [Upstream GitHub issue evaluation](evaluations/msl-upstream-open-issues-2026-09-24/README.md) | Complete 353-issue/1,637-comment census, independent capability assessment, focused reproductions and feature gaps. |
| [Known-issue detection follow-up](evaluations/msl-upstream-open-issues-2026-09-24/detection-followup.md) | Five reproduced issues detected natively, six across explicit backends; behavioral contracts, controls and remaining coverage gaps. |
| [findings/](findings/) | Bugs in **target programs** — Modelica models and libraries. The results. |
| [bitcode-reference.md](bitcode-reference.md) | Every type in the artifact format, generated from `schema.rs` and checked for staleness. The narrative is in [SPEC_RUMOCA_BITCODE.md](SPEC_RUMOCA_BITCODE.md). |
| [SPEC_RUMOCA_BITCODE.md §2a](SPEC_RUMOCA_BITCODE.md#2a-two-irs-equation-and-execution) | The two IRs an artifact carries -- equation IR (what must hold) and execution IR (what the host does) -- why there are two, what the file does *not* carry, and how they stay consistent. |
| [bitcode-linking.md](bitcode-linking.md) | Combine independent artifacts with the native linker or Python SDK; wiring and execution safety rules. |
| [combining-models.md](combining-models.md) | Start-to-finish tutorial: build two models, link and connect their ports, run equation/execution passes, then simulate. |
| [modelsan-bitcode-integration.md](modelsan-bitcode-integration.md) | Current sanitizer integration: preserved executable passes, identity-based network analysis, supported overrides and regressions. |
| [connector-validation.md](connector-validation.md) | Native connection-law checks, raw-artifact regression cases, and incomplete-interface refusal. |
| [method/](method/) | How a candidate becomes a finding, and what each verification stage removed. |
| [toolbugs/](toolbugs/) | Defects in the instruments (Rumoca, `rumoca-bitcode`) and what they cost in coverage. Not results. |
| [verified bugs/](verified%20bugs/) | All confirmed reports, including fixed regressions, with verification level and current status. |

The split matters: a crash in AddressSanitizer is not a finding about the
program under test. Tool defects are recorded because each one caps how much of
a corpus the detectors can see, not because they count.
