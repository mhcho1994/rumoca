# TOOLBUG-095 — `min`/`max` over Boolean values rejected

**Status:** fixed (this change).
**Severity:** medium — `ED020 … expected a numeric expression, found Boolean`
for every model using `Modelica.Math.BooleanVectors.allTrue`/`anyTrue`
(e.g. CDL `Routing.RealExtractSignal`, IDEAS compressor
`TemperatureProtection`).

## What

`allTrue` is `size(b, 1) == 0 or min(b)`. MLS §3.7.1.1/§10.3.4 order
Boolean values `false < true`, so `min`/`max` are defined on them. The DAE
type rules applied the numeric-operand gate to `min`/`max` before looking at
the operator.

## Fix

`rumoca-ir-dae/src/expression/type_rules.rs`: `boolean_extremum_result`
types `min`/`max` over Boolean operands (a Boolean scalar for the reduction,
the common Boolean type for the binary form).

## Test

`structural_evaluation_regressions.rs`: `boolean_min_and_max_are_defined`.
