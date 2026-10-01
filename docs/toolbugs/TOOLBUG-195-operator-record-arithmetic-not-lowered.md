# TOOLBUG-195 — operator-record arithmetic left as plain operators; record equations under-counted

**Status:** fixed.
**Severity:** high. Every model whose equations use `Complex` (or another
operator record) arithmetic, or an if-equation with record branches. In
R2-balance: OpenIPSL `Electrical.Events.Breaker`, PowerGrids
`VoltageTransducers.ExciterVoltageTransducerIEEE`,
`Test.TestExciterVoltageTransducerIEEE`, `Test.TestTerminalVoltageTransducerIEEE`
(4 of 32; all balanced in OpenModelica). The same lowering is what the wider
OpenIPSL / PowerGrids / MSL `ComplexBlocks` and `QuasiStatic` code needs.

## What

```modelica
model Rec2
  Complex a, b(re = time, im = 1), c;
equation
  a = -b;                     // counted as 1 equation, not 2
  c = a + b*Complex(2, 3);    // likewise
end Rec2;                     // ED001: 4 equations, 6 unknowns

model Brk                     // OpenIPSL Breaker, reduced
  Complex vs, vr, is, ir;  Boolean Open;
equation
  if Open then is = Complex(0); ir = Complex(0);
  else         vs = vr;        is = -ir;  end if;   // 2 rows instead of 4
  ...
end Brk;
```

MLS §14.5 says `a + b` on operator records is a call of the record's
`'+'` operator function (with a constructor conversion of a Real operand,
`2*b` = `'*'.multiply(Complex(2), b)`). Rumoca never performed this
lowering: Flat kept `Binary`/`Unary` nodes on records, the operator
functions were never collected, and the DAE record-equation analysis, which
destructures only `r = record_call(...)` and `r1 = r2`, fell back to
counting the equation as one scalar row. If-equations were worse: their
branches became one conditional residual `if c then (is - Complex(0)) else
(vs - vr)` whose scalar count came from AST shape inference (1).

## Fix

New Flat post-pass `rumoca-phase-flatten/src/postprocess/operator_records.rs`
(+ `typing.rs`, `split.rs`), run in `finalize_flat_model` right after the
first function collection (callee result types are known then); functions
are collected again when it changes anything.

- Types each operand (record instance, constructor/function result, Real,
  Integer, else unknown → left untouched), finds the operator in the record
  class (or its bases), picks the overload whose inputs match exactly, and
  otherwise the one reachable by a single unique `'constructor'` conversion.
  Unary minus uses the one-argument `'-'` function; unary plus is identity.
- A record-valued `if`-expression operand is distributed:
  `k*(if c then conj(u) else u)` → `if c then k*conj(u) else k*u`, so the
  record-argument ABI (which destructures record variables and calls) can
  pass every argument.
- Record equations the DAE analysis cannot destructure (conditional
  residuals from if-equations, `r = if ...`, two record expressions) are
  split into one equation per field (MLS §8.3.1), projecting record
  variables to their field variables, positional constructors to their
  argument, and other expressions through `FieldAccess`. `value = r` is
  re-oriented to `r = value`. Equations inside structured families are left
  as they are.

Known limitation: the existing record-parameter ABI duplicates nested call
arguments per field (`f(g(x).re, g(x).im)`), so deeply nested operator
expressions grow 2^depth before CSE.

## Test

`crates/rumoca-contracts/tests/oprec_contracts.rs`:
`oprec_operator_expressions_lower_to_operator_function_calls`,
`oprec_if_equation_with_record_branches_counts_every_field`,
`oprec_conditional_record_operand_is_lowered_per_branch` (balanced +
simulated values against hand-computed results).
