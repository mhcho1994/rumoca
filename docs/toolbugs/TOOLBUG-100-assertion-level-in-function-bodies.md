# TOOLBUG-100 — `assert(..., AssertionLevel.error)` refused inside functions

**Status:** fixed (warning level in a function body still refused, see below).
**Severity:** medium — AixLib window-ventilation models
(`AixLib.Airflow.WindowVentilation.EmpiricalExpressions.Jiang`,
`...OpeningAreas.OpeningAreaSashVDI2078`, 2 cases in cluster D) failed with
`ED008 unresolved Flat reference AssertionLevel.error`.

## What

```modelica
model M
  function f
    input Real w; output Real a;
  algorithm
    assert(w <= 2, "too big", AssertionLevel.error);
    a := 2*w;
  end f;
  Real x;
equation
  x = f(time);
end M;
```

Two defects stacked:

1. The predefined enumeration types `StateSelect` and `AssertionLevel`
   (MLS §4.9.5) had type ids but no entry in `type_ids_by_def_id`, so the DAE
   function-scope catalog could not recognise `AssertionLevel.error` as an
   enumeration literal (a user literal resolves to its type's declaration; a
   predefined literal resolves to its own predefined identity, rooted at the
   type) and reported it unresolved. Model-level asserts took a different
   path and worked (TOOLBUG-056).
2. Once resolved, any explicit `level` in a function body was refused, even
   `AssertionLevel.error`, which MLS §8.3.7 defines as the default.

## Fix

- `rumoca-phase-typecheck` (`build_type_context`): register the predefined
  enumeration types under their predefined DefIds.
- `rumoca-phase-dae` (`ShapeEnvironment::is_enumeration_literal`): a
  two-part reference rooted at an enumeration type declaration is a literal.
- `rumoca-phase-dae` (`plan_proven_function_assertion`): a level proven to be
  ordinal 2 (`error`) is the default severity.

`AssertionLevel.warning` inside a function body is still refused with its own
ED019 message: the DAE function-body assertion has no warning severity, and
lowering it as an error would stop simulations OpenModelica continues.

## Test

`crates/rumoca/tests/suite_core/frontend_functions_names.rs`:
`function_assertion_with_error_level_is_the_default_assertion`,
`function_assertion_with_warning_level_is_refused_not_promoted`.
