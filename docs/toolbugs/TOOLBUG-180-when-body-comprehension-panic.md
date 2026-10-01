# TOOLBUG-180 — panic on an array constructor in a when-equation body

**Status:** fixed.
**Severity:** high (compiler panic).

## What

```modelica
when {initial(), trigger} then
  index = mod(pre(iSample), n) + 1;
  ySample = {if i == index then u else pre(ySample[i]) for i in 1:n};
  iSample = pre(iSample) + 1;
end when;
```

The DAE analysis planned array-comprehension domains only for plain
equations, bindings and assertions; when-chain bodies were never visited, so
lowering hit `expect("analysis proves the exact comprehension occurrence")`
at `construction/expression.rs:1809`. The same hole existed for array
constructors inside replayed initial-algorithm values (e.g.
`off := sum({days[i] for i in 1:m})`).

Affected: IBPSA/Buildings/AixLib CDL `Discrete.TriggeredMovingMean` and every
model that instantiates it.

## Fix

`crates/rumoca-phase-dae/src/construction/analysis/comprehensions.rs`:
`when_chain_expressions` collects activation conditions and every body
operand (nested if-equations included); `extend_comprehensions` adds the
replayed initial-algorithm values, assertions and parameter bindings.
`analysis.rs` feeds both into the comprehension plans.

## Test

`suite_core/frontend_event_lowering.rs::an_array_constructor_in_a_when_body_writes_the_selected_slot`
(simulates the ring buffer and checks each slot and the wrap-around).
