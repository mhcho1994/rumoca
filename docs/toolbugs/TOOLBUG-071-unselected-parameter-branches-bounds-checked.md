# TOOLBUG-071 — array bounds checked in branches a parameter condition removes

**Status:** fixed (this change).
**Severity:** medium — ET009 ("subscript 1 ... out of bounds for dimension
of size 0") on valid models with empty arrays.

## What

The late type check already walks only the selected branch of a parameter
if-equation (MLS §8.3.4), but two common shapes were not recognised:

1. A condition written with `size()` of a time-varying array. `size(u, 1)` is
   a parameter expression whatever the variability of `u` (MLS §3.8), but the
   condition collector saw the input `u` and treated the condition as
   time-varying, so every branch was bounds-checked:

   ```modelica
   block MultiOr
     parameter Integer nin = 0;
     Interfaces.BooleanInput u[nin];
   protected
     Boolean uTemp[nin];
   equation
     if size(u, 1) > 1 then
       uTemp[1] = u[1]; ...
     elseif size(u, 1) == 1 then
       uTemp[1] = u[1]; y = uTemp[1];
     else
       y = false;
     end if;
   end MultiOr;
   ```

2. If-*expressions* with parameter conditions were never branch-selected:
   `X_w = if Medium.nXi == 0 then 0 else Xi[1]` (IBPSA/AixLib
   `Airflow.Multizone.MediumColumn`, for a medium without moisture).

Affected cluster-A models: the CDL `Logical.MultiAnd`/`MultiOr` blocks and
their validation models in Buildings, IBPSA, IDEAS and AixLib (10 models), and
`Airflow.Multizone.MediumColumn` (AixLib, IBPSA) whose next error is outside
this fix.

## Fix

`crates/rumoca-phase-typecheck/src/typechecker/late_methods/parameter_branches.rs`:
the condition-reference collector skips the array operand of `size`/`ndims`;
a shared `select_parameter_branch` serves if-equations and the new
`select_parameter_if_expression_branch`.
`crates/rumoca-phase-typecheck/src/typechecker/traversal_adapter.rs`: the
traversal asks for an if-expression's selected branch and walks only its
conditions and value (the whole expression still gets its operator check).

## Test

`crates/rumoca-contracts/tests/arr_contracts.rs`:
`arr_026_size_condition_selects_if_equation_branch`,
`arr_026_parameter_if_expression_skips_unselected_branch`,
`arr_026_parameter_if_expression_checks_selected_branch` (the selected branch
is still bounds-checked).
