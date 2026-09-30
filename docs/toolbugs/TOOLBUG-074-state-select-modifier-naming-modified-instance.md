# TOOLBUG-074 — `stateSelect` modifier naming a member of the modified instance rejected

**Status:** fixed (this change).
**Severity:** high — EI032 ("invalid value for type attribute `stateSelect`")
on every Buildings/IBPSA/IDEAS/AixLib model with a mixing volume or
conservation equation.

## What

```modelica
model Mixer
  model B
    parameter Boolean flag = false;
    Real p;
  end B;
  B medium(p(stateSelect = if medium.flag then StateSelect.prefer else StateSelect.default));
end Mixer;
```

`IBPSA.Fluid.Interfaces.ConservationEquation` writes
`Medium.BaseProperties medium(Xi(each stateSelect = if medium.preferredMediumStates then ...))`.
`stateSelect` must be evaluated while instantiating `medium.p`. The modifier's
source scope is `Mixer`, where `medium.flag` names the member `flag` of the
instance being built, but that instance's parameters are only in the
effective components of `B`; the source-scope lookup found nothing and the
attribute was rejected.

Affected cluster-A models: all 12 `start=X_start[1:Medium.nXi]`-labelled
EI032 cases (Buildings/IBPSA/IDEAS/AixLib `MixingVolumes`, `Movers`,
`HeatPumps`, `Airflow.Multizone` examples) and models whose next error it
was (e.g. `SolarCollectors.Validation.EN12975_*`). ThermoPower
`Water.Mixer`/`TestMixer*` and TRANSFORM `GenericPipe` evaluate
`Medium.singleState` of a partial medium, which has no value; those are not
fixed here.

## Fix

`crates/rumoca-phase-instantiate/src/attributes.rs`: when evaluation in the
source scope fails, `reanchor_to_owner_instance` rewrites references written
in the source scope that denote members of the instance being built
(`source scope + reference` starts with that instance's path) into local
references and evaluates again against its effective components. The
instance path reaches `extract_attributes` through the new `AttributeScope`.

## Test

`crates/rumoca-contracts/tests/inst_contracts.rs`:
`inst_043_state_select_modifier_may_name_members_of_the_modified_instance`.
