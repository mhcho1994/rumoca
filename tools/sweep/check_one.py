#!/usr/bin/env python3
"""Run the static sanitizers over ONE model, and print what they find.

This is the command every report in `docs/v2/bugs/` points at. A reader holding a
report needs to reproduce exactly that one finding, and the corpus sweep is the
wrong instrument for that: it takes half an hour and prints 5000 lines.

    tools/sweep/check_one.py Modelica.Mechanics.Translational.Examples.Damper
    tools/sweep/check_one.py <model> --target mass1.m
    tools/sweep/check_one.py <model> --sanitizer divisor --json

The model is named, not pathed: the path is looked up in `tools/sweep/ALL.list`,
so a reader does not have to know where in the corpus a model lives. A path is
accepted too, with `--model` to say which class inside it to compile.
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path[:0] = ["packages/rumoca-bitcode", "packages/modelsan"]

RUMOCA = "./target/debug/rumoca"
ROOTS = ["target/msl/ModelicaStandardLibrary-4.1.0",
         "target/corpus/ModelicaStandardLibrary-4.1.0",
         "target/cmm/CMM-a642c381"]

#: Every static sanitizer, by the name it reports itself under. The name is the
#: one that appears in a report's "Found by" row, so the two cannot drift.
def sanitizers():
    from modelsan.sanitizers import (
        DimensionSan, DivisorSan, InitStaticSan, NetworkSan, PhysicalSan,
        StructureSan)
    return {
        "physical": PhysicalSan,
        "divisor": DivisorSan,
        "network": NetworkSan,
        "structure": StructureSan,
        "dimension": DimensionSan,
        "init-static": InitStaticSan,
    }


def locate(model: str) -> str | None:
    """The corpus path declaring `model`, from the sweep's own list."""
    for line in Path("tools/sweep/ALL.list").read_text().splitlines():
        parts = line.split("\t")
        if len(parts) >= 2 and parts[1].strip() == model:
            return parts[0]
    return None


def compile_model(path: str, model: str, artifact: Path, keep_chains: bool):
    command = [RUMOCA, "compile", path, "--model", model,
               "--emit-bitcode", str(artifact)]
    for root in ROOTS:
        if Path(root).is_dir():
            command += ["--source-root", root]
    if keep_chains:
        # Without this a derived parameter binding is folded to its value, so
        # a finding names `d` (which a user cannot set) rather than `k` (which
        # they can). The sweep passes it, so a reader reproducing a sweep
        # finding must too.
        command.append("--no-fold-parameter-bindings")
    done = subprocess.run(command, capture_output=True, text=True)
    if done.returncode or not artifact.exists():
        return done
    return None


def _assumptions(args):
    """The user's zero contracts, unless the run is asked to prove everything."""
    if args.no_assumptions or not args.assumptions:
        return None
    from modelsan.contracts import AssumptionSet
    return AssumptionSet.load(args.assumptions)


def _explain(model, context, symbol: str) -> int:
    """Every candidate contract for one symbol, and the one that won."""
    from modelsan.contracts import resolve
    from modelsan.divisor import build

    contracts = resolve(model, build(model),
                        assumptions=getattr(context, "assumptions", None))
    matches = [v for v in model.variables
               if v.name == symbol or v.name.endswith("." + symbol)]
    if not matches:
        print(f"{symbol} is not a symbol of this model", file=sys.stderr)
        return 2
    for variable in matches:
        print(f"{variable.name}")
        print(contracts.explain(variable.id))
        print()
    unused = getattr(getattr(context, "assumptions", None), "unused", [])
    for assumption in unused:
        print(f"  warning: assumption {assumption.target!r} matched nothing",
              file=sys.stderr)
    return 0


def names_in(evidence: dict) -> set[str]:
    """Every variable or parameter name a finding's evidence refers to.

    Matched as whole dotted names, not as substrings: filtering for `f` with a
    substring test hits the `f` inside `InvertingAmp.mo` and inside the word
    "of", and returns every finding in the model.
    """
    names: set[str] = set()
    if evidence.get("parameter"):
        names.add(str(evidence["parameter"]))
    # A divisor finding names an assignment rather than a single parameter.
    for piece in str(evidence.get("witness", "")).split(","):
        if "=" in piece:
            names.add(piece.split("=")[0].strip())
    for key in ("required", "where", "observed", "variable", "state", "equation",
                "denominator"):
        for token in re.findall(r"[A-Za-z_]\w*(?:\.\w+|\[\d+\])*", str(evidence.get(key, ""))):
            names.add(token)
    for part in str(evidence.get("path", "")).split("->"):
        if part.strip():
            names.add(part.strip())
    return names


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run the static sanitizers over one model.")
    parser.add_argument("model", help="model name, or a .mo path with --model")
    parser.add_argument("--model", dest="class_name",
                        help="class to compile, when the first argument is a path")
    parser.add_argument("--sanitizer", action="append", default=[],
                        help="restrict to these (repeatable); default is all static ones")
    parser.add_argument("--target",
                        help="only findings naming this variable or parameter")
    parser.add_argument("--keep-parameter-chains", action="store_true",
                        help="keep parameter bindings symbolic, as the sweep does")
    parser.add_argument("--json", action="store_true", help="machine-readable output")
    parser.add_argument("--assumptions", metavar="TOML",
                        help="a semantics file carrying [[contracts]] entries")
    parser.add_argument("--no-assumptions", action="store_true",
                        help="ignore every user contract; a source-proved run")
    parser.add_argument("--explain-contract", metavar="SYMBOL",
                        help="show how this symbol's zero contract was chosen, "
                             "and every candidate that was considered")
    parser.add_argument("--keep-artifact", metavar="PATH",
                        help="also write the bitcode here, for `rumoca bitcode disasm`")
    args = parser.parse_args()

    if args.model.endswith(".mo"):
        path, model = args.model, args.class_name or Path(args.model).stem
    else:
        model = args.model
        path = locate(model)
        if path is None:
            print(f"{model} is not in tools/sweep/ALL.list; pass its .mo path "
                  f"and --model instead", file=sys.stderr)
            return 2

    available = sanitizers()
    unknown = [s for s in args.sanitizer if s not in available]
    if unknown:
        print(f"unknown sanitizer {unknown}; choose from {sorted(available)}",
              file=sys.stderr)
        return 2
    chosen = args.sanitizer or ["physical", "divisor"]

    with tempfile.TemporaryDirectory() as work:
        artifact = Path(work) / "m.rbc"
        failed = compile_model(path, model, artifact, args.keep_parameter_chains)
        if failed is not None:
            print(f"{model} does not compile under Rumoca:", file=sys.stderr)
            print((failed.stdout + failed.stderr)[-1200:], file=sys.stderr)
            return 1
        if args.keep_artifact:
            Path(args.keep_artifact).write_bytes(artifact.read_bytes())

        import rumoca_bitcode
        from modelsan.analysis import AnalysisContext
        from modelsan.findings.signature import attach

        dae = rumoca_bitcode.Model.load(artifact)
        context = AnalysisContext(dae)
        context.assumptions = _assumptions(args)

        if args.explain_contract:
            return _explain(dae, context, args.explain_contract)

        findings = []
        for name in chosen:
            findings.extend(available[name]().analyze(dae, context))
        attach(findings)

    rows = []
    for finding in findings:
        evidence = {k: (v if isinstance(v, (int, float, bool, type(None)))
                        else str(v))
                    for k, v in finding.evidence.items()}
        where = str(finding.source_locations[0]) if finding.source_locations else ""
        if args.target and args.target not in names_in(evidence):
            continue
        rows.append({"signature": finding.signature, "sanitizer": finding.sanitizer,
                     "kind": finding.kind, "severity": finding.severity.value,
                     "source": where, "evidence": evidence})

    if args.json:
        print(json.dumps({"model": model, "findings": rows}, indent=1))
        return 0

    print(f"{model}")
    print(f"  {len(dae.variables)} variables, {len(dae.equations)} equations, "
          f"{len(getattr(dae, 'equation_families', []) or [])} equation families")
    print(f"  sanitizers: {', '.join(chosen)}")
    for assumption in getattr(getattr(context, "assumptions", None), "unused", []):
        print(f"  warning: assumption {assumption.target!r} matched nothing")
    if args.target:
        print(f"  filtered to: {args.target}")
    print(f"  {len(rows)} finding(s)\n")
    for row in rows:
        print(f"  [{row['severity']:<6}] {row['kind']}   ({row['sanitizer']})")
        print(f"            at {row['source'] or '?'}")
        for key, value in row["evidence"].items():
            print(f"            {key} = {value}")
        print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
