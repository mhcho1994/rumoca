# Draft upstream issue — MSL: components inherit numeric bounds their equations cannot honour

**Status: SUPERSEDED — do not submit this draft.** Later source-recompilation
and initialization controls refuted several blanket zero-storage claims.
Related work is already discussed in
[MSL #4814](https://github.com/modelica/ModelicaStandardLibrary/issues/4814).
See the [current reporting review](reporting-2026-10-01/README.md). The old
draft below is retained as history, not an approved issue body.
Target: `modelica/ModelicaStandardLibrary` · Version examined: MSL 4.1.0
Counts below are from the completed 827-model sweep. Rows are marked with
whether they survived cross-confirmation in a second tool; **only the confirmed
rows should be filed.**

---

## Suggested title

> Several components accept a parameter value their own declared bounds permit
> but their equations cannot survive (`Inductor.L`, `Capacitor.C`, `Mass.m`,
> `Inertia.J`, semiconductor `Tnom`)

## Body

### Summary

A number of MSL components declare — or inherit from `Modelica.Units.SI` — a
numeric domain that includes a value the component's own equations cannot
integrate. Each affected model simulates cleanly at its declared parameter
values and fails at a value the declaration permits.

Every case below was reproduced in **both OpenModelica 1.27.0-dev and a second,
independent Modelica compiler**, with the unmodified model passing as a
baseline in each. Cases where only one tool failed were discarded, precisely
because those tend to be tool gaps rather than library defects.

This is reported as one issue rather than several because the instances share a
root cause and, we think, a single decision about `Units.mo`.

### The clearest case: `Inductor`

```modelica
// Modelica/Electrical/Analog/Basic/Inductor.mo
parameter SI.Inductance L(start=1) "Inductance";
equation
  L*der(i) = v;
```

Three things line up badly here:

1. `SI.Inductance` declares **no bound**, so zero *and negative* inductance are
   permitted values.
2. The component's own documentation states:
   > The Inductance *L* is allowed to be positive, or zero.
3. At `L = 0` the equation becomes `0 = v`, and both tools fail.

```
$ omc  # ChuaCircuit, -override L.L=0
Simulation execution failed for model: Modelica.Electrical.Analog.Examples.ChuaCircuit
division by zero
```

Confirmed on `ChuaCircuit`, `ParallelResonance`, `SeriesResonance`.

`Capacitor` carries the identical sentence, and honours it *conditionally*:
`i = C*der(v)` at `C = 0` is an open circuit, which `CauerLowPassAnalog`
survives and `ChuaCircuit` does not. So zero capacitance is topology-dependent,
while zero inductance failed in every model tested. Both are reported, but they
are different defects — the inductor has no bound at all, the capacitor has a
deliberate one whose validity the declaration cannot express.

**MSL already defines the type that would fix it:**

```modelica
// Modelica/Units.mo:546,549
type Inductance = Real(final quantity="Inductance", final unit="H");
type SelfInductance = Inductance(min=0);
```

`SelfInductance` exists for exactly this and `Basic.Inductor` does not use it.

### Where the other bounds come from

Most affected components declare no bound themselves; it arrives with the SI
type. `Capacitor.mo` contains no `min` at all — it comes from:

```modelica
// Modelica/Units.mo:505
type Capacitance = Real(final quantity="Capacitance", final unit="F", min=0);
```

| Type / component | Bound | Equation | Reach | Two tools? |
|---|---|---|---|---|
| `Rotational.Components.Inertia.J` | `min=0` on the component | `J*a = tau` | 29 models | **yes** |
| `Translational.Components.Mass.m` | `min=0` (via `SI.Mass`) | `m*a = f` | 17 models | **yes** |
| `SI.Capacitance` → `Basic.Capacitor.C` | `min=0` | `i = C*der(v)` | 12 models | **yes** (topology-dependent) |
| `SI.Inductance` → `Basic.Inductor.L` | **none** | `L*der(i) = v` | 8 models | **yes** |
| `SI.Resistance` → `Basic.Resistor.R` | **none** | `v = R*i` | 12 models | *single tool — hold* |
| `SI.Temperature` → `Semiconductors.NPN.Tnom`, `PNP.Tnom`, `Diode.TNOM` | `min=0` | thermal voltage `~ k*T/q` | 5 models | *single tool — hold* |
| `SI.Density` → `FluidHeatFlow.Media.Medium.rho` | `min=0` | divisor | 8 models | *single tool — hold* |

One candidate of the same shape was **discarded**: `HeatTransfer.HeatCapacitor.C`
reached 20 models — the largest group in the sweep — and the second tool
survives the trigger. It is not in this report.

`min=0` on a temperature is correct physics — absolute zero is the floor — and
simultaneously fatal to any equation dividing by `T`. The bound is right as a
physical statement and wrong as a contract with the solver.

### Why this matters in practice

Nobody sets a mass to zero deliberately. Parameter sweeps, optimisers,
calibration loops and automated model configuration do it by construction: they
have only the declared bounds to go on, and the declared bounds say these values
are fine. The failure then surfaces as a solver error naming an internal
variable (`mass1.a`) rather than a diagnostic naming the parameter the user set.

### The inconsistency, which is the actual argument

MSL is not careless here — it makes both choices, sometimes within one package:

| Bounded correctly | Unbounded / fatal |
|---|---|
| `SelfInductance = Inductance(min=0)` is defined | `Basic.Inductor` uses plain `SI.Inductance` |
| `QuadraticSpeedDependentForce.v_nominal(min=Modelica.Constants.eps)` | `Translational.Mass.m(min=0)` |
| `FundamentalWave.EddyCurrent` guards: `if G > 0 then … else V_m.re = 0` | `FluxTubes.VariablePermeance` takes an unbounded input |

Where a component knows the degenerate value is a problem, MSL either tightens
the bound or handles the case structurally. The instances above are the ones
where neither was done.

### Suggested direction

Two options, and the choice is yours rather than ours:

1. **Tighten the bounds**, so a tool can reject the configuration with a
   message naming the parameter. `min=Modelica.Constants.small` is what MSL
   already uses elsewhere. For `Inductor`, this is just using `SelfInductance`.
2. **Keep the bounds and handle the degenerate case**, as `EddyCurrent` does.

Either way, the declaration and the documentation should agree — `Inductor`
currently promises something it cannot deliver.

### How these were found

An automated sanitizer that, for each model, simulates at the declared values
and then at values the declaration permits, keeping only cases where the first
succeeds and the second fails. Findings were then cross-checked in a second
compiler; anything only one tool failed on was discarded.

Happy to supply the full reproduction scripts, the per-model list, or to split
this into separate issues if that suits your workflow better.

---

## Reviewer checklist before filing

- [x] Re-run instance counts against the completed 827-model sweep
- [ ] Cross-confirm `Resistor.R`, `Temperature` and `Density` rows, or delete
      them from the report. `Resistor.R` resisted a first attempt: Rumoca
      cannot compile any of the 12 models, or fails at their declared values.
- [ ] Decide: one issue, or `Inductor` alone first as the sharpest case
- [ ] Name the second tool explicitly, or describe it neutrally as here
- [ ] Attach `tools/sweep/` reproduction commands
