# TOOLBUG-072 — outer `extends` redeclaration of a grandparent's component lost

**Status:** fixed (this change).
**Severity:** high — EI012 ("cannot instantiate partial class
`...PartialTwoWayValve` for component `valLin.res1`"), and silently wrong
parameter values when the intermediate class was not partial.

## What

```modelica
partial model Res
  replaceable PI r constrainedby PI;
end Res;
partial model Valve
  extends Res(redeclare replaceable PV r constrainedby PV(k = 2));
end Valve;
model V3
  extends Valve(redeclare Lin r);
end V3;
```

This is the `Fluid.Actuators` three-way valve structure of Buildings, IBPSA,
IDEAS and AixLib (`PartialThreeWayResistance` declares `res1`,
`PartialThreeWayValve` redeclares it to `PartialTwoWayValve`,
`ThreeWayLinear` redeclares it to `TwoWayLinear`). Two defects:

1. `merge_class_content` applies an extends clause's redeclarations only to
   the base class's *own* components. `r` reaches `V3` through `Valve`'s
   recursive merge, so `redeclare Lin r` was dropped and the partial `PV`
   instantiated (EI012). With a non-partial intermediate the wrong class was
   instantiated without any diagnostic.
2. The parser folded an element-redeclaration inside a modification
   (`redeclare replaceable PV r constrainedby PV(k = 2)`) into
   `Modification { r, PV() }` and dropped the constraining clause with its
   modifiers. MLS §7.3.2 applies those modifiers to the element, and the
   valves pass `m_flow_nominal`, `dpValve_nominal`, `CvData`, ... this way,
   so they were lost even without a further redeclaration (`k` stayed 1).

Affected cluster-A models: `Fluid.Actuators.Valves.Examples.ThreeWayValves`
(AixLib), `Fluid.HydronicConfigurations.ActiveNetworks.Diversion`
(Buildings), and every model instantiating a three-way valve.

## Fix

- `crates/rumoca-phase-instantiate/src/inheritance.rs` /
  `inheritance/redeclaration.rs`: after the recursive merge,
  `apply_extends_modifications` validates and applies the clause's
  redeclarations that target transitively inherited components
  (`collect_inherited_redeclarations`); the type/dimension application shared
  with `merge_class_content` moved into `apply_collected_redeclarations`.
  The innermost-first merge order makes the outermost redeclaration win.
  A redeclaration without a constraining clause records the original
  declaration's type as the element's constraint (MLS §7.3.2), so a further
  redeclaration is checked against the original constraint, not against the
  intermediate type (OpenIPSL `redeclare replaceable Integrator sISO` then
  `redeclare replaceable IntegratorLimVar sISO`, both `SISO`s).
- `crates/rumoca-phase-parse/src/expressions.rs`:
  `merge_redeclare_constraining_modifications` appends a redeclaration's
  constraining-clause modifiers to its class modification; a modifier the
  declaration states itself takes precedence (as for body declarations).

## Test

`crates/rumoca-contracts/tests/inst_contracts.rs`:
`inst_043_outer_extends_redeclare_replaces_transitively_inherited_component`
simulates the reduced model and checks `w.r.x = 20` (class `Lin`, `k = 2`).
