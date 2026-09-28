//! MLS §16.3 event clocks `Clock(condition, startInterval)`: a clocked
//! partition that ticks on the rising edge of a Boolean coordinate.

use rumoca::Compiler;
use rumoca_sim::{SimOptions, SimResult, simulate_dae};

fn simulate(source: &str, model: &str, t_end: f64) -> SimResult {
    let compiled = match Compiler::new()
        .model(model)
        .compile_str(source, &format!("{model}.mo"))
    {
        Ok(compiled) => compiled,
        Err(error) => panic!("{model} compiles: {error:?}"),
    };
    let options = SimOptions {
        t_end,
        dt: Some(0.01),
        ..Default::default()
    };
    let result = match simulate_dae(&compiled.dae, &options) {
        Ok(result) => result,
        Err(error) => panic!("{model} simulates: {error:?}"),
    };
    assert!(result.times.len() > 10, "{model} produced an output grid");
    result
}

fn refusal(source: &str, model: &str) -> String {
    match Compiler::new()
        .model(model)
        .compile_str(source, &format!("{model}.mo"))
    {
        Ok(_) => panic!("{model} is refused"),
        Err(error) => format!("{error:?}"),
    }
}

/// The value `name` holds at the last output instant not after `time`.
fn value_at(result: &SimResult, name: &str, time: f64) -> f64 {
    let Some(column) = result.names.iter().position(|candidate| candidate == name) else {
        panic!("result has no column `{name}`");
    };
    let row = result
        .times
        .iter()
        .rposition(|candidate| *candidate <= time + 1.0e-12)
        .expect("an output instant precedes the probe");
    result.data[column][row]
}

/// The clock ticks once per period at the rising edge of `b` (t = 1/12 + k),
/// never at its falling edge. `previous`, `sample`, and `interval()` read the
/// partition's own ticks; `interval()` is the default `startInterval` 0 at the
/// first tick (MLS §16.10 Operator 16.15).
#[test]
fn an_event_clock_ticks_on_the_rising_edge_of_its_condition() {
    let source = r#"
model RisingEdge
  Real s(start = 0, fixed = true);
  Boolean b = sin(2 * 3.141592653589793 * time) > 0.5;
  Clock c = Clock(b);
  Integer n(start = 0);
  Real y(start = 0);
  Real dt(start = 0);
  Real held;
equation
  der(s) = 1;
  when c then
    n = previous(n) + 1;
    y = sample(s);
    dt = interval();
  end when;
  held = hold(y);
end RisingEdge;
"#;
    let result = simulate(source, "RisingEdge", 3.0);
    let first = 1.0 / 12.0;
    for (tick, time) in [(0.0, 0.05), (1.0, 0.5), (1.0, 1.05), (2.0, 1.5), (3.0, 2.9)] {
        assert_eq!(value_at(&result, "n", time), tick, "n at {time}");
    }
    assert!((value_at(&result, "y", 0.5) - first).abs() < 1.0e-6);
    assert!((value_at(&result, "y", 1.5) - (first + 1.0)).abs() < 1.0e-6);
    assert_eq!(value_at(&result, "dt", 0.5), 0.0);
    assert!((value_at(&result, "dt", 1.5) - 1.0).abs() < 1.0e-6);
}

/// MLS §16.3: the event clock and `sample` both delay by one event
/// iteration, so the first tick of `Clock(b)` is at 0.5 and samples `b` true.
#[test]
fn the_first_tick_samples_its_own_condition_true() {
    let source = r#"
model SampledCondition
  Boolean b = time >= 0.5;
  Clock c = Clock(b);
  Boolean b2 = sample(b, c);
  Real v;
  Real held;
  Real s(start = 0, fixed = true);
equation
  der(s) = 1;
  v = if b2 then 1.0 else -1.0;
  held = hold(v);
end SampledCondition;
"#;
    let result = simulate(source, "SampledCondition", 1.0);
    assert_eq!(value_at(&result, "held", 0.4), -1.0);
    assert_eq!(value_at(&result, "held", 0.5), 1.0);
    assert_eq!(value_at(&result, "b2", 0.9), 1.0);
}

/// An event clock has no periodic lattice, so an exact clock conversion of it
/// is refused, and its condition must name a Boolean coordinate.
#[test]
fn unsupported_event_clock_forms_are_refused() {
    let conversion = r#"
model Conversion
  Boolean b = time >= 0.5;
  Clock c = subSample(Clock(b), 2);
  Real y = sample(time, c);
end Conversion;
"#;
    let message = refusal(conversion, "Conversion");
    assert!(
        message.contains("event clock has no periodic lattice"),
        "{message}"
    );
    let inline = r#"
model Inline
  Clock c = Clock(time >= 0.5);
  Real y = sample(time, c);
end Inline;
"#;
    let message = refusal(inline, "Inline");
    assert!(
        message.contains("event clock condition must name a scalar Boolean variable"),
        "{message}"
    );
}

/// `firstTick()` and a conditional in an event-clock partition, as in the MSL
/// `IntegerChange` block: `changed` is false at the first tick and true
/// whenever the sampled Integer differs from its previous tick.
#[test]
fn first_tick_and_conditions_follow_the_event_clock() {
    let source = r#"
model ChangeDetector
  Boolean b = sin(2 * 3.141592653589793 * time) > 0.5;
  Clock c = Clock(b);
  Integer k = integer(floor(time / 1.5));
  Integer u(start = 0);
  Boolean changed(start = false);
  Real flag(start = 0);
  Real held;
  Real s(start = 0, fixed = true);
equation
  der(s) = 1;
  when c then
    u = sample(k);
    changed = if firstTick() then false else not (u == previous(u));
    flag = if changed then 1.0 else 0.0;
  end when;
  held = hold(flag);
end ChangeDetector;
"#;
    let result = simulate(source, "ChangeDetector", 3.0);
    // Ticks at 1/12 + j sample k = 0, 0, 1, so only the tick at 2 1/12 changes it.
    assert_eq!(value_at(&result, "held", 0.5), 0.0);
    assert_eq!(value_at(&result, "held", 1.5), 0.0);
    assert_eq!(value_at(&result, "held", 2.5), 1.0);
}
