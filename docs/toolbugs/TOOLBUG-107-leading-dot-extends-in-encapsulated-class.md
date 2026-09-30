# TOOLBUG-107 — `extends .A.B` inside an encapsulated class: ER003

**Status:** fixed.
**Severity:** medium — `ER003 unresolved extends base class:
Modelica.Icons.Function` / `Modelica.Icons.Package` on the
Modelica_DeviceDrivers AVR examples (3 cases in cluster D).

## What

```modelica
encapsulated package Digital
  extends .Modelica.Icons.Package;
end Digital;
```

MLS §5.3.3: a name with a leading dot is looked up in the global scope. The
parser kept the leading dot of a type specifier only for partial function
applications; for `extends` clauses (and short class definitions) it was
dropped, so `Modelica` was looked up lexically and the encapsulated boundary
correctly hid it.

## Fix

- `rumoca-ir-ast` `Extend`: new `global_scope` flag (serde default `false`),
  printed back as `extends .A.B` by `to_modelica`.
- `rumoca-phase-parse`: set from the type specifier's leading dot for
  `extends` clauses, short class definitions, partial-application and
  `der(...)` short forms.
- `rumoca-phase-resolve` (`resolve_extends`): a global base name is looked up
  from the global scope.

## Test

`crates/rumoca/tests/suite_core/frontend_functions_names.rs`:
`leading_dot_extends_crosses_an_encapsulated_boundary`.
