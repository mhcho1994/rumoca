# TOOLBUG-110 — a relational `when` statement failed solve lowering

**Status:** fixed (this change).
**Severity:** high — compiles, then `rumoca sim` refuses the model. The
pattern (`when x < 0.5 then n := pre(n) + 1; end when;` in an algorithm
section) is common in Buildings/IBPSA/AixLib controllers.

## What

```modelica
model Alg2
  Real x(start = 1, fixed = true);
  discrete Real n(start = 0, fixed = true);
equation
  der(x) = -2;
algorithm
  when x < 0.5 then
    n := pre(n) + 1;
  end when;
end Alg2;
```

`rumoca sim alg2.mo --model Alg2 --t-end 5` failed with
`[EL005] DAE system is not computable: model-event transaction has no
construction-issued final definition for every target under exact
periodic-clock activation`.

DAE construction lowers every event algorithm twice: each assignment as the
equation-shaped definition its when-equation spelling would have
(`lower_when_assignment`), and the whole section as one model-event
transaction whose steps keep the sequential order. Solve lowering can execute
a transaction only when every step is activated by a periodic clock
(`eligible_event_transaction`), and it replaces the per-statement definitions
by the transaction, so a transaction under a relational activation was a hard
error even when the per-statement definitions already meant exactly the
algorithm.

## Fix

`crates/rumoca-phase-dae/src/construction/algorithm_lowering.rs`,
`event_projections_are_exact`: the transaction is not issued when the
per-statement definitions are provably the algorithm's meaning — no periodic
clock step, no `elsewhen` (branch priority, MLS §8.3.5), every target defined
by exactly one statement, and all discrete-valued targets (which share one
B.1c owner) written under one activation. The algorithm is then exactly the
when-equations it spells. Every other shape keeps the transaction and the
existing refusal (e.g. `an_algorithm_section_chain_without_an_activation_owner_is_rejected`
still passes). `rumoca-ir-dae` gained read accessors on `ModelEventStep`.

Result: `n` becomes 1 at `t = 0.25` and stays 1 (rising edge only), as in
OpenModelica.

## Test

`crates/rumoca/tests/suite_core/frontend_event_lowering.rs`:
`relational_when_statement_on_a_discrete_real_fires_once_at_its_crossing`
(discrete `Real` and `Integer` targets).

## Known remaining

A relational `when` chain with `elsewhen`, several whens writing one target,
or a sequential read of a target written earlier in the same algorithm
(PowerGrids `LimiterWithLag`, Buildings `StateMachineVoltCtrl`, ThermoSysPro
`Table1DTempsBool`) still needs a model-event transaction that solve lowering
can execute under non-clock activations.
