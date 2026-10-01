# TOOLBUG-150 — Package constant in a function body left unfolded when the model also reads it (ED008)

**Status:** fixed.
**Severity:** medium.

## What

A Real package constant that an *equation* reads is declared by flatten as a
model constant under its qualified name (`Modelica.Constants.eps`). The
substituter then treated that name as a live model variable everywhere,
including inside function bodies, so a function reading the same constant kept
the reference. The DAE function owner has no access to model coordinates and
reported `ED008 unresolved Flat reference Modelica.Constants.eps`.

```modelica
package P
  package C constant Real eps = 1e-15; end C;
  function F input Real a; output Real y;
  algorithm if a < C.eps then y := 0; else y := 1/a; end if; end F;
  model M Real x = time + 1; Real y; Real z;
  equation y = F(x); z = if noEvent(x > C.eps) then x else 0; end M;
end P;
```

Affected: `AixLib.Airflow.WindowVentilation.EmpiricalExpressions.Jiang`
(function `EquivalentOpeningArea`), 1 model of cluster R2-fnbody.

## Fix

`crates/rumoca-phase-flatten/src/postprocess.rs`: function bodies are
substituted with an empty live-variable set — a function body cannot read a
model variable (MLS §12.2), so no model variable can shadow a name there.

## Test

`crates/rumoca/tests/suite_core/function_body_and_binding_regressions.rs::function_body_folds_a_package_constant_the_model_also_declares`.
