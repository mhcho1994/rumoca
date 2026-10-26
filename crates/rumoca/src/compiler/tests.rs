use super::*;
use tempfile::tempdir;

#[test]
fn test_simple_model() {
    let source = r#"
        model Test
            Real x(start=0);
        equation
            der(x) = 1;
        end Test;
    "#;

    let result = Compiler::new().model("Test").compile_str(source, "test.mo");

    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
    let result = result.unwrap();
    assert_eq!(
        result.dae.inspect(|view| {
            view.variables()
                .filter(|(_, variable)| variable.role() == VariableRole::State)
                .count()
        }),
        1
    );
}

#[test]
fn test_dae_only_compile_returns_dae_without_full_artifacts() {
    let source = r#"
        model Test
            Real x(start=0);
        equation
            der(x) = 1;
        end Test;
    "#;

    let result = Compiler::new()
        .model("Test")
        .compile_str_dae(source, "test.mo")
        .expect("DAE-only compile should succeed");

    assert_eq!(
        result.dae.inspect(|view| {
            view.variables()
                .filter(|(_, variable)| variable.role() == VariableRole::State)
                .count()
        }),
        1
    );
    assert_eq!(result.balance_detail.state_unknowns, 1);
}

#[test]
fn test_dae_only_compile_preserves_structured_parse_diagnostics() {
    let err = Compiler::new()
        .model("Broken")
        .compile_str_dae("model Broken\n  Real x\nend Broken;\n", "Broken.mo")
        .expect_err("broken active document must fail DAE-only strict compile");

    match err {
        CompilerError::CompileDiagnosticsError {
            failures,
            source_map,
            ..
        } => {
            assert!(
                source_map.is_some(),
                "DAE-only compile must preserve the source map for CLI diagnostics"
            );
            assert!(
                failures
                    .iter()
                    .any(|failure| failure.error_code.as_deref() == Some("EP001")
                        && failure.primary_label.is_some()),
                "DAE-only compile must preserve structured parse failures: {failures:?}"
            );
        }
        other => panic!("expected structured compile diagnostics, got {other:?}"),
    }
}

#[test]
fn test_typecheck_failure_carries_spanned_diagnostics_to_the_cli() {
    // Phase failures must reach the CLI as structured diagnostics with
    // the error's real source label, not a stringified summary anchored
    // at the class header ("phase failed").
    let source = r#"
        model BadDim
            constant Integer n = -1;
            Real x[n];
        equation
            der(x[1]) = 1;
        end BadDim;
    "#;

    let err = Compiler::new()
        .model("BadDim")
        .compile_str(source, "BadDim.mo")
        .expect_err("unevaluable dimensions must fail typecheck");

    match err {
        CompilerError::CompileDiagnosticsError {
            failures,
            source_map,
            ..
        } => {
            assert!(source_map.is_some(), "source map required for rendering");
            let failure = failures
                .iter()
                .find(|failure| failure.error_code.as_deref() == Some("ET004"))
                .unwrap_or_else(|| panic!("expected an ET004 failure: {failures:?}"));
            let label = failure
                .primary_label
                .as_ref()
                .expect("typecheck failure must carry its own source label");
            assert_ne!(
                label.message.as_deref(),
                Some("phase failed"),
                "label must be the diagnostic's own span, not the class-header fallback"
            );
        }
        other => panic!("expected structured compile diagnostics, got {other:?}"),
    }
}

#[test]
fn test_resolve_failure_does_not_cascade_into_later_phases() {
    // One user error must produce one diagnostic: an unresolved reference
    // is reported by resolve (ER002) and must not be re-reported by
    // flatten/ToDae (ED008 etc.) after the pipeline runs on anyway.
    let source = r#"
        model Cascade
            Real x;
        equation
            x = y + 1;
        end Cascade;
    "#;

    let err = Compiler::new()
        .model("Cascade")
        .compile_str(source, "Cascade.mo")
        .expect_err("unresolved reference must fail compile");

    match err {
        CompilerError::CompileDiagnosticsError { failures, .. } => {
            assert!(
                failures
                    .iter()
                    .any(|failure| failure.error_code.as_deref() == Some("ER002")),
                "resolve failure must be reported: {failures:?}"
            );
            assert!(
                failures.iter().all(|failure| failure.phase.is_none()),
                "no later phase may run (and re-report) after a resolve \
                 failure in the target's files: {failures:?}"
            );
        }
        other => panic!("expected structured compile diagnostics, got {other:?}"),
    }
}

#[test]
fn test_compile_file_tolerates_path_spelling_variants() {
    // The directory scan yields the requested file under its own
    // spelling; a different but equivalent argument spelling (`./`
    // segment, bare relative name) must not register the file twice and
    // report EM001 "duplicate class" against itself.
    let temp = tempdir().expect("tempdir");
    let solo = temp.path().join("Solo.mo");
    fs::write(
        &solo,
        r#"
        model Solo
            Real x(start=0, fixed=true);
        equation
            der(x) = 1;
        end Solo;
        "#,
    )
    .expect("write solo");

    let dotted = format!("{}/./Solo.mo", temp.path().display());
    let result = Compiler::new().model("Solo").compile_file(&dotted);
    assert!(
        result.is_ok(),
        "equivalent path spelling must not self-duplicate: {:?}",
        result.err()
    );
}

#[test]
fn test_compile_file_skips_unreadable_sibling_sources() {
    // A non-UTF-8 sibling the user never referenced must not poison the
    // compile of an unrelated file in the same directory.
    let temp = tempdir().expect("tempdir");
    let solo = temp.path().join("Solo.mo");
    fs::write(
        &solo,
        r#"
        model Solo
            Real x(start=0, fixed=true);
        equation
            der(x) = 1;
        end Solo;
        "#,
    )
    .expect("write solo");
    fs::write(temp.path().join("junk.mo"), [0xff, 0xfe, 0xff]).expect("write junk");

    let result = Compiler::new()
        .model("Solo")
        .compile_file(&solo.to_string_lossy());
    assert!(
        result.is_ok(),
        "unreadable sibling must be skipped, not fatal: {:?}",
        result.err()
    );
}

#[test]
fn test_compile_file_loads_same_directory_siblings() {
    let temp = tempdir().expect("tempdir");
    let helper = temp.path().join("Helper.mo");
    let root = temp.path().join("Root.mo");
    fs::write(
        &helper,
        r#"
        model Helper
            Real x(start=0);
        equation
            der(x) = 1;
        end Helper;
        "#,
    )
    .expect("write helper");
    fs::write(
        &root,
        r#"
        model Root
            Helper h;
        end Root;
        "#,
    )
    .expect("write root");

    let result = Compiler::new()
        .model("Root")
        .compile_file(&root.to_string_lossy());

    assert!(
        result.is_ok(),
        "sibling files in the same directory must be part of the compile unit: {:?}",
        result.err()
    );
}

#[test]
fn test_compile_file_ignores_unrelated_sibling_parse_error() {
    let temp = tempdir().expect("tempdir");
    let helper = temp.path().join("Helper.mo");
    let broken = temp.path().join("Broken.mo");
    let root = temp.path().join("Root.mo");
    fs::write(
        &helper,
        r#"
        model Helper
            Real x(start=0);
        equation
            der(x) = 1;
        end Helper;
        "#,
    )
    .expect("write helper");
    fs::write(&broken, "model Broken\n  Real x\nend Broken;\n").expect("write broken");
    fs::write(
        &root,
        r#"
        model Root
            Helper h;
        end Root;
        "#,
    )
    .expect("write root");

    let result = Compiler::new()
        .model("Root")
        .compile_file(&root.to_string_lossy())
        .expect("strict target compile must ignore unrelated sibling parse errors");
    let index = rumoca_ir_ast::ClassDefIndex::from_tree(result.resolved.inner());
    assert!(
        index.get_by_qualified_name("Root").is_some(),
        "the compilation must carry the requested model's Resolve proof"
    );
    assert!(
        index.get_by_qualified_name("Helper").is_some(),
        "the Resolve proof must contain the requested dependency"
    );
    assert!(
        index.get_by_qualified_name("Broken").is_none(),
        "the Resolve proof must be the exact target closure, not the recovered global plan"
    );
}

#[test]
fn test_compile_file_reports_required_sibling_parse_error() {
    let temp = tempdir().expect("tempdir");
    let helper = temp.path().join("Helper.mo");
    let root = temp.path().join("Root.mo");
    fs::write(&helper, "model Helper\n  Real x\nend Helper;\n").expect("write helper");
    fs::write(
        &root,
        r#"
        model Root
            Helper h;
        end Root;
        "#,
    )
    .expect("write root");

    let err = Compiler::new()
        .model("Root")
        .compile_file(&root.to_string_lossy())
        .expect_err("required broken sibling must fail strict compile");
    let message = err.to_string();
    // The compile unit is collected under canonical paths, so the error
    // names the helper in canonical spelling; a raw tempdir spelling can
    // differ (Windows 8.3 short names, macOS /tmp symlink).
    let helper_canonical = helper.canonicalize().expect("canonicalize helper");
    assert!(
        message.contains(&helper_canonical.to_string_lossy().to_string()),
        "original helper parse error should be surfaced: {message}"
    );
    assert!(
        !message.contains("unresolved type reference"),
        "required broken sibling must not degrade into unresolved type errors: {message}"
    );
}

#[test]
fn test_compile_file_reports_active_parse_error_via_compile_diagnostics() {
    let temp = tempdir().expect("tempdir");
    let broken = temp.path().join("Broken.mo");
    fs::write(&broken, "model Broken\n  Real x\nend Broken;\n").expect("write broken");

    let err = Compiler::new()
        .model("Broken")
        .compile_file(&broken.to_string_lossy())
        .expect_err("broken active document must fail strict compile");
    match err {
        CompilerError::CompileDiagnosticsError { failures, .. } => {
            assert!(
                failures
                    .iter()
                    .any(|failure| failure.error_code.as_deref() == Some("EP001")),
                "active parse errors must surface as structured parse diagnostics: {failures:?}"
            );
        }
        other => panic!("expected structured compile diagnostics, got {other:?}"),
    }
}

#[test]
fn test_compile_file_loads_enclosing_package_tree() {
    let temp = tempdir().expect("tempdir");
    let pkg = temp.path().join("Pkg");
    let sub = pkg.join("Sub");
    fs::create_dir_all(&sub).expect("mkdir");
    fs::write(pkg.join("package.mo"), "package Pkg end Pkg;").expect("write package");
    fs::write(sub.join("package.mo"), "within Pkg; package Sub end Sub;")
        .expect("write sub package");
    fs::write(
        sub.join("Helper.mo"),
        r#"
        within Pkg.Sub;
        model Helper
            Real x(start=0);
        equation
            der(x) = 1;
        end Helper;
        "#,
    )
    .expect("write helper");
    let root = sub.join("Root.mo");
    fs::write(
        &root,
        r#"
        within Pkg.Sub;
        model Root
            Helper h;
        end Root;
        "#,
    )
    .expect("write root");
    fs::write(
        temp.path().join("Unrelated.mo"),
        "model Unrelated end Unrelated;",
    )
    .expect("write unrelated");

    let result = Compiler::new()
        .model("Pkg.Sub.Root")
        .compile_file(&root.to_string_lossy());

    assert!(
        result.is_ok(),
        "compile unit must include the enclosing package tree without unrelated parents: {:?}",
        result.err()
    );
}

/// TOOLBUG-105: a library `package.mo` compiled as the main file never
/// names the other library its classes use (ThermoSysPro has no `uses`
/// annotation), so that root must be selected through the files of the
/// roots that were loaded.
#[test]
fn test_compile_package_file_loads_roots_its_classes_use() {
    let temp = tempdir().expect("tempdir");
    let lib = temp.path().join("lib");
    let dep = temp.path().join("dep");
    fs::create_dir_all(lib.join("L")).expect("mkdir lib");
    fs::create_dir_all(dep.join("Mod")).expect("mkdir dep");
    fs::write(lib.join("L/package.mo"), "package L\nend L;\n").expect("write L");
    fs::write(
        lib.join("L/T.mo"),
        "within L;\nmodel T\n  parameter Real a = Mod.C.eps;\n  Real x;\nequation\n  x = a*time;\nend T;\n",
    )
    .expect("write T");
    fs::write(dep.join("Mod/package.mo"), "package Mod\nend Mod;\n").expect("write Mod");
    fs::write(
        dep.join("Mod/C.mo"),
        "within Mod;\npackage C\n  final constant Real eps = 1e-15;\nend C;\n",
    )
    .expect("write C");

    let result = Compiler::new()
        .model("L.T")
        .source_root(&dep.to_string_lossy())
        .source_root(&lib.to_string_lossy())
        .compile_file(&lib.join("L/package.mo").to_string_lossy());

    assert!(
        result.is_ok(),
        "a root used only by the loaded library's classes must be loaded: {:?}",
        result.err()
    );
}

/// Write a source root holding `Pkg` with a model, its dependency, and an
/// unrelated function whose missing purity prefix is a warning.
fn write_source_root_library(root: &Path) -> std::path::PathBuf {
    let sub = root.join("Pkg").join("Sub");
    fs::create_dir_all(&sub).expect("mkdir");
    fs::write(root.join("Pkg").join("package.mo"), "package Pkg end Pkg;").expect("write package");
    fs::write(sub.join("package.mo"), "within Pkg; package Sub end Sub;")
        .expect("write sub package");
    fs::write(
        root.join("Pkg").join("Ext.mo"),
        "within Pkg;\nfunction Ext\n  input Real x;\n  output Real y;\n  external \"C\" y = ext(x);\nend Ext;\n",
    )
    .expect("write unrelated function");
    fs::write(
        sub.join("Helper.mo"),
        "within Pkg.Sub;\nmodel Helper\n  Real x(start=0);\nequation\n  der(x) = 1;\nend Helper;\n",
    )
    .expect("write helper");
    let model = sub.join("Root.mo");
    fs::write(
        &model,
        "within Pkg.Sub;\nmodel Root\n  Helper h;\n  Real y;\nequation\n  y = 2*h.x;\nend Root;\n",
    )
    .expect("write root");
    model
}

fn loaded_session(compiler: &Compiler, source: &str, file_name: &str) -> Session {
    let mut session = Session::new(SessionConfig::default());
    compiler
        .load_required_source_roots(&mut session, source)
        .expect("load source roots");
    compiler
        .load_local_compile_unit(&mut session, source, file_name)
        .expect("load compile unit");
    session
}

/// SPEC_0040 DAE-C22: only a parameter guard over structurally different
/// branches is selected at translation, and it is reported once, at its
/// equation, naming the parameter that can no longer be set.
#[test]
fn a_structural_parameter_selection_is_reported_at_its_equation() {
    let source = "model GuardKinds\n  parameter Boolean on = true;\n  \
                  parameter Boolean useB = true;\n  Real x(start = 1, fixed = true);\n  \
                  Real b;\n  Real y;\n  Real z;\nequation\n  der(x) = -x;\n  b = 2 * x;\n  \
                  y = if on then x else 3 * x;\n  z = if useB then x else b;\nend GuardKinds;\n";
    let compiler = Compiler::new().model("GuardKinds");
    let mut session = loaded_session(&compiler, source, "GuardKinds.mo");
    let warnings = rendered_warnings(&mut session, "GuardKinds")
        .into_iter()
        .filter(|warning| warning.contains("WD001"))
        .collect::<Vec<_>>();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    // The label is the `z = if useB ...` equation: zero-based line 11.
    assert!(
        warnings[0].contains("parameter useB cannot be set")
            && warnings[0].contains("GuardKinds.mo:11"),
        "{warnings:?}"
    );
}

/// SPEC_0040 DAE-C22: a call argument a declared output dimension of the
/// callee reads fixes the result shape at translation, so it is reported
/// once, at the call, naming the parameter that can no longer be set.
#[test]
fn a_keyed_function_argument_parameter_is_reported_at_its_call() {
    let source = "function ramp\n  input Integer n;\n  output Real y[n];\nalgorithm\n  \
                  y := zeros(n);\n  for i in 1:n loop\n    y[i] := i;\n  end for;\nend ramp;\n\
                  model Keyed\n  parameter Integer n = 3;\n  Real y;\nequation\n  \
                  y = sum(ramp(n));\nend Keyed;\n";
    let compiler = Compiler::new().model("Keyed");
    let mut session = loaded_session(&compiler, source, "Keyed.mo");
    let warnings = rendered_warnings(&mut session, "Keyed")
        .into_iter()
        .filter(|warning| warning.contains("WD001"))
        .collect::<Vec<_>>();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    // The label is the `ramp(n)` call: zero-based line 13.
    assert!(
        warnings[0].contains("parameter n cannot be set") && warnings[0].contains("Keyed.mo:13"),
        "{warnings:?}"
    );
}

fn rendered_warnings(session: &mut Session, model_name: &str) -> Vec<String> {
    let diagnostics = session.compile_model_diagnostics(model_name);
    let source_map = diagnostics.source_map;
    diagnostics
        .diagnostics
        .iter()
        .map(|diagnostic| {
            let location = source_map
                .as_ref()
                .zip(diagnostic.labels.first())
                .and_then(|(map, label)| {
                    rumoca_compile::compile::source_span_location(map, label.span)
                })
                .map(|loc| format!("{}:{}", loc.file_name, loc.start.line));
            format!(
                "{:?} {:?} {} {:?}",
                diagnostic.severity, diagnostic.code, diagnostic.message, location
            )
        })
        .collect()
}

#[test]
fn test_model_file_inside_loaded_source_root_reuses_source_root_documents() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("lib");
    let model_file = write_source_root_library(&root);
    let model_name = "Pkg.Sub.Root";
    let by_name_source = format!("model Probe\n  {model_name} r;\nend Probe;\n");
    let probe_file = root.join("Probe.mo");
    fs::write(&probe_file, &by_name_source).expect("write probe");
    let probe_name = probe_file.to_string_lossy().to_string();
    let root_str = root.to_string_lossy().to_string();
    let compiler = Compiler::new().model(model_name).source_root(&root_str);

    let file_source = fs::read_to_string(&model_file).expect("read model");
    let file_name = model_file.to_string_lossy().to_string();
    let mut by_file = loaded_session(&compiler, &file_source, &file_name);
    let mut by_name = loaded_session(&compiler, &by_name_source, &probe_name);
    assert_eq!(by_file.document_uris(), by_name.document_uris());
    assert_eq!(by_file.document_uris().len(), 6);

    // Every library file stays the source root's parsed document with no
    // live text overlay: none was re-read and re-parsed as a workspace
    // document of the compile unit.
    for file in [
        "Pkg/package.mo",
        "Pkg/Ext.mo",
        "Pkg/Sub/package.mo",
        "Pkg/Sub/Helper.mo",
        "Pkg/Sub/Root.mo",
        "Probe.mo",
    ] {
        // Documents are keyed by the path the source-root walk produced
        // under the root as given: native separators, no canonicalization
        // (a canonical key differs on macOS, where the temporary root is a
        // symlink, and on Windows, where it carries the verbatim prefix).
        let uri = file
            .split('/')
            .fold(root.clone(), |path, component| path.join(component))
            .to_string_lossy()
            .to_string();
        assert_eq!(
            by_file
                .get_document(&uri)
                .expect("library document")
                .parsed(),
            by_name
                .get_document(&uri)
                .expect("library document")
                .parsed()
        );
        for session in [&by_file, &by_name] {
            assert!(
                session.is_source_root_backed_document(&uri),
                "{uri} must remain a source-root document"
            );
            let document = session.get_document(&uri).expect("library document");
            assert!(
                document.content.is_empty(),
                "{uri} must not be re-parsed as a workspace document"
            );
            assert_eq!(
                session.document_source_text(&uri).expect("source").as_ref(),
                fs::read_to_string(&uri).expect("read source")
            );
        }
    }

    assert_eq!(
        rendered_warnings(&mut by_file, model_name),
        rendered_warnings(&mut by_name, model_name),
        "a model file inside a loaded source root reports the by-name diagnostics"
    );

    // SPEC_0001: declaration ids belong to a compilation unit. Both paths
    // use the same source inventory, so the complete storage must match.
    let dae_storage = |json: String| {
        let value: serde_json::Value = serde_json::from_str(&json).expect("DAE JSON");
        value["storage"].clone()
    };
    let from_file = compiler
        .compile_file(&file_name)
        .expect("compile model file")
        .to_json()
        .expect("json");
    let from_name = compiler
        .compile_str(&by_name_source, &probe_name)
        .expect("compile by name")
        .to_json()
        .expect("json");
    assert_eq!(dae_storage(from_file), dae_storage(from_name));
}

#[test]
fn test_edited_model_file_inside_source_root_compiles_the_given_text() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("lib");
    let model_file = write_source_root_library(&root);
    let compiler = Compiler::new()
        .model("Pkg.Sub.Root")
        .source_root(&root.to_string_lossy());
    let edited = fs::read_to_string(&model_file)
        .expect("read model")
        .replace("y = 2*h.x;", "y = 7*h.x;");
    let file_name = model_file.to_string_lossy().to_string();

    let session = loaded_session(&compiler, &edited, &file_name);
    let document = session.get_document(&file_name).expect("model document");
    assert_eq!(
        document.content.as_ref(),
        edited,
        "text that differs from the file must be compiled as given"
    );
    let json = compiler
        .compile_str(&edited, &file_name)
        .expect("compile edited text")
        .to_json()
        .expect("json");
    assert!(
        json.contains("\"integer\": 7"),
        "edited equation must reach the DAE: {json}"
    );
}

#[test]
fn test_no_model_specified() {
    let source = "model Test end Test;";
    let result = Compiler::new().compile_str(source, "test.mo");
    assert!(matches!(result, Err(CompilerError::NoModelSpecified)));
}

#[test]
fn test_to_json() {
    let source = r#"
        model Test
            Real x(start=0);
        equation
            der(x) = 1;
        end Test;
    "#;

    let result = Compiler::new()
        .model("Test")
        .compile_str(source, "test.mo")
        .unwrap();

    let json = result.to_json();
    assert!(json.is_ok());
    let value: serde_json::Value = serde_json::from_str(&json.unwrap()).unwrap();
    let obj = value.as_object().expect("DAE JSON should be an object");
    assert_eq!(
        obj.get("schema_version")
            .and_then(serde_json::Value::as_u64),
        Some(u64::from(rumoca_ir_dae::DAE_SCHEMA_VERSION))
    );
    assert!(obj.contains_key("source_map"));
    let storage = obj
        .get("storage")
        .and_then(serde_json::Value::as_object)
        .expect("wire-v11 DAE must contain private storage records");
    assert_eq!(
        storage
            .get("variables")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len),
        Some(1)
    );
    assert!(
        storage
            .get("continuous_equation_operations")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|operations| {
                operations.len() == 1 && operations[0].get("residual").is_some()
            }),
        "checked continuous ownership must survive the wire projection"
    );
    assert!(
        storage
            .get("expressions")
            .and_then(|expressions| expressions.get("provenance"))
            .and_then(serde_json::Value::as_array)
            .is_some_and(|provenance| !provenance.is_empty()),
        "every checked expression must retain provenance in wire-v11"
    );
    assert!(!obj.contains_key("x"));
    assert!(!obj.contains_key("f_x"));
}

#[test]
fn test_to_json_hybrid_includes_runtime_partitions() {
    let source = r#"
        model Hybrid
            parameter Real k = 1;
            Real x(start=0);
            discrete Real zr(start=0);
            discrete Integer mi(start=0);
        initial equation
            x = 0;
        equation
            der(x) = k;
            when x > 0.5 then
                zr = pre(zr) + 1;
                mi = pre(mi) + 1;
            end when;
        end Hybrid;
    "#;

    let result = Compiler::new()
        .model("Hybrid")
        .compile_str(source, "hybrid.mo")
        .unwrap();

    let value: serde_json::Value = serde_json::from_str(&result.to_json().unwrap()).unwrap();
    let storage = value
        .get("storage")
        .and_then(serde_json::Value::as_object)
        .expect("checked wire-v11 storage");
    assert_eq!(
        storage
            .get("variables")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len),
        Some(4)
    );
    assert!(
        storage
            .get("conditions")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|conditions| !conditions.is_empty())
    );
    assert!(
        storage
            .get("discrete_real_equations")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|equations| equations.len() == 1),
        "the discrete Real when-assignment must remain in B.1b"
    );
    assert!(
        storage
            .get("discrete_value_owners")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|owners| owners.len() == 1),
        "the Integer when-assignment must remain in B.1c"
    );
    assert!(
        storage
            .get("event_actions")
            .and_then(serde_json::Value::as_array)
            .is_some_and(Vec::is_empty),
        "ordinary when-assignments must not become flow event actions"
    );
    assert!(
        storage
            .get("initialization_equation_operations")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|operations| !operations.is_empty())
    );
}

#[test]
fn test_solve_ir_rendering_preserves_tensor_nodes_from_native_dae() {
    let source = r#"
        model TensorTargetDemo
          Real omega[2](start={0, 0});
          parameter Real J[2,2] = [2, 0; 0, 4];
          parameter Real tau[2] = {8, 20};
        equation
          J * der(omega) = tau;
        end TensorTargetDemo;
    "#;

    let result = Compiler::new()
        .model("TensorTargetDemo")
        .compile_str(source, "TensorTargetDemo.mo")
        .expect("tensor target demo should compile");

    let solve_json = result
        .to_ir_json(TemplateIr::Solve)
        .expect("Solve IR JSON should render");
    assert!(
        solve_json.contains("\"LinSolve\""),
        "Solve JSON should preserve the tensor LinSolve node from the native DAE: {solve_json}"
    );

    let rendered = result
        .render_template_str_with_name_and_ir(
            "{{ solve_blocks.continuous.derivative_rhs.tensor_node_count }}",
            "TensorTargetDemo",
            TemplateIr::Solve,
        )
        .expect("Solve template should render");
    assert_eq!(
        rendered.trim(),
        "1",
        "Solve templates should see tensor nodes before scalar fallback"
    );
}

#[test]
fn test_render_template_exposes_orbit_algebraics_from_native_dae() {
    let source = r#"
        model SatelliteOrbit2D
          parameter Real mu = 398600.4418;
          parameter Real r0 = 7000;
          parameter Real v0 = sqrt(mu / r0);
          Real rx(start = r0, fixed = true);
          Real ry(start = 0, fixed = true);
          Real vx(start = 0, fixed = true);
          Real vy(start = v0, fixed = true);
          Real inv_r;
          Real inv_v2;
          Real inv_h;
          Real inv_energy;
          Real inv_a;
          Real inv_rv;
          Real inv_ex;
          Real inv_ey;
          Real inv_ecc;
        equation
          der(rx) = vx;
          der(ry) = vy;
          inv_r = sqrt(rx * rx + ry * ry);
          inv_v2 = vx * vx + vy * vy;
          inv_h = rx * vy - ry * vx;
          inv_energy = 0.5 * inv_v2 - mu / inv_r;
          inv_a = 1 / (2 / inv_r - inv_v2 / mu);
          inv_rv = rx * vx + ry * vy;
          inv_ex = ((inv_v2 - mu / inv_r) * rx - inv_rv * vx) / mu;
          inv_ey = ((inv_v2 - mu / inv_r) * ry - inv_rv * vy) / mu;
          inv_ecc = sqrt(inv_ex * inv_ex + inv_ey * inv_ey);
          der(vx) = -mu * rx / (inv_r ^ 3);
          der(vy) = -mu * ry / (inv_r ^ 3);
        end SatelliteOrbit2D;
    "#;

    let result = Compiler::new()
        .model("SatelliteOrbit2D")
        .compile_str(source, "orbit.mo")
        .expect("compilation should succeed");
    let rendered = result
        .render_template_str(
            "{% for variable in dae.variables %}{% if variable.role == \"algebraic\" %}{{ variable.name }}\n{% endif %}{% endfor %}",
        )
        .expect("template render should succeed");

    for expected in [
        "inv_r",
        "inv_v2",
        "inv_h",
        "inv_energy",
        "inv_a",
        "inv_rv",
        "inv_ex",
        "inv_ey",
        "inv_ecc",
    ] {
        assert!(
            rendered.lines().any(|line| line.trim() == expected),
            "expected algebraic `{expected}` in template output; got:\n{rendered}"
        );
    }
}

#[test]
fn test_dae_template_rendering_does_not_lower_solve_ir() {
    let source = r#"
        model DaeOnlyTemplate
          parameter Real p = 2;
          Real x[2](start = {1, 2});
        equation
          der(x) = {-p * x[1], -p * x[2]};
        end DaeOnlyTemplate;
    "#;

    let result = Compiler::new()
        .model("DaeOnlyTemplate")
        .compile_str(source, "DaeOnlyTemplate.mo")
        .expect("compilation should succeed");
    let rendered = result
        .render_template_str_with_name_and_ir(
            "{{ dae.systems.continuous.owners | length }} {% for variable in dae.variables %}{% if variable.role == \"state\" %}{{ variable.value_type.dimensions | join(\",\") }}{% endif %}{% endfor %}",
            "DaeOnlyTemplate",
            TemplateIr::Dae,
        )
        .expect("DAE template should render from native DAE context");

    assert_eq!(rendered.trim(), "1 2");
}

#[test]
fn non_materialized_family_is_exposed_only_as_a_structured_owner() {
    let source = r#"
        model CascadedDaeTemplate
          constant Integer N = 8;
          Real x[N](each start = 1.0);
        equation
          der(x[1]) = 1.0 - x[1];
          for i in 2:N loop
            der(x[i]) = x[i - 1] - x[i];
          end for;
        end CascadedDaeTemplate;
    "#;
    let result = Compiler::new()
        .model("CascadedDaeTemplate")
        .compile_str(source, "CascadedDaeTemplate.mo")
        .expect("cascaded family should compile");

    let rendered = result
        .render_template_str(
            "{% for owner in dae.systems.continuous.owners %}{% if owner.kind == \"structured\" %}structured:{{ owner.scalar_rows }}:{{ owner.bodies | length }}{% else %}residual:1:0{% endif %}\n{% endfor %}",
        )
        .expect("checked DAE projection must expose canonical semantic owners");
    assert!(
        rendered.lines().any(|line| line == "structured:7:1"),
        "the compact 2:8 family must remain one seven-row owner:\n{rendered}"
    );
}

#[test]
fn test_solve_template_dae_context_preserves_array_equation_ownership() {
    let source = r#"
        model SolveTemplateDaeContext
          parameter Integer N = 2;
          Real x[N](start = {1, 2});
        equation
          der(x) = {-x[1], -x[N]};
        end SolveTemplateDaeContext;
    "#;

    let result = Compiler::new()
        .model("SolveTemplateDaeContext")
        .compile_str(source, "SolveTemplateDaeContext.mo")
        .expect("compilation should succeed");
    let rendered = result
        .render_template_str_with_name_and_ir(
            "{% for owner in dae.systems.continuous.owners %}{% if owner.kind == \"structured\" %}structured:{{ owner.scalar_rows }}{% else %}residual:1{% endif %} {% endfor %}",
            "SolveTemplateDaeContext",
            TemplateIr::Solve,
        )
        .expect("Solve template should expose the checked DAE projection");

    assert_eq!(
        rendered.trim(),
        "structured:2",
        "Solve templates must not receive a scalarized compatibility DAE"
    );
}

#[test]
fn test_strict_reachable_requested_success_ignores_unreachable_failures() {
    let source = r#"
        package P
          model Good
            Real x(start=0);
          equation
            der(x) = 1;
          end Good;

          model BadNeedsInner
            outer Real shared;
          equation
            shared = 1;
          end BadNeedsInner;
        end P;
    "#;

    let result = Compiler::new()
        .model("P.Good")
        .compile_str(source, "test.mo");
    assert!(
        result.is_ok(),
        "Compilation failed unexpectedly: {:?}",
        result.err()
    );
}

#[test]
fn test_strict_reachable_requested_failure_excludes_unreachable_context() {
    let source = r#"
        package P
          model Good
            Real x(start=0);
          equation
            der(x) = 1;
          end Good;

          model BadNeedsInner
            outer Real shared;
          equation
            shared = 1;
          end BadNeedsInner;

          model BadNeedsInner2
            outer Real shared2;
          equation
            shared2 = 2;
          end BadNeedsInner2;
        end P;
    "#;

    let err = Compiler::new()
        .model("P.BadNeedsInner")
        .compile_str(source, "test.mo")
        .expect_err("Requested model should fail");
    let msg = err.to_string();
    assert!(!msg.contains("Related failures"));
    assert!(!msg.contains("P.BadNeedsInner2"));
}

#[test]
fn test_strict_reachable_fails_when_instantiated_dependency_fails() {
    let source = r#"
        package P
          model BadDep
            outer Real shared;
          equation
            shared = 1;
          end BadDep;

          model Root
            BadDep dep;
            Real x(start=0);
          equation
            der(x) = 1;
          end Root;
        end P;
    "#;

    let err = Compiler::new()
        .model("P.Root")
        .compile_str(source, "test.mo")
        .expect_err("reachable dependency failure must fail strict compile");
    let msg = err.to_string();
    assert!(
        msg.contains("requires inner declarations"),
        "actual message: {msg}"
    );
    assert!(msg.contains("shared"), "actual message: {msg}");
}
