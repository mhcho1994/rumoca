# TOOLBUG-133 — `redeclare function extends F` of a short-class alias overflows the stack

**Status:** fixed.
**Severity:** high (compiler crash).

## What

```modelica
model BaseFlow                          // ThermoPower.Functions.FanCharacteristics.Models
  replaceable function flowCharacteristic = dummyFlow constrainedby baseFlow;
end BaseFlow;
model SplineFlow
  extends BaseFlow;
  redeclare function extends flowCharacteristic
  algorithm
  end flowCharacteristic;
end SplineFlow;
```

`thread 'main' has overflowed its stack`. Typecheck constant collection
(`constant_collection.rs::extract_class_constants_from_extends`) looked the
extends base up by its spelling in the scope of the redeclaring class, found
the redeclaring class itself, and recursed without end.

Affected (cluster G): ThermoPower `FanCharacteristics.Models.SplineFlow` —
1 model; now compiles.

## Fix

`rumoca-phase-typecheck/src/constant_collection.rs`: the extends walk uses the
base `DefId` Resolve bound to the clause (the inherited `F`, MLS §7.3.1) and
falls back to scope lookup only without one; each class is visited once, so a
cycle cannot recurse.

## Test

`crates/rumoca-contracts/tests/inst_contracts.rs::redeclare_function_extends_short_alias_does_not_recurse`.
