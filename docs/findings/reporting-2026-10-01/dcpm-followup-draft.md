# Draft follow-up to MSL #4098

Destination: https://github.com/modelica/ModelicaStandardLibrary/issues/4098
Status: not posted. This is an existing-report follow-up, not a new discovery.

---

The DCPM_Drive case mentioned in
https://github.com/modelica/ModelicaStandardLibrary/issues/4098#issuecomment-1486606129
appears to remain after the battery fixes in #4112.

At MSL revision `4c40388dde27ccb6702cea876adbf54c76d75b97`,
[DCPM_Drive.mo](https://github.com/modelica/ModelicaStandardLibrary/blob/4c40388dde27ccb6702cea876adbf54c76d75b97/Modelica/Electrical/Machines/Examples/DCMachines/DCPM_Drive.mo#L81)
still contains:

```modelica
Analog.Basic.Resistor resistor(R=0.05*dcpmData.VaNominal/1000)
```

`VaNominal` has voltage units, whereas `R` expects resistance units. The bare
`1000` does not state the current scale needed to make this expression V/A.
The files changed in #4112 are all under Electrical.Batteries; this example
was not included.

One possible correction, if 1000 A is the intended fixed scale, is to name
that scale with an `SI.Current` declaration and divide by it. That would
preserve the current numerical resistance while making the units explicit.
Using `dcpmData.IaNominal` is another possibility only if nominal armature
current is actually the intended design scale; we have not established that
intent and are not proposing a numerical change on that assumption.

This report is limited to dimensional consistency. It does not claim a
simulation failure or an independently demonstrated incorrect trajectory.
Would you prefer to reopen this remaining case here or track it separately?
