# TOOLBUG-185 — discrete-valued initial equations reading unsettled coordinates

**Status:** open (analysed, not fixed).
**Severity:** medium. 6 models seen in evaluation: CDL `Logical.Latch` users.
In R2-events: `IBPSA/AixLib.Controls.OBC.CDL.Logical.VariablePulse`
(through `TrueFalseHold`) and Buildings `ChillerCooled.Controls.Reheat` and
its validation model (through MSL `StateGraph.PartialCompositeStep`).

## What

```modelica
// CDL Latch                      // CDL TrueFalseHold         // MSL StateGraph
initial equation                  initial equation             initial equation
  y = not clr and u;                pre(y) = u;                  pre(newActive) = pre(localActive);
```

MLS §8.6 allows initial equations on discrete-valued (Boolean/Integer)
variables. Rumoca has two owners for initial equations:

- `InitialDiscreteValue`: a direct assignment applied as an initialization
  update row. It is accepted only when the right-hand side reads `time`,
  parameters, constants and enumeration literals
  (`analysis/initial_algorithms.rs::has_only_initialization_settled_reads`),
  and the solve lowering (`rumoca-phase-solve/src/lower/initial_discrete.rs`)
  relies on that.
- the numeric initialization residual system, which only takes Real
  residuals and only solves for continuous unknowns and `fixed = false`
  parameters.

`y = not clr and u` reads two Boolean inputs, so it falls through to the
residual system: "ED020 expected a numeric expression, found Boolean".
`pre(y) = u` matches both sides as discrete targets: "ED013 an initial
equation relating two unsettled discrete coordinates". A discrete *Real*
defined the same way fails at runtime instead ("row reads a coordinate
outside the planned initialization unknown space: a discrete-time coordinate
or its `pre`").

## Why it was not fixed here

The direct-assignment owner would be correct if its row were re-evaluated
inside the initialization fixed point (`settle_initialization_system`), and
if the coordinates it reads were current there. They are not.
`settle_initialization_system` applies only `initialization.update_rhs` and
the numeric projection. Discrete values defined by ordinary equations
(`u = x > 0.5`) are runtime assignments, applied in the event iteration
*after* initialization. Accepting `y = not clr and u` as an update row would
read the inputs' declared start values. That gives a wrong initial `y`
without any error.

The fix that keeps this exact: have the initialization fixed point also apply
the discrete runtime assignments (with relation memory initialized from the
initial state), then admit discrete definitions whose reads are discrete
coordinates, inputs or algebraics as update rows, and drop the
"settled reads" restriction in the DAE constructor and the solve lowering. That
is a solver/runtime change affecting every model's initialization, so it
needs a full simulation-suite run. Not done in this cluster pass.

## Test

None (no behaviour change).
