# Catalog: declaration sites permitting physically impossible values

**Historical catalog — not a submission list.** Later review found that an
SI quantity or missing bound does not by itself establish the component's
physical contract. The title and counts below reflect the original survey;
do not treat all 911 entries as bugs or propose global positivity constraints.
See the [current reporting review](../reporting-2026-10-01/README.md).

Every entry is a place in MSL 4.1.0 where a quantity that cannot be
negative is declared without a lower bound, with `file:line` so it can be
checked directly.

**These are sites, not independent defects.** All of them trace to
**17 SI type definitions** in `Units.mo` that declare no `min`.
Fixing those fixes every site below, which is why the count to quote is the
number of types, not the number of lines.

- **17** SI types with no lower bound on an impossible-negative quantity
- **911** declarations that inherit one and add no bound of their own
- **162** declarations that do add their own bound (already safe)

The guarded column is the evidence that the omissions are omissions: the same
library, for the same quantity, sometimes writes the bound and sometimes does
not.

## By type

| Quantity | Domain | Exposed | Guarded | Detail |
|---|---|---|---|---|
| `SI.Resistance` | electrical | **281** | 43 | [sites](by-type/resistance.md) |
| `SI.Length` | mechanical | **215** | 10 | [sites](by-type/length.md) |
| `SI.Inductance` | electrical | **190** | 29 | [sites](by-type/inductance.md) |
| `SI.Inertia` | mechanical | **83** | 28 | [sites](by-type/inertia.md) |
| `SI.Conductance` | electrical | **43** | 47 | [sites](by-type/conductance.md) |
| `SI.Area` | mechanical | **42** | 1 | [sites](by-type/area.md) |
| `SI.Volume` | mechanical | **15** | 0 | [sites](by-type/volume.md) |
| `SI.Reluctance` | magnetic | **8** | 0 | [sites](by-type/reluctance.md) |
| `SI.Resistivity` | electrical | **8** | 0 | [sites](by-type/resistivity.md) |
| `SI.Conductivity` | electrical | **5** | 0 | [sites](by-type/conductivity.md) |
| `SI.Period` | timing | **4** | 0 | [sites](by-type/period.md) |
| `SI.HeatCapacity` | thermal | **4** | 2 | [sites](by-type/heatcapacity.md) |
| `SI.ThermalConductance` | thermal | **4** | 0 | [sites](by-type/thermalconductance.md) |
| `SI.SpecificHeatCapacity` | thermal | **4** | 1 | [sites](by-type/specificheatcapacity.md) |
| `SI.Permeance` | magnetic | **3** | 0 | [sites](by-type/permeance.md) |
| `SI.Duration` | timing | **1** | 1 | [sites](by-type/duration.md) |
| `SI.ThermalResistance` | thermal | **1** | 0 | [sites](by-type/thermalresistance.md) |

## Execution-confirmed defects

Sites where a permitted value provably breaks the model in two
independent tools. These are defects, not merely sites.

| Component | Parameter | Models |
|---|---|---|
| `Modelica.Mechanics.Translational.Examples.Damper` | `m` | 6 |
| `Modelica.Clocked.Examples.SimpleControlledDrive.Continuous` | `J` | 5 |
| `Modelica.Electrical.Analog.Examples.ParallelResonance` | `L` | 4 |
| `Modelica.Electrical.Analog.Examples.InvertingAmp` | `f` | 3 |
| `Modelica.Clocked.Examples.SimpleControlledDrive.Continuous` | `T` | 2 |
| `Modelica.Electrical.Analog.Examples.ParallelResonance` | `C` | 2 |
| `ModelicaTest.Magnetic.FluxTubes.Sensors` | `B_myMax` | 2 |
| `ModelicaTest.Magnetic.FluxTubes.Sensors` | `l` | 2 |
| `Modelica.Electrical.Analog.Examples.ChuaCircuit` | `L` | 1 |
| `Modelica.Electrical.Analog.Examples.ChuaCircuit` | `C` | 1 |
| `Modelica.Magnetic.FluxTubes.Examples.Utilities.TranslatoryArmatureAndStopper` | `s_ref` | 1 |
| `Modelica.Mechanics.Rotational.Examples.ElasticBearing` | `J` | 1 |
| `Modelica.Mechanics.Translational.Examples.ElastoGap` | `s_ref` | 1 |
| `Modelica.Mechanics.Translational.Examples.WhyArrows` | `m` | 1 |
| `ModelicaTest.Magnetic.FluxTubes.Sources` | `B_myMax` | 1 |
| `ModelicaTest.Magnetic.FluxTubes.Sources` | `l` | 1 |
