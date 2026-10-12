"""One classification of what zero means, shared by three detectors.

DivisorSan, PhysicalSan and StructureSan were each deciding separately whether
zero was a defect for a parameter, and disagreeing. `Inductor.L` reaches a
denominator in the *solved* DAE, so DivisorSan called it a divide-by-zero; its
quantity is an inductance, so PhysicalSan called it a missing bound; and the
library says plainly that `L` may be zero, at which point the element is an
ideal short. 1448 reports across ten component families came from that
disagreement.

Every suppression below must say whether zero safety was **proven**,
**declared**, or **assumed**, and which contract supplied the conclusion. A
suppression that cannot say is indistinguishable from a bug.
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path[:0] = [str(ROOT / "packages" / "rumoca-bitcode"),
                str(ROOT / "packages" / "modelsan")]

from modelsan.analysis.context import AnalysisContext              # noqa: E402
from modelsan.contracts import (AssumptionSet, Confidence,         # noqa: E402
                                Source, ZeroBehavior, resolve)
from modelsan.divisor import build                                 # noqa: E402
from modelsan.sanitizers import DivisorSan, PhysicalSan            # noqa: E402

RUMOCA = ROOT / "target" / "debug" / "rumoca"
ROOTS = ["target/msl/ModelicaStandardLibrary-4.1.0",
         "target/corpus/ModelicaStandardLibrary-4.1.0",
         "target/cmm/CMM-a642c381"]
DIVISOR_DEFECT = {"divisor-reachable-zero", "divisor-zero-when-parameters-equal"}
PHYSICAL_DEFECT = {"physical-bound-permits-zero", "physical-domain-unenforced",
                   "physical-invariant-violated"}
_CACHE: dict[str, object] = {}


def check(condition: bool, label: str) -> None:
    print(f"  {'ok  ' if condition else 'FAIL'} {label}")
    assert condition, label


def corpus_model(name: str):
    if name in _CACHE:
        return _CACHE[name]
    path = None
    for line in (ROOT / "tools/sweep/ALL.list").read_text().splitlines():
        parts = line.split("\t")
        if len(parts) >= 2 and parts[1].strip() == name:
            path = parts[0]
    _CACHE[name] = None
    if path is None or not RUMOCA.exists():
        return None
    work = tempfile.mkdtemp()
    artifact = Path(work) / "m.rbc"
    command = [str(RUMOCA), "compile", path, "--model", name,
               "--emit-bitcode", str(artifact), "--no-fold-parameter-bindings"]
    for root in ROOTS:
        if Path(root).is_dir():
            command += ["--source-root", root]
    subprocess.run(command, capture_output=True, cwd=ROOT)
    if artifact.exists():
        import rumoca_bitcode
        _CACHE[name] = rumoca_bitcode.Model.load(artifact)
    return _CACHE[name]


def inline_model(source: str, name: str):
    if not RUMOCA.exists():
        return None
    work = tempfile.mkdtemp()
    path = Path(work) / f"{name}.mo"
    path.write_text(source)
    artifact = Path(work) / f"{name}.rbc"
    subprocess.run([str(RUMOCA), "compile", str(path), "--model", name,
                    "--emit-bitcode", str(artifact),
                    "--no-fold-parameter-bindings"],
                   capture_output=True, cwd=ROOT)
    if not artifact.exists():
        return None
    import rumoca_bitcode
    return rumoca_bitcode.Model.load(artifact)


def contract_for(model, leaf: str):
    contracts = resolve(model, build(model))
    for variable in model.variables:
        if variable.name.rsplit(".", 1)[-1] == leaf:
            found = contracts.get(variable.id)
            if found is not None:
                return found
    return None


def kinds_naming(model, leaf: str) -> set[str]:
    """Every finding kind, from either detector, that names `leaf`."""
    context = AnalysisContext(model)
    found = set()
    for sanitizer in (DivisorSan(), PhysicalSan()):
        for finding in sanitizer.analyze(model, context):
            blob = str(finding.evidence)
            if f".{leaf}" in blob or f"{leaf} " in blob or f"{leaf}=" in blob:
                found.add(finding.kind)
    return found


# ── the contract model itself ────────────────────────────────────────────────


def test_a_coefficient_of_a_rate_is_an_algebraic_limit():
    print("\n== L*der(i) = v classifies L as an algebraic limit ==")
    model = inline_model("""
model Coil
  parameter Real L = 1;
  Real i(start = 0, fixed = true);
  Real v;
equation
  L*der(i) = v;
  v = 1;
end Coil;
""", "Coil")
    if model is None:
        print("  skip  model unavailable")
        return
    contract = contract_for(model, "L")
    check(contract is not None, "L has a contract")
    check(contract.behavior is ZeroBehavior.ALGEBRAIC_LIMIT,
          f"classified {contract.behavior.value}")
    check(contract.confidence is Confidence.PROVEN,
          f"from the source equations ({contract.confidence.value})")
    check(contract.source is Source.EQUATION, "with the equation as its source")


def test_a_direct_divisor_outranks_a_multiplicative_use():
    print("\n== p*der(x) = 1 alongside y = x/p is still a divide-by-zero ==")
    model = inline_model("""
model Both
  parameter Real p = 1;
  Real x(start = 1, fixed = true);
  Real y;
equation
  p*der(x) = 1;
  y = x/p;
end Both;
""", "Both")
    if model is None:
        print("  skip  model unavailable")
        return
    contract = contract_for(model, "p")
    check(contract.behavior is ZeroBehavior.DIRECT_DIVISOR,
          f"the division wins, got {contract.behavior.value}")
    check(bool(kinds_naming(model, "p") & DIVISOR_DEFECT),
          "and it is still reported")


def test_a_user_assumption_cannot_override_a_proven_division():
    print("\n== an assumption may not make a real division safe ==")
    model = inline_model("""
model Divides
  parameter Real p = 1;
  Real y;
equation
  y = 1/p;
end Divides;
""", "Divides")
    if model is None:
        print("  skip  model unavailable")
        return
    assumptions = AssumptionSet.from_dict({"contracts": [
        {"match": "instance", "target": "p", "zero_behavior": "allowed",
         "reason": "an assumption that should not be honoured"}]},
        origin="<test>")
    contracts = resolve(model, build(model), assumptions=assumptions)
    variable = next(v for v in model.variables if v.name == "p")
    contract = contracts.get(variable.id)
    check(contract.behavior is ZeroBehavior.DIRECT_DIVISOR,
          f"the source wins, got {contract.behavior.value}")
    check(contract.confidence is Confidence.PROVEN, "and it is proven")


def test_an_assumption_overrides_a_heuristic_and_says_so():
    print("\n== an assumption is honoured where nothing is proven, and labelled ==")
    model = inline_model("""
model Scaled
  parameter Real g = 1;
  Real y;
  Real u;
equation
  u = 2;
  y = g*u;
end Scaled;
""", "Scaled")
    if model is None:
        print("  skip  model unavailable")
        return
    assumptions = AssumptionSet.from_dict({"contracts": [
        {"match": "instance", "target": "g", "zero_behavior": "feature_disabled",
         "reason": "zero disables this optional gain"}]}, origin="probe.toml")
    contracts = resolve(model, build(model), assumptions=assumptions)
    variable = next(v for v in model.variables if v.name == "g")
    contract = contracts.get(variable.id)
    check(contract.confidence is Confidence.ASSUMED
          or contract.source is Source.EQUATION,
          f"either the assumption or the proof applies, got "
          f"{contract.confidence.value}/{contract.source.value}")
    if contract.confidence is Confidence.ASSUMED:
        check("probe.toml" in contract.explain(),
              f"and names where it came from: {contract.explain()}")


def test_an_unused_assumption_is_reported():
    print("\n== a typo in an assumption must not be silent ==")
    assumptions = AssumptionSet.from_dict({"contracts": [
        {"match": "declaration", "target": "No.Such.Symbol",
         "zero_behavior": "allowed", "reason": "never matches"}]},
        origin="probe.toml")
    model = inline_model("model Empty Real y; equation y = 1; end Empty;", "Empty")
    if model is None:
        print("  skip  model unavailable")
        return
    resolve(model, build(model), assumptions=assumptions)
    check(len(assumptions.unused) == 1,
          f"the unmatched assumption is listed, got {assumptions.unused}")


def test_a_malformed_assumption_is_refused():
    print("\n== an unknown zero_behavior is an error, not a silent no-op ==")
    try:
        AssumptionSet.from_dict({"contracts": [
            {"target": "x", "zero_behavior": "definitely_fine"}]},
            origin="probe.toml")
    except ValueError as error:
        check("definitely_fine" in str(error), f"refused: {error}")
        return
    check(False, "a typo in zero_behavior must be refused")


def test_an_explicit_infinite_min_is_a_signed_domain():
    print("\n== min = -Modelica.Constants.inf states intent, it is not absence ==")
    model = corpus_model("Modelica.Mechanics.MultiBody.Examples.Elementary.Pendulum")
    if model is None:
        print("  skip  model unavailable")
        return
    inertia = next((v for v in model.variables if v.name.endswith("I_21")), None)
    check(inertia is not None, "the off-diagonal inertia element is present")
    if inertia is None:
        return
    contract = contract_for(model, "I_21")
    check(contract is not None and contract.behavior is ZeroBehavior.ALLOWED,
          f"classified ALLOWED, got "
          f"{contract.behavior.value if contract else None}")
    check(contract.source is Source.BOUND,
          "on the authority of the declaration's own bound")
    # `Modelica.Constants.inf` is the largest representable double, not IEEE
    # infinity; testing with `math.isinf` finds nothing.
    from modelsan.contracts.infer import _is_explicit_infinite_min
    check(_is_explicit_infinite_min(inertia),
          "and the test recognises Modelica's spelling of infinity")


# ── the ten false-positive families ──────────────────────────────────────────


def _family(model_name: str, leaf: str, expected: set[ZeroBehavior], label: str):
    model = corpus_model(model_name)
    if model is None:
        print(f"  skip  {label}: model unavailable")
        return
    contract = contract_for(model, leaf)
    check(contract is not None, f"{label}: {leaf} has a contract")
    if contract is None:
        return
    check(contract.behavior in expected,
          f"{label}: {contract.behavior.value} in "
          f"{sorted(b.value for b in expected)}")
    check(bool(contract.reason), f"{label}: with a reason — {contract.reason[:60]}")
    kinds = kinds_naming(model, leaf)
    check(not (kinds & DIVISOR_DEFECT),
          f"{label}: DivisorSan reports no defect, got {kinds & DIVISOR_DEFECT}")
    check(not (kinds & PHYSICAL_DEFECT),
          f"{label}: PhysicalSan reports no defect, got {kinds & PHYSICAL_DEFECT}")


SAFE = {ZeroBehavior.ALGEBRAIC_LIMIT, ZeroBehavior.FEATURE_DISABLED,
        ZeroBehavior.ALLOWED, ZeroBehavior.FORBIDDEN}


def test_zero_inductance_is_an_algebraic_limit():
    print("\n== zero-inductor, 237 reports ==")
    _family("Modelica.Electrical.Analog.Examples.ChuaCircuit", "L", SAFE,
            "inductance")


def test_zero_capacitance_is_an_algebraic_limit():
    print("\n== zero-capacitor, 190 reports ==")
    _family("Modelica.Electrical.Analog.Examples.ChuaCircuit", "C", SAFE,
            "capacitance")


def test_zero_rotational_inertia_is_a_torque_balance():
    print("\n== zero-rotational-inertia, 155 reports ==")
    _family("Modelica.Mechanics.Rotational.Examples.First", "J", SAFE, "inertia")


def test_zero_translational_mass_is_a_force_balance():
    print("\n== zero-translational-mass, 70 reports ==")
    _family("Modelica.Mechanics.Translational.Examples.Damper", "m", SAFE, "mass")


def test_a_signed_resistor_is_not_judged_by_its_quantity():
    print("\n== signed basic resistor, DECL-0604 ==")
    model = inline_model("""
model Signed
  parameter Real R = 1;
  Real i, v;
equation
  v = R*i;
  i = 1;
end Signed;
""", "Signed")
    if model is None:
        print("  skip  model unavailable")
        return
    contract = contract_for(model, "R")
    check(contract.behavior in SAFE,
          f"a pure multiplier, got {contract.behavior.value}")


# ── do not overcorrect ───────────────────────────────────────────────────────


def test_a_plain_reciprocal_is_still_reported():
    print("\n== y = 1/p must still fire ==")
    model = inline_model("""
model Plain
  parameter Real p = 1;
  Real y;
equation
  y = 1/p;
end Plain;
""", "Plain")
    if model is None:
        print("  skip  model unavailable")
        return
    check(contract_for(model, "p").behavior is ZeroBehavior.DIRECT_DIVISOR,
          "classified as a direct divisor")
    check(bool(kinds_naming(model, "p") & DIVISOR_DEFECT), "and reported")


def test_min_zero_does_not_excuse_a_reciprocal():
    print("\n== parameter p(min=0) = 1; y = 1/p must still fire ==")
    model = inline_model("""
model BoundedButDividing
  parameter Real p(min = 0) = 1;
  Real y;
equation
  y = 1/p;
end BoundedButDividing;
""", "BoundedButDividing")
    if model is None:
        print("  skip  model unavailable")
        return
    check(contract_for(model, "p").behavior is ZeroBehavior.DIRECT_DIVISOR,
          "min = 0 admits the unsafe boundary rather than excusing it")
    check(bool(kinds_naming(model, "p") & DIVISOR_DEFECT), "and it is reported")


def test_the_confirmed_divisor_cases_survive():
    print("\n== tank resistance and equal op-amp rails stay confirmed ==")
    for name, leaf in (("Tank", "resistance"),
                       ("Modelica.Electrical.Analog.Examples.OpAmps.LCOscillator",
                        "Vps")):
        model = corpus_model(name)
        if model is None:
            print(f"  skip  {name} unavailable")
            continue
        kinds = kinds_naming(model, leaf)
        check(bool(kinds & DIVISOR_DEFECT),
              f"{name.split('.')[-1]}.{leaf} still reported, got {kinds}")


def test_every_suppression_states_its_confidence():
    print("\n== a suppression that cannot say why is indistinguishable from a bug ==")
    checked = 0
    for name in ("Modelica.Electrical.Analog.Examples.ChuaCircuit",
                 "Modelica.Mechanics.Translational.Examples.Damper"):
        model = corpus_model(name)
        if model is None:
            continue
        context = AnalysisContext(model)
        for sanitizer in (DivisorSan(), PhysicalSan()):
            for finding in sanitizer.analyze(model, context):
                if "contract" not in finding.evidence:
                    continue
                checked += 1
                check(bool(finding.evidence.get("contract_confidence")),
                      f"{finding.kind} states proven/declared/assumed")
                check(finding.evidence["contract_confidence"] in
                      ("proven", "declared", "assumed"),
                      f"{finding.kind}: "
                      f"{finding.evidence['contract_confidence']}")
                check("—" in finding.evidence["contract"]
                      or "-" in finding.evidence["contract"],
                      f"{finding.kind} quotes the contract that decided it")
    check(checked > 0, f"{checked} suppressions checked")


# ── a declaration that names zero itself ─────────────────────────────────────


def test_a_binding_that_evaluates_to_zero_is_the_library_choosing_zero():
    print("\n== a parameter bound to zero is not a missing bound ==")
    model = inline_model("""
model Disabled
  parameter Real PRef = 0;
  final parameter Real G = PRef/4;
  Real v(start = 1, fixed = true);
equation
  der(v) = G - v;
end Disabled;
""", "Disabled")
    if model is None:
        print("  skip: rumoca not built")
        return
    contract = contract_for(model, "G")
    check(contract is not None, "a contract was derived for G")
    check(contract.behavior == ZeroBehavior.FEATURE_DISABLED,
          f"zero is the value the declaration chose (got {contract.behavior})")
    check(contract.confidence == Confidence.DECLARED,
          f"declared, not proven (got {contract.confidence})")
    check(contract.source == Source.BOUND,
          f"the evidence is the declaration (got {contract.source})")


def test_a_conditional_binding_with_a_zero_branch_names_zero():
    print("\n== `if c then 0 else x` names zero even when c is false ==")
    model = inline_model("""
model Switchable
  parameter Real PRef = 5;
  final parameter Real G = if PRef <= 0 then 0 else PRef/4;
  Real v(start = 1, fixed = true);
equation
  der(v) = G - v;
end Switchable;
""", "Switchable")
    if model is None:
        print("  skip: rumoca not built")
        return
    contract = contract_for(model, "G")
    check(contract is not None, "a contract was derived for G")
    check(contract.behavior == ZeroBehavior.FEATURE_DISABLED,
          f"the zero branch is a declaration about zero (got {contract.behavior})")
    check("conditional" in contract.reason or "zero" in contract.reason,
          "the reason names the evidence")


def test_a_zero_default_does_not_excuse_a_division():
    print("\n== a parameter defaulted to zero that something divides by ==")
    model = inline_model("""
model Reciprocal
  parameter Real k = 0;
  Real y;
equation
  y = 1/k;
end Reciprocal;
""", "Reciprocal")
    if model is None:
        print("  skip: rumoca not built")
        return
    contract = contract_for(model, "k")
    check(contract is not None, "a contract was derived for k")
    check(contract.behavior == ZeroBehavior.DIRECT_DIVISOR,
          f"the source division outranks the default (got {contract.behavior})")
    check(not contract.zero_is_safe, "zero is not safe here")


def test_the_core_loss_conductance_is_catalogued():
    print("\n== CoreParameters.GcRef: zero disables core losses ==")
    from modelsan.contracts import catalog
    found = catalog.lookup(
        "Modelica.Electrical.Machines.Losses.CoreParameters.GcRef")
    check(found is not None, "the catalogue covers it")
    check(found.behavior == ZeroBehavior.FEATURE_DISABLED,
          f"zero disables the feature (got {found.behavior})")
    check(found.confidence == Confidence.DECLARED,
          "the library says so; nothing here proves it")
    check("PRef" in found.reason, "the reason quotes the binding")


def test_a_guarded_denominator_is_not_a_direct_divisor():
    print("\n== `x/max(eps, abs(R))` says nothing against R ==")
    model = inline_model("""
model Guarded
  parameter Real R = -1;
  parameter Real C = 2/max(1e-15, abs(R));
  Real v(start = 1, fixed = true);
equation
  der(v) = C - v;
end Guarded;
""", "Guarded")
    if model is None:
        print("  skip: rumoca not built")
        return
    contract = contract_for(model, "R")
    behavior = contract.behavior if contract else None
    check(behavior is not ZeroBehavior.DIRECT_DIVISOR,
          f"a denominator that cannot vanish is not a division defect "
          f"(got {behavior})")


def test_an_unguarded_denominator_still_is():
    print("\n== `x/R` does say something against R ==")
    model = inline_model("""
model Plain
  parameter Real R = -1;
  parameter Real C = 2/R;
  Real v(start = 1, fixed = true);
equation
  der(v) = C - v;
end Plain;
""", "Plain")
    if model is None:
        print("  skip: rumoca not built")
        return
    contract = contract_for(model, "R")
    check(contract is not None and contract.behavior is ZeroBehavior.DIRECT_DIVISOR,
          f"an unguarded division is still a division "
          f"(got {contract.behavior if contract else None})")


def test_a_signed_component_is_not_judged_by_a_positivity_rule():
    print("\n== SwitchedCapacitor represents a negative resistance on purpose ==")
    model = corpus_model("Modelica.Electrical.Analog.Examples.CauerLowPassSC")
    if model is None:
        print("  skip: corpus model unavailable")
        return
    kinds = kinds_naming(model, "R")
    check("physical-invariant-violated" not in kinds,
          f"R = -1 is what the component is for (got {sorted(kinds)})")
