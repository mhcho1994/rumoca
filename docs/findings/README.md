# ModelSan findings

**Reporting review (2026-10-01):** the historical claims and counts below
include entries superseded by later adjudications. Use the
[reporting review and drafts](reporting-2026-10-01/README.md) before submitting
issues. In particular, two-tool zero-storage failures do not establish a
primitive-component defect, and several older claims were withdrawn.

Bugs in **target programs** — Modelica models and libraries. This is what
ModelSan is for and the only thing that counts as a result.

A finding is a model that is *valid* and *behaves incorrectly* under some
reachable parameter value, initial state, or execution condition. Not a model
that fails to parse, and not a limitation of any tool used to find it.

**How a candidate becomes a finding is written down separately**, in
[`../method/`](../method/README.md). Every stage there exists because something
was reported that turned out not to be a bug; the numbers each stage removed are
recorded with it.

Tool defects encountered along the way are in [`../toolbugs/`](../toolbugs/).
They are engineering cost, not results — they belong there for the same reason
a crash in AddressSanitizer is not a finding about the program under test. They
matter here only because they cap coverage, which the table below records.

## Findings

| ID | Severity | Target | Trigger | Cross-confirmed | Summary |
|---|---|---|---|---|---|
| [BUG-010](../verified%20bugs/BUG-010-inductor-documents-zero-it-cannot-honour.md) | High | MSL 4.1.0 `Analog.Basic.Inductor` | `L = 0` | 3 models | No bound at all, and the documentation explicitly says zero is allowed. Both tools fail. `Capacitor` carries the identical sentence and honours it. |
| [BUG-011](../verified%20bugs/BUG-011-fluxtubes-b-mymax-unguarded-divisor.md) | Medium | MSL 4.1.0 `SoftMagnetic.BaseData` | `B_myMax = 0` | 2 models | Unguarded divisor in two files, no declared lower bound. |
| [BUG-012](../verified%20bugs/BUG-012-variablepermeance-unbounded-input.md) | Medium | MSL 4.1.0 `FluxTubes.VariablePermeance` | permeance input ≤ 0 | 1 model | Sole coefficient supplied as an *unbounded input*, where the declared bound is the only possible defense. |
| [BUG-002](../verified%20bugs/BUG-002-msl-zero-mass-within-declared-bound.md) | Medium | MSL 4.1.0 `Translational.Mass`, `Rotational.Inertia` | `m = 0` | 4 models | `min=0` admits a value that makes the model structurally singular. |
| [BUG-003](../verified%20bugs/BUG-003-switchedrlc-zero-resistance.md) | Low | `examples/models/SwitchedRLC.mo` | `R = 0` | — | Divides by a parameter with no declared lower bound. |
| [BUG-005](../verified%20bugs/BUG-005-multibody-rotor1d-zero-inertia.md) | Medium | MSL 4.1.0 `MultiBody.Parts.Rotor1D` | `J = 0` | static only | Same defect as BUG-002 in a package that report did not cover. |
| [BUG-006](../verified%20bugs/BUG-006-bound-propagated-into-a-different-component.md) | Medium | MSL 4.1.0 `OpAmpCircuits.Der` | `k = 0` | static only | The unsound bound and the singular coefficient are in *different components*, linked by `C = k/(2*pi*f*R)`. No per-component check finds it. |

**7 findings, 5 of them in MSL itself.** All are latent parameter-triggered
failures: the model simulates cleanly at its declared values and fails at a
value its own declaration permits.

**10 model instances are confirmed in two independent tools** — Rumoca and
OpenModelica 1.27.0-dev — each passing at declared values and failing at the
trigger. Cross-confirmation is required before anything is listed here, for the
reason below.

## Why every finding is cross-confirmed

A failure in one tool is ambiguous. `Inductor.L = 0` produces `0 = v`, which a
tool could plausibly fail on merely by not re-indexing a degenerate equation —
that would be a gap in the tool, not a defect in MSL. Running the same trigger
through a second, mature implementation settles it.

It changes the answer in practice. Of 12 parameter-triggered failures Rumoca
found in MSL and ModelicaTest:

| | Count |
|---|---|
| Confirmed — both tools pass at declared values and fail at the trigger | **10** |
| **Excluded** — OpenModelica survives the trigger, so the failure is Rumoca's | **2** |

The two excluded are `Rotational.Examples.CompareBrakingTorque` and
`Translational.Examples.CompareBrakingForce`. Without this stage they would have
been reported as MSL bugs, and they are not.

## Full-corpus sweep, OpenModelica backend

The findings above were found through Rumoca, which compiles 332 of 847 models.
The dynamic search also runs through OpenModelica, which reaches far more of the
corpus. See [the method note](../method/README.md) for why the pipeline is
dual-backend rather than switched.

**Complete, 827 models:**

| | Rumoca backend | OMC backend |
|---|---|---|
| Searched (clean baseline, parameters varied) | 289 | **739** |
| Trials | ~4 000 | 14 000 |
| Failing (model, parameter) pairs | 19 | 1 167 |
| Candidate (class, parameter) groups | — | 623 |
| Verified past attribution, guards, reachability | — | 398 |
| …in core library rather than Examples/Utilities | — | **147** |

Of the 147: 39 `zero-permitted-by-bound`, 100 `zero-permitted-by-omission`,
8 `fails-at-its-own-positive-bound`. By where the bound is written: 31 on the
component, 17 inherited from an SI type, 99 nowhere at all.

### Cross-confirmation, full corpus

Every core-library candidate goes through the second tool, trying up to six of
its models rather than deciding a component from one topology:

| | Count |
|---|---|
| **Confirmed** — both tools clean at declared values, both fail at the trigger | **8 components, 77 model instances** |
| **Excluded** — the second tool survives the trigger | **79** |
| Single-tool — no model of that candidate gives the second tool a clean baseline | 81 |

**47% of what passed static verification did not survive a second opinion.**
The largest single group in an earlier sweep — `HeatTransfer.HeatCapacitor.C`,
20 models — is among the excluded.

The eight confirmed components:

| Component | Parameter | Models | Claim |
|---|---|---|---|
| `Rotational.Components.Inertia` | `J` | 32 | zero permitted by bound |
| `Translational.Components.Mass` | `m` | 18 | zero permitted by bound |
| `Analog.Basic.Capacitor` | `C` | 12 | zero permitted by bound |
| `Analog.Basic.Inductor` | `L` | 7 | zero permitted by omission |
| `FluxTubes.GenericFluxTube` | `area` | 3 | zero permitted by omission |
| `FluxTubes.GenericFluxTube` | `l` | 3 | zero permitted by omission |
| `Analog.Ideal.IdealizedOpAmpLimited` | `Vps` | 1 | **negative** permitted by omission |
| `Rotational.Components.IdealGear` | `ratio` | 1 | zero permitted by omission |

`IdealizedOpAmpLimited.Vps` is the first finding from the negative-value probe,
and the first whose missing constraint is *relational* rather than a box —
see [BUG-016](../verified%20bugs/BUG-016-relational-invariant-between-two-parameters.md).

Nearly all of them are instances of one thing, written up as a study rather
than as 147 bug reports: [MSL components inherit SI type bounds their equations
cannot honour](si-type-bounds.md).

## Studies

| | Summary |
|---|---|
| [SI type bounds](si-type-bounds.md) | Where MSL's declared domains actually come from, and the three ways components inherit a bound they cannot keep. Includes the sharpest single item found: MSL defines `SelfInductance(min=0)` and `Basic.Inductor` does not use it. |
| [min=0 census](min0-census.md) | 326 `min=0` declarations in MSL against 147 correctly guarded. Records that the source-text form of this check runs at 23% precision and why the detector belongs on the DAE. |

## Full-corpus sweep

847 models — MSL 4.1.0 examples, ModelicaTest, CogniPilot CMM, repo examples —
with every detector, 81 minutes:

| | Count |
|---|---|
| Models attempted | 847 |
| Compiled to bitcode | 332 |
| Refused at compile (typed diagnostic, fail-closed) | 515 |
| Exported only partially (bitcode v1 gaps) | 198 |
| Failed at declared values, nothing attributable | 27 |
| **Searched** (clean baseline, parameters varied) | **289** |
| **Parameter-triggered failures found** | **19** |
| …of which in MSL / ModelicaTest | 12 |
| …of which survive cross-confirmation in OpenModelica | **10** |

The remaining 7 are in repo examples and CMM, including three fixtures written
for this project, which are not counted as findings.

Compile refusals are dominated by one diagnostic — `ED019`
(`unsupported Flat semantic owner`) accounts for 267 of 515. That single gap,
not detector strength, is the binding constraint on how many findings this
corpus can yield.

## Detectors, and what each has produced

| Detector | Needs the model to compile? | Findings |
|---|---|---|
| Singular-coefficient risk (`find_singular_risks`) | yes | BUG-002, BUG-005, BUG-006, BUG-010, BUG-012 |
| Unsafe-domain parameter search (`find_domain_sites`) | yes | BUG-003, BUG-011 |
| Declared-range breach (`declared_ranges`) | yes | — |
| Declaration contradictions (`declcheck`) | **no** | 0 across 5 348 files (see below) |
| Cross-tool confirmation (`crossconfirm`) | yes | *filters* — removed 2 false findings |
| OMC-backed parameter search (`omc_backend`) | **no** | 73 core-library candidates, in verification |

`declcheck` looks for a declaration that contradicts itself: `min > max`, a
`start` or default outside the declared range, a non-positive `nominal`. It
reads source only, so it covers 100% of any corpus. It reports **zero** across all
5 348 files of MSL 4.1.0, ModelicaTest, CMM and the repo examples — a real
negative result, validated against a synthetic file where it catches all 8
planted contradictions and leaves the clean declaration alone. Mature Modelica
libraries do not contradict themselves *within* a declaration; the defects are
between a declaration and the equations that use it.

## The coverage problem, stated honestly

Every detector except `declcheck` needs Rumoca to compile the model, and Rumoca
compiles a minority of MSL. That is the binding constraint on how many findings
this can produce, and it is why the tool bugs in `../toolbugs/` were worth
fixing at all — each one widens the set of models the detectors can see.

Concretely: 515 of 847 models never reach a detector at all, and 198 more are
analysed from an incomplete picture. The findings above come from the 289 that
were fully searched.

## Reporting standard

A report here should let someone else act without rerunning anything:

1. **A reproducer that stands alone**, with exact command and exact output.
2. **The trigger isolated** — not "this model fails" but which single value
   makes the difference, shown against a variant that works.
3. **A nominal baseline** — the model simulating cleanly at declared values, so
   the failure is attributable to the trigger and not pre-existing.
4. **Honest severity.** BUG-002 is a latent fragility, not a crash in correct
   usage, and says so.
5. **Confirmed separated from inferred.** Where a model could not be simulated
   end-to-end, the report says which part is read off the source.
