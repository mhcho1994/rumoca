//! High-level API for compiling Modelica models to DAE representations.
//!
//! This module provides a clean, ergonomic interface for using rumoca as a library.
//! The main entry point is the [`Compiler`] struct, which uses a builder pattern
//! for configuration.
//!
//! # Examples
//!
//! Basic usage:
//!
//! ```ignore
//! use rumoca::Compiler;
//!
//! let result = Compiler::new()
//!     .model("MyModel")
//!     .compile_file("model.mo")?;
//! ```
//!
//! Compiling from a string:
//!
//! ```ignore
//! use rumoca::Compiler;
//!
//! let modelica_code = r#"
//!     model Integrator
//!         Real x(start=0);
//!     equation
//!         der(x) = 1;
//!     end Integrator;
//! "#;
//!
//! let result = Compiler::new()
//!     .model("Integrator")
//!     .compile_str(modelica_code, "Integrator.mo")?;
//! ```

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use rumoca_compile::analysis as dae_analysis;
use rumoca_compile::codegen::{
    CodegenError, SolveTemplateRenderer, render_ast_template_with_name, render_dae_template,
    render_dae_template_with_name, render_flat_template_with_name,
};
use rumoca_compile::compile::{
    Dae, DaeCompilationResult as CompileDaeCompilationResult, FlatModel, SemanticDiagnosticsMode,
    Session, SessionConfig, SourceRootKind, VariableRole,
};
use rumoca_compile::parsing::{StoredDefinition, collect_compile_unit_source_files};
use rumoca_compile::source_roots::{
    PackageLayoutError, canonical_path_key, parse_source_root_with_cache, plan_source_root_loads,
    referenced_unloaded_source_root_paths, render_source_root_status_message,
    resolve_source_root_cache_dir, source_root_paths_referenced_by_files,
    source_root_source_set_key,
};
use rumoca_core::DiagnosticSeverity;
use rumoca_phase_resolve::ResolvedTree;
use rumoca_sim::{lower_solve_artifacts, lower_solve_package, lower_solve_problem};

use crate::error::CompilerError;

#[cfg(test)]
mod tuple_calls_tests;

/// Result of a successful compilation.
#[derive(Debug)]
pub struct CompilationResult {
    /// The DAE representation.
    pub dae: Arc<Dae>,
    /// Detailed continuous balance inputs validated during DAE construction.
    pub balance_detail: dae_analysis::BalanceDetail,
    /// The flat model (intermediate).
    pub flat: FlatModel,
    /// The resolved tree (intermediate, before instantiation and typechecking).
    pub resolved: ResolvedTree,
    /// Cached solve-template renderer (scalarized DAE + lowered solve
    /// problem, as typed template values). Building it dominates target
    /// generation on large models and a multi-file target renders several
    /// strings against the same input.
    solve_template_renderer: std::sync::OnceLock<SolveTemplateRenderer>,
    /// Cached solve-template renderer for targets that never read the `dae`
    /// template entry.
    solve_template_renderer_without_dae:
        std::sync::OnceLock<Result<SolveTemplateRenderer, CompilerError>>,
    /// The artifact the bitcode pass stage produced, whose rebuild is `dae`.
    /// Its derived fields (`reads`, effective values) describe the model
    /// before the passes, so `--emit-bitcode` still exports `dae`, and takes
    /// from here only what a DAE has no place for: trace points and an
    /// execution section an external pass added. `None` when the stage was
    /// skipped or its artifact holds neither.
    pub bitcode: Option<rumoca_bitcode::RbcFile>,
}

#[derive(Clone, Copy)]
struct DaeCounts {
    states: usize,
    algebraics: usize,
    parameters: usize,
    constants: usize,
    inputs: usize,
    outputs: usize,
    continuous_equations: usize,
}

fn dae_counts(model: &Dae) -> DaeCounts {
    model.inspect(|view| {
        let mut counts = DaeCounts {
            states: 0,
            algebraics: 0,
            parameters: 0,
            constants: 0,
            inputs: 0,
            outputs: 0,
            continuous_equations: view.continuous_equation_count(),
        };
        for (_, variable) in view.variables() {
            match variable.role() {
                VariableRole::State => counts.states += 1,
                VariableRole::Algebraic => counts.algebraics += 1,
                VariableRole::Parameter => counts.parameters += 1,
                VariableRole::Constant => counts.constants += 1,
                VariableRole::Input => counts.inputs += 1,
                VariableRole::Output => counts.outputs += 1,
                VariableRole::DiscreteReal | VariableRole::DiscreteValue => {}
            }
        }
        counts
    })
}

/// Lean result of a successful DAE-only compilation.
pub type DaeCompilationResult = CompileDaeCompilationResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateIr {
    Dae,
    Solve,
    Flat,
    Ast,
}

fn build_solve_template_renderer(dae_model: &Dae) -> Result<SolveTemplateRenderer, CompilerError> {
    let package = lower_solve_package(dae_model)
        .map_err(|err| CompilerError::TemplateError(CodegenError::template(err.to_string())))?;
    let artifacts = lower_solve_artifacts(&package.problem)
        .map_err(|err| CompilerError::TemplateError(CodegenError::template(err.to_string())))?;
    SolveTemplateRenderer::new_owned_with_dae_and_calls(
        package.problem,
        artifacts,
        package.pure_calls,
        dae_model,
    )
    .map_err(CompilerError::TemplateError)
}

fn build_solve_template_renderer_without_dae(
    dae_model: &Dae,
) -> Result<SolveTemplateRenderer, CompilerError> {
    let package = lower_solve_package(dae_model)
        .map_err(|err| CompilerError::TemplateError(CodegenError::template(err.to_string())))?;
    SolveTemplateRenderer::new_owned_with_calls(
        package.problem,
        rumoca_ir_solve::SolveArtifacts::default(),
        package.pure_calls,
        "",
    )
    .map_err(CompilerError::TemplateError)
}

impl CompilationResult {
    pub fn new(
        dae: Arc<Dae>,
        balance_detail: dae_analysis::BalanceDetail,
        flat: FlatModel,
        resolved: ResolvedTree,
    ) -> Self {
        Self {
            dae,
            balance_detail,
            flat,
            resolved,
            solve_template_renderer: std::sync::OnceLock::new(),
            solve_template_renderer_without_dae: std::sync::OnceLock::new(),
            bitcode: None,
        }
    }

    /// Render the DAE using a template file.
    pub fn render_template(&self, template_path: &str) -> Result<String, CompilerError> {
        let template_content = fs::read_to_string(template_path)
            .map_err(|e| CompilerError::io_error(template_path, e.to_string()))?;

        self.render_template_str(&template_content)
    }

    /// Render the DAE using a template string.
    pub fn render_template_str(&self, template: &str) -> Result<String, CompilerError> {
        render_dae_template(&self.dae, template).map_err(CompilerError::TemplateError)
    }

    /// Render the DAE using a template string with an explicit model name.
    ///
    /// The model name is exposed as `model_name` in the template context.
    pub fn render_template_str_with_name(
        &self,
        template: &str,
        model_name: &str,
    ) -> Result<String, CompilerError> {
        render_dae_template_with_name(&self.dae, template, model_name)
            .map_err(CompilerError::TemplateError)
    }

    pub fn render_template_str_with_name_and_ir(
        &self,
        template: &str,
        model_name: &str,
        ir: TemplateIr,
    ) -> Result<String, CompilerError> {
        match ir {
            TemplateIr::Dae => self.render_template_str_with_name(template, model_name),
            TemplateIr::Solve => {
                // Build the template context once per compilation, straight
                // from typed data: the previous path serialized the
                // scalarized DAE plus the lowered solve problem to JSON and
                // deserialized the problem back, per compilation - several
                // seconds of round-trip on large models with the actual
                // template rendering measured in milliseconds.
                if self.solve_template_renderer.get().is_none() {
                    let renderer = build_solve_template_renderer(&self.dae)?;
                    let _ = self.solve_template_renderer.set(renderer);
                }
                let renderer = self.solve_template_renderer.get().ok_or_else(|| {
                    CompilerError::TemplateError(CodegenError::template(
                        "solve template renderer was not initialized after build",
                    ))
                })?;
                renderer
                    .render_with_name(template, model_name)
                    .map_err(CompilerError::TemplateError)
            }
            TemplateIr::Flat => render_flat_template_with_name(&self.flat, template, model_name)
                .map_err(CompilerError::TemplateError),
            TemplateIr::Ast => {
                render_ast_template_with_name(self.resolved.inner(), template, model_name)
                    .map_err(CompilerError::TemplateError)
            }
        }
    }

    pub fn render_solve_template_str_without_dae(
        &self,
        template: &str,
        model_name: &str,
    ) -> Result<String, CompilerError> {
        self.solve_renderer_without_dae()?
            .render_with_name(template, model_name)
            .map_err(CompilerError::TemplateError)
    }

    pub(crate) fn require_scalar_target_projection(&self) -> Result<(), CompilerError> {
        self.solve_renderer_without_dae()?
            .require_scalar_derivative_programs()
            .map_err(CompilerError::TemplateError)
    }

    fn solve_renderer_without_dae(&self) -> Result<&SolveTemplateRenderer, CompilerError> {
        self.solve_template_renderer_without_dae
            .get_or_init(|| build_solve_template_renderer_without_dae(&self.dae))
            .as_ref()
            .map_err(Clone::clone)
    }

    pub fn to_ir_json(&self, ir: TemplateIr) -> Result<String, CompilerError> {
        match ir {
            TemplateIr::Dae => self.to_json(),
            TemplateIr::Solve => {
                let solve = lower_solve_problem(&self.dae)
                    .map_err(|err| CompilerError::JsonError(err.to_string()))?;
                serde_json::to_string_pretty(&solve)
                    .map_err(|err| CompilerError::JsonError(err.to_string()))
            }
            TemplateIr::Flat => serde_json::to_string_pretty(&self.flat)
                .map_err(|err| CompilerError::JsonError(err.to_string())),
            TemplateIr::Ast => serde_json::to_string_pretty(self.resolved.inner())
                .map_err(|err| CompilerError::JsonError(err.to_string())),
        }
    }

    /// Equation balance (equations - unknowns).
    pub fn balance(&self) -> i64 {
        self.balance_detail.balance()
    }

    /// Whether equation/unknown balance is exact.
    pub fn is_balanced(&self) -> bool {
        self.balance_detail.is_balanced()
    }

    /// Convert the DAE to JSON.
    pub fn to_json(&self) -> Result<String, CompilerError> {
        serde_json::to_string_pretty(&self.dae).map_err(|e| CompilerError::JsonError(e.to_string()))
    }
}

/// A high-level compiler for Modelica models.
///
/// This struct provides a builder-pattern interface for configuring and executing
/// the compilation pipeline from Modelica source code to DAE representation.
#[derive(Debug, Clone, Default)]
pub struct Compiler {
    /// The main model to compile.
    model_name: Option<String>,
    /// Additional source-root paths to load.
    source_root_paths: Vec<String>,
    /// Enable verbose output.
    verbose: bool,
    /// Keep a derived parameter's declaration binding as written instead of
    /// replacing it with the constant it evaluates to. See
    /// `SessionConfig::fold_parameter_declaration_bindings`.
    no_fold_parameter_bindings: bool,
    freeze_parameters: bool,
    /// Bitcode passes to run between the frontend's DAE and everything
    /// downstream of it. Empty runs the `default` group; `none` alone skips
    /// the stage.
    passes: Vec<String>,
}

impl Compiler {
    fn log_verbose(&self, message: impl AsRef<str>) {
        if self.verbose {
            eprintln!("{}", message.as_ref());
        }
    }

    /// Create a new compiler with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the main model to compile.
    pub fn model(mut self, name: &str) -> Self {
        self.model_name = Some(name.to_string());
        self
    }

    /// Enable or disable verbose output.
    pub fn verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Keep derived parameter bindings unfolded, so `parameter Real d = k * 10`
    /// reaches the flat and DAE models as `k * 10` rather than as its value.
    pub fn no_fold_parameter_bindings(mut self, no_fold: bool) -> Self {
        self.no_fold_parameter_bindings = no_fold;
        self
    }

    /// Run these bitcode passes over the compiled DAE (`DAE -> RBC ->
    /// [passes] -> DAE`) before anything consumes it.
    pub fn passes(mut self, passes: &[String]) -> Self {
        self.passes = passes.to_vec();
        self
    }

    /// Keep main's native compiler path unless an interchange pass is requested.
    /// `default`/`O1` selects the bitcode optimization group; `round-trip`
    /// exercises transport alone. Recursive bodies are outside the bitcode
    /// execution profile and must not restrict ordinary native compilation.
    fn requested_passes(&self) -> Vec<String> {
        match self.passes.as_slice() {
            [] => Vec::new(),
            requested
                if requested
                    .iter()
                    .flat_map(|value| value.split(','))
                    .all(|element| matches!(element.trim(), "none" | "O0")) =>
            {
                Vec::new()
            }
            requested => requested.to_vec(),
        }
    }

    /// Build an artifact specialized to the declared fixed parameter values.
    /// Its evaluated parameters cannot be overridden without recompilation.
    pub fn freeze_parameters(mut self, freeze: bool) -> Self {
        self.freeze_parameters = freeze;
        self
    }

    /// The session configuration these settings describe.
    fn session_config(&self) -> SessionConfig {
        SessionConfig {
            fold_parameter_declaration_bindings: !self.no_fold_parameter_bindings,
            freeze_parameters: self.freeze_parameters,
            ..SessionConfig::default()
        }
    }

    /// Add a source-root path to load before compiling.
    ///
    /// Source-root paths can be either:
    /// - A single .mo file
    /// - A directory containing .mo files
    pub fn source_root(mut self, path: &str) -> Self {
        self.source_root_paths.push(path.to_string());
        self
    }

    /// Add multiple source-root paths.
    pub fn source_roots(mut self, paths: &[String]) -> Self {
        self.source_root_paths.extend(paths.iter().cloned());
        self
    }

    /// Load a source-root path into the session.
    ///
    /// Handles both single files and directories recursively.
    ///
    /// Returns the files the root contributed, so root selection can follow
    /// the top-level names they use.
    fn load_source_root_into_session(
        &self,
        session: &mut Session,
        path: &str,
    ) -> Result<Vec<String>, CompilerError> {
        let path_obj = Path::new(path);
        let parsed_source_root = parse_source_root_with_cache(path_obj).map_err(|e| {
            if let Some(package_layout_error) = e.downcast_ref::<PackageLayoutError>() {
                return CompilerError::SourceDiagnosticsError {
                    summary: package_layout_error.to_string(),
                    diagnostics: package_layout_error.diagnostics().to_vec(),
                    source_map: Box::new(package_layout_error.source_map().clone()),
                };
            }
            CompilerError::ParseError(format!("{}: {}", path, e))
        })?;

        let files = parsed_source_root
            .documents
            .iter()
            .map(|(file, _)| file.clone())
            .collect();
        let source_root_key = source_root_source_set_key(path);
        session.replace_parsed_source_set(
            &source_root_key,
            SourceRootKind::External,
            parsed_source_root.documents,
            None,
        );
        let cache_dir = resolve_source_root_cache_dir();
        let _ = session.sync_source_root_semantic_summary_cache(
            &source_root_key,
            path_obj,
            cache_dir.as_deref(),
        );

        if self.verbose
            && let Some(status) = session.source_root_status(&source_root_key)
        {
            eprintln!("{}", render_source_root_status_message(&status));
        }
        Ok(files)
    }

    /// Load the source roots the compile unit uses, closed over the roots
    /// the loaded roots themselves use.
    fn load_required_source_roots(
        &self,
        session: &mut Session,
        source: &str,
    ) -> Result<(), CompilerError> {
        let mut loaded_source_root_path_keys = HashSet::new();
        let mut referenced = referenced_unloaded_source_root_paths(
            source,
            &self.source_root_paths,
            &loaded_source_root_path_keys,
        );
        while !referenced.is_empty() {
            let load_plan = plan_source_root_loads(&referenced, &loaded_source_root_path_keys);
            for skipped in &load_plan.duplicate_root_skips {
                self.log_verbose(format!(
                    "[rumoca] Skipping source root {} (duplicate root '{}' already loaded from {})",
                    skipped.source_root_path, skipped.root_name, skipped.provider_path
                ));
                loaded_source_root_path_keys.insert(canonical_path_key(&skipped.source_root_path));
            }
            let mut loaded_files = Vec::new();
            for source_root_path in &load_plan.load_paths {
                self.log_verbose(format!(
                    "[rumoca] Loading source root: {}",
                    source_root_path
                ));
                loaded_files.extend(self.load_source_root_into_session(session, source_root_path)?);
                loaded_source_root_path_keys.insert(canonical_path_key(source_root_path));
            }
            referenced = source_root_paths_referenced_by_files(
                &loaded_files,
                &self.source_root_paths,
                &loaded_source_root_path_keys,
            );
        }
        for source_root_path in &self.source_root_paths {
            if !loaded_source_root_path_keys.contains(&canonical_path_key(source_root_path)) {
                self.log_verbose(format!(
                    "[rumoca] Skipping unused source root: {}",
                    source_root_path
                ));
            }
        }
        Ok(())
    }

    fn load_local_compile_unit(
        &self,
        session: &mut Session,
        source: &str,
        file_name: &str,
    ) -> Result<(), CompilerError> {
        let path = Path::new(file_name);
        if !path.is_file() {
            let _ = session.update_document(file_name, source);
            return Ok(());
        }
        // A file that a loaded source root already serves with this exact text
        // is compiled from that root's cached parse, the same as a model
        // referenced by name. Reading its enclosing package tree here would
        // re-parse the whole library as workspace documents.
        if let Some(uri) =
            loaded_source_root_document_uri(session, path, source, &self.source_root_paths)
        {
            self.log_verbose(format!(
                "[rumoca] Using loaded source-root document for {file_name}: {uri}"
            ));
            return Ok(());
        }

        let files = collect_compile_unit_source_files(path)
            .map_err(|e| CompilerError::ParseError(format!("{}", e)))?;
        // The directory scan can yield the requested file under a different
        // spelling (`./Solo.mo` vs `Solo.mo`), so compare canonical paths —
        // a raw `Path` comparison registers the file twice and reports it as
        // a duplicate class of itself.
        let requested_canonical = path.canonicalize().ok();
        for sibling in files {
            if sibling == path
                || (requested_canonical.is_some()
                    && sibling.canonicalize().ok() == requested_canonical)
            {
                continue;
            }
            let sibling_path = sibling.to_string_lossy().to_string();
            // A sibling the user never referenced must not poison the
            // compile: skip unreadable files with a warning. If the model
            // actually needs the file, resolution reports the missing names.
            let sibling_source = match fs::read_to_string(&sibling) {
                Ok(sibling_source) => sibling_source,
                Err(error) => {
                    eprintln!(
                        "[rumoca] warning: skipping unreadable sibling source `{sibling_path}`: {error}"
                    );
                    continue;
                }
            };
            let _ = session.update_document(&sibling_path, &sibling_source);
        }

        let _ = session.update_document(file_name, source);
        Ok(())
    }

    /// Compile a Modelica file.
    pub fn compile_file(&self, path: &str) -> Result<CompilationResult, CompilerError> {
        let source =
            fs::read_to_string(path).map_err(|e| CompilerError::io_error(path, e.to_string()))?;

        self.compile_str(&source, path)
    }

    /// Parse a Modelica file into its syntax tree (SPEC_0007 Stage 1).
    ///
    /// The AST stage carries syntax, comments, and spans only, so this reads
    /// just the named file: no source root is loaded, no model is selected,
    /// and no name is resolved. The dump therefore stays available for a file
    /// whose referenced libraries are missing or fail to resolve.
    pub fn parse_file(&self, path: &str) -> Result<StoredDefinition, CompilerError> {
        let source =
            fs::read_to_string(path).map_err(|e| CompilerError::io_error(path, e.to_string()))?;

        self.parse_str(&source, path)
    }

    /// Compile a Modelica file through the flat-model stage only.
    pub fn compile_file_flat(&self, path: &str) -> Result<FlatModel, CompilerError> {
        let source =
            fs::read_to_string(path).map_err(|e| CompilerError::io_error(path, e.to_string()))?;

        self.compile_str_flat(&source, path)
    }

    /// Compile a Modelica file from a Path.
    pub fn compile_path(&self, path: &Path) -> Result<CompilationResult, CompilerError> {
        let path_str = path.to_string_lossy().to_string();
        self.compile_file(&path_str)
    }

    /// Compile a Modelica file through DAE only.
    ///
    /// This avoids retaining Flat and Resolved artifacts in callers that only
    /// need simulation-ready DAE output.
    pub fn compile_file_dae(&self, path: &str) -> Result<DaeCompilationResult, CompilerError> {
        let source =
            fs::read_to_string(path).map_err(|e| CompilerError::io_error(path, e.to_string()))?;

        self.compile_str_dae(&source, path)
    }

    /// Compile a Modelica file from a Path through DAE only.
    pub fn compile_path_dae(&self, path: &Path) -> Result<DaeCompilationResult, CompilerError> {
        let path_str = path.to_string_lossy().to_string();
        self.compile_file_dae(&path_str)
    }

    /// Compile Modelica source code.
    pub fn compile_str(
        &self,
        source: &str,
        file_name: &str,
    ) -> Result<CompilationResult, CompilerError> {
        let model_name = self
            .model_name
            .as_ref()
            .ok_or(CompilerError::NoModelSpecified)?;

        if self.verbose {
            eprintln!("[rumoca] Compiling model: {}", model_name);
            eprintln!("[rumoca] Source file: {}", file_name);
        }

        let mut session = Session::new(self.session_config());
        self.load_required_source_roots(&mut session, source)?;

        if self.verbose {
            eprintln!("[rumoca] Phase 1-2: Parsing and resolving...");
        }
        self.load_local_compile_unit(&mut session, source, file_name)?;

        if self.verbose {
            eprintln!(
                "[rumoca] Phase 3-6: Strict-reachable compile (with recovery diagnostics)..."
            );
        }

        let compilation = session.compile_model_strict(model_name).map_err(|report| {
            CompilerError::CompileDiagnosticsError {
                summary: report.failure_summary(usize::MAX),
                failures: report.failures,
                source_map: report.source_map.map(Box::new),
            }
        })?;
        let (mut result, resolved) = compilation.into_parts();
        report_compile_warnings(&mut session, model_name);
        let passes = self.requested_passes();
        let mut bitcode = None;
        if !passes.is_empty() {
            let (dae, file) = crate::pass_stage::apply(
                &result.dae,
                &result.flat,
                model_name,
                &passes,
                self.verbose,
            )?;
            result.dae = dae;
            bitcode = Some(file)
                .filter(|file| !file.model.trace_points.is_empty() || file.execution.is_some());
        }

        if self.verbose {
            eprintln!("[rumoca] Compilation complete.");
            let counts = dae_counts(&result.dae);
            eprintln!("[rumoca]   States: {}", counts.states);
            eprintln!("[rumoca]   Algebraics: {}", counts.algebraics);
            eprintln!("[rumoca]   Parameters: {}", counts.parameters);
            eprintln!(
                "[rumoca]   Continuous equations (f_x): {}",
                counts.continuous_equations
            );
            eprintln!("[rumoca]   Balance: {}", result.balance_detail.balance());
        }

        let mut compiled =
            CompilationResult::new(result.dae, result.balance_detail, result.flat, resolved);
        compiled.bitcode = bitcode;
        Ok(compiled)
    }

    /// Parse Modelica source code into its syntax tree (SPEC_0007 Stage 1).
    ///
    /// In-memory counterpart to [`Self::parse_file`]: the session stores the
    /// document exactly as a compile would (extension constructs expanded to
    /// standard Modelica), and a syntax error is reported with the parser's
    /// spans instead of a tree.
    pub fn parse_str(
        &self,
        source: &str,
        file_name: &str,
    ) -> Result<StoredDefinition, CompilerError> {
        if self.verbose {
            eprintln!("[rumoca] Parsing source: {}", file_name);
            eprintln!("[rumoca] Phase 1: Parsing (no source roots, no resolution)...");
        }

        let mut session = Session::new(SessionConfig::default());
        let parse_error = session.update_document(file_name, source);
        if let Some(message) = parse_error {
            let Some((diagnostics, source_map)) =
                session.document_parse_diagnostics_with_source_map(file_name)
            else {
                return Err(CompilerError::ParseError(message));
            };
            return Err(CompilerError::SourceDiagnosticsError {
                summary: format!("failed to parse `{file_name}`"),
                diagnostics,
                source_map: Box::new(source_map),
            });
        }
        session
            .get_document(file_name)
            .and_then(|document| document.parsed().cloned())
            .ok_or_else(|| {
                CompilerError::ParseError(format!("no syntax tree stored for `{file_name}`"))
            })
    }

    /// Compile Modelica source code through the flat-model stage only.
    pub fn compile_str_flat(
        &self,
        source: &str,
        file_name: &str,
    ) -> Result<FlatModel, CompilerError> {
        let model_name = self
            .model_name
            .as_ref()
            .ok_or(CompilerError::NoModelSpecified)?;

        if self.verbose {
            eprintln!("[rumoca] Compiling model through Flat: {}", model_name);
            eprintln!("[rumoca] Source file: {}", file_name);
            eprintln!("[rumoca] Phase 1-5: Parsing, resolving, and flattening...");
        }

        let mut session = Session::new(self.session_config());
        self.load_required_source_roots(&mut session, source)?;
        self.load_local_compile_unit(&mut session, source, file_name)?;
        let flat = session
            .compile_model_flat_strict_reachable_uncached_with_recovery(model_name)
            .map_err(CompilerError::FlattenError)?;
        report_compile_warnings(&mut session, model_name);
        Ok(flat)
    }

    /// Compile Modelica source code through DAE only.
    pub fn compile_str_dae(
        &self,
        source: &str,
        file_name: &str,
    ) -> Result<DaeCompilationResult, CompilerError> {
        let model_name = self
            .model_name
            .as_ref()
            .ok_or(CompilerError::NoModelSpecified)?;

        if self.verbose {
            eprintln!("[rumoca] Compiling model through DAE: {}", model_name);
            eprintln!("[rumoca] Source file: {}", file_name);
        }

        let mut session = Session::new(self.session_config());
        self.load_required_source_roots(&mut session, source)?;

        if self.verbose {
            eprintln!("[rumoca] Phase 1-2: Parsing and resolving...");
        }
        self.load_local_compile_unit(&mut session, source, file_name)?;

        if self.verbose {
            eprintln!("[rumoca] Phase 3-6: Strict-reachable DAE compile...");
        }

        let result =
            match session.compile_model_dae_strict_reachable_uncached_with_recovery(model_name) {
                Ok(result) => result,
                Err(summary) => {
                    let report =
                        session.compile_model_strict_reachable_uncached_with_recovery(model_name);
                    let summary = if report.failures.is_empty() {
                        summary
                    } else {
                        report.failure_summary(usize::MAX)
                    };
                    return Err(CompilerError::CompileDiagnosticsError {
                        summary,
                        failures: report.failures,
                        source_map: report.source_map.map(Box::new),
                    });
                }
            };

        report_compile_warnings(&mut session, model_name);
        let mut result = result;
        let passes = self.requested_passes();
        if !passes.is_empty() {
            result.dae = crate::pass_stage::apply(
                &result.dae,
                &result.flat,
                model_name,
                &passes,
                self.verbose,
            )?
            .0;
        }

        if self.verbose {
            eprintln!("[rumoca] DAE compilation complete.");
            let counts = dae_counts(&result.dae);
            eprintln!("[rumoca]   States: {}", counts.states);
            eprintln!("[rumoca]   Algebraics: {}", counts.algebraics);
            eprintln!("[rumoca]   Parameters: {}", counts.parameters);
            eprintln!(
                "[rumoca]   Continuous equations (f_x): {}",
                counts.continuous_equations
            );
            eprintln!("[rumoca]   Balance: {}", result.balance_detail.balance());
        }

        Ok(*result)
    }
}

/// Return the URI of the loaded source-root document that serves `path`, when
/// that document was read from the same file and `source` is exactly the
/// file's current text.
///
/// Source-root documents are keyed by the source-root path joined with the
/// file's path relative to that root, so the lookup maps the canonical file
/// path back into each root's own spelling. A root that was not loaded has no
/// backed documents and never matches.
fn loaded_source_root_document_uri(
    session: &Session,
    path: &Path,
    source: &str,
    source_root_paths: &[String],
) -> Option<String> {
    let canonical_file = path.canonicalize().ok()?;
    let uri = source_root_paths.iter().find_map(|root| {
        let root_path = Path::new(root);
        let relative = canonical_file
            .strip_prefix(root_path.canonicalize().ok()?)
            .ok()?;
        let uri = if relative.as_os_str().is_empty() {
            root.clone()
        } else {
            root_path.join(relative).to_string_lossy().to_string()
        };
        session.is_source_root_backed_document(&uri).then_some(uri)
    })?;
    let file_text = fs::read_to_string(&canonical_file).ok()?;
    (file_text == source).then_some(uri)
}

/// Print every warning-severity diagnostic the compiled model carries.
///
/// A compile that succeeds still has things the language requires a tool to
/// say: MLS 3.7 §12.3 deprecates an external function that declares no
/// purity, and MLS 3.6 §12.3 stated the report as a requirement. Those
/// diagnostics exist in the session, but until they are printed the only
/// consumers are the editor and the API, so a `rumoca compile` run reports
/// nothing and the deprecation reaches nobody. Warnings never change the exit
/// status: they are reports, not rejections.
///
/// The report is the model's target-scoped (save) diagnostics: they read the
/// strict target resolution the compile retained and the stage queries it
/// filled, so reporting resolves nothing again and covers exactly the classes
/// the model reaches.
fn report_compile_warnings(session: &mut Session, model_name: &str) {
    let diagnostics = session.semantic_diagnostics_query(model_name, SemanticDiagnosticsMode::Save);
    let source_map = diagnostics.source_map;
    for diagnostic in diagnostics
        .diagnostics
        .iter()
        .filter(|diagnostic| matches!(diagnostic.severity, DiagnosticSeverity::Warning))
    {
        let code = diagnostic.code.as_deref().unwrap_or("warning");
        match source_map
            .as_ref()
            .zip(diagnostic.labels.first())
            .and_then(|(map, label)| rumoca_compile::compile::source_span_location(map, label.span))
        {
            // `TextPosition` is the editor-protocol form: zero-based line and
            // zero-based UTF-16 column. A terminal `file:line:col` is one-based
            // everywhere else this compiler prints one, so both fields are
            // shifted here rather than reported in an editor's coordinates.
            Some(location) => eprintln!(
                "[rumoca] warning[{code}]: {} ({}:{}:{})",
                diagnostic.message,
                location.file_name,
                location.start.line + 1,
                location.start.character + 1
            ),
            None => eprintln!("[rumoca] warning[{code}]: {}", diagnostic.message),
        }
    }
}

#[cfg(test)]
mod tests;
