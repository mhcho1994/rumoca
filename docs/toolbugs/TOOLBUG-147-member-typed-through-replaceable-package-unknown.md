# TOOLBUG-147 — connector member typed through a replaceable package reported unknown in modifiers

**Status:** fixed.
**Severity:** high — `UAeff(final y = ... abs(port_a.m_flow) ...)` in IBPSA
`HeatExchangers.EvaporatorCondenser` (and every modifier reading a fluid port's
`m_flow`, `p`, `h_outflow`) failed with `ET001 unknown member m_flow on
component reference port_a.m_flow of type Modelica.Fluid.Interfaces.FluidPort_a`.

## What

```modelica
connector Port
  replaceable package Medium = PM;
  flow Medium.MassFlowRate m_flow;
  Real p;
end Port;
model Vol
  replaceable package Medium = PM;
  Port port_a(redeclare package Medium = Medium);
  Pass pas(u = abs(port_a.m_flow));
end Vol;
```

The member table used to type component references inside modifiers dropped
every member whose declared type crosses a replaceable edge
(`Medium.MassFlowRate` has no lexical type identity, MLS §7.3), so the member
looked absent. Plain equations were unaffected because they use instance
data.

## Fix

`crates/rumoca-phase-typecheck/src/modifier_targets.rs`: such a member is
recorded with an unknown type instead of being dropped; the existing path
treats an unknown-typed member as unchecked.

## Test

`crates/rumoca-contracts/tests/func_contracts.rs::package_forwarded_under_another_name_selects_the_enclosing_choice`
(the same model reads `port_a.m_flow` in a modifier).
