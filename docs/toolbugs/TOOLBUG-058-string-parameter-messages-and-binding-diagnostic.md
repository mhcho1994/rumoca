# TOOLBUG-058 — String-parameter assert messages refused; unbound-parameter refusal misnamed

**Status:** fixed (this change).
**Severity:** low — one refusal and one misleading diagnostic.

## What

1. IBPSA's CDL `Utilities.Assert` passes a `parameter String message` to
   `assert`. Event messages accepted only literals, concatenation and
   `String(...)` conversions, so the parameter was refused ("Solve event
   messages require String literals, ...").
2. Every translation-time read of a parameter with no binding failed with
   "affine derivative coefficient parameter has no static binding", the text
   of the first caller that used the helper. OpenIPSL's Simulink `PSS` and
   `ExcitationSystem` were reported as an affine-derivative limitation when
   the cause was an unset gain (`parameter Real Kp;`), and the report
   classified them as backend limits.

## Fix

- A String parameter in a message lowers to its binding: Solve has no String
  storage, so its value is fixed at translation time.
- The refusal names the parameter: "parameter `Kp` has no static binding, but
  its value is needed at translation time".

The evaluation's unset-parameter check also now recognises the default
`start = 0` the parser gives built-in `Real` declarations (spanned at the
declaration itself) as unset.
