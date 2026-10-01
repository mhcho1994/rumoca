# TOOLBUG-143 — partial media interface functions mis-selected (icon base, alias + target)

**Status:** fixed (the call now reaches the DAE, which rejects the still-partial
function).
**Severity:** medium — every model whose `Medium` is still
`Modelica.Media.Interfaces.PartialMedium` (or calls through an inherited
`replaceable function position = positionBase`) failed in Flatten with
`EF025 ... callable owner has multiple exact exposures for the selected
implementation` or `... function source package has multiple exact override
package selections`, i.e. a bogus Flatten-internal diagnosis.

## What

```modelica
partial function Icon end Icon;                     // Modelica.Icons.Function
partial package PM
  replaceable partial function f extends Icon; input Real x; output Real y; end f;
  replaceable partial function g extends Icon; input Real x; output Real y; end g;
end PM;
model S
  replaceable package Medium = PM;
  Real y = Medium.f(Medium.g(time));
end S;
```

1. A body-less function with a function base was treated as an alias of that
   base, and the alias chain walked to its end. `PartialMedium.setState_phX
   extends Modelica.Icons.Function` therefore "implemented" the empty icon,
   and so did every sibling, so the owner exposed the icon five times.
2. `replaceable package Medium = PM` puts both `S.Medium` and `PM` on the
   override list; both contain `PM`, so the source-package lookup reported
   two selections although the alias simply denotes its target.

Affected in cluster R2-media: the 16 "multiple exact exposures" and 9
"multiple exact override package selections" models (IBPSA/IDEAS/Buildings
`Fluid.Sensors.*`, `Airflow.Multizone.MediumColumn*`, ThermoPower
`DittusBoelter`, `HeatTransfer2phDB`, TRANSFORM closure relations, ...).
Those whose medium really is partial now stop at the DAE with
`ED008 unresolved Flat reference Medium.setState_phX` (a partial function has
no body; OpenModelica checks these models but cannot simulate them). The
others reach later, unrelated errors.

## Fix

`crates/rumoca-phase-flatten/src/pipeline/function_overrides_and_dims/`:
- `exact_identity_queries.rs`: a function that declares its own formals is
  its own interface, not an alias (`function_alias_requires_exact_selection`),
  and the alias chain stops at the first base declaring formals
  (`replaceable function position = positionBase`).
- `function_selection.rs`: `retain_most_specific_packages` drops a candidate
  override package that another candidate specializes (its package chain
  contains it). Unrelated candidates stay ambiguous.

## Test

`crates/rumoca-contracts/tests/func_contracts.rs::partial_medium_interface_call_is_selected_through_its_alias`
(Flatten succeeds; the DAE rejects the partial call).
