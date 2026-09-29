# TOOLBUG-043 — `array .+ scalar` is rejected by DAE construction

**Status:** fixed (this change).
**Severity:** low-medium — valid Modelica refused at compile time.

## What

```modelica
model EW
  Real x[3](each start = 1, each fixed = true);
  parameter Real k[3] = {1, 2, 3};
equation
  der(x) = -(k .* x) ./ (k .+ 1);
end EW;
```

```
[ED020] canonical DAE construction rejected an invalid operation:
  expression shape mismatch      (at `k .+ 1`)
```

MLS §10.6.1 defines element-wise addition of an array and a scalar
(`a .+ s` adds `s` to every element). The checked constructor accepts
`array .+ array` of equal shape but not the scalar broadcast, so the
frontend must either lower the scalar operand to a `fill` of the array's
shape or the DAE's element-wise type rule must admit a scalar operand.

Found while writing the element-wise bitcode fixture (TOOLBUG-042); the
fixture uses `k .+ x` instead.

## Fix

`.+` and `.-` now use the element-wise type rule that `.*`, `./` and `.^`
already used, which admits a scalar on either side; `+` and `-` keep
requiring equal shapes, as MLS §10.6.1 says. Verified end to end:
`y = 2 .+ x` simulates with every element equal to `2 + x[i]`, and the model
round-trips through bitcode.
