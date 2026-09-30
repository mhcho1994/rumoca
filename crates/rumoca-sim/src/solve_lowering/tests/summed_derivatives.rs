//! A matched derivative that is one affine term of a sum, or sits under
//! derivative-free products and quotients, is isolated rather than refused;
//! a state row, or a discrete owner, may read a state's derivative through its
//! definition.

use super::*;

fn final_value(source: &str, model: &str, name: &str) -> f64 {
    let dae = compile(source, model);
    let options = SimOptions {
        t_end: 1.0,
        dt: Some(0.01),
        ..SimOptions::default()
    };
    let result = simulate_dae(&dae, &options).expect("affine derivative row must execute");
    let column = result
        .names
        .iter()
        .position(|candidate| candidate == name)
        .expect("result column");
    *result.data[column].last().expect("trajectory sample")
}

#[test]
fn first_order_filter_written_as_a_sum_isolates_its_derivative() {
    // `y + T*der(y) = u`: y(1) = 1 - exp(-1/T) from y(0) = 0.
    let y = final_value(
        concat!(
            "model Filter\n",
            "  parameter Real T = 0.5;\n",
            "  Real y(start=0, fixed=true);\n",
            "equation\n",
            "  y + T*der(y) = 1;\n",
            "end Filter;\n",
        ),
        "Filter",
        "y",
    );
    let expected = 1.0 - (-2.0f64).exp();
    assert!(
        (y - expected).abs() < 1.0e-4,
        "y(1) = {y}, expected {expected}"
    );
}

#[test]
fn derivative_scaled_by_a_state_and_negated_in_a_difference_is_isolated() {
    // `2 - x*der(x)/4 = 0` is der(x) = 8/x, so x^2 = 1 + 16 t from x(0) = 1.
    let x = final_value(
        concat!(
            "model Scaled\n",
            "  Real x(start=1, fixed=true);\n",
            "equation\n",
            "  2 - x*der(x)/4 = 0;\n",
            "end Scaled;\n",
        ),
        "Scaled",
        "x",
    );
    let expected = 17.0f64.sqrt();
    assert!(
        (x - expected).abs() < 1.0e-4,
        "x(1) = {x}, expected {expected}"
    );
}

#[test]
fn state_row_reading_another_states_derivative_substitutes_its_definition() {
    // der(b) = der(a) + 1 with der(a) = 2: b(1) = 3 from b(0) = 0.
    let b = final_value(
        concat!(
            "model Chained\n",
            "  Real a(start=0, fixed=true);\n",
            "  Real b(start=0, fixed=true);\n",
            "equation\n",
            "  der(a) = 2;\n",
            "  der(b) = der(a) + 1;\n",
            "end Chained;\n",
        ),
        "Chained",
        "b",
    );
    assert!((b - 3.0).abs() < 1.0e-8, "b(1) = {b}, expected 3");
}

#[test]
fn boolean_equation_reads_a_state_derivative_through_its_definition() {
    // `rising = der(x) > 0` with der(x) = 0.5 - t turns false at t = 0.5.
    let y = final_value(
        concat!(
            "model Rising\n",
            "  Real x(start=0, fixed=true);\n",
            "  Boolean rising;\n",
            "  Real y;\n",
            "equation\n",
            "  der(x) = 0.5 - time;\n",
            "  rising = der(x) > 0;\n",
            "  y = if rising then 1 else -1;\n",
            "end Rising;\n",
        ),
        "Rising",
        "y",
    );
    assert_eq!(y, -1.0, "rising must be false after t = 0.5");
}
