# TOOLBUG-054 — Integer exponentiation typed Integer, refused by Solve

**Status:** fixed (this change).
**Severity:** medium — a contract failure ("typed program operand or result
type mismatch") on ordinary library code; found in IBPSA (`round`, the CDL
`Discrete` and `SampleTrigger` validations, `VariablePulse`).

## What

```modelica
function round
  input Real x; input Integer n; output Real y;
protected
  Real fac = 10^n;
algorithm
  y := floor(x*fac + 0.5)/fac;
end round;
```

The checked DAE typed `10^n` as `Integer`, promoting the operands the way it
does for `+` and `*`. SPEC_0022 TYPE-034 (MLS §6.7) makes exponentiation Real
even over Integer operands -- `10^(-1)` is `0.1`. The typed-function lowering
then passed the two Integer registers to Solve's `Power`, which accepts only
Real operands, and program construction failed.

## Fix

- `rumoca-ir-dae` type rules: scalar `^` and elementwise `.^` are Real; a
  matrix power (repeated multiplication) keeps its element type.
- Typed-function lowering converts Integer operands of `^`/`.^` to Real, as
  it already did for division.

`round(time, -1)` evaluates; IBPSA `Round`, `UnitDelay`, `SampleTrigger`
and the other `Discrete` validations run.
