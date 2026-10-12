use rumoca_contracts::test_support::expect_success;

#[test]
fn inst_032_disabled_conditional_outer_needs_no_inner() {
    expect_success(
        r#"
        model M
            connector C
                Real e;
                flow Real f;
            end C;
            model Sub
                parameter Boolean has = false;
                outer C shared if has;
                Real y = 1;
            end Sub;
            Sub s;
        end M;
    "#,
        "M",
    );
}

// =============================================================================
// INST-038: Modifications of elements declared with both inner and outer only
// applied to inner declaration
// =============================================================================

#[test]
fn inst_038_inner_outer_modification_accepted() {
    expect_success(
        r#"
        model M
            model Sub
                inner outer Real shared = 5;
                Real y = shared;
            end Sub;
            Sub s;
            inner Real shared = 1;
        end M;
    "#,
        "M",
    );
}

// =============================================================================
// `redeclare function extends F` of a short-class alias (TOOLBUG-133)
// =============================================================================

/// ThermoPower `FanCharacteristics.Models.SplineFlow`: the redeclared class
/// extends the inherited `flowCharacteristic`, not itself. Constant collection
/// looked the base up by spelling, found the redeclaring class and recursed
/// until the stack overflowed.
#[test]
fn redeclare_function_extends_short_alias_does_not_recurse() {
    expect_success(
        r#"
        package P
            partial function baseFlow
                input Real q;
                output Real H;
            end baseFlow;
            function dummyFlow
                extends baseFlow;
            algorithm
                H := q;
            end dummyFlow;
            model BaseFlow
                replaceable function flowCharacteristic = P.dummyFlow
                    constrainedby P.baseFlow;
                Real y = flowCharacteristic(time);
            end BaseFlow;
            model SplineFlow
                extends BaseFlow;
                redeclare function extends flowCharacteristic
                algorithm
                    H := 2 * q;
                end flowCharacteristic;
            end SplineFlow;
        end P;
    "#,
        "P.SplineFlow",
    );
}
