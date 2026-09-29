# TOOLBUG-053 — affine state equations refused unless written `c*der(x) = f`

**Status:** fixed (this change).
**Severity:** medium — refused models the compiler accepts at runtime
("matched derivative is not an isolated affine product"); found in OpenIPSL
and ThermoPower.

## What

The Solve lowering isolated a matched derivative only from `der(x) = f` and
`c*der(x) = f`, with `c` a compile-time number and `f` derivative-free.
Three ordinary forms were refused:

```modelica
umin + Tfilter*der(umin) = if ... then u else umin;   // OpenIPSL PVD1
J*der(omega_m)*omega_m = Pm - Pd - Pe/eta;             // ThermoPower Generator
der(e2r) = -Omegab*s*(e1m - e2m) + der(e1r) + ...;     // OpenIPSL MotorTypeV
```

Each is affine in the matched derivative: the first has it as one term of a
sum, the second under a product with a runtime coefficient, the third reads
another state's derivative that has its own explicit row.

The runtime-coefficient refusal was stricter than needed: a runtime divide by
zero is already a recorded domain violation, so only a coefficient that is
the constant zero is a translation-time error.

Once ThermoPower's generator lowered, `TestElectrical2` then failed at its
first event: the grid row `f = fnom*(1 + droop*(P - Poff)/Pgrid)` reads
`f ~ 50` while its unknown `P ~ 0` enters with coefficient `2.5e-9`, so the
Jacobian row scale asked for a residual of `2.5e-19` — far below one ulp of
`f` (`7.1e-15`, the residual actually reached).

## Fix

- `lower/summed_derivative.rs`: the side holding the derivative is flattened
  through `+`/`-`/unary signs into signed terms; exactly one may contain the
  derivative, and it must peel to `der(x)` through products and quotients
  whose other operand is derivative-free. The row computes
  `der(x) = (other - Σ offsets) / Π factors`, with the sign restored.
- A coefficient that is not compile-time numeric is divided at run time; a
  constant zero is still refused before runtime.
- `DerivativeReads::Substituted`: a state row, and a derivative definition
  substituted into another row, may read other states' derivatives, which are
  recomputed from their own definitions (the existing active stack refuses a
  definition that reaches itself). Tensor-family lowering keeps the strict
  derivative-free form.
- Event-boundary projection: a row that misses its scaled tolerance is
  accepted when its residual is below its own rounding noise,
  `64 * eps * Σ_j |∂r/∂y_j| |y_j|` from the row's full gradient. Rows without
  a gradient are never accepted this way.

`GenerationTripping` and `TestElectrical2` now run; `MotorTypeV` and
`TestElectrical6/7` lower and fail at initialization instead, as standalone
components with held inputs (the same class as `MotorTypeI/III`).
`ULTC_VoltageControl` stays refused: its branch conditions read `der(m)`
itself, so the row is genuinely implicit.
