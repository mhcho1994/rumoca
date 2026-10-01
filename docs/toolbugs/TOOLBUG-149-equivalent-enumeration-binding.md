# TOOLBUG-149 — binding of an equivalent enumeration type rejected

**Status:** fixed.
**Severity:** medium — CDL `Reals.Sources.CalendarTime` passes its
`CDL.Types.ZeroTime` parameter to the `Utilities.Time.Types.ZeroTime`
parameter of the utility model it wraps; TRANSFORM binds its own
`Types.Dynamics` to `Modelica.Fluid.Types.Dynamics`. Both failed with
`ET002 type mismatch: expected ...Utilities.Time.Types.ZeroTime, found
...CDL.Types.ZeroTime`.

## What

```modelica
type A = enumeration(One, Two, Three);
type B = enumeration(One, Two, Three);
model Inner parameter B b = B.One; end Inner;
model M parameter A a = A.Two; Inner sub(b = a); end M;
```

MLS §6.4 (SPEC_0022 TYPE-029): an enumeration type is a subtype of another
enumeration type with the same literals in the same order, so the binding is
legal (OpenModelica accepts it). The binding check compared enumeration types
nominally.

Affected in cluster R2-media: the 5 ZeroTime models (AixLib, Buildings, IBPSA,
IDEAS CDL `CalendarTime*` validations) and the 3 TRANSFORM `Dynamics` models.

## Fix

`crates/rumoca-phase-typecheck/src/typechecker/equation_compat.rs`:
`check_expected_expression_type` (declaration and modification bindings)
accepts an enumeration whose literal list equals the expected one. Equation
compatibility stays nominal (TYPE-013), and enumerations with different
literals stay ET002.

## Test

`crates/rumoca-contracts/tests/type_contracts.rs`:
`equivalent_enumeration_types_are_assignable` (simulates, checks the selected
branch) and `different_enumeration_types_stay_incompatible`.
