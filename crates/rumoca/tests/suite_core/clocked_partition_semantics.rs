//! MLS §16 clocked-partition semantics exercised by the `Modelica.Clocked`
//! library: inferred sub-sampling factors, left-limit sampling, vector and
//! multi-output clocked definitions, parameter-selected clocks, and clock
//! inference through conversions.

use rumoca::Compiler;
use rumoca_sim::{SimOptions, SimResult, simulate_dae};

fn simulate(source: &str, model: &str, t_end: f64, dt: f64) -> SimResult {
    let compiled = Compiler::new()
        .model(model)
        .compile_str(source, &format!("{model}.mo"))
        .unwrap_or_else(|error| panic!("{model} compiles: {error:?}"));
    simulate_dae(
        &compiled.dae,
        &SimOptions {
            t_end,
            dt: Some(dt),
            ..Default::default()
        },
    )
    .map(|result| {
        assert!(result.times.len() > 10, "{model} produced an output grid");
        result
    })
    .unwrap_or_else(|error| panic!("{model} simulates: {error:?}"))
}

fn column<'result>(result: &'result SimResult, name: &str) -> &'result [f64] {
    let index = result
        .names
        .iter()
        .position(|candidate| candidate == name)
        .unwrap_or_else(|| panic!("result has no column `{name}`"));
    &result.data[index]
}

/// The value `name` holds at the last output instant not after `time`.
fn value_at(result: &SimResult, name: &str, time: f64) -> f64 {
    let index = result
        .times
        .iter()
        .rposition(|candidate| *candidate <= time + 1.0e-12)
        .expect("an output instant precedes the probe");
    column(result, name)[index]
}

/// MLS §16.5.2: `superSample(u)` without `factor` infers the factor from the
/// source and target partitions. One block class instantiated twice owns one
/// source span but two partitions, and each instance keeps its own factor.
#[test]
fn inferred_super_sample_factor_follows_each_instance_partition() {
    let source = r#"
model InferredFactor
  block Up
    input Real u;
    output Real y;
  protected
    Real dummy;
  equation
    when Clock() then
      dummy = u;
    end when;
    when Clock() then
      y = superSample(u);
    end when;
  end Up;
  block UpBy
    parameter Integer f = 3;
    input Real u;
    output Real y;
  equation
    when Clock() then
      y = superSample(u, f);
    end when;
  end UpBy;
  Clock c = Clock(1, 10);
  Real x;
  Up up1;
  Up up2;
  UpBy by3;
  UpBy by2(f = 2);
  Real s3;
  Real s2;
  Real h3;
  Real h2;
equation
  x = sample(time, c);
  up1.u = x;
  up2.u = x;
  by3.u = x;
  by2.u = x;
  s3 = up1.y + by3.y;
  s2 = up2.y + by2.y;
  h3 = hold(s3);
  h2 = hold(s2);
end InferredFactor;
"#;
    let result = simulate(source, "InferredFactor", 0.35, 0.005);
    for time in [0.05, 0.15, 0.25] {
        let base = (time * 10.0_f64).floor() / 10.0;
        for name in ["h3", "h2"] {
            let actual = value_at(&result, name, time);
            assert!(
                (actual - 2.0 * base).abs() < 1.0e-12,
                "{name} at {time}: expected {}, got {actual}",
                2.0 * base
            );
        }
    }
}

/// MLS §16.5.1: `sample(u)` is the left limit of `u` at the tick, so a clocked
/// value that feeds back through `hold` reads the value held before the tick
/// and forms no same-instant loop.
#[test]
fn sampling_a_discrete_value_reads_its_left_limit() {
    let source = r#"
model LeftLimit
  Clock c = Clock(1, 10);
  Integer y(start = 0);
  Integer s;
  Integer h;
equation
  h = hold(y);
  s = sample(h, c);
  y = s + 1;
end LeftLimit;
"#;
    let result = simulate(source, "LeftLimit", 0.35, 0.01);
    for (time, expected) in [(0.05, 1.0), (0.15, 2.0), (0.25, 3.0), (0.35, 4.0)] {
        let actual = value_at(&result, "h", time);
        assert_eq!(actual, expected, "h at {time}");
    }
}

/// MLS §8.5: equations active at one event instant are solved together, so an
/// unclocked `when` body that reads a variable it defines sees the new value.
#[test]
fn simultaneous_when_equations_read_the_instant_value() {
    let source = r#"
model Simultaneous
  discrete Real a(start = 0, fixed = true);
  discrete Real b(start = 0, fixed = true);
equation
  when {time >= 0.05, initial()} then
    a = 3 + time;
    b = a;
  end when;
end Simultaneous;
"#;
    let result = simulate(source, "Simultaneous", 0.1, 0.01);
    for time in [0.0, 0.02, 0.06, 0.09] {
        assert_eq!(
            value_at(&result, "a", time),
            value_at(&result, "b", time),
            "b must equal a at {time}"
        );
    }
    assert!((value_at(&result, "b", 0.09) - 3.05).abs() < 1.0e-12);
}

/// MLS §8.3.4: the rows of an if-equation are sets; branches that assign the
/// same variables in different orders are paired by variable.
#[test]
fn if_equation_rows_pair_by_assigned_variable() {
    let source = r#"
model TickPulse
  Clock c = Clock(1, 10);
  Integer counter(start = 0);
  Boolean started(start = false);
  Real h;
equation
  when c then
    if previous(started) then
      counter = if previous(counter) == 2 then 0 else previous(counter) + 1;
      started = previous(started);
    else
      started = previous(counter) >= 1;
      counter = if started then 0 else previous(counter) + 1;
    end if;
  end when;
  h = hold(counter);
end TickPulse;
"#;
    let result = simulate(source, "TickPulse", 0.55, 0.01);
    let expected = [1.0, 0.0, 1.0, 2.0, 0.0, 1.0];
    for (tick, expected) in expected.iter().enumerate() {
        let time = tick as f64 / 10.0 + 0.05;
        assert_eq!(value_at(&result, "h", time), *expected, "h at {time}");
    }
}

/// MLS §10.4.1: a range bound written over a settled parameter proves the same
/// compact range inside a clocked `when` body as in a plain equation.
#[test]
fn clocked_body_range_over_parameter_is_settled() {
    let source = r#"
model Fir
  parameter Integer n = 2;
  Clock c = Clock(1, 10);
  Real u;
  Real buffer[n + 1](each start = 0);
  Real y;
  Real h;
equation
  u = sample(time, c);
  when c then
    buffer = cat(1, {u}, previous(buffer[1:n]));
    y = sum(buffer);
  end when;
  h = hold(y);
end Fir;
"#;
    let result = simulate(source, "Fir", 0.35, 0.01);
    let actual = value_at(&result, "h", 0.35);
    assert!((actual - (0.3 + 0.2 + 0.1)).abs() < 1.0e-12, "h = {actual}");
}

/// MLS §16.5.1 samples a vector element-wise; each element is a clocked
/// discrete Real definition, not a continuous residual.
#[test]
fn vector_sample_defines_clocked_discrete_reals() {
    let source = r#"
model VectorSample
  Clock c = Clock(1, 10);
  Real u[2];
  Real y[2];
equation
  u = {time, 2 * time};
  y = sample(u, c);
end VectorSample;
"#;
    let result = simulate(source, "VectorSample", 0.25, 0.01);
    assert!((value_at(&result, "y[1]", 0.25) - 0.2).abs() < 1.0e-12);
    assert!((value_at(&result, "y[2]", 0.25) - 0.4).abs() < 1.0e-12);
}

/// MLS §12.4.3: a multi-result call equation defines discrete receivers the
/// same way it defines continuous ones.
#[test]
fn multi_output_call_defines_clocked_discrete_receivers() {
    let source = r#"
model MultiOutput
  function split
    input Real s;
    output Real x;
    output Integer n;
  algorithm
    x := 2 * s;
    n := integer(s);
  end split;
  Clock c = Clock(1, 2);
  Real u;
  Real x;
  Integer n;
  Real hx;
  Real hn;
equation
  u = sample(3 * time, c);
  (x, n) = split(u);
  hx = hold(x);
  hn = hold(n);
end MultiOutput;
"#;
    let result = simulate(source, "MultiOutput", 1.2, 0.05);
    assert!((value_at(&result, "hx", 1.1) - 6.0).abs() < 1.0e-12);
    assert_eq!(value_at(&result, "hn", 1.1), 3.0);
}

/// MLS §16.7: a Clock defined by a parameter-selected expression takes the
/// clock the parameter values select.
#[test]
fn parameter_selected_clock_expression_has_a_static_schedule() {
    let source = r#"
model SelectedClock
  parameter Boolean fast = false;
  Clock c;
  Real y;
  Real h;
equation
  if fast then
    c = Clock(1, 100);
  else
    c = subSample(Clock(1, 100), 10);
  end if;
  y = sample(time, c);
  h = hold(y);
end SelectedClock;
"#;
    let result = simulate(source, "SelectedClock", 0.25, 0.01);
    assert!((value_at(&result, "h", 0.25) - 0.2).abs() < 1.0e-12);
}

/// MLS §16.5.1 infers the clock of a `sample(u)` partition from the clock
/// relations it takes part in before any fallback to a unique model clock.
#[test]
fn sample_clock_is_inferred_through_a_super_sample_relation() {
    let source = r#"
model InferThroughConversion
  Clock fast = Clock(1, 50);
  Real slow;
  Real fastValue;
  Real reference;
  Real mixed;
  Real held;
equation
  slow = sample(time);
  fastValue = superSample(slow, 5);
  reference = sample(time, fast);
  mixed = fastValue + 0 * reference;
  held = hold(mixed);
end InferThroughConversion;
"#;
    let result = simulate(source, "InferThroughConversion", 0.25, 0.01);
    assert!((value_at(&result, "held", 0.25) - 0.2).abs() < 1.0e-12);
    assert!((value_at(&result, "held", 0.19) - 0.1).abs() < 1.0e-12);
}
