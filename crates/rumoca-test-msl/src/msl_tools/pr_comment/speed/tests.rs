use super::*;

/// A (high, 5 equations) is faster than OMC everywhere; B (near, 12
/// equations) is slower than OMC in compiler work and simulation; C does not
/// agree with OMC and is never timed.
fn write_inputs(results: &Path, omc_workers: u64, rumoca_workers: u64) {
    let rumoca_row = |name: &str, eqs: u64, front: f64, solve: f64, build: f64, run: f64| {
        serde_json::json!({
            "model_name": name,
            "scalar_equations": eqs,
            "compile_seconds": front,
            "instantiate_seconds": front * 0.1,
            "typecheck_seconds": front * 0.1,
            "flatten_seconds": front * 0.6,
            "dae_seconds": front * 0.2,
            "ir_solve_seconds": solve,
            "sim_backend_build_seconds": build - solve,
            "sim_build_seconds": build,
            "sim_run_seconds": run,
            "sim_settings": {
                "solver": "auto", "rtol": 1e-6, "atol": 1e-6,
                "output_points": 501, "steps": 40, "events": 2
            }
        })
    };
    let msl = serde_json::json!({ "model_results": [
        rumoca_row("A", 5, 1.0, 0.2, 0.5, 0.5),
        rumoca_row("B", 12, 2.0, 1.0, 1.5, 3.0),
        rumoca_row("C", 12, 2.0, 1.0, 1.5, 3.0),
    ]});
    let omc_row = |total: f64, sim: f64, work: f64| {
        serde_json::json!({
            "status": "success",
            "total_system_seconds": total,
            "sim_system_seconds": sim,
            "omc_phases": {
                "frontend": work * 0.5, "backend": work * 0.3, "sim_code": work * 0.1,
                "templates": work * 0.1, "compile": total - sim - work,
                "simulation": sim, "total": total
            },
            "omc_settings": {
                "method": "dassl", "tolerance": 1e-6, "number_of_intervals": 500,
                "start_time": 0.0, "stop_time": 1.0
            }
        })
    };
    let omc = serde_json::json!({
        "models": { "A": omc_row(5.0, 1.0, 2.4), "B": omc_row(8.0, 2.0, 2.0), "C": omc_row(8.0, 2.0, 2.0) },
        "timing": {
            "workers_used": omc_workers, "rumoca_sim_workers": rumoca_workers,
            "rumoca_stage_workers": 3, "omc_threads": 1, "shards": 4,
            "batches_total": 3, "batches_ran": 2,
            "host": { "logical_cpus": 4, "physical_cores": 2, "image": "ubuntu24", "runner_environment": "github-hosted" }
        }
    });
    let trace = serde_json::json!({ "models": {
        "A": { "channel_high_percent": 1.0, "channel_minor_percent": 0.0, "channel_deviation_percent": 0.0 },
        "B": { "channel_high_percent": 0.8, "channel_minor_percent": 0.1, "channel_deviation_percent": 0.1 },
        "C": { "channel_high_percent": 0.2, "channel_minor_percent": 0.1, "channel_deviation_percent": 0.7 }
    }});
    for (name, value) in [
        ("msl_results.json", msl),
        ("omc_simulation_reference.json", omc),
        ("sim_trace_comparison.json", trace),
    ] {
        fs::write(results.join(name), value.to_string()).expect("write fixture");
    }
}

#[test]
fn every_aggregate_is_over_the_agreeing_set_with_a_high_only_line() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_inputs(temp.path(), 2, 2);
    let rendered = render_speed_section(temp.path()).expect("render");
    assert!(
        rendered.contains("**2 trace-agreeing models**"),
        "{rendered}"
    );
    assert!(rendered.contains("1 high, 1 near"));
    // Total: rumoca 2.0 + 6.5, OMC 5 + 8.
    assert!(rendered.contains("| Total (model to results) | 2 | 8.5 | 13.0 | **1.53** |"));
    // Compiler work: rumoca 1.2 + 3.0, OMC 2.4 + 2.0.
    assert!(rendered.contains("| Compiler work | 2 | 4.2 | 4.4 | **1.05** |"));
    // Runnable: rumoca 1.5 + 3.5, OMC 4 + 6.
    assert!(
        rendered.contains("| Time to runnable (JIT vs C toolchain) | 2 | 5.0 | 10.0 | **2.00** |")
    );
    assert!(rendered.contains("| Simulation | 2 | 3.5 | 3.0 | **0.86** |"));
    assert!(rendered.contains("High band only (1 models)"));
    assert!(rendered.contains("Compiler work 2.00×"));
    assert!(rendered.contains("**FMU path:** not measured in CI"));
    assert!(rendered.contains("`timeTotal - timeSimulation`, which includes `timeCompile`"));
}

#[test]
fn the_size_tables_and_slower_list_name_the_dominant_cost() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_inputs(temp.path(), 2, 2);
    let rendered = render_speed_section(temp.path()).expect("render");
    assert!(rendered.contains("| 1–9 | 1 |"));
    assert!(rendered.contains("| 10–24 | 1 |"));
    assert!(rendered.contains("##### Compiler work (1 slower)"));
    assert!(
        rendered.contains("| `B` | 12 | 0.67 | flatten 1.200 s |"),
        "{rendered}"
    );
    assert!(rendered.contains("##### Simulation (1 slower)"));
    assert!(rendered.contains("| `B` | 12 | 0.67 | 40 steps, 2 events, 501 output points |"));
    assert!(rendered.contains("##### Time to runnable (JIT vs C toolchain) (0 slower)"));
    assert!(!rendered.contains("`C`"));
}

#[test]
fn the_methodology_states_recorded_conditions_and_flags_unequal_contention() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_inputs(temp.path(), 2, 2);
    let rendered = render_speed_section(temp.path()).expect("render");
    assert!(rendered.contains(
        "image ubuntu24, environment github-hosted, 4 logical CPUs, 2 physical cores; 4 shard(s)"
    ));
    assert!(rendered.contains("rumoca compile stage 3, rumoca simulations 2, OMC reference simulations 2 (OMC threads 1); simulation contention equal on both tools"));
    assert!(rendered.contains("rumoca solver auto (2/2), rtol 0.000001 (2/2); OMC method dassl (2/2), tolerance 0.000001 (2/2)")
        || rendered.contains("rumoca solver auto (2/2), rtol 1e-6 (2/2); OMC method dassl (2/2), tolerance 1e-6 (2/2)"), "{rendered}");
    assert!(rendered.contains("output_points 501 (2/2); OMC number_of_intervals 500 (2/2)"));
    assert!(
        rendered.contains("OMC references ran for 2 of 3 models this run (1 reused from cache)")
    );

    write_inputs(temp.path(), 8, 3);
    let unequal = render_speed_section(temp.path()).expect("render");
    assert!(
        unequal
            .contains("OMC ran 8 simulations at a time against rumoca's 3, which favours rumoca")
    );
    write_inputs(temp.path(), 2, 3);
    let reversed = render_speed_section(temp.path()).expect("render");
    assert!(
        reversed.contains("rumoca ran 3 simulations at a time against OMC's 2, which favours OMC")
    );
}

#[test]
fn missing_inputs_report_not_measured() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rendered = render_speed_section(temp.path()).expect("render");
    assert!(rendered.contains("Speed vs OMC not measured"));
}
