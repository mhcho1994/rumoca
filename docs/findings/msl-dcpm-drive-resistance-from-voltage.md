# MSL: a resistance computed from a voltage in DCPM_Drive

**Status:** previously reported upstream; unresolved source occurrence checked
2026-10-01. The original novelty claim was incorrect: the exact case appears
in [a comment on #4098](https://github.com/modelica/ModelicaStandardLibrary/issues/4098#issuecomment-1486606129).
The closing PR #4112 changed battery examples, not DCPM_Drive. See the
[follow-up draft](reporting-2026-10-01/dcpm-followup-draft.md).
**Found:** 2026-09-24 by `QuantitySan`'s `binding-unit-conflict` check, on its
first sweep of the library. No issue-specific code was involved.

## The defect

`Modelica/Electrical/Machines/Examples/DCMachines/DCPM_Drive.mo:81`

```modelica
Analog.Basic.Resistor resistor(R=0.05*dcpmData.VaNominal/1000)
```

`dcpmData.VaNominal` is a voltage. Dividing it by the dimensionless literal
`1000` leaves a voltage, and the result is bound to `R`, declared
`SI.Resistance`:

```
declared   Ohm    kg.m2.s-3.A-2
binding           kg.m2.s-3.A-1      reads VaNominal[V]
```

The divisor would need current units to make the expression a resistance.
The source does not establish whether it is intended to represent a fixed
1000 A scale or the machine's nominal armature current.

## Why it matters

The supported claim is dimensional inconsistency and an unstated current
scale. The earlier report inferred author intent and claimed incorrect
rerating behavior without independent evidence; those claims are withdrawn.
No incorrect numerical trajectory is established by this finding.

This is the same mechanism as upstream [#4078](https://github.com/modelica/ModelicaStandardLibrary/issues/4078)
and [#4079](https://github.com/modelica/ModelicaStandardLibrary/issues/4079), where a capacitance
is bound to the reciprocal of an inductance. In all three the declaration and
its own binding disagree dimensionally, and in all three the equations are
silent because nothing is inconsistent *between* equations.

## Suggested fix

If the fixed scale is intended, name it with current units while preserving
the numerical value:

```modelica
parameter Modelica.Units.SI.Current IScale = 1000;
Analog.Basic.Resistor resistor(R=0.05*dcpmData.VaNominal/IScale)
```

Using `dcpmData.IaNominal` instead requires confirmation of the intended
design relationship; it is not justified by unit checking alone.

## Detection

`QuantitySan.binding-unit-conflict`, severity high. Pinned by
`packages/modelsan/tests/test_quantity.py::
test_a_resistance_bound_to_a_voltage_over_a_bare_number_is_a_conflict`.

The check only claims a conflict when the binding reads at least one variable
carrying a dimensional unit; a bare numeric binding is how every parameter in
the library is written and is never reported. See
[the pattern catalogue](../method/recurring-issue-patterns.md), P1.
