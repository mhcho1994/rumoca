# TOOLBUG-103 — function shape over a field of a package record constant

**Status:** fixed (the models then reach a separate DAE function-loop limit).
**Severity:** medium — `EF023 unresolved flat reference:
IDEAS.Media.Antifreeze.PropyleneGlycolWater.proCoe.nT` on the IDEAS and AixLib
propylene-glycol media validations (4 cases in cluster D).

## What

```modelica
package Med
  record Coef parameter Integer n; Integer nT[n]; end Coef;
  constant Coef proCoe(n = 2, nT = {2, 1});
  function total
    input Real a[sum(proCoe.nT)];
    output Real f;
  algorithm
    f := sum(a);
  end total;
end Med;
```

Flatten materializes the constants a function parameter's dimension reads
(`function_shape_constants.rs`) by the target DefId of each reference. The
target of `proCoe.nT` is the declaration of the field `nT` inside `Coef`,
which every instance of the record shares and which has no value. The value
belongs to the constant `proCoe`.

## Fix

`crates/rumoca-phase-flatten/src/postprocess/function_shape_constants.rs`:
when the target declaration has no value and the reference is an
unsubscripted field path, the value is read from the record constant: the
qualified name of the root segment's declaration (by DefId) extended with the
structured field path, which is how constant extraction records record
constant fields.

## Test

`crates/rumoca/tests/suite_core/frontend_functions_names.rs`:
`function_shape_over_a_record_constant_field_reads_the_constant`.

## Remaining

The IDEAS/AixLib `polynomialProperty` then fails in DAE construction with
`ED019 function loop domain ... requires a compact dependent-domain
transition`: its inner loop range `0:proCoe.nT[i+1]-1` depends on the outer
index, which the DAE function-loop lowering does not support.
