//! `compile-bitcode --simulate --t-start`: the window a model's
//! `experiment(StartTime=...)` names.
//!
//! Some validation models are only defined after their start time -- IBPSA's
//! `ExponentialIntegralE1` evaluates `E1(time)` from `StartTime = 0.01`, and
//! `E1(0)` is infinite -- so running every model from 0 reports failures that
//! are the harness's, not the model's.

use std::fs;
use std::process::{Command, Output};

use tempfile::tempdir;

const FIXTURE: &str = "\
model LogFromStart
  Real y;
equation
  y = log(time);
end LogFromStart;
";

fn rumoca(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumoca"))
        .args(args)
        .output()
        .expect("rumoca runs")
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn check(artifact: &str, window: &[&str]) -> Output {
    let mut args = vec!["compile-bitcode", artifact, "--simulate", "--check"];
    args.extend_from_slice(window);
    rumoca(&args)
}

#[test]
fn a_model_defined_only_after_its_start_time_runs_from_that_start() {
    let work = tempdir().expect("temp dir");
    let source = work.path().join("LogFromStart.mo");
    fs::write(&source, FIXTURE).expect("fixture written");
    let artifact = work.path().join("LogFromStart.rbc");
    let artifact = artifact.to_str().expect("utf-8 path");
    let emit = rumoca(&[
        "compile",
        source.to_str().expect("utf-8 path"),
        "--model",
        "LogFromStart",
        "--emit-bitcode",
        artifact,
    ]);
    assert!(emit.status.success(), "{}", text(&emit));

    let from_zero = check(artifact, &["--t-end", "1"]);
    assert!(
        text(&from_zero).contains("non-finite"),
        "log(0) must be reported: {}",
        text(&from_zero)
    );

    let from_start = check(artifact, &["--t-start", "0.5", "--t-end", "1"]);
    assert!(from_start.status.success(), "{}", text(&from_start));
    assert_eq!(
        String::from_utf8_lossy(&from_start.stdout).trim(),
        "[]",
        "{}",
        text(&from_start)
    );

    let reversed = check(artifact, &["--t-start", "1", "--t-end", "0.5"]);
    assert!(!reversed.status.success(), "{}", text(&reversed));
    assert!(text(&reversed).contains("must be after"), "{}", text(&reversed));
}
