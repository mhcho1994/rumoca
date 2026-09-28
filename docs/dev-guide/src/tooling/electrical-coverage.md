# Electrical coverage work

Reviewed reference boundaries for `Modelica.Electrical.Analog` and
`Modelica.Electrical.PowerConverters` parity. A boundary here grants no
strict-high or certification credit; the model stays in every cohort sweep.

## AmplifierWithOpAmpDetailed has no converged OMC reference

Against the default OMC reference the Rumoca trace compares 66 channels: 24
high, 31 minor, and 11 deviating (maximum bounded normalized L1 0.221). The
deviating channels are the op-amp's internal states. Between 0.9 ms and 1.4 ms
the default OMC trace makes a single non-periodic excursion that the periodic
Rumoca trace does not make; outside that window the traces agree.

OMC cannot refine its own reference. The same model, simulated by the same OMC
build with only the tolerance tightened, fails partway through the run:

```modelica
loadModel(Modelica, {"4.1.0"}); getErrorString();
simulate(Modelica.Electrical.Analog.Examples.AmplifierWithOpAmpDetailed,
  tolerance=1e-10, numberOfIntervals=12, outputFormat="csv",
  variableFilter="time|opAmp.v_in|opAmp.v_source|opAmp.q_fp1",
  fileNamePrefix="amp_tight"); getErrorString();
simulate(Modelica.Electrical.Analog.Examples.AmplifierWithOpAmpDetailed,
  numberOfIntervals=12, outputFormat="csv",
  variableFilter="time|opAmp.v_in|opAmp.v_source|opAmp.q_fp1",
  fileNamePrefix="amp_default"); getErrorString();
```

Tolerance 1e-10 (`method = 'dassl'`, stop time 0.003 s):

```text
messages = "Simulation execution failed for model: Modelica.Electrical.Analog.Examples.AmplifierWithOpAmpDetailed
DASKR--  NONLINEAR SOLVER FAILED TO CONVERGE
LOG_STDOUT        | info    | model terminate | Integrator failed. | Simulation terminated at time 0.000136928
```

Default tolerance 2e-7:

```text
LOG_SUCCESS       | info    | The initialization finished successfully without homotopy method.
LOG_SUCCESS       | info    | The simulation finished successfully.
```

The only OMC trace that exists is therefore the one with the excursion, and no
tighter OMC run can confirm or refute it. The comparison is not identifying,
so the model carries a `reference_failure` row in
`crates/rumoca-test-msl/tests/msl_tests/msl_trace_compare_exclusions.json`.
Official reference traces and tolerances are unchanged.
