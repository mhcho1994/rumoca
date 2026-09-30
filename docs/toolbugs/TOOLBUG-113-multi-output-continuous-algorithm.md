# TOOLBUG-113 — continuous algorithms with several outputs, loops or asserts

**Status:** fixed (this change).
**Severity:** medium — `[ED013] unsupported model algorithm in canonical DAE:
a multi-output algorithm requires one checked atomic vector-equation owner`
and `... declarative algorithm requires scalar assignments and conditionals`.

## What

```modelica
model BiotNumber            // TRANSFORM
  input Real alpha, L, lambda;
  Real Bi;
  output Real y;
algorithm
  Bi := alpha*L/lambda;
  y := Bi;
end BiotNumber;
```

A continuous (non-event) algorithm was accepted only with a single scalar
target and only as assignments and `if` statements. MLS §11.1.2 gives the
section the meaning of the values its targets hold when it finishes; with no
target read before it is definitely assigned, each final value is a closed
expression over the section's inputs, i.e. one declarative definition per
target. The same holds for a `for` loop over a settled integer range
(unrolled) and for a top-level `assert` that reads no target (CDL
`RealExtractor`).

Affected in the cluster: TRANSFORM `BiotNumber`, `BiotNumber_general`,
`SimpleCylinder`, AixLib `ShadowEffect` (loop), Buildings
`NumberOfRequests` (loop), G36 `Capacities` (assert via `RealExtractor`).

## Fix

- `analysis/model_algorithms.rs`: `ModelAlgorithmPlan::Declarative` carries
  `targets` and the settled `loop_ranges`; `analyze_multi_output_declarative`
  / `DeclarativeSequenceProof` prove the grammar, definite assignment on every
  path, no read-before-definition, settled loop ranges (at most 4096
  iterations), and assertions that read no target. A single-target algorithm
  that fails the old proof falls back to this one (its original error is kept
  if both fail).
- `model_algorithm.rs`: SSA replay keyed by target, conditional merge per
  target defined on all paths, loop unrolling with the index bound to an
  Integer literal.
- `algorithm_lowering.rs`: the section's top-level asserts are lowered as
  ordinary assertion owners.

## Test

`frontend_event_lowering.rs::a_multi_output_algorithm_defines_each_target_by_its_final_value`,
`...reading_a_target_before_writing_it_stays_refused`.
