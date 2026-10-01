# TOOLBUG-148 — inherited package constant takes one value for every extending package

**Status:** fixed.
**Severity:** high (silent wrong value, caught only by a later shape check) —
`Medium.nXi`, `Medium.nX`, ... of a medium that modifies `substanceNames` or
`reducedX` more than one extends level below `PartialMedium` were substituted
with another package's value. In the cluster this showed up as
`EF004 for-equation range end must be a constant integer or parameter (scope
..., got Medium.nXi)` on `{... for i in 1:Medium.nXi}` (IBPSA/Buildings/IDEAS/
AixLib `ConservationEquation.s`), and, once that range was deferred, as a
binding with the wrong number of elements.

## What

```modelica
partial package PM
  constant String names[:] = {"a"};
  constant Boolean red = false;
  final constant Integer nS = size(names, 1);
  final constant Integer n = if red then nS - 1 else nS;
end PM;
partial package PM2 extends PM(red = true); end PM2;
package W extends PM2(names = {"x", "y", "z"}); end W;
model ByAlias
  package Medium = W;
  parameter Real s[Medium.n] = {i for i in 1:Medium.n};   // n = 2
end ByAlias;
```

1. Array-comprehension bindings are expanded before package constants are
   injected, and an unevaluable range was an error instead of leaving the
   comprehension structured for later lowering.
2. Constant substitution resolves a source reference by its declaration
   identity first. `constant_values_by_def_id` held one value per declaration
   (last writer wins: here `PM2.n = size({"a"},1) - 1 = 0`), although an
   inherited package constant has a value per extending package (MLS §7.2).

Affected in cluster R2-media: the 15 `1:Medium.nXi` EF004 models. They now
reach `inStream() cannot resolve indexed stream reference` (connection
handling, outside this cluster) or EF015.

## Fix

`crates/rumoca-phase-flatten/src/`:
- `array_comprehension.rs`: a range that is not yet evaluable keeps the
  comprehension structured (`try_expand_index_ranges` already documented this).
- `constant_extraction.rs` / `pipeline/constant_injection.rs`:
  `record_constant_value_by_def_id` remembers declarations recorded with
  differing values, and an extends modification marks the modified declaration
  (`Context::ambiguous_constant_def_ids`).
- `postprocess/constant_substituter.rs`: for such a declaration reached through
  a package other than its declaring class (`Medium.n`, not `PM.n` written
  explicitly), the value recorded for the occurrence's package path
  (`c.Medium.n`, then `Medium.n`) is used.

## Test

`crates/rumoca-contracts/tests/func_contracts.rs::inherited_package_constant_takes_the_extending_package_value`
(package alias and component redeclaration; simulates).
