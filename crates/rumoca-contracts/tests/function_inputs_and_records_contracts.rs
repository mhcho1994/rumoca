use rumoca_compile::compile::FailedPhase;
use rumoca_contracts::test_support::expect_failure_in_phase_with_code;

// =============================================================================
// FUNC-039: Component bindings read inputs (MLS §12.4.4)
// =============================================================================

#[test]
fn func_039_protected_binding_reads_record_input_fields() {
    let trace = rumoca_contracts::test_support::simulate_model(
        r#"
        package F1
          record Data
            Real diameter_a;
            Real diameter_b;
            Boolean zeta1_at_a = true;
            Real zeta1;
          end Data;
          function k
            input Real D;
            input Real zeta;
            output Real y;
          algorithm
            y := zeta/D^2;
          end k;
          function loss
            input Real m;
            input Data data;
            output Real dp;
          protected
            Real k1 = k(if data.zeta1_at_a then data.diameter_a else data.diameter_b, data.zeta1);
          algorithm
            dp := k1*m;
          end loss;
          model Top
            parameter Data data(diameter_a = 0.1, diameter_b = 0.2, zeta1 = 1);
            Real dp = loss(1 + time, data);
          end Top;
        end F1;
    "#,
        "F1.Top",
        1.0,
    );
    assert!((trace.final_value("dp") - 200.0).abs() < 1e-9);
}

#[test]
fn func_027_vectorized_call_evaluates_at_translation() {
    // MLS §12.4.6: a scalar function applied to an array argument maps over
    // its elements, also when a parameter binding evaluates it at translation.
    let trace = rumoca_contracts::test_support::simulate_model(
        r#"
        model M
            function toK
                input Real c;
                output Real k;
            algorithm
                k := c + 273.15;
            end toK;
            function f
                input Real T;
                output Real y;
            protected
                Real invTK[2] = 1 ./ toK({10, 20});
            algorithm
                y := invTK[1]*T + invTK[2];
            end f;
            parameter Real p = f(300);
            Real y = p*time;
        end M;
    "#,
        "M",
        1.0,
    );
    let expected = 300.0 / 283.15 + 1.0 / 293.15;
    assert!((trace.final_value("y") - expected).abs() < 1e-12);
}

// =============================================================================
// FUNC-024: Uninitialized error (MLS §12.4.4)
// =============================================================================

#[test]
fn func_024_uninitialized_record_result_field_rejected() {
    expect_failure_in_phase_with_code(
        r#"
        package P
            record Data
                Real d;
                Real e;
                Real c0 = 1;
            end Data;
            function make
                input Real d;
                output Data data;
            algorithm
                data.d := d;
            end make;
            model M
                Data r = make(time);
            end M;
        end P;
    "#,
        "P.M",
        FailedPhase::ToDae,
        "ED022",
    );
}

// =============================================================================
// FUNC-040: Function arguments (MLS §12.4.2.1)
// =============================================================================

#[test]
fn func_040_function_name_and_partial_application_arguments() {
    let trace = rumoca_contracts::test_support::simulate_model(
        r#"
        package P
            partial function Equation
                input Real u;
                output Real y;
            end Equation;
            function apply
                input Equation f;
                input Real x;
                output Real y;
            algorithm
                y := f(x);
            end apply;
            function square
                extends Equation;
            algorithm
                y := u*u;
            end square;
            function affine
                extends Equation;
                input Real a;
                input Real b;
            algorithm
                y := a*u + b;
            end affine;
            model M
                Real s = apply(square, 1 + time);
                Real a = apply(function affine(a = 2, b = time), 3);
            end M;
        end P;
    "#,
        "P.M",
        1.0,
    );
    assert!((trace.final_value("s") - 4.0).abs() < 1e-12);
    assert!((trace.final_value("a") - 7.0).abs() < 1e-12);
}

// =============================================================================
// FUNC-043: Recursive functions (MLS §12.2)
// =============================================================================

const FUNC_043_SOURCE: &str = r#"
    package R
        function fib
            input Integer n;
            output Real y;
        algorithm
            y := if n < 2 then n else fib(n - 1) + fib(n - 2);
        end fib;
        function isEven
            input Integer n;
            output Boolean even;
        algorithm
            even := if n == 0 then true else isOdd(n - 1);
        end isEven;
        function isOdd
            input Integer n;
            output Boolean odd;
        algorithm
            odd := if n == 0 then false else isEven(n - 1);
        end isOdd;
        model M
            parameter Integer n = 10;
            parameter Real f = fib(n);
            parameter Boolean even = isEven(n);
            Real x(start = 0, fixed = true);
        equation
            der(x) = if even then f else -f;
        end M;
    end R;
"#;

#[test]
fn func_043_recursive_and_mutually_recursive_calls() {
    let trace = rumoca_contracts::test_support::simulate_model(FUNC_043_SOURCE, "R.M", 1.0);
    // fib(10) = 55 and 10 is even.
    assert!((trace.final_value("x") - 55.0).abs() < 1e-9);
}

#[test]
fn func_043_recursion_beyond_the_profile_depth_limit_fails() {
    let source = FUNC_043_SOURCE
        .replace("parameter Integer n = 10;", "parameter Integer n = 100;")
        .replace("parameter Real f = fib(n);", "parameter Real f = 1;");
    let error = rumoca_contracts::test_support::simulate_model_failure(&source, "R.M", 1.0);
    assert!(error.contains("depth limit"), "{error}");
}

// =============================================================================
// FUNC-026: Vectorization non-replaceable (MLS §12.4.6, §6.3.1, §7.3)
// =============================================================================

const FUNC_026_SOURCE: &str = r#"
    package V
        partial package Base
            replaceable function prop
                input Real T;
                output Real y;
            algorithm
                y := T;
            end prop;
        end Base;
        package A
            extends Base;
            redeclare function prop
                input Real T;
                output Real y;
            algorithm
                y := 2*T + 1;
            end prop;
        end A;
        partial package Abstract
            replaceable partial function prop
                input Real T;
                output Real y;
            end prop;
        end Abstract;
        model Selected
            replaceable package Medium = A;
            Real y[2] = Medium.prop({1, 2}*time);
        end Selected;
        model Unselected
            replaceable package Medium = Abstract;
            Real y[2] = Medium.prop({1, 2}*time);
        end Unselected;
    end V;
"#;

#[test]
fn func_026_vectorized_call_through_selected_replaceable_package_accepted() {
    let trace = rumoca_contracts::test_support::simulate_model(FUNC_026_SOURCE, "V.Selected", 1.0);
    assert!((trace.final_value("y[1]") - 3.0).abs() < 1e-12);
    assert!((trace.final_value("y[2]") - 5.0).abs() < 1e-12);
}

#[test]
fn func_026_vectorized_call_of_unselected_callee_rejected() {
    expect_failure_in_phase_with_code(FUNC_026_SOURCE, "V.Unselected", FailedPhase::ToDae, "ED008");
}

// =============================================================================
// FUNC-041: Element definedness through comprehensions (MLS §12.4.4, §10.4.2)
// =============================================================================

#[test]
fn func_041_comprehension_reads_of_defined_columns_accepted() {
    let trace = rumoca_contracts::test_support::simulate_model(
        r#"
        model ColumnFill
            function vandermondeSum
                input Real u[:];
                input Integer n;
                output Real s;
            protected
                Real V[size(u, 1), n + 1];
            algorithm
                V[:, n + 1] := ones(size(u, 1));
                for j in n:-1:1 loop
                    V[:, j] := {u[i] * V[i, j + 1] for i in 1:size(u, 1)};
                end for;
                s := sum(V);
            end vandermondeSum;
            Real x = time + 2;
            Real y = vandermondeSum({x, 2 * x}, 2);
        end ColumnFill;
    "#,
        "ColumnFill",
        1.0,
    );
    // Rows [u^2, u, 1] for u = 3 and u = 6.
    assert!((trace.final_value("y") - 56.0).abs() < 1e-9);
}

// =============================================================================
// FUNC-042: Text in pure-call interfaces (MLS §4.9.4, §12.4)
// =============================================================================

#[test]
fn func_042_record_with_text_field_passed_to_calls() {
    let trace = rumoca_contracts::test_support::simulate_model(
        r#"
        package NamedData
            record DataRecord
                String name;
                Real R_s;
                Real a[2];
            end DataRecord;
            constant DataRecord H2O(name = "H2O", R_s = 461.5, a = {1, 2});
            function cp_T
                input DataRecord d;
                input Real T;
                output Real cp;
            algorithm
                cp := d.R_s * (d.a[1] + d.a[2] * T);
            end cp_T;
            function cp
                input Real T;
                output Real y;
            algorithm
                y := cp_T(H2O, T);
            end cp;
            model M
                Real T = 1 + time;
                Real nested = cp(T);
                Real direct = cp_T(H2O, T);
            end M;
        end NamedData;
    "#,
        "NamedData.M",
        1.0,
    );
    assert!((trace.final_value("nested") - 461.5 * 5.0).abs() < 1e-9);
    assert!((trace.final_value("direct") - 461.5 * 5.0).abs() < 1e-9);
}

// =============================================================================
// FUNC-045: Nested record field assignment (MLS §11.2.1, §12.4.4)
// =============================================================================

#[test]
fn func_045_nested_record_field_paths_assemble() {
    let trace = rumoca_contracts::test_support::simulate_model(
        r#"
        model NestedFields
            record Identity
                Integer sequence;
                Real weight;
            end Identity;
            record State
                Identity identity;
                Real time;
            end State;
            function seed
                input Real w;
                output State state;
            algorithm
                state.identity.sequence := 7;
                state.time := 2 * w;
                state.identity.weight := w;
            end seed;
            function observe
                input Real w;
                output Real y;
            protected
                State s;
            algorithm
                s := seed(w);
                y := s.identity.sequence + s.identity.weight + s.time;
            end observe;
            Real y = observe(time);
        end NestedFields;
    "#,
        "NestedFields",
        1.0,
    );
    // At t = 1: 7 + 1 + 2.
    assert!((trace.final_value("y") - 10.0).abs() < 1e-9);
}
