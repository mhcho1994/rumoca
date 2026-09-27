"""Saved-program observation transport; never re-lowers numerical edits.

SPEC_0007 stage 4 / writable-execution-ir: effects are execution-owned and
equation edits invalidate their derivation. Native validation is authoritative.
"""
from __future__ import annotations

import csv
import math
import json
import re
from pathlib import Path

from rumoca_bitcode.execution import Program

PASS_NAME = "modelsan.observe-variables"
FILENAME = "modelsan-observations.csv"


def observe(model, variable_ids):
    """Register one trace point per requested variable; return `{id: point}`.

    ModelSan is the producer of its own observation requests (execution IR v2
    D2): lowering takes no observation list, so a pass that wants a value
    registers the trace point that demands it. Idempotent by `(label,
    variable)`, so a replay reuses the points it made the first time.
    """
    variables = {v.id: v for v in model.variables}
    existing = {(p.label, p.variable.id): p.id for p in model.trace_points}
    points = {}
    for identifier in variable_ids:
        target = variables.get(identifier)
        if target is None:
            raise ValueError(f"no such variable to observe: {identifier}")
        label = f"{PASS_NAME}:{target.name}"
        known = existing.get((label, identifier))
        points[identifier] = known if known is not None else model.add_trace_point(
            target, label=label, added_by=PASS_NAME).id
    model.refresh()
    return points


def instrument(program, model, *, variable_ids):
    """Replayable execution pass: append observations without changing equations.

    `variable_ids` arrives as a list when a caller builds the pass and as the
    recorded text when replay supplies it: pass options are typed scalars, so
    a list of ids travels as one `text` option rather than as an untyped blob.
    """
    if isinstance(variable_ids, str):
        variable_ids = [int(part) for part in variable_ids.split(",") if part]
    variable_ids = list(variable_ids)
    if len(variable_ids) != len(set(variable_ids)):
        raise ValueError("duplicate observation variable ID")
    with program.builder(PASS_NAME, options={"variable_ids": ",".join(
            str(i) for i in variable_ids)}) as b:
        points = observe(model, variable_ids)
        sink = b.declare_csv_sink(
            key=PASS_NAME, filename=FILENAME,
            columns=[{"name": "time_s", "ty": "real"},
                     {"name": "publish_id", "ty": "integer"},
                     {"name": "phase", "ty": "text"},
                     *[{"name": f"v{i}", "ty": "real"} for i in variable_ids]],
            # Identity only: this sink is not connector instrumentation, so it
            # names no connector, and the runtime resolves names and units
            # from the equation IR when it writes the manifest.
            metadata={"members": [{"trace_point": points[i]} for i in variable_ids]},
        )
        with b.before_return("run_start") as ir:
            ir.emit("csv.open", sink=sink)
        with b.before_return("publish") as ir:
            values = [ir.emit("snapshot.time"), ir.emit("snapshot.sequence"),
                      ir.emit("snapshot.phase")]
            values += [ir.emit("snapshot.value", trace_point=points[i])
                       for i in variable_ids]
            ir.emit("csv.write_row", sink=sink, values=values)
        with b.before_return("run_finish") as ir:
            ir.emit("csv.close", sink=sink)


def prepare(source: Path, destination: Path, *, executable: str, timeout: float,
            replay=None):
    program = Program.load(source, executable=executable, timeout=timeout)
    program.validate()
    if any(sink["filename"] == "domain-diagnostics.json"
           for sink in program.raw["program"]["sinks"]):
        raise ValueError("domain-diagnostics.json is reserved by ModelSan")
    targets = [v for v in program.model.variables if not v.is_parameter]
    if not targets:
        raise ValueError("no traceable variables")
    # No re-lower for coverage: nothing was observed at lowering, because the
    # program is what demands observations. Replay is still explicit, for a
    # program whose referenced identities have changed.
    if replay is not None and any(p["id"] == PASS_NAME for p in program.raw["passes"]):
        program = program.relower(replay=replay)
    else:
        instrument(program, program.model, variable_ids=[v.id for v in targets])
    program.save(destination)
    return targets


def read_trace(path: Path, targets):
    """Reject malformed evidence; preserve NaN/Inf values for NumericSan."""
    if not path.exists():
        return [], {}
    expected = ["time_s", "publish_id", "phase", *[f"v{v.id}" for v in targets]]
    times, columns = [], {v.name: [] for v in targets}
    with path.open(newline="", encoding="utf-8") as stream:
        rows = csv.DictReader(stream)
        if rows.fieldnames != expected:
            raise ValueError("malformed executable observation header")
        for row in rows:
            if None in row or any(value is None for value in row.values()):
                raise ValueError("malformed executable observation row")
            t = float(row["time_s"])
            if (not math.isfinite(t) or (times and t <= times[-1])
                    or int(row["publish_id"]) != len(times)
                    or row["phase"] not in {"initial", "sample", "settled"}):
                raise ValueError("invalid executable publication coordinates")
            times.append(t)
            for v in targets:
                columns[v.name].append(float(row[f"v{v.id}"]))
    return times, columns


def read_domain_diagnostics(path, stream):
    """Execution identities are deliberately not presented as DAE expression IDs."""
    from ..runtime.anchors import BackendAnchor, EntityKind
    from ..runtime.observations import ExpressionObservation
    if not path.exists():
        return {"available": False}
    evidence = json.loads(path.read_text())
    if (not isinstance(evidence, dict) or evidence.get("schema_version") != 1 or evidence.get("kind") != "solve-domain-diagnostics"
            or evidence.get("coordinates") != "internal-evaluation"
            or evidence.get("execution_policy") != "interpreter"
            or evidence.get("coverage") != "scalar-output-rows"
            or type(evidence.get("unobserved_evaluations")) is not int
            or evidence["unobserved_evaluations"] < 0
            or type(evidence.get("truncated")) is not bool
            or not isinstance(evidence.get("faults"), list)):
        raise ValueError("unknown domain diagnostic schema")
    requirements = {"division": "nonzero", "sqrt": "non-negative", "log": "positive",
                    "inverse-trig": "unit-interval"}
    for fault in evidence["faults"]:
        operation, requirement = fault["operation"], fault["requirement"]
        if requirements.get(operation) != requirement:
            raise ValueError("unknown domain operation/requirement")
        value, time = float(fault["operand_value"]), float(fault["time"])
        fingerprint, index = fault["program_sha1"], fault["instruction_index"]
        if (not math.isfinite(time) or not math.isfinite(value)
                or not re.fullmatch(r"[0-9a-f]{40}", fingerprint)
                or type(index) is not int or index < 0):
            raise ValueError("invalid domain diagnostic coordinate")
        stream.add(ExpressionObservation(time=time, value=value,
            backend=BackendAnchor("rumoca", f"solve:{fingerprint}/op:{index}", EntityKind.EXPRESSION),
            role=f"executed:{operation}:{requirement}"))
    from .rumoca_diagnostics import read_solver
    read_solver(evidence["solver"], stream)
    return evidence
