# TOOLBUG-111 — `getInstanceName()` in a declaration or modifier binding

**Status:** fixed (this change).
**Severity:** medium — `[EF004] unsupported equation form: getInstanceName()
requires a model/block instance scope` for every model with
`parameter String insNam = getInstanceName()` (IBPSA/AixLib/IDEAS/Buildings
file writers and schedules, EnergyPlus coupling, DeviceDrivers
`TriggeredPrint`).

## What

```modelica
package GIN
  block Writer
    parameter String fileName = getInstanceName() + ".csv";
  end Writer;
  model Top
    Writer w1;
    Writer w2(fileName = getInstanceName() + ".txt");
  end Top;
end GIN;
```

MLS built-in `getInstanceName()` returns the name of the simulated model followed
by the instance path of the class instance the call is written in. Equations
and algorithms were lowered with that name; variable bindings were not:

1. `collect_component_binding_values` (structural constant pre-evaluation)
   qualified every parameter binding without an instance scope and propagated
   the lowering error, aborting flattening before the variable was reached.
2. `variables.rs` lowered declaration, modifier and attribute bindings with
   `instance_name: None`.

## Fix

- `pipeline/constant_injection/component_binding_values.rs`: a binding that
  cannot be lowered there is simply not a structural value (the variable's
  own flattening lowers it with its scope).
- `variables.rs`: `VariableImportContext` carries the simulated root name
  (threaded from `Context::simulated_root_name` through
  `process_component_instances_for_flatten`); a declaration or attribute
  binding uses the instance of the declaring class (`Top.w1`), a modifier
  binding the instance the modifier is written in (`Top`).

## Test

`frontend_event_lowering.rs::get_instance_name_in_bindings_names_the_enclosing_instance`.

## Known remaining

The file-writer blocks then reach a zero-output external call in a `when`
statement (`writeLine(filWri, str, 0)`), which has no event-call owner yet
(`function-call assignment must retain at least one output`).
