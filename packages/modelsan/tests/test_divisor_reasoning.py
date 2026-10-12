"""`constraints AND path AND denominator == 0`, decided three ways.

A divide-by-zero analysis asks a satisfiability question, and there are three
honest answers to it. Collapsing them to two is what produced the false
positives these tests exist to prevent:

    SAT      report, with the assignment that satisfies all three conjuncts
    UNSAT    suppress, and record the proof
    UNKNOWN  report as unresolved, never as confirmed

Every case below is one an external review named. The false-positive cases must
come back UNSAT or UNKNOWN; the true-positive cases must come back SAT with a
witness.
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
from modelsan.divisor import build, classify, collect, interval_of  # noqa: E402
from modelsan.sanitizers import DivisorSan                         # noqa: E402

RUMOCA = ROOT / "target" / "debug" / "rumoca"
ROOTS = ["target/msl/ModelicaStandardLibrary-4.1.0",
         "target/corpus/ModelicaStandardLibrary-4.1.0",
         "target/cmm/CMM-a642c381"]
DEFECT = {"divisor-reachable-zero", "divisor-zero-when-parameters-equal"}
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
    if path is None or not RUMOCA.exists():
        _CACHE[name] = None
        return None
    work = tempfile.mkdtemp()
    artifact = Path(work) / "m.rbc"
    command = [str(RUMOCA), "compile", path, "--model", name,
               "--emit-bitcode", str(artifact), "--no-fold-parameter-bindings"]
    for root in ROOTS:
        if Path(root).is_dir():
            command += ["--source-root", root]
    subprocess.run(command, capture_output=True, cwd=ROOT)
    _CACHE[name] = None
    if artifact.exists():
        import rumoca_bitcode
        _CACHE[name] = rumoca_bitcode.Model.load(artifact)
    return _CACHE[name]


def verdicts_for(name: str, leaves: tuple[str, ...]):
    """Every site whose denominator or witness names one of `leaves`."""
    model = corpus_model(name)
    if model is None:
        return None
    environment = build(model)
    out = []
    for site in collect(model, environment):
        text = repr(site.denominator)
        verdict = classify(site, environment)
        witness = (verdict.witness.rendered(environment.names)
                   if verdict.witness else "")
        names = {t.split("=")[0].strip().rsplit(".", 1)[-1]
                 for t in witness.split(",") if "=" in t}
        if names & set(leaves) or any(f".{leaf}" in text or text == leaf
                                      for leaf in leaves):
            out.append(verdict)
    return out


def kinds_for(name: str, leaf: str) -> set[str] | None:
    model = corpus_model(name)
    if model is None:
        return None
    found = set()
    for finding in DivisorSan().analyze(model, AnalysisContext(model)):
        witness = finding.evidence.get("witness", "")
        if any(p.split("=")[0].strip().rsplit(".", 1)[-1] == leaf
               for p in witness.split(",") if "=" in p):
            found.add(finding.kind)
    return found


# ── the simplification rules, directly ───────────────────────────────────────


def test_interval_propagation_proves_a_max_guard_nonzero():
    print("\n== max(eps*oneOhm, abs(R)) is at least eps  (switched-capacitor) ==")
    verdicts = verdicts_for(
        "Modelica.Electrical.Analog.Examples.CauerLowPassSC", ("R", "oneOhm"))
    if verdicts is None:
        print("  skip  model unavailable")
        return
    guarded = [v for v in verdicts if v.status == "UNSAT"]
    check(bool(guarded), f"proved UNSAT, got {[v.status for v in verdicts]}")
    check(any("excludes zero" in v.proof for v in guarded),
          f"by interval propagation: {guarded[0].proof[:90]}")
    check(not any(v.status == "SAT" for v in verdicts),
          "and nothing is reported as satisfiable")


def test_a_zero_coefficient_does_not_zero_a_sum():
    print("\n== 1 + alpha*(T - T_ref) at alpha = 0 is 1 ==")
    kinds = kinds_for("Modelica.Electrical.Analog.Examples.ChuaCircuit", "alpha")
    if kinds is None:
        print("  skip  model unavailable")
        return
    check(not (kinds & DEFECT), f"no defect claim, got {kinds}")


def test_zeroing_one_operand_does_not_zero_a_composite():
    print("\n== 1 + c_b*B_N + B_N^n needs more than c_b = 0 ==")
    kinds = kinds_for(
        "Modelica.Magnetic.FluxTubes.Examples.BasicExamples.QuadraticCoreAirgap",
        "c_b")
    if kinds is None:
        print("  skip  model unavailable")
        return
    check(not (kinds & DEFECT), f"no defect claim, got {kinds}")


def test_an_assertion_puts_zero_outside_the_domain():
    print("\n== assert(abs(k) >= small) protects 1/(k*Ni)  (limPID) ==")
    kinds = kinds_for("ModelicaTest.Blocks.Continuous", "k")
    if kinds is None:
        print("  skip  model unavailable")
        return
    check(not (kinds & DEFECT), f"no defect claim, got {kinds}")


# ── branch feasibility ───────────────────────────────────────────────────────


def test_a_zero_duration_empties_the_ramp_branch():
    print("\n== Ramp: duration = 0 makes time >= startTime and time < startTime "
          "contradictory ==")
    verdicts = verdicts_for(
        "Modelica.Clocked.Examples.SimpleControlledDrive.ClockedWithDiscreteController",
        ("duration",))
    if verdicts is None:
        print("  skip  model unavailable")
        return
    check(any(v.status == "UNSAT" for v in verdicts),
          f"proved unreachable, got {[v.status for v in verdicts]}")
    unsat = next(v for v in verdicts if v.status == "UNSAT")
    check("cannot hold together" in unsat.proof or "false under" in unsat.proof,
          f"and the proof names the contradiction: {unsat.proof[:100]}")


def test_a_path_the_artifact_cannot_decide_is_unresolved():
    print("\n== Trapezoid: T_start is a `when` variable, so the branch is "
          "undecidable ==")
    verdicts = verdicts_for("Modelica.Electrical.Analog.Examples.NandGate",
                            ("rising", "falling"))
    if verdicts is None:
        print("  skip  model unavailable")
        return
    check(bool(verdicts), "the divisions are found")
    check(not any(v.status == "SAT" for v in verdicts),
          f"none is claimed as a defect, got {[v.status for v in verdicts]}")
    unknown = [v for v in verdicts if v.status == "UNKNOWN"]
    check(bool(unknown), "they are unresolved rather than suppressed")
    check("does not determine" in unknown[0].proof,
          f"and the reason is named: {unknown[0].proof[:100]}")
    # An unresolved site must not be reported under a defect kind.
    kinds = kinds_for("Modelica.Electrical.Analog.Examples.NandGate", "rising")
    check(not (kinds & DEFECT),
          f"the finding kind is not a defect claim, got {kinds}")


# ── the true positives ───────────────────────────────────────────────────────


def test_equal_supply_rails_are_satisfiable():
    print("\n== Vps - Vns = 0 solves to Vps = Vns  (BUG-024) ==")
    verdicts = verdicts_for(
        "Modelica.Electrical.Analog.Examples.OpAmps.LCOscillator", ("Vps",))
    if verdicts is None:
        print("  skip  model unavailable")
        return
    sat = [v for v in verdicts if v.status == "SAT"]
    check(bool(sat), f"SAT, got {[v.status for v in verdicts]}")
    check(sat[0].witness is not None and sat[0].witness.residual == 0.0,
          "with a witness verified to zero the denominator")
    check("equal to" in sat[0].witness.rationale,
          f"and the witness is an equality: {sat[0].witness.rationale}")


def test_a_direct_unguarded_division_is_satisfiable():
    print("\n== level/resistance is unguarded  (FINDING-05128) ==")
    verdicts = verdicts_for("Tank", ("resistance",))
    if verdicts is None:
        print("  skip  model unavailable")
        return
    sat = [v for v in verdicts if v.status == "SAT"]
    check(bool(sat), f"SAT, got {[v.status for v in verdicts]}")
    check("not inside any branch" in sat[0].proof,
          f"with no path condition to satisfy: {sat[0].proof}")


def test_every_reported_defect_carries_the_four_required_parts():
    print("\n== denominator, active path, constraints, witness ==")
    checked = 0
    for name in ("Modelica.Electrical.Analog.Examples.OpAmps.LCOscillator",
                 "Modelica.Electrical.Analog.Examples.ChuaCircuit",
                 "Modelica.Electrical.Analog.Examples.NandGate"):
        model = corpus_model(name)
        if model is None:
            continue
        for finding in DivisorSan().analyze(model, AnalysisContext(model)):
            checked += 1
            evidence = finding.evidence
            for field in ("denominator", "path_condition", "constraints",
                          "witness", "verdict", "proof"):
                check(field in evidence,
                      f"{finding.kind} carries {field}")
            check(evidence["verdict"] in ("SAT", "UNSAT", "UNKNOWN"),
                  f"{finding.kind} verdict is three-valued: {evidence['verdict']}")
            if finding.kind in DEFECT:
                check(evidence["verdict"] == "SAT",
                      f"{finding.kind} is only claimed when SAT")
                check(abs(evidence["denominator_at_witness"]) < 1e-12,
                      f"{finding.kind} has a verified witness")
    check(checked > 0, f"{checked} findings checked")
