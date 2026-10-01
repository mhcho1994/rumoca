# TOOLBUG-142 — function without purity prefix calling an impure function rejected

**Status:** fixed (accepted with warning WR013).
**Severity:** medium — MSL 4 `ModelicaServices.ExternalReferences.loadResource`
is declared `function` (no prefix) and calls the impure
`Modelica.Utilities.Files.fullPathName`. Once TOOLBUG-141 selected it, the DAE
rejected it: `ED019 impure function ... is called from a pure function body`.

## What

```modelica
impure function g input Real x; output Real y; external "C" y = sin(x); end g;
function f input Real x; output Real y; algorithm y := g(x); end f;
model M parameter Real p = f(1); end M;
```

MLS §12.3 forbids a pure function to call an impure one, and functions are
pure by default. OpenModelica accepts such a function and treats it as
impure; MSL itself depends on that.

## Fix

- `crates/rumoca-phase-dae/src/construction/analysis/function_impurity.rs`:
  the pure-body proof covers only functions that wrote `pure` explicitly
  (`purity_declared`). An explicit `pure` keeps the error.
- `crates/rumoca-phase-resolve/src/semantic_checks/restrictions.rs`: new
  warning WR013 for a function without purity prefix whose algorithm calls an
  impure function (reported where Resolve can see the callee).

## Test

- `crates/rumoca-contracts/tests/func_contracts.rs::undeclared_purity_function_calling_impure_compiles_with_warning`
- `crates/rumoca-phase-resolve/src/tests/overstrict_checks.rs::undeclared_purity_function_calling_impure_warns`
