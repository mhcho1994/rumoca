# TOOLBUG-089 — `each` on a scalar component and `parameter input` function formals rejected (ER104, ER035)

**Status:** fixed (accepted with warnings WR009 / WR010).
**Severity:** medium.

## What

```modelica
SimpleNLayer simpleNLayer(each final T_start = fill(T0, n));   // ER104 (AixLib)
WattsLawPlug wattsLawPlug(each numPha = 1, ...);               // ER104 (IDEAS)
function windPressureProfile
  parameter input Real xd[:];                                  // ER035 (IDEAS)
```

MLS §7.2.5 gives `each` a meaning only on array components; OpenModelica warns
("'each' used when modifying non-array element") and applies the modification
as written, which is what instantiation already does for a scalar owner.
A `parameter` prefix on a function input constrains nothing the function body
can observe; OpenModelica accepts it.

Affected (cluster B): ER104 — aixlib 3, ideas 3, transform 2; ER035 — ideas 3
— 11 models.

## Fix

- `restrictions/decl.rs::check_modification_restrictions`: WR009 instead of ER104.
- `semantic_checks/mod.rs::check_input_parameter_combination`: WR010 for
  `parameter input` in functions; other classes keep ER035.

## Test

Contract `inst_040_each_on_scalar_component_ignored_with_warning` (simulates:
`Sub s(each T = {2, 3, 4})` integrates slopes 2/3/4 per element);
`overstrict_checks.rs::each_on_scalar_and_function_parameter_input_warn`.
