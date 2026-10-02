# Psychrometric validation ramps exceed the declared relative-humidity maximum

Proposed destination: `lbl-srg/modelica-buildings`. Status: draft, independently reproduced; not submitted. Severity: low, validation input inconsistency.

The default `DewPoint_TDryBulPhi` and `SpecificEnthalpy_TDryBulPhi` validation models use `phi(duration=1, height=1, offset=0.001)`. At their stop time of 1 s, this supplies `phi=1.001` to psychrometric blocks whose relative-humidity inputs have `max=1`.

Pinned sources: [dew-point validation](https://github.com/lbl-srg/modelica-buildings/blob/a3cfdde4e2fa1605f351875c2199b6aafaee7fe0/Buildings/Controls/OBC/CDL/Psychrometrics/Validation/DewPoint_TDryBulPhi.mo), [enthalpy validation](https://github.com/lbl-srg/modelica-buildings/blob/a3cfdde4e2fa1605f351875c2199b6aafaee7fe0/Buildings/Controls/OBC/CDL/Psychrometrics/Validation/SpecificEnthalpy_TDryBulPhi.mo), [dew-point input contract](https://github.com/lbl-srg/modelica-buildings/blob/a3cfdde4e2fa1605f351875c2199b6aafaee7fe0/Buildings/Controls/OBC/CDL/Psychrometrics/DewPoint_TDryBulPhi.mo). Live master reads on 2026-10-01 retain the same declarations; blob identities are recorded in `upstream-evidence.json`.

## Reproduction

With Buildings at the linked revision and Modelica 4.1.0 loaded, run in OpenModelica:

```modelica
simulate(Buildings.Controls.OBC.CDL.Psychrometrics.Validation.DewPoint_TDryBulPhi,
  stopTime=1, numberOfIntervals=100, outputFormat="csv");
simulate(Buildings.Controls.OBC.CDL.Psychrometrics.Validation.SpecificEnthalpy_TDryBulPhi,
  stopTime=1, numberOfIntervals=100, outputFormat="csv");
```

OpenModelica 1.27.1~2-g6db4671 finishes both simulations successfully but emits input min/max warnings at time 1. Both CSV traces reach `phi.y=1.001`. Expected for an in-domain validation sweep: humidity remains within the blocks' declared [0,1] domain.

## Suggested correction and control

Change each ramp's `height=1` to `height=0.999`, preserving the positive offset. Independently simulated derived controls with `extends ... (phi(height=0.999))` both finish successfully, reach exactly 1.000, and emit no humidity min/max warning. The positive lower endpoint remains 0.001. If intentional out-of-domain testing is desired instead, document that purpose and expected warning explicitly.

This is a fixture/domain mismatch; no failure of the psychrometric equations or simulation crash is claimed. Equivalent fixture declarations occur in IBPSA, IDEAS and AixLib; please coordinate the shared change rather than count these as separate root causes.

Evidence: [original output](evidence/humidity-output.txt), [control output](evidence/humidity-controls-output.txt), [measured maxima and trace hashes](evidence/humidity-results.json), [control source](evidence/HumidityControls.mo). Targeted duplicate searches found no exact report; historical issue #2139 concerns refactoring unused pressure inputs, not this ramp endpoint.
