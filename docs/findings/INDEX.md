# Index

For current submission decisions, start with the
[October 1 reporting review](reporting-2026-10-01/README.md). This historical
index includes claims later refuted and does not establish upstream novelty.

Everything produced by this project, in the order you would read it.

## Start here

| | |
|---|---|
| [../README.md](../README.md) | Why findings, method and toolbugs are separate folders |
| [README.md](README.md) | The findings, with the corpus numbers behind them |
| [../method/README.md](../method/README.md) | How a candidate becomes a finding — the eight verification stages, and what each one removed |

## Findings — bugs in target programs

Latent parameter-triggered failures: the model simulates cleanly at its declared
values and fails at a value its own declaration permits.

| ID | Fix site | Trigger | Confirmed in |
|---|---|---|---|
| [BUG-010](../verified%20bugs/BUG-010-inductor-documents-zero-it-cannot-honour.md) | `Analog.Basic.Inductor.L` | `L = 0` | both tools, 3 models |
| [BUG-002](../verified%20bugs/BUG-002-msl-zero-mass-within-declared-bound.md) | `Translational.Components.Mass.m` | `m = 0` | both tools, 17 models |
| [BUG-018](../verified%20bugs/BUG-018-rotational-inertia-zero-within-declared-bound.md) | `Rotational.Components.Inertia.J` | `J = 0` | both tools, 29 models |
| [BUG-013](../verified%20bugs/BUG-013-capacitor-zero-capacitance-topology-dependent.md) | `Analog.Basic.Capacitor.C` | `C = 0` | both tools, 12 models |
| [BUG-014](../verified%20bugs/BUG-014-genericfluxtube-l-unbounded-divisor.md) | `FluxTubes.GenericFluxTube.l` | `l = 0` | both tools, 3 models |
| [BUG-017](../verified%20bugs/BUG-017-genericfluxtube-area-chain-divisor.md) | `FluxTubes.GenericFluxTube.area` | `area = 0` | both tools, 3 models |
| [BUG-011](../verified%20bugs/BUG-011-fluxtubes-b-mymax-unguarded-divisor.md) | `SoftMagnetic.BaseData.B_myMax` | `B_myMax = 0` | both tools, 2 models |
| [BUG-015](../verified%20bugs/BUG-015-idealgear-zero-ratio.md) | `Rotational.Components.IdealGear.ratio` | `ratio = 0` | both tools, 1 model |
| [BUG-016](../verified%20bugs/BUG-016-relational-invariant-between-two-parameters.md) | `Analog.Ideal.IdealizedOpAmpLimited` (`Vps`/`Vns`) | `Vps = Vns` | both tools, 1 model |
| [BUG-019](../verified%20bugs/BUG-019-invertingamp-frequency-unbounded-divisor.md) | `Analog.Examples.InvertingAmp.f` | `f = 0` | both tools, 3 models |
| [BUG-012](../verified%20bugs/BUG-012-variablepermeance-unbounded-input.md) | `FluxTubes.VariablePermeance` (input) | permeance ≤ 0 | both tools, 1 model |
| [BUG-005](../verified%20bugs/BUG-005-multibody-rotor1d-zero-inertia.md) | `MultiBody.Parts.Rotor1D.J` | `J = 0` | static only |
| [BUG-006](../verified%20bugs/BUG-006-bound-propagated-into-a-different-component.md) | `OpAmpCircuits.Der.k` | `k = 0` | isolated shape only |
| [BUG-003](../verified%20bugs/BUG-003-switchedrlc-zero-resistance.md) | `examples/models/SwitchedRLC.mo` | `R = 0` | this repo, not MSL |

## Per-instance reports — one file per confirmed occurrence

[**INSTANCES.md**](../verified%20bugs/INSTANCES.md) — **26 files**, BUG-020 to
BUG-045, one for every occurrence that was execution-confirmed in both tools.

The two layers answer different questions and a developer needs both:

| | Answers | Example |
|---|---|---|
| fix site, BUG-002 … BUG-019 | *which declaration do I change* | `Mass.m` declares `min=0` |
| instance, BUG-020 … BUG-045 | *which instance was proven to fail, where* | `Damper.mo:6`, `mass1`, at `m = 0` |

`Damper.mo` instantiates three masses and all three were confirmed separately,
so all three have their own file with their own line reference. Every line
reference is checked against MSL 4.1.0 source; regenerate with
`tools/sweep/gen_instance_reports.py`.

**One file per fix site.** A defect that needs two edits in two files is two
entries, even where the cause is identical: BUG-002 and BUG-018 are the same
`min=0` mistake in `Mass.m` and `Inertia.J`, and a developer fixing one does not
thereby fix the other. Likewise BUG-014 and BUG-017, two declarations in one
component that fail by different routes.

### Withdrawn

| Was reported | Actually declares | Why withdrawn |
|---|---|---|
| `Translational.Components.ElastoGap.s_ref = 0` | `min=Modelica.Constants.eps` | the model is correct |
| `Blocks.Continuous.PI.T = 0` | `min=Modelica.Constants.small` | the model is correct |

Both were execution-confirmed in two tools and both were wrong: the trigger was
a value the declaration already forbids. Cause and fix in
[TOOLBUG-010](../toolbugs/TOOLBUG-010-divisorsan-misread-non-literal-min.md).

## Findings — dimensional defects

A declaration whose own binding expression computes a different SI dimension.
Distinct from the parameter-triggered failures above: nothing has to be
*triggered*, and the equations are silent because nothing is inconsistent
*between* equations — the disagreement is inside one declaration.

| Finding | Fix site | Declared | Binding computes |
|---|---|---|---|
| [DCPM_Drive resistance](msl-dcpm-drive-resistance-from-voltage.md) | `Electrical.Machines.Examples.DCMachines.DCPM_Drive:81` | `Ohm` | a voltage |

Found by `QuantitySan`'s `binding-unit-conflict` check with no issue-specific
code; the same check reproduces upstream #4078 and #4079, which share the
mechanism. Method in
[the pattern catalogue](../method/recurring-issue-patterns.md), P1.

## Catalog — every declaration site, with source references

[`catalog/`](catalog/) lists **911 declarations** in MSL 4.1.0 that accept a
physically impossible value, each with `file:line` and its declared modifiers,
grouped into the **17 SI type definitions** they all inherit from.

Read the 17 as the defect count and the 911 as its blast radius: they share
seventeen missing lines in `Units.mo`, and fixing those fixes every site.

Each page also lists the declarations that *do* bound themselves — 43 for
`SI.Resistance` against 281 that don't. That contrast, within one library and
one quantity, is what makes the omissions omissions rather than a design choice.

## Studies

| | |
|---|---|
| [si-type-bounds.md](si-type-bounds.md) | Where MSL's declared domains come from, and the three ways components inherit a bound they cannot keep. The sharpest single item: MSL defines `SelfInductance(min=0)` and `Basic.Inductor` does not use it. |
| [documented-sign-latitude.md](documented-sign-latitude.md) | MSL states that `Basic.Resistor`'s R "is allowed to be positive, zero, or negative". Four of this project's electrical rules asserted otherwise; 114 corpus findings came from the error. Where the bound *does* hold, and why `C`/`L` are different. |
| [sentinel-parameters.md](sentinel-parameters.md) | 74 Spice3 declarations that encode "unset" as `-1e40`, 12 of them on quantities that cannot be negative. Not a defect — a documented workaround — but the same false-positive shape as Chua's diode. |
| [min0-census.md](min0-census.md) | All 326 `min=0` declarations against 147 guarded ones — and why the text-level version of this check, at 23% precision, is not usable. |

## Upstream drafts — nothing has been filed

| | |
|---|---|
| [UPSTREAM-msl-inherited-bounds.md](UPSTREAM-msl-inherited-bounds.md) | Consolidated issue for `modelica/ModelicaStandardLibrary`, with a pre-filing checklist |
| [UPSTREAM-rumoca-compiler-defects.md](UPSTREAM-rumoca-compiler-defects.md) | Compiler defects, for the Rumoca remote |

## Tool defects — not results

In [`../toolbugs/`](../toolbugs/). Recorded because each caps how much of a
corpus the detectors can see.

| | |
|---|---|
| [BUG-001](../verified%20bugs/BUG-001-comprehension-in-for-equation-panic.md) | Compiler panic; root cause verified by prediction |
| [BUG-004](../verified%20bugs/BUG-004-record-array-destructured-without-subscript.md) | Blocks `QuasiStatic.Polyphase` |
| [BUG-007](../verified%20bugs/BUG-007-dae-drops-predefined-enum-in-assert.md) | `AssertionLevel` — **fixed** |
| [BUG-008](../verified%20bugs/BUG-008-bitcode-enumeration-round-trip.md) | Bitcode enumerations — **fixed**, in this project's own code |
| [BUG-009](../verified%20bugs/BUG-009-bitcode-omits-discrete-definitions.md) | Bitcode discrete definitions — **fixed**, likewise |
| [LIMITATION-001](../toolbugs/LIMITATION-001-index-reduction-drops-fixed-start.md) | Correct refusal; costs 2 MSL examples |
| [LIMITATION-002](../toolbugs/LIMITATION-002-no-solver-tolerance-control.md) | No `--rtol`/`--atol` |
| [coverage-blockers.md](../toolbugs/coverage-blockers.md) | The measured 515 blocked models, and one abandoned fix attempt |

## Tools

All in `tools/sweep/`, all reproducible.

| Script | Does |
|---|---|
| `fullsweep.py` | Rumoca-backed sweep, all detectors |
| `omcsweep.py` | OpenModelica-backed parameter search — one model, fan out with `xargs -P` |
| `verify2.py` | The funnel: type attribution, bound provenance, guards, claim strength |
| `resolve_types.py` | Resolves an instance path to the class that declares the parameter |
| `crossconfirm.py` | Re-runs each finding in the other tool |
| `trajdiff.py` | Trajectory differential, with signal-scaled tolerance |
| `diffbatch.py` | Batched acceptance differential |
| `min0_census.py` | Source-level `min=0` census |
| `roundtrip_audit.py` | Bitcode export/import fidelity over construct probes |
| `tests/test_compare_scaled.py` | Calibration for the divergence detector — 7 cases, both directions |
