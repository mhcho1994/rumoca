"""`modelsan check`: sanitize one model from the command line, like
`gcc -fsanitize=...`.

    modelsan check Tank.mo --model Tank
    modelsan check Tank.mo --model Tank -fsanitize=domain,range --stop-time 10
    modelsan check Pkg/package.mo --model Pkg.Examples.Demo --source-root MSL -fsanitize=all

The model is compiled with Rumoca, the selected sanitizers analyse it and
watch one nominal simulation, and each finding is printed as a compiler-style
diagnostic. Static-only selections do not run the simulator. Exit status:
0 no violation or coverage gap, 1 actionable findings, 2 incomplete checking
or command misuse. Completed static findings survive a runtime failure.
"""
from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
import re
import subprocess
import sys
import tempfile

from . import sanitizers as san
from .backends.process import execute
from .findings.finding import Severity

#: Every sanitizer `check` can run, by its short name. BehaviorSan needs a
#: contract file and is run by `modelsan contracts`; the comparative oracles
#: need several executions and are run through `Pipeline.run_comparative`.
SANITIZERS = {
    cls().name: cls
    for cls in (*san.DEFAULT, san.QuantitySan, san.DimensionSan, san.StructureSan,
                san.NetworkSan, san.InitStaticSan)
}

GROUPS = {
    "default": [cls().name for cls in san.DEFAULT],
    "all": sorted(SANITIZERS),
    # Analyses that need no simulation result.
    "static": ["singularity", "structure", "dimension", "quantity", "network",
               "init-static", "divisor", "discontinuity"],
    # Watchers of the simulation.
    "runtime": ["domain", "numeric", "range", "solver", "assert", "event", "zeno",
                "physical"],
}


def selected(spec: str) -> list[str]:
    """Expand `-fsanitize=` text: names and groups, comma-separated; a
    leading `-` removes one (`all,-network`)."""
    names: list[str] = []
    for item in filter(None, (part.strip() for part in spec.split(","))):
        remove = item.startswith("-")
        item = item.lstrip("-")
        expanded = GROUPS.get(item, [item])
        unknown = [name for name in expanded if name not in SANITIZERS]
        if unknown:
            known = ", ".join(sorted(GROUPS) + sorted(SANITIZERS))
            raise ValueError(f"unknown sanitizer `{unknown[0]}`; known: {known}")
        for name in expanded:
            if remove and name in names:
                names.remove(name)
            elif not remove and name not in names:
                names.append(name)
    return names


def add_parser(commands) -> None:
    check = commands.add_parser(
        "check", help="sanitize one model (like gcc -fsanitize=...)",
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    check.add_argument("source", type=Path, help="Modelica file (or a library's package.mo)")
    check.add_argument("--model", required=True, help="model to check, e.g. Pkg.Examples.Demo")
    check.add_argument("-fsanitize", "--sanitize", default="default", metavar="LIST",
                       help="comma-separated sanitizers or groups "
                            f"({', '.join(GROUPS)}); prefix `-` to remove one. "
                            "Default: default")
    check.add_argument("--source-root", action="append", default=[],
                       help="library directory to load (repeatable), e.g. the MSL")
    check.add_argument("--stop-time", type=float, default=1.0)
    check.add_argument("--start-time", type=float, default=0.0)
    check.add_argument("--timeout", type=float, default=300.0)
    check.add_argument("--rumoca", default="rumoca",
                       help="Rumoca executable (default: `rumoca` on PATH)")
    check.add_argument("--json", type=Path, help="also write the findings as JSON")
    check.add_argument("--list", action="store_true", help="list sanitizers and groups, then exit")


def run(args) -> int:
    if args.list:
        for group, names in GROUPS.items():
            print(f"{group:8} {', '.join(names)}")
        return 0
    try:
        names = selected(args.sanitize)
        if not names:
            raise ValueError('select at least one sanitizer')
        if not math.isfinite(args.timeout) or args.timeout <= 0:
            raise ValueError('timeout must be finite and positive')
    except ValueError as error:
        print(f"modelsan: {error}", file=sys.stderr)
        return 2
    from rumoca_bitcode import Model
    from .backends.rumoca import RumocaBackend
    from .pipeline import Pipeline

    with tempfile.TemporaryDirectory(prefix="modelsan-") as work:
        artifact = Path(work) / "model.rbc"
        command = [args.rumoca, "compile", str(args.source), "--model", args.model,
                   "--emit-bitcode", str(artifact), "--pass", "none",
                   "--no-fold-parameter-bindings"]
        for root in args.source_root:
            command += ["--source-root", root]
        try:
            compiled = execute(command, Path.cwd(), args.timeout)
        except subprocess.TimeoutExpired:
            print(f'modelsan: compilation exceeded {args.timeout:g}s', file=sys.stderr)
            return 2
        except OSError as error:
            print(f'modelsan: cannot launch compiler: {error}', file=sys.stderr)
            return 2
        if compiled.returncode != 0:
            sys.stderr.write(compiled.stderr)
            print(f"modelsan: {args.model} did not compile", file=sys.stderr)
            return 2
        model = Model.load(artifact)
        registry = san.SanitizerRegistry()
        for name in names:
            registry.register(SANITIZERS[name]())
        static_only = set(names) <= set(GROUPS['static'])
        if static_only:
            outcome = Pipeline(registry, None).run_static(model)
        else:
            try:
                backend = RumocaBackend(args.rumoca, t_start=args.start_time, t_end=args.stop_time,
                                        timeout=args.timeout, source_roots=args.source_root)
            except ValueError as error:
                print(f'modelsan: {error}', file=sys.stderr)
                return 2
            try:
                outcome = Pipeline(registry, backend).run(model, str(artifact), model.name)
            finally:
                backend.close()
        findings = [finding for bug in outcome.database.bugs for finding in bug.findings]
        for finding in findings:
            print(diagnostic(finding, args.source))
        gaps = outcome.coverage
        violations = any(f.severity is not Severity.INFO for f in findings)
        status = 'violated' if violations else 'incomplete' if gaps else 'no-violation-observed'
        for component, reason in gaps.items():
            print(f"note: not checked: {component}: {reason}", file=sys.stderr)
        print(f"{len(findings)} finding(s) from {len(names)} sanitizer(s)", file=sys.stderr)
        if args.json:
            args.json.write_text(json.dumps({
                "model": args.model, "sanitizers": names,
                "status": status, "mode": 'static' if static_only else 'static-and-runtime',
                "executions": [dict(status=r.status.value, phase=r.phase.value,
                    failure=r.failure.raw if r.failure else None) for r in outcome.results],
                "findings": [as_json(finding) for finding in findings],
                "not_checked": gaps}, indent=2, default=str) + "\n")
    return 1 if violations else 2 if gaps else 0


def diagnostic(finding, source: Path) -> str:
    """`file:line: severity: [sanitizer] kind: evidence`, the shape compilers use."""
    location = next(iter(finding.source_locations), None)
    where = f"{location.file}:{location.line}" if location else str(source)
    detail = brief(finding.evidence)
    text = f"{where}: {finding.severity.value}: [{finding.sanitizer}] {finding.kind}"
    if finding.time is not None:
        text += f" at t={finding.time:g}"
    return f"{text}: {detail}" if detail else text


def brief(evidence: dict) -> str:
    for key in ("reason", "shape", "message", "detail"):
        value = evidence.get(key)
        if isinstance(value, str) and value:
            return clean(value)
    pairs = [f"{k}={v}" for k, v in evidence.items()
             if isinstance(v, (int, float, str)) and k not in ("raw", "tool_side")]
    return ", ".join(pairs[:4])


def as_json(finding) -> dict:
    return {
        "sanitizer": finding.sanitizer, "kind": finding.kind,
        "severity": finding.severity.value, "time": finding.time,
        "source": [f"{loc.file}:{loc.line}" for loc in finding.source_locations],
        "evidence": finding.evidence,
    }


def clean(text: str) -> str:
    """One readable line from a tool message: no terminal colours, no
    internal span dumps, starting at the failure itself when there is one."""
    text = re.sub(r"\x1b\[[0-9;]*m", "", text).replace("│", " ")
    text = re.sub(r"@ Span \{[^}]*\}", "", text)
    marker = text.find("simulation failed:")
    if marker >= 0:
        text = text[marker:]
    return " ".join(text.split())[:200]
