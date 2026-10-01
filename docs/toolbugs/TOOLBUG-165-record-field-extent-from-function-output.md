# TOOLBUG-165 — flexible record field fixed by a function's record output (ET004)

**Status:** fixed.
**Severity:** high (blocks every Buildings-family mover model).

## What

Minimal form (`Buildings.Fluid.Movers.Data.Generic`):

```modelica
record R  parameter Real y[:]; parameter Real eta[size(y, 1)]; end R;
function f
  input Real p;
  output R res(y = {0, 0.5, 1}, eta = zeros(n));
protected
  constant Integer n = 3;
algorithm
  for j in 1:n loop res.eta[j] := p*j; end for;
end f;
model M  final parameter R r = f(2); end M;
```

`r.y` is bound to `f(2).res.y`; MLS §10.1 takes its extent from that binding,
i.e. from the output declaration's modifier. Rumoca failed in four places
along the pipeline:

1. Typecheck could not infer the shape of `f(p).res.y` (`ET004 unevaluable
   array dimensions for 'r.eta'` — `size(y, 1)` never resolved).
2. Flatten dropped the output's field modifiers entirely, so the function
   output started without `y = {...}` (also a value bug: `res.y` is never
   assigned in the body).
3. The DAE shape proof had no extent for the record field `y[:]` of a
   function value, resolved later field extents (`size(y, 1)`) without the
   sibling field in scope (also for record-constructor calls), and interned
   record value types from the declared `0` sentinel instead of the proven
   extent.
4. The checked function IR assembles a record value only from straight-line
   field writes, so `res.eta[j] := ...` inside a loop/branch was rejected
   (`ED019 record output assembly`).

The same sibling-extent problem hit record *inputs* decomposed into scalar
ABI inputs (`flowParameters.dp[size(V_flow, 1)]` → `ED008 unresolved Flat
reference V_flow`).

Affected in cluster R2-dims: the 15 `motorEfficiency_yMot_generic.eta` cases
(Buildings, IBPSA, IDEAS, AixLib movers). All 15 now pass this point; they
stop at later, different errors (see the cluster report).

## Fix

- `rumoca-eval-ast/src/eval/record_output_dims.rs` (+ trait hook in
  `eval/mod.rs`, `dimension_inference.rs`): shape of `f(args).out.field` is the
  shape of the output declaration's `field` modifier, evaluated with the
  function's inputs and Integer constants bound.
- `rumoca-phase-flatten/src/functions/record_value_modifiers.rs`: a
  record-typed function value with field modifiers gets the record
  constructor call `R(field = ...)` as its declaration equation.
- `rumoca-phase-flatten/src/function_lowering/sibling_field_shapes.rs`:
  decomposed record inputs rename sibling field references in their extents
  (`size(y, 1)` → `size(r_y, 1)`).
- `rumoca-phase-flatten/src/function_lowering/record_field_locals.rs`: a record
  output/local whose fields are written under control flow is carried as one
  local per field (entry value from the constructor default or the field's
  binding), assembled by the constructor before every return.
- `rumoca-phase-dae/src/construction/function_shapes/record_field_shapes.rs`:
  record field shapes take the constructor default's extents for `:` fields
  and see earlier sibling fields; record-constructor calls do the same.
- `rumoca-phase-dae/src/construction/function_construction.rs`:
  `function_value_type_proven` interns record field types from the proven
  shapes.

## Test

`crates/rumoca/tests/suite_core/array_dimension_regressions.rs`:
`record_field_extent_comes_from_function_output_modifier` (simulates and
checks values through branch, loop and later field writes) and
`decomposed_record_input_extent_reads_sibling_field`.
