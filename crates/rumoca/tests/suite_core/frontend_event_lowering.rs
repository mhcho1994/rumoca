//! Event, discrete-time and algorithm lowering shapes from real libraries
//! (Buildings/IBPSA/AixLib/IDEAS controllers, OpenIPSL governors, TRANSFORM
//! and the MSL) that OpenModelica accepts and rumoca used to refuse.
//!
//! Each model is the minimal reduction of one root cause; the expected values
//! are the ones the MLS gives the source (and OpenModelica produces).

use rumoca_sim::{SimOptions, SimResult, SimSolverMode, simulate_dae_with_diagnostics};

fn compile(name: &str, source: &str) -> rumoca::CompilationResult {
    rumoca::Compiler::new()
        .model(name)
        .compile_str(source, "frontend_event_lowering.mo")
        .unwrap_or_else(|error| panic!("`{name}` should compile: {error}"))
}

fn simulate(name: &str, source: &str, t_end: f64) -> SimResult {
    let compiled = compile(name, source);
    simulate_dae_with_diagnostics(
        &compiled.dae,
        &SimOptions {
            t_end,
            dt: Some(0.01),
            solver_mode: SimSolverMode::RkLike,
            ..SimOptions::default()
        },
    )
    .unwrap_or_else(|error| panic!("`{name}` should simulate: {error}"))
}

/// The last recorded sample at or before `t` (the post-event row at an event).
fn value_at(sim: &SimResult, name: &str, t: f64) -> f64 {
    let index = sim
        .names
        .iter()
        .position(|candidate| candidate == name)
        .unwrap_or_else(|| panic!("trace should contain `{name}`; names={:?}", sim.names));
    let mut result = sim.data[index][0];
    for (sample, &time) in sim.times.iter().enumerate() {
        if time > t + 1.0e-9 {
            break;
        }
        result = sim.data[index][sample];
    }
    result
}

/// TOOLBUG-110: a relational `when` statement that updates a discrete `Real`
/// once is exactly the when-equation it spells, so it needs no model-event
/// transaction (which exists only under a periodic clock).
const RELATIONAL_WHEN_ALGORITHM: &str = "model Alg2
  Real x(start = 1, fixed = true);
  discrete Real n(start = 0, fixed = true);
  Integer k(start = 0, fixed = true);
equation
  der(x) = -2;
algorithm
  when x < 0.5 then
    n := pre(n) + 1;
    k := pre(k) + 2;
  end when;
end Alg2;";

#[test]
fn relational_when_statement_on_a_discrete_real_fires_once_at_its_crossing() {
    let sim = simulate("Alg2", RELATIONAL_WHEN_ALGORITHM, 1.0);
    assert_eq!(value_at(&sim, "n", 0.2), 0.0);
    assert_eq!(value_at(&sim, "k", 0.2), 0.0);
    assert_eq!(value_at(&sim, "n", 0.3), 1.0, "x crosses 0.5 at t = 0.25");
    assert_eq!(value_at(&sim, "k", 0.3), 2.0);
    assert_eq!(
        value_at(&sim, "n", 1.0),
        1.0,
        "a when fires on the rising edge only"
    );
}

/// TOOLBUG-111: `getInstanceName()` in a declaration or modifier binding.
const INSTANCE_NAME_BINDINGS: &str = "package GIN
  block Writer
    parameter String fileName = getInstanceName() + \".csv\";
    parameter String insNam = getInstanceName();
    Real x = 1;
  end Writer;
  model Top
    Writer w1;
    Writer w2(fileName = getInstanceName() + \".txt\");
  end Top;
end GIN;";

#[test]
fn get_instance_name_in_bindings_names_the_enclosing_instance() {
    let compiled = compile("GIN.Top", INSTANCE_NAME_BINDINGS);
    let binding = |name: &str| {
        let variable = compiled
            .flat
            .variables
            .get(&rumoca_core::VarName::new(name))
            .unwrap_or_else(|| panic!("`{name}` is a flat variable"));
        format!("{:?}", variable.binding)
    };
    assert!(
        binding("w1.insNam").contains("\"Top.w1\""),
        "{}",
        binding("w1.insNam")
    );
    assert!(binding("w1.fileName").contains("\"Top.w1\""));
    assert!(
        binding("w2.fileName").contains("\"Top\""),
        "a modifier is written in, and names, the enclosing instance: {}",
        binding("w2.fileName")
    );
}

/// TOOLBUG-112: a fixed parameter without a binding takes its `start` value
/// (MLS §8.6), so `Modelica.Blocks.Interfaces.DiscreteBlock.samplePeriod`
/// (declared with `start = 0.1` only) gives `sample` a static interval.
const UNBOUND_SAMPLE_PERIOD: &str = "model SP
  parameter Real samplePeriod(start = 0.1);
  discrete Real y(start = 0, fixed = true);
equation
  when sample(0, samplePeriod) then
    y = pre(y) + 1;
  end when;
end SP;";

#[test]
fn an_unbound_sample_period_uses_its_start_value() {
    let sim = simulate("SP", UNBOUND_SAMPLE_PERIOD, 0.35);
    assert_eq!(value_at(&sim, "y", 0.05), 1.0);
    assert_eq!(value_at(&sim, "y", 0.15), 2.0);
    assert_eq!(value_at(&sim, "y", 0.35), 4.0);
}

/// TOOLBUG-113: a continuous algorithm assigning several scalars (TRANSFORM
/// `BiotNumber`, `SimpleCylinder`) is one declarative definition per target.
const MULTI_OUTPUT_ALGORITHM: &str = "model Multi
  input Real a = 2;
  Real b;
  Real c;
  Integer r;
algorithm
  b := a*3;
  r := 1;
  if a > 1 then
    c := b + 1;
  else
    c := b - 1;
  end if;
  b := b + c;
end Multi;";

#[test]
fn a_multi_output_algorithm_defines_each_target_by_its_final_value() {
    let sim = simulate("Multi", MULTI_OUTPUT_ALGORITHM, 0.1);
    assert_eq!(value_at(&sim, "c", 0.0), 7.0);
    assert_eq!(
        value_at(&sim, "b", 0.0),
        13.0,
        "b := 6 + c after c reads b = 6"
    );
    assert_eq!(value_at(&sim, "r", 0.0), 1.0);
}

const READ_BEFORE_WRITE_ALGORITHM: &str = "model ReadFirst
  Real b;
  Real c;
algorithm
  c := b + 1;
  b := 2;
end ReadFirst;";

#[test]
fn a_multi_output_algorithm_reading_a_target_before_writing_it_stays_refused() {
    let error = rumoca::Compiler::new()
        .model("ReadFirst")
        .compile_str(READ_BEFORE_WRITE_ALGORITHM, "frontend_event_lowering.mo")
        .expect_err("a read before definition needs start initialization");
    assert!(
        error.to_string().contains("read before definition"),
        "{error}"
    );
}

/// TOOLBUG-114: `initial algorithm p0 := PMECH0;` for a `fixed = false`
/// parameter and a coordinate the initialization system solves (the OpenIPSL
/// governor idiom) is the initial equation `p0 = PMECH0`.
const INITIAL_ALGORITHM_FROM_A_COORDINATE: &str = "model Gov
  Real pmech = 2 + time;
  parameter Real p0(fixed = false);
  Real x;
initial algorithm
  p0 := pmech;
initial equation
  x = p0;
equation
  der(x) = -x;
end Gov;";

#[test]
fn an_initial_algorithm_parameter_read_from_a_coordinate_is_solved_at_initialization() {
    let sim = simulate("Gov", INITIAL_ALGORITHM_FROM_A_COORDINATE, 0.1);
    assert!((value_at(&sim, "x", 0.0) - 2.0).abs() < 1.0e-9);
}

/// TOOLBUG-115: `delay(u, 0)` (`FixedDelay(delayTime = 0)`) is `u` itself.
const ZERO_DELAY: &str = "model ZeroDelay
  parameter Real T = 0;
  Real u = time;
  Real y = delay(u, T);
end ZeroDelay;";

#[test]
fn a_zero_delay_is_its_source() {
    let sim = simulate("ZeroDelay", ZERO_DELAY, 0.5);
    assert!((value_at(&sim, "y", 0.3) - 0.3).abs() < 1.0e-9);
}

/// TOOLBUG-116: element equations of a discrete array written in a `for`
/// loop (CDL `BooleanExtractSignal`), including a one-element array whose
/// single element is the whole coordinate.
const EXTRACT_SIGNAL: &str = "model Extract
  parameter Integer extract[2] = {3, 1};
  parameter Integer one[1] = {2};
  Boolean u[3] = {time > 0.2, time > 0.4, time > 0.6};
  Boolean y[2];
  Boolean z[1];
equation
  for i in 1:2 loop
    y[i] = u[extract[i]];
  end for;
  for i in 1:1 loop
    z[i] = u[one[i]];
  end for;
end Extract;";

#[test]
fn discrete_element_equations_in_a_for_loop_define_the_whole_array() {
    let sim = simulate("Extract", EXTRACT_SIGNAL, 1.0);
    assert_eq!(value_at(&sim, "y[1]", 0.5), 0.0);
    assert_eq!(value_at(&sim, "y[2]", 0.5), 1.0);
    assert_eq!(value_at(&sim, "y[1]", 0.7), 1.0);
    assert_eq!(value_at(&sim, "z[1]", 0.3), 0.0);
    assert_eq!(value_at(&sim, "z[1]", 0.5), 1.0);
}

/// TOOLBUG-117: `Modelica.StateGraph.Interfaces.CompositeStepState` declares
/// `output Boolean suspend = false` and writes `suspend =
/// subgraphStatePort.suspend`; the equality defines its other side.
const SYMMETRIC_DISCRETE_EQUALITY: &str = "model Root
  output Boolean suspend = false;
  Boolean portSuspend;
equation
  suspend = portSuspend;
end Root;";

#[test]
fn a_discrete_equality_defines_the_side_no_other_row_defines() {
    let sim = simulate("Root", SYMMETRIC_DISCRETE_EQUALITY, 0.1);
    assert_eq!(value_at(&sim, "portSuspend", 0.1), 0.0);
}

/// TOOLBUG-113: a declarative algorithm with an unrolled `for` loop and a
/// top-level `assert` (Buildings `NumberOfRequests`, CDL `RealExtractor`).
const DECLARATIVE_LOOP: &str = "model Loop
  parameter Integer n = 3;
  input Real u[n] = {1, 2, 3};
  Integer y;
  Real s;
  Real t;
algorithm
  assert(n > 0, \"n must be positive\");
  y := 0;
  s := 0;
  for i in 1:n loop
    if u[i] > 1.5 then
      y := y + 1;
    end if;
    s := s + u[i]*i;
  end for;
  t := s/2;
end Loop;";

#[test]
fn a_declarative_loop_is_unrolled_over_its_settled_range() {
    let sim = simulate("Loop", DECLARATIVE_LOOP, 0.1);
    assert_eq!(value_at(&sim, "y", 0.0), 2.0);
    assert_eq!(value_at(&sim, "s", 0.0), 14.0);
    assert_eq!(value_at(&sim, "t", 0.0), 7.0);
}

/// TOOLBUG-118: the continuous prefix of an event algorithm (ThermoSysPro
/// `ConvAD`) is its own declarative algorithm.
const MIXED_ALGORITHM: &str = "model ConvAD
  parameter Real maxval = 1;
  parameter Real minval = -maxval;
  Real u = time;
  discrete Real y(start = 0, fixed = true);
  Real q;
algorithm
  q := (maxval - minval)/4;
  when sample(0, 0.25) then
    y := q*floor(u/q + 0.5);
  end when;
end ConvAD;";

#[test]
fn the_continuous_prefix_of_an_event_algorithm_is_split_off() {
    let sim = simulate("ConvAD", MIXED_ALGORITHM, 1.0);
    assert_eq!(value_at(&sim, "q", 0.5), 0.5);
    assert_eq!(value_at(&sim, "y", 0.2), 0.0);
    assert_eq!(
        value_at(&sim, "y", 0.3),
        0.5,
        "u = 0.25 rounds to 0.5 at the tick"
    );
    assert_eq!(value_at(&sim, "y", 0.8), 1.0);
}

/// TOOLBUG-119: `min`/`max` of a Boolean vector (MSL `BooleanVectors.andTrue`
/// / `orTrue`, reached by the CDL `ExtractSignal` range assertion).
const BOOLEAN_EXTREMA: &str = "model BooleanExtrema
  function allTrue
    input Boolean b[:];
    output Boolean result = size(b, 1) == 0 or min(b);
  algorithm
  end allTrue;
  function anyTrue
    input Boolean b[:];
    output Boolean result = size(b, 1) > 0 and max(b);
  algorithm
  end anyTrue;
  Boolean u[3] = {time > 0.2, time > 0.4, true};
  Boolean all = allTrue(u);
  Boolean any = anyTrue({time > 0.6, false});
end BooleanExtrema;";

#[test]
fn boolean_vectors_order_false_below_true() {
    let sim = simulate("BooleanExtrema", BOOLEAN_EXTREMA, 1.0);
    assert_eq!(value_at(&sim, "all", 0.3), 0.0);
    assert_eq!(value_at(&sim, "all", 0.5), 1.0);
    assert_eq!(value_at(&sim, "any", 0.5), 0.0);
    assert_eq!(value_at(&sim, "any", 0.7), 1.0);
}

/// TOOLBUG-117: a discrete-valued target defined in nested if-equation
/// branches (VehicleInterfaces `ShiftOutput`), which Flat renders as
/// `(if ... ) - 0.0` inside the outer conditional residual.
const NESTED_IF_DISCRETE: &str = "model ShiftOutput
  Real s = time;
  Integer gear;
equation
  if s <= 0.25 then
    if s >= 0.1 then
      gear = 1;
    else
      gear = 2;
    end if;
  else
    gear = 3;
  end if;
end ShiftOutput;";

#[test]
fn a_discrete_target_in_nested_if_equations_is_one_definition() {
    let sim = simulate("ShiftOutput", NESTED_IF_DISCRETE, 0.5);
    assert_eq!(value_at(&sim, "gear", 0.05), 2.0);
    assert_eq!(value_at(&sim, "gear", 0.2), 1.0);
    assert_eq!(value_at(&sim, "gear", 0.4), 3.0);
}

/// TOOLBUG-119: an array comprehension inside an `assert` condition (the CDL
/// `ExtractSignal` range check) has a comprehension plan.
const ASSERTED_COMPREHENSION: &str = "model Cmp
  function andTrue
    input Boolean b[:];
    output Boolean result = size(b, 1) == 0 or min(b);
  algorithm
  end andTrue;
  parameter Integer nin = 3;
  parameter Integer nout = 2;
  parameter Integer extract[nout] = {3, 1};
  Real x = time;
initial equation
  assert(andTrue({(extract[i] > 0 and extract[i] <= nin) for i in 1:nout}), \"bad\");
end Cmp;";

#[test]
fn a_comprehension_in_an_assertion_condition_lowers() {
    let sim = simulate("Cmp", ASSERTED_COMPREHENSION, 0.1);
    assert!((value_at(&sim, "x", 0.1) - 0.1).abs() < 1.0e-9);
}

/// TOOLBUG-180: an array constructor inside a when-equation body (CDL
/// `TriggeredMovingMean`) had no comprehension plan, because the analysis only
/// planned constructors of plain equations, and lowering panicked.
const WHEN_BODY_COMPREHENSION: &str = "model TriggeredRing
  parameter Integer n = 3;
  Real u = time;
  Integer iSample(start = 0, fixed = true);
  Integer index(start = 0, fixed = true);
  discrete Real ySample[n](start = zeros(n), each fixed = true);
equation
  when sample(0.5, 1) then
    index = mod(pre(iSample), n) + 1;
    ySample = {if i == index then u else pre(ySample[i]) for i in 1:n};
    iSample = pre(iSample) + 1;
  end when;
end TriggeredRing;";

#[test]
fn an_array_constructor_in_a_when_body_writes_the_selected_slot() {
    let sim = simulate("TriggeredRing", WHEN_BODY_COMPREHENSION, 4.0);
    // Events at 0.5, 1.5, 2.5 fill slots 1..3; the event at 3.5 wraps to slot 1.
    assert_eq!(value_at(&sim, "ySample[1]", 3.0), 0.5);
    assert_eq!(value_at(&sim, "ySample[2]", 3.0), 1.5);
    assert_eq!(value_at(&sim, "ySample[3]", 3.0), 2.5);
    assert_eq!(value_at(&sim, "ySample[1]", 4.0), 3.5);
    assert_eq!(value_at(&sim, "index", 4.0), 1.0);
}

/// TOOLBUG-181: an initial algorithm with a `for` loop over a parameter range,
/// a coordinate assigned only on some paths (it keeps its `start`, MLS
/// §11.1.2), an array constructor, and an enumeration-literal guard
/// (IBPSA/IDEAS/AixLib `CalendarTime`).
const INITIAL_ALGORITHM_LOOP: &str = "model InitialLoop
  type Zero = enumeration(A, B);
  parameter Zero z = Zero.B;
  parameter Integer days[3] = {31, 28, 31};
  parameter Real ts[4] = {-1, 1, 2, 3};
  parameter Real off(fixed = false);
  discrete Integer idx;
  discrete Integer k(start = 5);
  Real x(start = 0, fixed = true);
initial algorithm
  off := 0;
  if z == Zero.B then
    off := sum({days[i] for i in 1:2});
  end if;
  for i in 2:4 loop
    if time < ts[i] and time >= ts[i - 1] then
      idx := i - 1;
    end if;
    if time > 100 then
      k := i;
    end if;
  end for;
equation
  der(x) = off;
  when time > 10 then
    idx = pre(idx) + 1;
    k = pre(k) + 1;
  end when;
end InitialLoop;";

#[test]
fn an_initial_algorithm_loop_unrolls_and_keeps_unassigned_starts() {
    let sim = simulate("InitialLoop", INITIAL_ALGORITHM_LOOP, 1.0);
    assert_eq!(value_at(&sim, "idx", 0.0), 1.0);
    assert_eq!(value_at(&sim, "k", 0.0), 5.0);
    assert!((value_at(&sim, "x", 1.0) - 59.0).abs() < 1.0e-6);
}

/// TOOLBUG-182: an if-equation in a when-branch whose guard reads the current
/// value another equation of the same branch defines (`CalendarTime`'s
/// `isLeapYear[yearIndex]`).
const WHEN_GUARD_READS_CURRENT_TARGET: &str = "model GuardCurrent
  parameter Real ts[4] = {1, 2, 3, 4};
  parameter Boolean leap[4] = {true, false, false, true};
  discrete Integer yearIndex(start = 1, fixed = true);
  discrete Integer month(start = 1, fixed = true);
equation
  when sample(0.5, 1) then
    if time - ts[pre(yearIndex)] > 0 then
      yearIndex = pre(yearIndex) + 1;
    else
      yearIndex = pre(yearIndex);
    end if;
    if leap[yearIndex] then
      month = pre(month) + 1;
    else
      month = pre(month);
    end if;
  end when;
end GuardCurrent;";

#[test]
fn a_when_branch_guard_reads_the_current_value_of_an_earlier_target() {
    let sim = simulate("GuardCurrent", WHEN_GUARD_READS_CURRENT_TARGET, 4.0);
    // t=0.5: index 1 (leap) -> month 2; t=1.5: index 2 -> 2; t=2.5: 3 -> 2;
    // t=3.5: index 4 (leap) -> 3. Reading pre(yearIndex) would give 3 at 1.5.
    assert_eq!(value_at(&sim, "month", 1.0), 2.0);
    assert_eq!(value_at(&sim, "month", 2.0), 2.0);
    assert_eq!(value_at(&sim, "yearIndex", 3.0), 3.0);
    assert_eq!(value_at(&sim, "month", 4.0), 3.0);
}

/// TOOLBUG-183: `floor`/`ceil`/`integer` of a continuous-time argument
/// generate events (MLS §3.7.2); a discrete `Integer` defined from them was
/// never re-evaluated and stayed at its initial value for the whole run.
const EVENT_ROUNDING: &str = "model EventRounding
  Integer f;
  Integer c;
  Integer i;
equation
  f = integer(floor(2 * time - 1.25));
  c = integer(ceil(2 * time - 1.25));
  i = integer(2 * time - 1.25);
end EventRounding;";

#[test]
fn rounding_a_continuous_argument_updates_discrete_integers_at_each_crossing() {
    let sim = simulate("EventRounding", EVENT_ROUNDING, 1.5);
    for (t, floor, ceil) in [
        (0.0, -2.0, -1.0),
        (0.3, -1.0, 0.0),
        (0.8, 0.0, 1.0),
        (1.4, 1.0, 2.0),
    ] {
        assert_eq!(value_at(&sim, "f", t), floor, "floor at t={t}");
        assert_eq!(value_at(&sim, "c", t), ceil, "ceil at t={t}");
        assert_eq!(value_at(&sim, "i", t), floor, "integer at t={t}");
    }
}

/// TOOLBUG-180 follow-up: with its size parameter unbound the same block has
/// zero-size arrays, and the bitcode export projected scalar 0 of the empty
/// when-assignment value to collect its reads.
#[test]
fn an_empty_when_body_array_constructor_exports() {
    let source = WHEN_BODY_COMPREHENSION
        .replace("parameter Integer n = 3;", "parameter Integer n = 0;")
        .replace("TriggeredRing", "EmptyRing");
    let compiled = compile("EmptyRing", &source);
    let exported = rumoca_bitcode::export(
        &compiled.dae,
        None,
        "EmptyRing",
        &rumoca_bitcode::ExportOptions::default(),
    );
    assert!(
        exported.is_ok(),
        "zero-size when assignment should export: {:?}",
        exported.err()
    );
}
