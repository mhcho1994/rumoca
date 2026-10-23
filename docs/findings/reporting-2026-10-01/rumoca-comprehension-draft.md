# Draft: algorithm comprehension aborts during DAE construction

Confirmed destination: local `Arslan8/rumoca` fork, `rumoca-bitcode-v1`.
Upstream `CogniPilot/rumoca` attribution: pending clean-upstream reproduction.
Status: not posted.

```modelica
model C6
  Real x(start=1, fixed=true);
  Real s;
algorithm
  s := sum({x*i for i in 1:2});
equation
  der(x) = -x;
end C6;
```

Command:

```sh
rumoca compile C6.mo --model C6 --pass none --emit-bitcode C6.rbc
```

Expected: compile the sum comprehension in the algorithm section, or report
an explicit unsupported construct without aborting the process.

Actual: exit 101, panic in DAE construction at
`crates/rumoca-phase-dae/src/construction/expression.rs:1795`:

```text
analysis proves the exact comprehension occurrence
```

Reproduced October 1 on the dirty local build based on
`73c6a7d588cc2d8af04764089edc6910f26e0aa0`; full binary identity and command
are in [compiler-results.json](compiler-results.json). The old report diagnoses
incomplete expression traversal as the cause; this rerun confirms the failure
but has not independently re-established that diagnosis on current source.
No MSL dependency is needed. No security impact is established by this example.
