# TOOLBUG-049 — array variables were invisible to traces, bound checks and RangeSan

**Status:** fixed (this change).
**Severity:** high for the runtime evaluation — every model with an array
variable failed to run under the modelsan Rumoca backend.

## One mismatch, three places

The solver reports an array variable one column per element (`w[1]`,
`w[2]`, ...). Three consumers looked columns up by the *variable's* name
(`w`) and so matched nothing:

| Where | Effect |
|---|---|
| `compile-bitcode --trace-out` (`resolve_trace_plan`) | a trace point on `w` published no rows: `trace point 2 (w) names 'w', which the solver does not report` |
| modelsan `RumocaBackend` | its column check then refused the run: `invalid observations: missing requested columns: ['w', 'x', 'y']`, and element observations lost their variable anchor |
| `compile-bitcode --check` and modelsan `RangeSan` | bounds are declared per variable, so `Real y[3](each max = 2.9)` and `Real w[3](max = {5, 2.5, 5})` were never checked |

In the v2 corpus runtime pass this was the "invalid observations" failure
class, and it silently removed array variables from every run that did
complete.

## Fix

- An array trace point publishes each element under its own trace id `id.k`
  with the element name in the `variable` column.
- The backend treats a variable as present when any element column is, and
  anchors an element observation to its variable while keeping the element
  in its name, so a finding says which element.
- `--check` and RangeSan apply a scalar bound to every element and a literal
  one-dimensional array bound element-wise; other shapes are left unchecked,
  not guessed.

On the fixture above both now report exactly `y[1]`, `y[2]`, `y[3]` and
`w[2]`.
