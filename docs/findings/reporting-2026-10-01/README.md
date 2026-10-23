# Findings reporting review — 2026-10-01

The historical finding inventories are not submission queues. This review
routes all 6,440 v2 report instances, reconciles the old headline claims with
later adjudications, reads the live upstream discussions, and reruns three
standalone compiler examples on the current local binary. It does not freshly
revalidate every finding or compare the compiler cases against clean upstream.
No issue or comment was published during this review.

## Already reported

- [MSL #4814](https://github.com/modelica/ModelicaStandardLibrary/issues/4814)
  is open, authored by Arslan8. Its original post reports 180 source locations;
  a later comment reports 46 additional locations. Those are published claims,
  not 226 independently revalidated root causes in this review. The spreadsheets
  have not been matched row-by-row against the local ledger here.
- [ModelicaSpecification #3941](https://github.com/modelica/ModelicaSpecification/issues/3941)
  is open and already covers mathematical-domain/error semantics. Do not open
  a second division-by-zero specification issue.
- The six known-issue regression cases correspond to existing MSL issues
  #4749, #4750, #3624, #4451, #4459 and #4771. Rediscovery is evidence for those
  issues, not six new discoveries.

The latest reviewed [MSL response](https://github.com/modelica/ModelicaStandardLibrary/issues/4814#issuecomment-5891197710)
explicitly distinguishes `L*der(i)=v` from `der(i)=v/L`: the former can support
zero through different state selection/index reduction. The local later
adjudications independently withdrew several blanket zero-storage claims.
Two tools failing the same override does not by itself prove a library defect.

## Submission decisions

| Finding family | Review decision | Next reporting action |
|---|---|---|
| DCPM_Drive resistor binding has voltage units | Source still contains the expression; **already reported** in a comment on closed #4098 | [Focused follow-up draft](dcpm-followup-draft.md); the closing PR #4112 changed six battery files and did not change DCPM_Drive |
| Direct source denominators and invalid mathematical domains | Historical evidence exists, but many are already under #4814; missing `min` alone is not a correctness proof | Match the two published spreadsheets before adding locations; provide one legal source-level witness and control per root cause |
| CriticalDamping `n=0` | Strong structural-domain candidate with two implementations and retained controls | Check the #4814 attachments before calling it new; frame as a missing positive-order contract, not a silent wrong-result case |
| Zero mass/inertia/capacitance/inductance in implicit equations | Several old headline claims were refuted or are topology/initialization dependent | Exclude blanket primitive-component accusations; [draft reply to #4814](4814-followup-draft.md) acknowledges the distinction |
| B_myMax in linear magnetic mode | Later review says unused normalization may be optimized away; older two-tool claim overstates the result | Retain as a qualified source/diagnostic question; do not claim robust active-material failure |
| 911 SI-type declaration sites | Missing bounds and quantity names do not establish physical intent | No bulk issue or global `Units.mo` positivity change |
| FIRE_CP_Glimpse structural singularity | Static analyzer and compiler share Rumoca evidence; not an independent oracle | Hold until an independent frontend and minimal source explanation establish the defect |
| BUG-001 comprehension panic | Reproduces locally: exit 101 | [Compiler draft](rumoca-comprehension-draft.md); ready as a fork report, clean-upstream reproduction required for upstream attribution |
| BUG-004 record-array call | Reproduces locally: ED008, `v.re` | [Compiler draft](rumoca-record-array-draft.md); same upstream attribution hold |
| BUG-007 AssertionLevel | The documented local example now compiles | Do not file as a currently failing local example; investigate upstream only if proposing a patch |
| Runtime loops, handles, initialization feature gaps | Current branch capability gaps, not automatically upstream regressions | Track as implementation work; compare clean upstream before reporting there |

## Inventory accounting

The [routing CSV](report-routing.csv) retains every stable ID and its source
evidence path. It preserves historical verdicts; it does not elevate any record
to newly confirmed status.

| Historical verdict | Report instances | Reporting disposition |
|---|---:|---|
| Confirmed | 830 | Review root cause, current source and already-published batches |
| Candidate | 44 | Hold for independent validation |
| Advisory | 638 | Hold for component intent |
| False positive / explicit non-defect | 3,373 | Exclude from bug submissions |
| Unresolved | 1,555 | Hold for evidence |
| Total | 6,440 | Not a count of independent bugs |

The September 24 evaluation's 819 confirmed entries concern a narrower
255-model MSL/ModelicaTest slice; they are not directly interchangeable with
these 830 ledger entries. The older original BUG list has 11 confirmed and 15
refuted entries in the later audit. IDs must be qualified by their inventory:
`verified bugs/BUG-001` is a compiler issue, whereas `v2/verified/BUG-001`
describes a mass/initialization claim.

## Evidence and publication boundary

[Upstream snapshot](upstream-evidence.json) retains issue states, relevant
comments, search results, DCPM blob identity and PR #4112 file patches. Empty
searches do not prove novelty: DCPM was found in comments despite an empty
exact-name issue search. MSL master resolved to
`4c40388dde27ccb6702cea876adbf54c76d75b97` during the review.

[Compiler results](compiler-results.json) retain exact sources, commands,
outputs, base commit and dirty-binary hash. Runs used `--pass none`,
`RAYON_NUM_THREADS=4` and a 45-second timeout. These are compilation checks,
not fresh simulation controls or clean-upstream reproductions.

Before publication, settle the destination and use the appropriate existing
thread. New findings require a pinned source revision, legal configuration,
nominal control, expected/actual behavior, and independent evidence appropriate
to the claim. A dimensional source inconsistency need not claim a runtime
failure; a compiler error must not be counted as a model defect. The drafts
are concrete proposed messages, not published submissions.
