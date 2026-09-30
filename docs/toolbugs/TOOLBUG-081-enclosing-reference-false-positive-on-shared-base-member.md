# TOOLBUG-081 — ER130 on a member the nested class inherits itself

**Status:** fixed.
**Severity:** medium.

## What

```modelica
block Outer
  extends Modelica.Blocks.Interfaces.DiscreteSISO(firstTrigger(start=false, fixed=true));
protected
  block UnitDelay
    extends Modelica.Blocks.Discrete.UnitDelay(firstTrigger(start=false, fixed=true));  // ER130
  end UnitDelay;
end Outer;
```

The MLS §5.3.1 check walked the scopes *enclosing* the current class and
reported any that declared or inherited the reference target. When the
nested class and its enclosing class both inherit the same declaration (same
`DefId`, from a common base), the reference goes through the nested class's
own inheritance, yet the check fired.

Affected (cluster B): Buildings `ChillerPlant...TrimAndRespond` family (4:
TrimAndRespond, .UnitDelay, Examples.TrimAndRespond,
Examples.ChillerSetPointControl) and IDEAS `Airflow.AHU.Adsolair58`
(`DummyExchanger` inherits `port_b2` like its enclosing model) — 5 models.

## Fix

`semantic_checks/enclosing_references.rs`: the nearest class scope that
declares or inherits the target decides; only if that is an enclosing class
(not the current one) is it ER130.

## Test

`overstrict_checks.rs::nested_class_inheriting_same_member_is_not_enclosing_reference`,
`genuine_enclosing_non_constant_reference_still_rejected`.
