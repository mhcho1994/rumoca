//! INST-043 contract tests (MLS §7.3.2): the constraint a redeclaration is
//! checked against, constraining-clause modifiers, and members looked up
//! through a selected package. Split from `inst_contracts.rs`.

use rumoca_compile::compile::FailedPhase;
use rumoca_contracts::test_support::{expect_failure_in_phase_with_code, expect_success};

#[test]
fn inst_043_original_type_is_used_as_implicit_constraint() {
    expect_success(
        r#"
        model BaseType
            Real x;
        equation
            x = 1;
        end BaseType;

        model DerivedType
            extends BaseType;
        end DerivedType;

        partial model Container
            replaceable BaseType c;
        end Container;

        model Test
            extends Container(redeclare DerivedType c);
        end Test;
    "#,
        "Test",
    );
}

#[test]
fn inst_043_redeclare_must_still_be_subtype_without_explicit_constrainedby() {
    expect_failure_in_phase_with_code(
        r#"
        model BaseType
            Real x;
        equation
            x = 1;
        end BaseType;

        model OtherType
            Integer y;
        equation
            y = 1;
        end OtherType;

        partial model Container
            replaceable BaseType c;
        end Container;

        model Test
            extends Container(redeclare OtherType c);
        end Test;
    "#,
        "Test",
        FailedPhase::Instantiate,
        "EI027",
    );
}

#[test]
fn inst_043_explicit_constrainedby_overrides_declared_type_as_constraint() {
    // MLS §7.3.2: with an explicit constraining clause the replacement only
    // has to be a subtype of the constraining type, not of the declared
    // default type (Buildings/IBPSA SolarCollectors `per` pattern).
    expect_success(
        r#"
        record Generic
            parameter Real A = 1;
        end Generic;

        record DataA
            extends Generic;
            parameter Real slope = 2;
        end DataA;

        record DataB
            extends Generic;
            parameter Real eta0 = 3;
        end DataB;

        partial model Container
            replaceable parameter DataA per constrainedby Generic;
            Real a;
        equation
            a = per.A;
        end Container;

        model Test
            extends Container(redeclare DataB per);
        end Test;
    "#,
        "Test",
    );
}

#[test]
fn inst_043_outer_extends_redeclare_replaces_transitively_inherited_component() {
    // `ThreeWayLinear extends PartialThreeWayValve(redeclare TwoWayLinear res1)`
    // where `res1` is declared two levels up and redeclared (to a partial
    // class) in between: the outermost redeclaration wins (MLS §7.2/§7.3), and
    // the constraining-clause modifier of the intermediate redeclaration
    // (`constrainedby PV(k = 2)`) carries over to the final element
    // (MLS §7.3.2).
    let trace = rumoca_contracts::test_support::simulate_model(
        r#"
        model M
            partial model PI
                Real x;
            end PI;
            partial model PV
                extends PI;
                parameter Real k = 1;
            end PV;
            model Lin
                extends PV;
            equation
                x = 10 * k;
            end Lin;
            partial model Res
                replaceable PI r constrainedby PI;
            end Res;
            partial model Valve
                extends Res(redeclare replaceable PV r constrainedby PV(k = 2));
            end Valve;
            model V3
                extends Valve(redeclare Lin r);
            end V3;
            V3 w;
            Real t(start = 0, fixed = true);
        equation
            der(t) = 1;
        end M;
    "#,
        "M",
        0.1,
    );
    assert_eq!(trace.final_value("w.r.x"), 20.0);
}

#[test]
fn inst_043_member_type_is_looked_up_in_the_selected_package() {
    // `Medium.fluidConstants[1].criticalPressure` (ThermoPower, TRANSFORM,
    // AixLib valves): `fluidConstants` is declared in the base package with
    // the replaceable record `FluidConstants`, which the two-phase package
    // redeclares in an extends modification. The member must be found in the
    // record as seen from the selected medium.
    let trace = rumoca_contracts::test_support::simulate_model(
        r#"
        package H
            package Types
                record Basic
                    Real molarMass;
                end Basic;
                record TwoPhase
                    extends Basic;
                    Real criticalPressure;
                end TwoPhase;
            end Types;
            partial package PartialMedium
                replaceable record FluidConstants = Types.Basic;
                constant FluidConstants[1] fluidConstants;
            end PartialMedium;
            partial package PartialTwoPhaseMedium
                extends PartialMedium(
                    redeclare replaceable record FluidConstants = Types.TwoPhase);
            end PartialTwoPhaseMedium;
            constant Types.TwoPhase waterConstants(molarMass = 0.018, criticalPressure = 22.064e6);
            package Water
                extends PartialTwoPhaseMedium(fluidConstants = {waterConstants});
            end Water;
            model Valve
                replaceable package Medium = PartialTwoPhaseMedium;
                Real x;
            equation
                x = Medium.fluidConstants[1].criticalPressure;
            end Valve;
            model Top
                Valve v(redeclare package Medium = Water);
                Real t(start = 0, fixed = true);
            equation
                der(t) = 1;
            end Top;
        end H;
    "#,
        "H.Top",
        0.1,
    );
    assert_eq!(trace.final_value("v.x"), 22.064e6);
}

#[test]
fn inst_043_forwarded_package_redeclare_in_derived_model_uses_inherited_package() {
    // `package Medium = Air` declared in a base model and forwarded by
    // `sou(redeclare package Medium = Medium)`: instantiating a *derived*
    // model must forward the inherited package, not leave the partial
    // default (Buildings/IBPSA/AixLib `MixingVolumes.Validation`).
    let trace = rumoca_contracts::test_support::simulate_model(
        r#"
        package R
            partial package PM
                replaceable partial model BaseProperties
                    Real p;
                end BaseProperties;
            end PM;
            package Air
                extends PM;
                redeclare model extends BaseProperties
                equation
                    p = 1;
                end BaseProperties;
            end Air;
            model Source
                replaceable package Medium = PM;
                Medium.BaseProperties medium;
            end Source;
            partial model Base
                package Medium = Air;
                Source sou(redeclare package Medium = Medium);
                Real t(start = 0, fixed = true);
            equation
                der(t) = 1;
            end Base;
            model Derived
                extends Base;
            end Derived;
        end R;
    "#,
        "R.Derived",
        0.1,
    );
    assert_eq!(trace.final_value("sou.medium.p"), 1.0);
}

#[test]
fn inst_043_state_select_modifier_may_name_members_of_the_modified_instance() {
    // `medium(Xi(each stateSelect = if medium.preferredMediumStates then
    // ...))` (IBPSA ConservationEquation): the modifier is evaluated in the
    // scope where it is written, where `medium.flag` names the member `flag`
    // of the instance being built.
    expect_success(
        r#"
        model Mixer
            model B
                parameter Boolean flag = false;
                Real p;
            end B;
            B medium(p(stateSelect = if medium.flag then StateSelect.prefer else StateSelect.default));
        equation
            medium.p = time;
        end Mixer;
    "#,
        "Mixer",
    );
}

#[test]
fn inst_043_constraining_clause_modifiers_apply_to_the_declaration() {
    // MLS §7.3.2: constraining-clause modifiers are applied in the
    // declaration itself, below the declaration's own modifiers
    // (IBPSA `replaceable MixingVolume volDyn constrainedby
    // MixingVolume(redeclare package Medium = Medium, V = 1)`).
    let trace = rumoca_contracts::test_support::simulate_model(
        r#"
        package C
            partial package PM
                replaceable partial model BaseProperties
                    Real p;
                end BaseProperties;
            end PM;
            package Air
                extends PM;
                redeclare model extends BaseProperties
                equation
                    p = 1;
                end BaseProperties;
            end Air;
            model Vol
                replaceable package Medium = PM;
                parameter Real V = 3;
                Medium.BaseProperties medium;
                Real y = V;
            end Vol;
            model Top
                package Medium = Air;
                replaceable Vol vol constrainedby Vol(redeclare package Medium = Medium, V = 2);
                replaceable Vol vol2(V = 5) constrainedby Vol(redeclare package Medium = Medium, V = 2);
                Real t(start = 0, fixed = true);
            equation
                der(t) = 1;
            end Top;
        end C;
    "#,
        "C.Top",
        0.1,
    );
    assert_eq!(trace.final_value("vol.medium.p"), 1.0);
    assert_eq!(trace.final_value("vol.y"), 2.0);
    assert_eq!(trace.final_value("vol2.y"), 5.0);
}
