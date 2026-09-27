//! The "Speed vs OMC" section of the MSL PR comment (SPEC_0025 §4a speed
//! report).
//!
//! Every published aggregate is taken over the trace-agreeing models (both
//! tools produced traces in the `high` or `near` band), with a high-band-only
//! line beside it. Compilation is compared three ways so each number compares
//! like with like: the compiler's own work, the time until the model can run
//! (rumoca's JIT against OMC's C toolchain), and the FMU path. A methodology
//! block states the recorded conditions of both tools' runs.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

use super::{json_f64, json_str, json_u64, nonnegative_json_f64, positive_json_f64};

const SIZE_BINS: &[(u64, Option<u64>)] = &[
    (1, Some(9)),
    (10, Some(24)),
    (25, Some(49)),
    (50, Some(99)),
    (100, Some(249)),
    (250, None),
];
const SLOWER_MODEL_LIMIT: usize = 10;

/// Rumoca's seconds for one model, from its results row.
#[derive(Clone, Default)]
struct RumocaSeconds {
    instantiate: f64,
    typecheck: f64,
    flatten: f64,
    dae: f64,
    /// Front end through the checked DAE (`compile_seconds`).
    front_end: f64,
    /// Solve lowering (`ir_solve_seconds`).
    solve: f64,
    /// Native code generation (`sim_backend_build_seconds`).
    jit: f64,
    /// Solve lowering, JIT, and preparation (`sim_build_seconds`).
    sim_build: f64,
    run: f64,
}

/// OMC's seconds for one model, from its self-reported `SimulationResult`.
#[derive(Clone, Default)]
struct OmcSeconds {
    /// Frontend + backend + SimCode + templates, when OMC reported all four.
    compiler_work: Option<f64>,
    /// `timeTotal - timeSimulation`: compiler work plus the C toolchain.
    runnable: f64,
    simulation: f64,
    total: f64,
}

#[derive(Clone)]
struct SpeedRecord {
    model_name: String,
    scalar_equations: u64,
    high: bool,
    rumoca: RumocaSeconds,
    omc: OmcSeconds,
    rumoca_settings: Option<Value>,
    omc_settings: Option<Value>,
}

/// One compared quantity.
#[derive(Clone, Copy)]
enum Comparison {
    Total,
    CompilerWork,
    Runnable,
    Simulation,
}

const COMPARISONS: [Comparison; 4] = [
    Comparison::Total,
    Comparison::CompilerWork,
    Comparison::Runnable,
    Comparison::Simulation,
];

impl Comparison {
    fn label(self) -> &'static str {
        match self {
            Self::Total => "Total (model to results)",
            Self::CompilerWork => "Compiler work",
            Self::Runnable => "Time to runnable (JIT vs C toolchain)",
            Self::Simulation => "Simulation",
        }
    }

    fn definition(self) -> &'static str {
        match self {
            Self::Total => {
                "Rumoca = front end + Solve lowering + JIT + integration; OMC = `timeTotal`."
            }
            Self::CompilerWork => {
                "Rumoca = front end + Solve lowering (`compile_seconds + ir_solve_seconds`); \
                 OMC = `timeFrontend + timeBackend + timeSimCode + timeTemplates`. \
                 Neither side includes native code generation."
            }
            Self::Runnable => {
                "Rumoca = front end + Solve lowering + Cranelift JIT (`compile_seconds + \
                 sim_build_seconds`); OMC = `timeTotal - timeSimulation`, which includes \
                 `timeCompile`, the C compiler and linker."
            }
            Self::Simulation => "Rumoca = `sim_run_seconds`; OMC = `timeSimulation`.",
        }
    }

    fn rumoca(self, record: &SpeedRecord) -> Option<f64> {
        let r = &record.rumoca;
        Some(match self {
            Self::Total => r.front_end + r.sim_build + r.run,
            Self::CompilerWork => r.front_end + r.solve,
            Self::Runnable => r.front_end + r.sim_build,
            Self::Simulation => r.run,
        })
    }

    fn omc(self, record: &SpeedRecord) -> Option<f64> {
        let o = &record.omc;
        match self {
            Self::Total => Some(o.total),
            Self::CompilerWork => o.compiler_work,
            Self::Runnable => Some(o.runnable),
            Self::Simulation => Some(o.simulation),
        }
    }

    /// `(rumoca, omc)` seconds when both are known and positive.
    fn pair(self, record: &SpeedRecord) -> Option<(f64, f64)> {
        let rumoca = self.rumoca(record).filter(|seconds| *seconds > 0.0)?;
        let omc = self.omc(record).filter(|seconds| *seconds > 0.0)?;
        Some((rumoca, omc))
    }

    /// The rumoca phase that took longest for `record` within this comparison.
    fn dominant_cost(self, record: &SpeedRecord) -> String {
        let r = &record.rumoca;
        let named = r.instantiate + r.typecheck + r.flatten + r.dae;
        let front_end = [
            (
                "resolve and other front end",
                (r.front_end - named).max(0.0),
            ),
            ("instantiate", r.instantiate),
            ("typecheck", r.typecheck),
            ("flatten", r.flatten),
            ("DAE", r.dae),
        ];
        let phases: Vec<(&str, f64)> = match self {
            Self::CompilerWork => front_end
                .into_iter()
                .chain([("Solve lowering", r.solve)])
                .collect(),
            Self::Runnable | Self::Total => front_end
                .into_iter()
                .chain([("Solve lowering", r.solve), ("JIT", r.jit)])
                .chain(matches!(self, Self::Total).then_some(("integration", r.run)))
                .collect(),
            Self::Simulation => return simulation_work(record.rumoca_settings.as_ref()),
        };
        phases
            .into_iter()
            .max_by(|left, right| left.1.total_cmp(&right.1))
            .map_or_else(String::new, |(name, seconds)| {
                format!("{name} {seconds:.3} s")
            })
    }
}

/// The recorded integration work of a rumoca simulation.
fn simulation_work(settings: Option<&Value>) -> String {
    let Some(settings) = settings else {
        return "work not recorded".to_string();
    };
    format!(
        "{} steps, {} events, {} output points",
        json_u64(settings, "steps").unwrap_or(0),
        json_u64(settings, "events").unwrap_or(0),
        json_u64(settings, "output_points").unwrap_or(0),
    )
}

/// Render the speed section from the joined MSL timing artifacts.
pub(super) fn render_speed_section(results_dir: &Path) -> Result<String> {
    let Some(inputs) = SpeedInputs::read(results_dir)? else {
        return Ok(
            "_Speed vs OMC not measured: the run published no joined timing inputs._\n".to_string(),
        );
    };
    let records = inputs.records();
    if records.is_empty() {
        return Ok("_No agreeing models had valid timings on both tools._\n".to_string());
    }
    let mut out = String::new();
    let high = records.iter().filter(|record| record.high).count();
    out.push_str(&format!(
        "Timed over the **{} trace-agreeing models** (both traces in the `high` or `near` \
         band: {high} high, {} near). `Speedup = OMC / Rumoca` (>1 means rumoca faster).\n\n",
        records.len(),
        records.len() - high,
    ));
    out.push_str("##### Aggregate\n\n");
    out.push_str(&render_aggregate_table(&records));
    out.push_str(&render_high_only_line(&records));
    out.push_str(FMU_PATH_LINE);
    out.push('\n');
    out.push_str(&render_methodology(&inputs, &records));
    out.push_str("\n<details>\n<summary><strong>Speed by system size</strong></summary>\n");
    for comparison in COMPARISONS {
        out.push('\n');
        out.push_str(&render_size_table(&records, comparison));
    }
    out.push_str("\n</details>\n\n");
    out.push_str(&render_slower_than_omc(&records));
    Ok(out)
}

const FMU_PATH_LINE: &str = "\n**FMU path:** not measured in CI; no CI job exports and builds FMUs \
     for timing. Local recipe: `rumoca compile <file> -m <Model> --target fmi3 --output <dir>` \
     plus the C build of the emitted sources, against OMC's `buildModelFMU(<Model>, version=\"3.0\")`.\n";

/// The three result artifacts the section joins.
struct SpeedInputs {
    rumoca: Vec<Value>,
    omc: serde_json::Map<String, Value>,
    omc_timing: Value,
    trace: serde_json::Map<String, Value>,
}

impl SpeedInputs {
    fn read(results_dir: &Path) -> Result<Option<Self>> {
        let paths = [
            "msl_results.json",
            "omc_simulation_reference.json",
            "sim_trace_comparison.json",
        ]
        .map(|name| results_dir.join(name));
        if !paths.iter().all(|path| path.is_file()) {
            return Ok(None);
        }
        let msl = read_json_file(&paths[0])?;
        let omc = read_json_file(&paths[1])?;
        let trace = read_json_file(&paths[2])?;
        let (Some(rumoca), Some(omc_models), Some(trace)) = (
            msl.get("model_results").and_then(Value::as_array),
            omc.get("models").and_then(Value::as_object),
            trace.get("models").and_then(Value::as_object),
        ) else {
            return Ok(None);
        };
        Ok(Some(Self {
            rumoca: rumoca.clone(),
            omc: omc_models.clone(),
            omc_timing: omc.get("timing").cloned().unwrap_or(Value::Null),
            trace: trace.clone(),
        }))
    }

    fn records(&self) -> Vec<SpeedRecord> {
        let rumoca_by_name: BTreeMap<&str, &Value> = self
            .rumoca
            .iter()
            .filter_map(|model| Some((json_str(model, "model_name")?, model)))
            .collect();
        self.trace
            .iter()
            .filter_map(|(name, trace)| {
                let band = agreement_band(trace)?;
                speed_record(
                    name,
                    band == Band::High,
                    rumoca_by_name.get(name.as_str())?,
                    self.omc.get(name)?,
                )
            })
            .collect()
    }
}

#[derive(PartialEq, Eq)]
enum Band {
    High,
    Near,
}

/// The model band of an agreeing trace comparison (the comparator's
/// thresholds); `None` when the traces do not agree.
fn agreement_band(trace: &Value) -> Option<Band> {
    let high = json_f64(trace, "channel_high_percent").unwrap_or(0.0);
    let near = json_f64(trace, "channel_minor_percent").unwrap_or(0.0);
    let deviation = json_f64(trace, "channel_deviation_percent").unwrap_or(1.0);
    if high >= 0.80 && deviation <= 0.01 {
        Some(Band::High)
    } else if high + near >= 0.90 && deviation <= 0.10 {
        Some(Band::Near)
    } else {
        None
    }
}

fn speed_record(name: &str, high: bool, rumoca: &Value, omc: &Value) -> Option<SpeedRecord> {
    if json_str(omc, "status")? != "success" {
        return None;
    }
    let seconds = |key| json_f64(rumoca, key).unwrap_or(0.0);
    let rumoca_seconds = RumocaSeconds {
        instantiate: seconds("instantiate_seconds"),
        typecheck: seconds("typecheck_seconds"),
        flatten: seconds("flatten_seconds"),
        dae: seconds("dae_seconds"),
        front_end: positive_json_f64(rumoca, "compile_seconds")?,
        solve: seconds("ir_solve_seconds"),
        jit: seconds("sim_backend_build_seconds"),
        sim_build: nonnegative_json_f64(rumoca, "sim_build_seconds")?,
        run: positive_json_f64(rumoca, "sim_run_seconds")?,
    };
    let total = positive_json_f64(omc, "total_system_seconds")?;
    let simulation = positive_json_f64(omc, "sim_system_seconds")?;
    let omc_seconds = OmcSeconds {
        compiler_work: omc.get("omc_phases").and_then(omc_compiler_work),
        runnable: Some(total - simulation).filter(|seconds| *seconds > 0.0)?,
        simulation,
        total,
    };
    Some(SpeedRecord {
        model_name: name.to_string(),
        scalar_equations: json_u64(rumoca, "scalar_equations")?,
        high,
        rumoca: rumoca_seconds,
        omc: omc_seconds,
        rumoca_settings: rumoca.get("sim_settings").cloned(),
        omc_settings: omc.get("omc_settings").cloned(),
    })
}

/// OMC's frontend + backend + SimCode + templates, when all four are known.
fn omc_compiler_work(phases: &Value) -> Option<f64> {
    ["frontend", "backend", "sim_code", "templates"]
        .into_iter()
        .map(|key| json_f64(phases, key))
        .sum::<Option<f64>>()
        .filter(|seconds| *seconds > 0.0)
}

/// Throughput and median per-model speedup of `comparison` over `records`.
fn speedups(
    records: &[SpeedRecord],
    comparison: Comparison,
) -> Option<(usize, f64, f64, f64, f64)> {
    let pairs: Vec<(f64, f64)> = records.iter().filter_map(|r| comparison.pair(r)).collect();
    if pairs.is_empty() {
        return None;
    }
    let rumoca: f64 = pairs.iter().map(|pair| pair.0).sum();
    let omc: f64 = pairs.iter().map(|pair| pair.1).sum();
    let median = median_value(pairs.iter().map(|(rumoca, omc)| omc / rumoca));
    Some((pairs.len(), rumoca, omc, omc / rumoca, median))
}

fn render_aggregate_table(records: &[SpeedRecord]) -> String {
    let mut out = String::from(
        "| Comparison | Models | Rumoca total (s) | OMC total (s) | Throughput speedup (×) | Median per-model speedup (×) |\n\
         |---|--:|--:|--:|--:|--:|\n",
    );
    for comparison in COMPARISONS {
        let row = match speedups(records, comparison) {
            Some((models, rumoca, omc, throughput, median)) => format!(
                "| {} | {models} | {rumoca:.1} | {omc:.1} | **{throughput:.2}** | **{median:.2}** |\n",
                comparison.label()
            ),
            None => format!(
                "| {} | 0 | - | - | not measured | not measured |\n",
                comparison.label()
            ),
        };
        out.push_str(&row);
    }
    out.push('\n');
    for comparison in COMPARISONS {
        out.push_str(&format!(
            "- **{}**: {}\n",
            comparison.label(),
            comparison.definition()
        ));
    }
    out
}

fn render_high_only_line(records: &[SpeedRecord]) -> String {
    let high: Vec<SpeedRecord> = records.iter().filter(|r| r.high).cloned().collect();
    let cells: Vec<String> = COMPARISONS
        .into_iter()
        .map(|comparison| match speedups(&high, comparison) {
            Some((_, _, _, _, median)) => format!("{} {median:.2}×", comparison.label()),
            None => format!("{} not measured", comparison.label()),
        })
        .collect();
    format!(
        "\nHigh band only ({} models), median per-model speedup: {}.\n",
        high.len(),
        cells.join("; ")
    )
}

fn render_size_table(records: &[SpeedRecord], comparison: Comparison) -> String {
    let mut out = format!(
        "##### {}\n\n{}\n\n| Scalar eqns | Models | Rumoca median (s) | OMC median (s) | Median speedup (×) |\n|---|--:|--:|--:|--:|\n",
        comparison.label(),
        comparison.definition()
    );
    for (label, bin) in speed_bins(records) {
        let pairs: Vec<(f64, f64)> = bin.iter().filter_map(|r| comparison.pair(r)).collect();
        if pairs.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "| {label} | {} | {:.4} | {:.4} | **{:.2}** |\n",
            pairs.len(),
            median_value(pairs.iter().map(|pair| pair.0)),
            median_value(pairs.iter().map(|pair| pair.1)),
            median_value(pairs.iter().map(|(rumoca, omc)| omc / rumoca)),
        ));
    }
    out
}

fn render_slower_than_omc(records: &[SpeedRecord]) -> String {
    let mut out = String::from(
        "<details>\n<summary><strong>Slower than OMC, per comparison</strong></summary>\n",
    );
    for comparison in COMPARISONS {
        let mut slower: Vec<(&SpeedRecord, f64)> = records
            .iter()
            .filter_map(|record| {
                let (rumoca, omc) = comparison.pair(record)?;
                (omc < rumoca).then_some((record, omc / rumoca))
            })
            .collect();
        slower.sort_by(|left, right| left.1.total_cmp(&right.1));
        out.push_str(&format!(
            "\n##### {} ({} slower)\n\n",
            comparison.label(),
            slower.len()
        ));
        if slower.is_empty() {
            out.push_str("_None._\n");
            continue;
        }
        out.push_str(
            "| Model | Scalar eqns | Speedup (×) | Dominant rumoca cost |\n|---|--:|--:|---|\n",
        );
        for (record, speedup) in slower.into_iter().take(SLOWER_MODEL_LIMIT) {
            out.push_str(&format!(
                "| `{}` | {} | {speedup:.2} | {} |\n",
                record.model_name,
                record.scalar_equations,
                comparison.dominant_cost(record)
            ));
        }
    }
    out.push_str("\n</details>\n");
    out
}

/// The recorded conditions of both tools' runs.
fn render_methodology(inputs: &SpeedInputs, records: &[SpeedRecord]) -> String {
    let timing = &inputs.omc_timing;
    let count = |key| json_u64(timing, key).map_or("not recorded".to_string(), |v| v.to_string());
    let host = timing.get("host");
    let host_field = |key| {
        host.and_then(|host| host.get(key))
            .filter(|value| !value.is_null())
            .map_or("not recorded".to_string(), |value| {
                value
                    .as_str()
                    .map_or_else(|| value.to_string(), str::to_string)
            })
    };
    let omc_workers = json_u64(timing, "workers_used");
    let rumoca_workers = json_u64(timing, "rumoca_sim_workers");
    let contention = match (omc_workers, rumoca_workers) {
        (Some(omc), Some(rumoca)) if omc == rumoca => "equal on both tools".to_string(),
        (Some(omc), Some(rumoca)) if omc > rumoca => format!(
            "**unequal**: OMC ran {omc} simulations at a time against rumoca's {rumoca}, which favours rumoca"
        ),
        (Some(omc), Some(rumoca)) => format!(
            "**unequal**: rumoca ran {rumoca} simulations at a time against OMC's {omc}, which favours OMC"
        ),
        _ => "**not comparable**: a worker count was not recorded".to_string(),
    };
    let ran = json_u64(timing, "batches_ran").unwrap_or(0);
    let total = json_u64(timing, "batches_total").unwrap_or(0);
    format!(
        "\n<details>\n<summary><strong>Methodology</strong></summary>\n\n\
         - Runner: image {}, environment {}, {} logical CPUs, {} physical cores; {} shard(s).\n\
         - Parallelism per shard: rumoca compile stage {}, rumoca simulations {}, OMC reference \
         simulations {} (OMC threads {}); simulation contention {contention}.\n\
         - Solver and tolerance: rumoca {}; OMC {}.\n\
         - Output density: rumoca {}; OMC {}.\n\
         - Cache: OMC references ran for {ran} of {total} models this run ({} reused from cache); \
         rumoca compiled and simulated every model this run.\n\
         - Front end scope: rumoca's `compile_seconds` covers resolving each model's reachable \
         library classes again for that model; OMC's `timeFrontend` follows one `loadModel` per \
         session, which no per-model OMC timer includes.\n\
         - Parity gating: only models whose traces agree with OMC (high or near band) are timed; \
         {} models.\n\n</details>\n",
        host_field("image"),
        host_field("runner_environment"),
        host_field("logical_cpus"),
        host_field("physical_cores"),
        count("shards"),
        count("rumoca_stage_workers"),
        count("rumoca_sim_workers"),
        count("workers_used"),
        count("omc_threads"),
        setting_summary(records, true, &["solver", "rtol"]),
        setting_summary(records, false, &["method", "tolerance"]),
        setting_summary(records, true, &["output_points"]),
        setting_summary(records, false, &["number_of_intervals"]),
        total.saturating_sub(ran),
        records.len(),
    )
}

/// The most common value of each `keys` setting across `records`, with its
/// share, for rumoca's rows (`rumoca`) or OMC's.
fn setting_summary(records: &[SpeedRecord], rumoca: bool, keys: &[&str]) -> String {
    keys.iter()
        .map(|key| {
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            for record in records {
                let settings = if rumoca {
                    record.rumoca_settings.as_ref()
                } else {
                    record.omc_settings.as_ref()
                };
                let value = settings
                    .and_then(|settings| settings.get(*key))
                    .filter(|value| !value.is_null())
                    .map_or("not recorded".to_string(), |value| {
                        value
                            .as_str()
                            .map_or_else(|| value.to_string(), str::to_string)
                    });
                *counts.entry(value).or_default() += 1;
            }
            let (value, count) = counts
                .into_iter()
                .max_by_key(|(_, count)| *count)
                .unwrap_or_else(|| ("not recorded".to_string(), 0));
            format!("{key} {value} ({count}/{})", records.len())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn speed_bins(records: &[SpeedRecord]) -> Vec<(String, Vec<&SpeedRecord>)> {
    SIZE_BINS
        .iter()
        .filter_map(|(low, high)| {
            let in_bin = records
                .iter()
                .filter(|record| {
                    record.scalar_equations >= *low
                        && high.is_none_or(|high| record.scalar_equations <= high)
                })
                .collect::<Vec<_>>();
            let label = high.map_or_else(|| format!("{low}+"), |high| format!("{low}–{high}"));
            (!in_bin.is_empty()).then_some((label, in_bin))
        })
        .collect()
}

fn read_json_file(path: &Path) -> Result<Value> {
    serde_json::from_str(
        &fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))
}

fn median_value(values: impl IntoIterator<Item = f64>) -> f64 {
    let mut values: Vec<f64> = values.into_iter().filter(|v| v.is_finite()).collect();
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    }
}

#[cfg(test)]
mod tests;
