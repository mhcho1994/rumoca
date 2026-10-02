# Clarify dimensions of the default derivative time constant in CDL rate limiters

Proposed destination: `lbl-srg/modelica-buildings`. Status: source-supported clarification draft, not submitted. No incorrect simulation trajectory established.

In [LimitSlewRate](https://github.com/lbl-srg/modelica-buildings/blob/a3cfdde4e2fa1605f351875c2199b6aafaee7fe0/Buildings/Controls/OBC/CDL/Reals/LimitSlewRate.mo), `raisingSlewRate` has `unit="1/s"`, while `Td` has `quantity="Time", unit="s"` and defaults to `raisingSlewRate*10`. [Ramp](https://github.com/lbl-srg/modelica-buildings/blob/a3cfdde4e2fa1605f351875c2199b6aafaee7fe0/Buildings/Controls/OBC/CDL/Reals/Ramp.mo) declares the same units with `Td=raisingSlewRate*0.001`.

If the numeric factors are dimensionless, the right-hand sides have rate dimensions rather than time dimensions. If those factors represent dimensioned empirical coefficients, their units and intended scaling are currently implicit. Both blocks use `(u-y)/Td`, and describe `Td` as a derivative time constant.

Could the intended scaling and coefficient units be clarified? Depending on that intent, a repair might declare a dimensioned coefficient or revise the time-constant expression. We have not established that taking the reciprocal is the correct numerical change and do not propose changing behavior on dimensional inference alone.

The expressions are present at the pinned revision and in live master source reads on 2026-10-01. Corresponding declarations occur in IBPSA, IDEAS and AixLib. The static campaign's repeated instantiated warnings should be deduplicated to these two definitions and their shared lineage.

Independent check: wrappers supplying `raisingSlewRate=1` and connected inputs build with OpenModelica 1.27.1~2-g6db4671 and Modelica 4.1.0 using `--unitChecking`; **no unit warning was emitted**. This report is therefore a source-level units/contract question, not an independently diagnosed compiler error or demonstrated physical failure. [Build transcript](build-results.json), [wrapper](evidence/Review.mo).

Targeted issue searches found no exact units report. Existing issue #4317 concerns Ramp/LimitSlewRate documentation and may be relevant context before opening a separate ticket.
