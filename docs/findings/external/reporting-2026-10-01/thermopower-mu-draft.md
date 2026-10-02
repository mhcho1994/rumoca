# Unused SecondaryController mu parameter has inconsistent declared dimensions

Proposed destination: `casella/ThermoPower`. Status: source-supported cleanup/clarification draft, not submitted. No simulation behavior defect demonstrated.

[SecondaryController in Test.mo](https://github.com/casella/ThermoPower/blob/e2b011ac7fd90f9cf5771f29f1aefa160550b6ee/ThermoPower/Test.mo#L11332) declares:

```modelica
final parameter SI.AngularFrequency mu = 5*Ts*Pnom/(f0*droop)
  "Integral controller gain";
```

`Ts` is time, `Pnom` is power, `f0` is frequency, and `droop` is dimensionless. Treating 5 as dimensionless gives W·s² for the expression, while the declared angular frequency has dimensions s⁻¹. The parameter is unused in this model. The implemented equation instead uses:

```modelica
der(powerOffset) = 5*Pnom/(Ts*droop)*(frequency-f0)/f0;
```

Thus `mu` also does not represent that equation's coefficient: its expression multiplies by `Ts` where the active equation divides by `Ts`. Removing the unused parameter may be the simplest cleanup if it is not part of a supported external interface. Otherwise please clarify its intended meaning, dimensions, and expression. This finding alone does not justify changing the active equation.

The declaration is present in the pinned evaluated source and a live master read on 2026-10-01. The static campaign reports it in the standalone controller and a containing electrical test; these are one declaration-level issue.

Independent check: a wrapper supplies `Pnom=40e6`, `Ts=300`, `frequency=50` and an inner `System`. OpenModelica 1.27.1~2-g6db4671 with Modelica 3.2.3 and `--unitChecking` builds it without a unit warning. No dynamic effect of this unused declaration has been demonstrated. [Wrapper](evidence/ThermoPowerReview.mo), [build transcript](build-results.json).

Targeted searches for SecondaryController and mu/unit did not find an exact existing report; this is not an exhaustive novelty claim.
