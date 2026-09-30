//! Structural evaluation during instantiation and flattening (MLS §4.4.5,
//! §7.2.4, §10.1): each test is the reduced form of a library model that
//! OpenModelica checks and Rumoca used to reject. See `docs/toolbugs/`
//! TOOLBUG-090 through TOOLBUG-098.

use rumoca_compile::compile::{CompilationResult, Session, SessionConfig};

fn compile(source: &str, model: &str) -> CompilationResult {
    let mut session = Session::new(SessionConfig::default());
    session
        .add_document("structural.mo", source)
        .expect("fixture parses");
    session
        .compile_model(model)
        .unwrap_or_else(|error| panic!("{model} should compile: {error:?}"))
}

fn has_variable(result: &CompilationResult, name: &str) -> bool {
    result
        .flat
        .variables
        .keys()
        .any(|candidate| candidate.as_str() == name)
}

fn dims(result: &CompilationResult, name: &str) -> Vec<i64> {
    result
        .flat
        .variables
        .iter()
        .find(|(candidate, _)| candidate.as_str() == name)
        .map(|(_, variable)| variable.dims.clone())
        .unwrap_or_else(|| panic!("no flat variable `{name}`"))
}

/// TOOLBUG-090: `Buildings.Controls.OBC.CDL.Integers.Sources.TimeTable`
/// infers `val[:,:]` from `integer(table[1:end, 2:end] + ...)`. The `end`
/// of each slice is the extent of the indexed dimension, and the element-wise
/// `integer` conversion keeps its operand's shape.
#[test]
fn colon_dimensions_follow_end_slices_and_elementwise_builtins() {
    let source = r#"
        package T
          block ITT
            parameter Real table[:,:];
            final parameter Integer nout = size(table, 2) - 1;
            final parameter Integer nT = size(table, 1);
            final parameter Real timeStamps[:] = table[1:end, 1];
            final parameter Integer val[:,:] =
              integer(table[1:end, 2:end] + ones(nT, nout)*1e-10);
            final parameter Real tail[:] = abs(table[2:end, 1]);
            output Integer y[nout];
          equation
            y = val[1, :];
          end ITT;
          block BTT
            parameter Real table[:,:];
            ITT intTimTab(final table = table);
          end BTT;
          model M
            BTT booTimTab(table = [0, 1, 0; 1.3, 1, 1; 2.9, 0, 0]);
          end M;
        end T;
    "#;
    let result = compile(source, "T.M");
    assert_eq!(dims(&result, "booTimTab.intTimTab.val"), vec![3, 2]);
    assert_eq!(dims(&result, "booTimTab.intTimTab.timeStamps"), vec![3]);
    assert_eq!(dims(&result, "booTimTab.intTimTab.tail"), vec![2]);
}

/// TOOLBUG-091: a top-level Integer or Boolean input connected to a
/// sub-component input drives it (MLS §9.1); the connection is a discrete
/// assignment, not a Real residual.
#[test]
fn top_level_discrete_inputs_drive_connected_inputs() {
    let source = r#"
        package P
          connector II = input Integer;
          connector BI = input Boolean;
          connector BO = output Boolean;
          block GT
            parameter Integer t = 0;
            II u;
            BI enable;
            BO y;
          equation
            y = enable and u > t;
          end GT;
          model M
            II u;
            BI enable;
            BO y;
            GT intGreThr;
          equation
            connect(u, intGreThr.u);
            connect(enable, intGreThr.enable);
            connect(intGreThr.y, y);
          end M;
        end P;
    "#;
    let result = compile(source, "P.M");
    assert!(has_variable(&result, "intGreThr.u"));
}

/// TOOLBUG-092: conditions reading package constants (MLS §5.3): the class
/// prefix is looked up lexically, `size()` of a constant array is its
/// constructor extent, and a forwarding `redeclare package Medium = Medium`
/// denotes the enclosing model's (non-replaceable) package.
#[test]
fn package_constant_conditions_follow_lexical_and_forwarded_packages() {
    let source = r#"
        package P
          partial package PM
            constant String substanceNames[:] = {"a"};
            constant Integer nS = size(substanceNames, 1);
            constant Integer nX = nS;
            constant Integer k = 1;
          end PM;
          package Air
            extends PM(substanceNames = {"water", "air"});
          end Air;
          block X
            output Real y = 2*time;
          end X;
          model Outside
            replaceable package Medium = PM;
            final parameter Boolean singleSubstance = (Medium.nX == 1);
            X x if not singleSubstance;
            X lexical if PM.k == 2;
          end Outside;
          model Forwarded
            package Medium = Air;
            Outside west(redeclare package Medium = Medium);
          end Forwarded;
          model Default
            Outside west;
          end Default;
        end P;
    "#;
    let forwarded = compile(source, "P.Forwarded");
    assert!(has_variable(&forwarded, "west.x.y"));
    assert!(!has_variable(&forwarded, "west.lexical.y"));
    let default = compile(source, "P.Default");
    assert!(!has_variable(&default, "west.x.y"));
}

/// TOOLBUG-093: a Real modifier is decided in the scope that wrote it
/// (MLS §7.2.4), so `noDynamics = not (T > 0)` under `T = T1` is structural;
/// an array reduction such as `sum(A) > 0` over literal modifiers is too.
#[test]
fn real_modifiers_and_array_reductions_decide_conditions() {
    let source = r#"
        package P
          block X
            parameter Real k = 1;
            output Real y = k*time;
          end X;
          model FirstOrder
            parameter Real T;
            parameter Boolean noDynamics = not (T > 0);
            X x(k = T) if not noDynamics;
            X gain if noDynamics;
          end FirstOrder;
          model Zone
            parameter Integer n(min = 1);
            parameter Real A[n];
            X solRad[n] if sum(A) > 0;
            X dark if max(A) <= 0;
          end Zone;
          model Top
            parameter Real T1 = 0.1;
            FirstOrder f(T = T1);
            Zone z(n = 2, A = {0, 7});
          end Top;
        end P;
    "#;
    let result = compile(source, "P.Top");
    assert!(has_variable(&result, "f.x.y"));
    assert!(!has_variable(&result, "f.gain.y"));
    assert!(has_variable(&result, "z.solRad[2].y"));
    assert!(!has_variable(&result, "z.dark.y"));
}

/// TOOLBUG-094: a condition that reads a missing `outer` element is decided
/// against the synthesized default inner (MLS §5.4), as for `Parts.Body`'s
/// `sphere if world.enableAnimation and sphereDiameter > 0`.
#[test]
fn conditions_on_missing_outer_use_the_synthesized_inner() {
    let source = r#"
        package P
          model World
            parameter Boolean enableAnimation = true;
            parameter Real nominalLength = 1;
            parameter Real d = nominalLength/9;
          end World;
          block X
            parameter Real k = 1;
            output Real y = k*time;
          end X;
          model Body
            outer World world;
            parameter Real sphereDiameter = world.d;
            X sphere(k = sphereDiameter) if world.enableAnimation and sphereDiameter > 0;
          end Body;
          model Top
            Body b;
          end Top;
        end P;
    "#;
    let result = compile(source, "P.Top");
    assert!(has_variable(&result, "b.sphere.y"));
}

/// TOOLBUG-095: `Modelica.Math.BooleanVectors.allTrue` is `min(b)` over a
/// Boolean vector, which MLS §10.3.4 defines with `false < true`.
#[test]
fn boolean_min_and_max_are_defined() {
    let source = r#"
        package P
          function allTrue
            input Boolean b[:];
            output Boolean result = size(b, 1) == 0 or min(b);
          algorithm
          end allTrue;
          model M
            Boolean b[2] = {time > 0.5, time > 0.2};
            Boolean all;
            Boolean any;
          equation
            all = allTrue(b);
            any = max(b);
          end M;
        end P;
    "#;
    compile(source, "P.M");
}

/// TOOLBUG-096: a function specialization settles the array-constructor
/// extents its inputs decide (`fill(u, nout)`, `fill(1.0, size(x, 1))`).
#[test]
fn function_array_constructors_take_extents_from_specialized_inputs() {
    let source = r#"
        package P
          function rep
            input Integer n;
            input Real u;
            output Real y[n];
          algorithm
            y := fill(u, n);
          end rep;
          function shifted
            input Real x[:];
            output Real y[size(x, 1)];
          algorithm
            y := fill(1.0, size(x, 1)) + x;
          end shifted;
          model M
            Real z[3] = rep(3, 2.0);
            Real w[2] = shifted({1.0, 2.0});
          end M;
        end P;
    "#;
    let result = compile(source, "P.M");
    let simulation = rumoca_sim::simulate_dae(&result.dae, &rumoca_sim::SimOptions::default())
        .expect("model simulates");
    for (name, expected) in [("z[1]", 2.0), ("z[3]", 2.0), ("w[1]", 2.0), ("w[2]", 3.0)] {
        let index = simulation
            .names
            .iter()
            .position(|candidate| candidate == name)
            .unwrap_or_else(|| panic!("trace has `{name}`"));
        let value = *simulation.data[index].last().expect("non-empty trace");
        assert!((value - expected).abs() < 1e-12, "{name} = {value}");
    }
}

/// TOOLBUG-097: `homotopy(actual = …, simplified = 0)` promotes the Integer.
#[test]
fn homotopy_promotes_integer_simplified() {
    let source = r#"
        model H
          Real x;
        equation
          x = homotopy(actual = sin(time), simplified = 0);
        end H;
    "#;
    compile(source, "H");
}

/// TOOLBUG-098: a comprehension inside an initial assertion is planned.
#[test]
fn comprehension_inside_initial_assertion_is_planned() {
    let source = r#"
        package P
          function andTrue
            input Boolean b[:];
            output Boolean result = size(b, 1) == 0 or min(b);
          algorithm
          end andTrue;
          model M
            parameter Integer nin = 2;
            parameter Integer nout = 2;
            parameter Integer extract[nout] = 1:nout;
            Real y = time;
          initial equation
            assert(andTrue({(extract[i] > 0 and extract[i] <= nin) for i in 1:nout}), "bad");
          end M;
        end P;
    "#;
    compile(source, "P.M");
}
