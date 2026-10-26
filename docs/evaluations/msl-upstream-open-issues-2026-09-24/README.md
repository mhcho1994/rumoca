# Upstream MSL open-issue evaluation — 2026-09-24

**October 1 regression follow-up:** all six reproduced issue patterns are now
detected in both native source and saved-bitcode gates using the explicit
frozen-parameter profile, with all five controls passing. See the
[review fixes and validation limits](review-fixes-2026-10-01.md). The earlier
[detection results](detection-followup.md) remain historical evidence. The census
and initial capability assessment below retain the state before those repairs.

The GitHub access blocker is resolved. The snapshot contains **353 open issues and all 1,637 issue comments** from [modelica/ModelicaStandardLibrary](https://github.com/modelica/ModelicaStandardLibrary/issues). Every issue has a capability assessment. **#4814, authored by Arslan8, is excluded**, leaving **352 independent issue entries**. Pull requests are excluded from the census.

The initial evaluation produced a complete issue inventory and capability screening, with focused execution of six reported issue patterns. It is **not** a benchmark that reproduced every issue or measured sanitizer recall. At that stage, no ModelSan detection was credited for the focused witnesses: OMC demonstrated the reported patterns, but the relevant Rumoca/ModelSan paths were blocked before observation. Other entries are explicitly not executed, unresolved, or outside numerical scope. The follow-up above records the subsequent implementation and new detections separately.

## Read the results

- [Complete issue-by-issue index](index.md): current capability, specific rationale, missing feature and execution status for all 353 entries.
- [Feature gaps and priorities](feature-gaps.md): implementation evidence and additions needed to cover the corpus.
- [Focused results](focused-results.md): #4749, #4750, #3624, #4451, #4459 and earlier #4771; also source/discussion checks for #4807, #4801 and #4770.
- [Machine-readable assessment](assessment.json), [CSV](assessment.csv), and [manually authored notes](assessment-notes.tsv).

## Initial findings

Finite wrong results need semantic contracts: entropy/state round trips, phase equivalence, sample history and quantizer level count. Domain, nonfinite and generic solver checks do not define those expectations. The retained OMC cases show a 300 K gas state returning 5204.05 K away from reference pressure, a 300 K water state returning 327.46 K, phase-equivalent pulses disagreeing, UnitDelay emitting the current sample, and a two-bit quantizer producing five levels. The earlier reduced-composition MoistAir case needs an array-shape contract.

Compiler/backend coverage initially prevented observing these defects through ModelSan. Record/package constant resolution, event/clock execution and instrumented expression support were identified as prerequisites for broader end-to-end coverage. Static warnings in the passing controls and dependent-parameter witnesses also show why findings must be matched to the reported defect, not counted by proximity.

The issue corpus is broader than numerical bugs: documentation, enhancements, reference/harness problems, external C/platform concerns and disputed claims all appear. For example, #4801's smoothness option is passed at table construction; a shared lookup name does not establish a defect. #4770 compares different spline/smoothing choices. #4807 has substantive near-zero quadrature termination evidence in its linked discussion, but was not executed here.

## Snapshot and review method

The initial API census began at **2026-09-24 13:39:18 UTC**. Six disjoint creation-date searches each returned fewer than the connector's 100-result cap: 85 + 80 + 84 + 39 + 33 + 32 = 353. A second search in descending order at **14:10:58 UTC** returned exactly the same issue IDs. Every issue was fetched individually, confirmed open, and its complete returned comment list was checked against its metadata count. The snapshot is a bounded sequence of live reads, not an atomic GitHub transaction. [Snapshot manifest](snapshot.json).

The prior cached count of 352 is superseded: newly opened #4815 is included. The 1,637-comment total covers the 353 issue threads; supplemental linked PR #4800 discussion is separately archived and not added to that count. Comment timestamp fields supplied by the connector were null; they are not invented. Search rows are not substituted for individually fetched issue records.

The assessment uses issue bodies and discussion to distinguish claims, corrections, questions and requested features, with source inspection and deep review focused on relevant numerical/runtime cases. Original attachments, every linked paper and every historical library version were not retrieved or executed. The complete archived discussions make the evidence inspectable; inclusion of a discussion is not a claim that every historical subclaim was independently verified. Categories are analyst screening judgments, not upstream confirmation of 352 distinct bugs. Umbrellas and overlapping issues stay visible as separate issues, with overlap noted; no deduplicated bug count or recall percentage is claimed.

## Archived evidence and validation

- [Full issues](issues.json), [full comment bodies](comments.json), [initial search snapshot](search-snapshot.json), [census/recheck manifest](snapshot.json).
- [Current focused execution evidence](semantic-results.json), [source/binary provenance](provenance.json), and [linked PR #4800](linked-pr-4800.json).
- `build_index.py` checks exactly one assessment per issue, open state, author exclusion, unique IDs, every comment count, and total coverage before rendering the index/JSON/CSV.

Regenerate the reviewed index from the archived data:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 docs/evaluations/msl-upstream-open-issues-2026-09-24/build_index.py
```

The working tree already contained compiler/runtime/sanitizer changes from earlier work. The initial evaluation added evidence and documentation only. The separately recorded detection follow-up adds production repairs and regression tests; neither stage published changes to GitHub. The [earlier local-report audit](../msl-issues-2026-09-24/README.md) remains a separate corpus and is not used to inflate independent results.

The preliminary [four-issue review](review-initial.md) and [original probe notes](probes.md) are retained as historical records. Their access limitations and #4807-unavailable status are superseded by this complete census and the current focused results.
