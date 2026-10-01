# TOOLBUG-167 — package constants read by a component's dimension (ET004)

**Status:** fixed.
**Severity:** high (basic pattern; blocks the Buildings-family electrical library).

## What

Two forms, both `ET004 unevaluable array dimensions`:

```modelica
package L
  package C  constant Integer n = 2; end C;
  model A  Real x[C.n]; end A;        // sibling package, relative name
  model B  A a; end B;
end L;
```

```modelica
partial model Source
  replaceable package PhaseSystem = PhaseSystems.OnePhase constrainedby PartialPhaseSystem;
  Real S[PhaseSystem.n];
end Source;
model FixedVoltage   // in another package
  extends Source(redeclare package PhaseSystem = PhaseSystems.OnePhase);  // relative name
end FixedVoltage;
model M  FixedVoltage E; end M;
```

Symbolic dimensions are evaluated late, in typecheck, by name in the
component's instance scope (`E.PhaseSystem.n`, `a.C.n`). Typecheck only
seeded package constants for the root model's own extends-redeclares and
enclosing classes, so (a) a package a component's class names by a relative
reference was never collected, and (b) an `extends(... redeclare package ...)`
inside a *component's* class was ignored. In addition, a relative
multi-segment redeclare target (`PhaseSystems.OnePhase`) was resolved to its
first segment's class (`PhaseSystems`), so even the root-model path took the
wrong package.

Affected in cluster R2-dims: the 7 `PhaseSystem.n` cases (IBPSA, Buildings,
IDEAS, AixLib electrical). `Buildings.Electrical.AC.OnePhase.Loads.Examples.DynamicLoads`
now compiles; the rest stop later (overconstrained `Connections.branch`,
a flatten package-constant reference in `PartialPhaseSystem.j`, a cable
record type mismatch).

## Fix

- `crates/rumoca-phase-typecheck/src/component_class_constants.rs`:
  `collect_component_extends_redeclare_constants` seeds
  `{component}.{alias}.*` from the most-derived extends-redeclare in each
  component's class chain (the component's own redeclare still wins), and
  `seed_dimension_reference_classes` seeds the class a dimension reference's
  first segment resolved to under the instance scope when nothing else did.
- `crates/rumoca-phase-typecheck/src/constant_collection.rs`
  (`resolve_redeclare_target_def_id`): a multi-segment target uses its last
  resolved segment or lexical resolution, never the root segment's class.

## Test

`array_dimension_regressions.rs::component_dimension_reads_sibling_package_constant`,
`component_dimension_uses_extends_redeclared_package` (redeclares to an
`n = 3` package and checks the value).
