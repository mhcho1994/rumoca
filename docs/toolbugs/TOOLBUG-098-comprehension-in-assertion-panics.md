# TOOLBUG-098 — array comprehension inside an `assert` panics DAE construction

**Status:** fixed (this change).
**Severity:** high (panic) — `analysis proves the exact comprehension
occurrence` (`rumoca-phase-dae/src/construction/expression.rs`) for the CDL
`Routing.RealExtractSignal` family:

```modelica
initial equation
  assert(Modelica.Math.BooleanVectors.andTrue(
           {(extract[i] > 0 and extract[i] <= nin) for i in 1:nout}), "...");
```

Pre-existing in the baseline, hidden behind TOOLBUG-095 on that model.

## What

Comprehension domains are planned once, model-wide, from
`all_model_expressions` (bindings, equations, initial equations). Assertion
conditions, messages and levels are not in that set, so a comprehension there
had no plan and lowering hit the `expect`.

## Fix

`rumoca-phase-dae/src/construction/analysis.rs`
(`analyze_expression_support`): plan comprehensions of the model and
initial assertions as well.

## Test

`structural_evaluation_regressions.rs`:
`comprehension_inside_initial_assertion_is_planned`.
