# TOOLBUG-105 — source roots used only by a loaded library were never loaded

**Status:** fixed.
**Severity:** high — every ThermoSysPro model (the harness compiles
`ThermoSysPro/package.mo --model ...`) failed name resolution on the first
MSL reference: `ER003 unresolved extends base class: Modelica.Icons.Function`,
`ER002 unresolved component reference: Modelica.Constants.eps`,
`ER002 unresolved function call: ...` (6 cases in cluster D, about 611 in the
full run).

This is not an MSL-version problem: ThermoSysPro 4.2 has no `uses`
annotation at all, and its classes are written against MSL 4.

## What

```
lib/L/package.mo   package L end L;
lib/L/T.mo         within L; model T parameter Real a = Mod.C.eps; ... end T;
dep/Mod/...        package Mod ... package C constant Real eps = 1e-15; ...
rumoca compile lib/L/package.mo --model L.T --source-root dep --source-root lib
```

The compiler loads a `--source-root` only if one of the root's top-level
names appears as an identifier in the main file's text
(`referenced_unloaded_source_root_paths`). With a library's `package.mo` as
the main file, `L` is named (so `lib` loads) but `Mod` is not, so `dep` was
skipped and every lookup of `Mod.*` inside the library failed. Libraries with
`annotation(uses(Modelica(...)))` in `package.mo` only worked because the
annotation happens to spell `Modelica`. Compiling `T.mo` directly worked.

## Fix

- `crates/rumoca-compile/src/source_root_discovery.rs`:
  `source_root_paths_referenced_by_files` reports which unloaded roots are
  named by the files of roots already loaded.
- `crates/rumoca/src/compiler.rs`: `load_required_source_roots` iterates to a
  fixed point — roots named by the main file, then roots named by the files
  of the roots just loaded, and so on. Duplicate-root handling is unchanged.

## Test

`crates/rumoca/src/compiler.rs`:
`test_compile_package_file_loads_roots_its_classes_use`.
