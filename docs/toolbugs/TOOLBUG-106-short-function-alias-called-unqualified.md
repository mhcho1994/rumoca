# TOOLBUG-106 — short function alias called unqualified: EF019

**Status:** fixed.
**Severity:** medium — `EF019 inconsistent resolved function reference:
rendered Modelica.Media.Water.IF97_Utilities.hl_p, structured hl_p` (and
`rhol_T`) on ThermoPower and TRANSFORM water models (5 cases in cluster D).

## What

MSL `Modelica.Media.Water.IF97_Utilities` declares
`function hl_p = BaseIF97.Regions.hl_p;` and its own functions call `hl_p(p)`.

```modelica
package Util
  package Base function hl_p input Real p; output Real h; algorithm h := 2*p; end hl_p; end Base;
  function hl_p = Base.hl_p;
  function twice input Real p; output Real h; algorithm h := hl_p(p); end twice;
end Util;
```

Resolve renders the call as the exposure it looked up (`Util.hl_p`) but binds
the root segment to the alias's target declaration (`Util.Base.hl_p`). The
flatten restatement that reconciles the structured path with the rendered
name only prepended the target's own enclosing scopes (`Util.Base`), which
never spell `Util.hl_p`, so the call was rejected.

## Fix

`crates/rumoca-phase-flatten/src/functions/callable_scope_identity.rs`:
`alias_qualified_path` restates such a call when the rendered name is
`<scope>.<root>...`, `<scope>` is a class, and `<scope>.<root>` is a pure short
alias (one base, no own components, algorithm or external clause) whose base
is exactly the root segment's resolved declaration. The prepended segments
carry `<scope>`'s own ancestry DefIds; nothing is looked up by spelling alone.

## Test

`crates/rumoca/tests/suite_core/frontend_functions_names.rs`:
`short_function_alias_called_by_a_sibling_keeps_one_exposure`.
