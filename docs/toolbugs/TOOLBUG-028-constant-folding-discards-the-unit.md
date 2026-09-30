# TOOLBUG-028: constant folding discards the unit

**Status:** frontend part fixed (2026-09-30): the frontend no longer folds package constants; `postprocess/package_constants.rs` keeps each Real scalar package constant declared, with its unit, under its qualified name. The default `inline-constants` bitcode pass still replaces references with literals, so a unit analysis must read `rumoca compile --pass none` output (or the artifact before that pass).
**Found:** 2026-09-24, verifying `QuantitySan`'s first full-corpus sweep. Three
of ten high-severity findings were this defect and not defects in MSL.

## The defect

A reference to a declared physical constant is folded to a bare numeric literal
and its unit is thrown away. `Modelica.Constants.mu_0` is declared with unit
`H/m`; after flattening, nothing in the artifact records that.

MSL source — `Magnetic/FluxTubes/Material/HardMagnetic/BaseData.mo`, correct:

```modelica
final parameter SI.RelativePermeability mu_r = B_r/(mu_0*H_cB)
```

The same declaration in the bitcode:

```
material.mu_r   unit='1'  quantity='RelativePermeability'
                binding=(material.B_r / (1.25663706212e-06 * material.H_cB))
```

`mu_0` is not a variable in the artifact at all. Dimensionally the source reads
`T / ((T.m/A) * (A/m))` = dimensionless, which is right. What the artifact
carries reads `T / (1 * A/m)` = `kg.m.s-2.A-2`, which is not.

## Why it matters

Any dimensional analysis over this IR is unsound on every expression that
mentions a physical constant, and it fails in the *reporting* direction: a
correct model looks wrong. Three of the ten `binding-unit-conflict` findings on
the first sweep were correct MSL code that this defect made look
dimensionally inconsistent:

| Model | Declaration |
|---|---|
| `Magnetic.FluxTubes.Examples.MovingCoilActuator.ArmatureStroke` | `pmActuator.material.mu_r` |
| `…MovingCoilActuator.Components.PermeanceActuator` | `material.mu_r` |
| `ModelicaTest.Magnetic.FluxTubes.BasicComponents` | `converter1.G_m` |

`DimensionSan` has the same exposure wherever a folded constant reaches an
additive or relational operand; it has not been audited for it.

The loss is not recoverable by a consumer in general. It happens to be
recoverable here only because the value is distinctive enough to match back
against `Modelica.Constants`, which is a workaround, not a fix.

## Fix

Folding should preserve the unit of what it folded. Either keep the constant as
a variable with its declaration intact and let consumers fold, or attach the
unit to the resulting literal. The first is preferable: a constant that keeps
its identity also keeps its provenance, and a reader of the artifact can still
see which constant a number came from.

Relevant code is in `crates/rumoca-eval-flat/src/constant/`, which has
uncommitted changes in the working tree as of this writing, so the fix should
be sequenced after that work.

## Interim mitigation

`QuantitySan` recovers the unit by matching a literal's exact value against the
constants `Modelica/Constants.mo` declares, so the three findings above are no
longer reported. That is a workaround for consumers of this artifact and does
not make the artifact correct — anything else reading the bitcode still sees a
dimensionless number where a permeability was written.

## Regression

A model binding a dimensionless parameter to an expression over `mu_0` must
produce no `binding-unit-conflict`. Pinned by
`packages/modelsan/tests/test_quantity.py::
test_a_folded_physical_constant_does_not_look_dimensionless`.
