//! The accepted steps and located events a simulation records for its speed
//! report row.

use rumoca::Compiler;
use rumoca_sim::{SimOptions, reset_step_counts, simulate_dae_with_diagnostics, step_counts};

const BOUNCE: &str = "
model Bounce
  Real h(start = 1, fixed = true);
  Real v(start = 0, fixed = true);
equation
  der(h) = v;
  der(v) = -9.81;
  when h < 0 then
    reinit(v, -0.8 * pre(v));
  end when;
end Bounce;
";

/// A falling ball integrates with accepted steps and bounces at located
/// events, and the counts start from zero at each reset.
#[test]
fn a_bouncing_ball_records_its_steps_and_events() {
    let compiled = match Compiler::new()
        .model("Bounce")
        .compile_str(BOUNCE, "Bounce.mo")
    {
        Ok(compiled) => compiled,
        Err(error) => panic!("compile Bounce: {error:#}"),
    };
    reset_step_counts();
    let options = SimOptions {
        t_end: 1.5,
        ..Default::default()
    };
    if let Err(error) = simulate_dae_with_diagnostics(&compiled.dae, &options) {
        panic!("simulate Bounce: {error:#}");
    }
    let counts = step_counts();
    assert!(counts.solver_steps > 10, "{counts:?}");
    assert!(counts.root_hits >= 2, "{counts:?}");
    reset_step_counts();
    assert_eq!(step_counts(), rumoca_sim::HotpathStatsSnapshot::default());
}
