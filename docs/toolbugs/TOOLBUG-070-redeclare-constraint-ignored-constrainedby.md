# TOOLBUG-070 — redeclaration checked against the declared type, not `constrainedby`

**Status:** fixed (this change).
**Severity:** high — valid library models were rejected with EI027
("redeclared element must be a subtype of the constraining type").

## What

Two over-strict rules in the MLS §7.3.2 constraint check of an `extends`
redeclaration.

1. The constraint was taken from the component's resolved *declared* type
   whenever that type had a resolved identity, so an explicit
   `constrainedby` clause was ignored:

   ```modelica
   partial model PartialSolarCollector
     replaceable parameter Data.GenericASHRAE93 per
       constrainedby Data.BaseClasses.Generic;
   end PartialSolarCollector;
   model EN12975
     extends PartialSolarCollector(redeclare Data.GenericEN12975 per);
   end EN12975;
   ```

   `GenericEN12975` extends `Generic` but not `GenericASHRAE93`, so the
   check failed (`... GenericEN12975 is not a subtype of ... GenericASHRAE93`).

2. The TYPE-022 rule (a transitively non-replaceable constraint needs a
   transitively non-replaceable replacement) was applied even when the
   replacement nominally extends the constraint. The generalized electrical
   `connector Terminal extends BaseTerminal` adds a replaceable
   `PhaseSystem` package, so `redeclare Terminal terminal_n` over
   `replaceable BaseTerminal terminal_n` was rejected. The same shape
   rejected `redeclare RefrigerantCycle refCyc` over
   `replaceable PartialModularRefrigerantCycle refCyc` (IBPSA/IDEAS
   `Chillers.ModularReversible`). OpenModelica accepts both.

Affected cluster-A models (EI027 first error): Buildings/IBPSA/IDEAS/AixLib
`SolarCollectors.Validation.EN12975_*`, AixLib
`Electrical.DC.Sensors.GeneralizedSensor`, IBPSA/IDEAS
`Chillers.ModularReversible.Examples.CarnotWithLosses`.

## Fix

`crates/rumoca-phase-instantiate/src/inheritance.rs` (`validate_redeclaration`):
the constraint identity is the `constrainedby` clause's def id when the clause
exists, the declared type's otherwise. `is_type_subtype_cached` applies the
TYPE-022 replaceability rule (now `plug_compat::replaceability_compatible`)
only when acceptance was structural (sibling classes), not for a nominal
subclass.

## Test

`crates/rumoca-contracts/tests/inst_contracts.rs`:
`inst_043_explicit_constrainedby_overrides_declared_type_as_constraint`;
`crates/rumoca-contracts/tests/type_contracts.rs`:
`type_022_nominal_subclass_with_replaceable_member_accepted` (the existing
`type_022_replacement_with_replaceable_member_rejected` still rejects the
sibling case).
