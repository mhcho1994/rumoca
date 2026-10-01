# TOOLBUG-146 — package redeclaration forwarded under another name uses the alias's default

**Status:** fixed.
**Severity:** high — `redeclare package Medium = MediumCon` (or `= Medium1`,
`= MediumA`, ...) where the right-hand side is the enclosing class's own
replaceable package left the nested component on `MediumCon`'s partial
default, so every IBPSA/IDEAS/Buildings heat pump and chiller
(`heaPum.con.vol.dynBal.medium`, `chi.con...`), borefield
(`borFie.borHol.intHex[1].vol1...`) and Templates load failed with
`EI012 cannot instantiate partial class Medium.BaseProperties`.

## What

```modelica
model Vol replaceable package Medium = PM; Medium.BaseProperties medium; end Vol;
model Mach
  replaceable package MediumCon = PM;
  Vol vol(redeclare package Medium = MediumCon);
end Mach;
model T Mach chi(redeclare package MediumCon = W); end T;
```

`extract_component_class_overrides` proved the redeclare value's class
identity lexically: `MediumCon` is `Mach.MediumCon`, a replaceable alias whose
declared default is `PM`. Only the self-forwarding spelling
(`redeclare package Medium = Medium`) consulted the enclosing instance's
selection; a differently named alias was taken at its default.

Affected in cluster R2-media: 10 EI012 models (heat pumps, chillers,
borefield, `LoadTwoWayValve`, TRANSFORM `OilWater_Simple_HX`). They now reach
later errors (TOOLBUG-147, record-parameter conditions EI006, array
dimensions ET004).

## Fix

`crates/rumoca-phase-instantiate/src/nested_scope.rs`
(`resolve_component_nested_type_overrides`): a class override whose target is
an alias the enclosing scope's type-override map selects is retargeted to
that selection and marked instance-dependent (forwarding).

## Test

`crates/rumoca-contracts/tests/func_contracts.rs::package_forwarded_under_another_name_selects_the_enclosing_choice`
(simulates; checks `W.BaseProperties` is used).
