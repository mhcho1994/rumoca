# TOOLBUG-082 — Quoted function names such as `'abs'` treated as operator functions (ER092)

**Status:** fixed.
**Severity:** medium — every PowerGrids model (MSL 3.2.3 `ComplexMath.'abs'`).

## What

`check_operator_placement` classified any function whose name starts with a
quote as an operator function and required it to live in an operator record.
MSL 3.2.3 `Modelica.ComplexMath` declares ordinary functions `'abs'`,
`'max'`, `'sum'`, ... (quoted only because the names clash with builtins).

Affected (cluster B): PowerGrids — 3 models.

## Fix

The AST does not retain the `operator` prefix of `operator function`, so the
check now recognises operator functions by the MLS §14 overloadable operator
names (`'constructor'`, `'0'`, `'String'`, `'+'`, ..., `'not'`).
`restrictions/decl.rs::is_overloadable_operator_name`.

## Test

`overstrict_checks.rs::quoted_non_operator_function_name_is_not_an_operator`;
`decl_026_operator_function_outside_operator_record_rejected` still passes.
