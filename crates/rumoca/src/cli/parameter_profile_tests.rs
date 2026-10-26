use super::*;

const SOURCE: &str = "model Parameters\n  parameter Real declared = 2;\n  parameter Real solved(fixed=false);\n  Real x;\ninitial equation\n  solved = 3;\nequation\n  x = declared + solved;\nend Parameters;\n";

#[test]
fn explicit_freeze_profile_reaches_file_and_in_memory_compilation() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("Parameters.mo");
    std::fs::write(&file, SOURCE).unwrap();
    for freeze in [false, true] {
        let mut arguments = vec![
            "rumoca",
            "compile",
            file.to_str().unwrap(),
            "--emit",
            "flat-json",
        ];
        if freeze {
            arguments.push("--freeze-parameters");
        }
        let cli = Cli::try_parse_from(arguments).unwrap();
        let Commands::Compile(args) = cli.command else {
            panic!("expected compile command");
        };
        let (artifact, _) =
            compile_early_ir_with_inferred_model(&args.input, CompilePhase::Flat, false).unwrap();
        let EarlyIrArtifact::Flat(flat) = artifact else {
            panic!("expected flat artifact");
        };
        let file_value = serde_json::to_value(flat).unwrap();
        let memory_value = compile_to_value(&args, SOURCE).unwrap();
        for value in [&file_value, &memory_value] {
            assert_eq!(value["variables"]["declared"]["evaluate"], freeze);
            assert_eq!(value["variables"]["solved"]["evaluate"], false);
        }
    }
}

#[test]
fn only_explicit_frozen_bindings_specialize_pure_calls() {
    const SOURCE: &str = "function iterate
      input Real seed; output Real y;
    algorithm
      y := seed;
      while y < 3 loop y := y + 1; end while;
    end iterate;
    model Frozen
      parameter Real seed = 1;
      Real actual = iterate(seed);
    end Frozen;";
    for freeze in [false, true] {
        let mut arguments = vec![
            "rumoca",
            "compile",
            "Frozen.mo",
            "--model",
            "Frozen",
            "--emit",
            "flat-json",
        ];
        if freeze {
            arguments.push("--freeze-parameters");
        }
        let cli = Cli::try_parse_from(arguments).unwrap();
        let Commands::Compile(args) = cli.command else {
            unreachable!()
        };
        let flat = compile_to_value(&args, SOURCE).unwrap();
        let binding = &flat["variables"]["actual"]["binding"];
        if freeze {
            let literal = &binding["Literal"]["value"];
            assert!(
                literal["Real"] == 3.0 || literal["Integer"] == 3,
                "{binding}"
            );
            assert!(flat["functions"].as_object().unwrap().is_empty());
        } else {
            assert!(binding.get("FunctionCall").is_some());
            assert!(!flat["functions"].as_object().unwrap().is_empty());
        }
    }
}

#[test]
fn frozen_profile_does_not_guess_initialization_or_runtime_inputs() {
    const SOURCE: &str = "function echo input Real x; output Real y;
      algorithm y := x; end echo;
    model Unsettled
      parameter Real seed(fixed=false);
      input Real u;
      Real a = echo(seed);
      Real b = echo(u);
    initial equation seed=2;
    end Unsettled;";
    let cli = Cli::try_parse_from([
        "rumoca",
        "compile",
        "Unsettled.mo",
        "--model",
        "Unsettled",
        "--emit",
        "flat-json",
        "--freeze-parameters",
    ])
    .unwrap();
    let Commands::Compile(args) = cli.command else {
        unreachable!()
    };
    let flat = compile_to_value(&args, SOURCE).unwrap();
    for name in ["a", "b"] {
        assert!(
            flat["variables"][name]["binding"]
                .get("FunctionCall")
                .is_some()
        );
    }
}

#[test]
#[cfg(feature = "scheduled-sim")]
fn config_simulation_cannot_silently_ignore_the_freeze_profile() {
    let cli = Cli::try_parse_from([
        "rumoca",
        "sim",
        "--config",
        "unused.toml",
        "--freeze-parameters",
    ])
    .unwrap();
    let error = run(cli).expect_err("config compilation does not carry this profile");
    assert!(
        error
            .to_string()
            .contains("requires direct model-file input")
    );
}
