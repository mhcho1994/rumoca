# TOOLBUG-047 — rumoca-bind-wasm tests race on the singleton session

**Status:** fixed (this change).
**Severity:** low (test reliability) — a spurious failure in `cargo test`.

`portable_source_root_tests::malformed_portable_cache_does_not_publish_a_partial_source_root`
measures the singleton session's source-root document count before and after
a merge that must fail, and asserts it unchanged. It failed about one run in
three under the default parallel test runner (`left: 2, right: 1`) and never
with `--test-threads=1`.

The crate serializes access to its process-wide session with
`session_test_guard()`, but 31 of its tests -- all of
`lsp_diagnostics_tests`, `scenario_config_tests`,
`workspace_config_api_tests`, and 19 in `tests.rs` -- did not take it, so
any of them could add a document between the two counts.

Every test that did not take the guard now does. Twelve consecutive
parallel runs pass.
