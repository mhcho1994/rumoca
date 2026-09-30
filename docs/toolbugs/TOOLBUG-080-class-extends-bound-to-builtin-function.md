# TOOLBUG-080 — `redeclare function extends product` binds to the builtin `product`

**Status:** fixed.
**Severity:** high — every model using `PhaseSystems.OnePhase` failed to resolve.

## What

```modelica
partial package Base
  replaceable partial function product
    input Real x[2]; input Real y[2]; output Real z[2];
  end product;
end Base;
package OnePhase
  extends Base;
  redeclare function extends product
  algorithm
    z := {x[1]*y[1] - x[2]*y[2], x[1]*y[2] + x[2]*y[1]};   // ER002: 'x', 'y', 'z' unresolved
  end product;
end OnePhase;
```

The base of a `class extends B` clause was looked up lexically first. The
inherited-member view is only available after the first extends round, so
the lookup found the builtin reduction `product` and bound the clause to it
(ER132 "cannot verify ExternalObject lifecycle ... resolved base is missing",
then ER002 for every inherited input/output). `j`, `rotate`, ... worked
because no builtin has those names.

Affected (cluster B): Buildings/IBPSA/AixLib/IDEAS `Electrical.PhaseSystems.OnePhase`
users — 13 models (aixlib 3, buildings 3, ibpsa 3, ideas 4).

## Fix

`rumoca-phase-resolve/src/extends.rs`: for the same-name form (MLS §7.3.1),
the inherited slot of the enclosing class is consulted first; while the
inherited view is not yet published, a lexical hit on a non-class declaration
is deferred to the next fixed-point round instead of being bound.

## Test

`rumoca-phase-resolve/src/tests/overstrict_checks.rs::class_extends_prefers_inherited_slot_over_builtin_name`.
