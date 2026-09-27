"""Writable public Solve programs and ordered lifecycle effects.

No numerical evaluator lives here. The installed compiler owns lowering,
checked reconstruction, operation types, effects, and runtime capabilities.
"""
from __future__ import annotations
from contextlib import contextmanager
from copy import deepcopy
from pathlib import Path
import hashlib
import json
import tempfile
from . import Model
from .compiler import invoke


def lower(model: Model, *, executable=None, timeout=None) -> "Program":
    """Record the lowering profile for `model` and return its host program.

    There is no `observe` argument. Observations are demanded by the program,
    not requested at lowering: a `snapshot.value` instruction names a trace
    point, and the numerical program is derived at load from exactly the trace
    points the program references. A model with no trace points and a program
    with no instructions is a valid artifact with zero observations.

    Use `observe_connector_members` to register trace points and reference
    them; that helper is the producer of observation requests.
    """
    with tempfile.TemporaryDirectory(prefix="rbc-lower-") as tmp:
        source, target = Path(tmp) / "equations.rbc", Path(tmp) / "execution.json"
        document = deepcopy(model._document)
        document.pop("execution", None)
        Model(document).save(source)
        invoke(
            "bitcode",
            "lower-execution",
            source,
            "--output",
            target,
            executable=executable,
            timeout=timeout,
        )
        artifact = Model.load(target)
    return Program(model, artifact._document["execution"],
                   executable=executable, timeout=timeout)


def observe_connector_members(model: Model, connectors=None) -> dict:
    """Register a trace point per connector member and return `{path: id}`.

    This is the single producer of observation requests. It edits the *model*,
    which is an equation-IR edit and so belongs here rather than in the CLI:
    `lower-execution` no longer takes an observation list.

    Trace points are created in connector-member order for readable artifacts.
    Nothing depends on that order -- `csv.write_row` owns column order.
    """
    by_id = {variable.id: variable for variable in model.variables}
    # Idempotent by (label, variable): replaying a logging pass must reuse the
    # trace points it registered the first time, not register them twice.
    existing = {
        (point.label, point.variable.id): point.id for point in model.trace_points
    }
    created = {}
    for connector in connectors if connectors is not None else model.connectors:
        for member in connector.members:
            path = f"{connector.path}.{member.name}"
            variable = by_id.get(member.variable_id)
            if variable is None:
                raise ValueError(f"connector member {path} has no bound variable")
            known = existing.get((path, variable.id))
            if known is not None:
                created[path] = known
                continue
            point = model.add_trace_point(
                variable,
                label=path,
                quantity=member.kind,
                added_by="rumoca_bitcode.execution.observe_connector_members",
            )
            created[path] = point.id
    model.refresh()
    return created


class Program:
    def __init__(self, model, raw, *, executable=None, timeout=None):
        self.model, self.raw = model, raw
        # Remembered so a pass-boundary digest refresh uses the compiler this
        # program was produced with, not whatever $RUMOCA happens to be.
        self.executable, self.timeout = executable, timeout
        #: Passes whose builder was created but never left. A pass refreshes
        #: the dependency digest when its builder exits, so an unclosed one
        #: means the digest does not describe the program.
        self.open_passes = set()
        self._saved_signature = self._signature()

    def _signature(self):
        raw = {k: v for k, v in self.raw.items() if k != "revision"}
        return hashlib.sha256(json.dumps(raw, sort_keys=True).encode()).hexdigest()

    @classmethod
    def load(cls, path, *, executable=None, timeout=None):
        model = Model.load(path)
        return cls(model, model._document["execution"],
                   executable=executable, timeout=timeout)

    @property
    def program(self):
        """The authored host program: functions and the sinks they name."""
        return self.raw["program"]

    def function(self, name):
        """One function's instruction body."""
        return self.raw["program"]["functions"][name]["body"]

    def locals_of(self, name):
        """One function's declared locals."""
        return self.raw["program"]["functions"][name]["locals"]

    def declare(self, name, local, ty):
        """Declare a typed local. A read of an undeclared local is a
        validation error, not a runtime one."""
        self.locals_of(name).append({"name": local, "ty": ty})
        return local

    def add_function(self, name):
        if name in self.raw["program"]["functions"]:
            raise ValueError(f"duplicate function: {name}")
        self.raw["program"]["functions"][name] = {
            "locals": [], "expressions": [], "body": []
        }
        return name

    def expressions(self, name):
        """One function's program-local expression arena.

        Separate from the model's arena: an execution pass must never write
        into the equation IR.
        """
        return Expressions(self, name)

    def referenced_trace_points(self):
        """Trace points the program names, through instructions and sinks."""
        found, pending = set(), []
        for function in self.raw["program"]["functions"].values():
            pending.extend(function["body"])
        while pending:
            instruction = pending.pop()
            if instruction.get("op") == "snapshot.value":
                found.add(instruction["trace_point"])
            pending.extend(instruction.get("then_body", []))
            pending.extend(instruction.get("else_body", []))
        for sink in self.raw["program"]["sinks"]:
            for member in sink["metadata"]["members"]:
                found.add(member["trace_point"])
        return found

    def builder(self, pass_id, *, version="1", options=None):
        return Builder(self, pass_id, version, options or {})

    def _document(self):
        # Deliberately reads current equation data, including public raw edits.
        doc = deepcopy(self.model._document)
        doc["execution"] = deepcopy(self.raw)
        return doc

    def refresh_digest(self, *, executable=None, timeout=None):
        """Recompute the derived staleness digest from the current program.

        The digest covers the identities the program *references*, so it is
        only knowable once a pass has emitted its instructions. It is derived
        data, so it is recomputed by re-lowering rather than hand-maintained;
        `lower-execution` is idempotent on an artifact that already carries a
        program, which is what makes that possible.

        Deliberately not called by `validate` or `save`: refreshing there
        would re-derive the digest against whatever the model has become, so
        nothing would ever be stale. A pass refreshes it at its own boundary,
        which is what `with program.builder(...)` does.
        """
        executable = executable or self.executable
        timeout = timeout if timeout is not None else self.timeout
        with tempfile.TemporaryDirectory(prefix="rbc-relower-") as tmp:
            source, target = Path(tmp) / "program.rbc", Path(tmp) / "program.json"
            Model(self._document()).save(source)
            invoke("bitcode", "lower-execution", source, "--output", target,
                   executable=executable, timeout=timeout)
            refreshed = Model.load(target)
        self.raw["dependency_digest"] = (
            refreshed._document["execution"]["dependency_digest"]
        )

    def validate(self, *, strict=True, executable=None, timeout=None):
        if self.open_passes:
            raise ValueError(
                f"pass {sorted(self.open_passes)} was never closed; write "
                "`with program.builder(...) as b:` so the dependency digest "
                "is refreshed at the pass boundary"
            )
        executable = executable or self.executable
        timeout = timeout if timeout is not None else self.timeout
        with tempfile.TemporaryDirectory(prefix="rbc-execution-check-") as tmp:
            path = Path(tmp) / "program.rbc"
            Model(self._document()).save(path)
            invoke("bitcode", "check", path, *(["--strict"] if strict else []),
                   executable=executable, timeout=timeout)

    def save(self, path, *, include_equations=True, executable=None, timeout=None):
        if not include_equations:
            raise ValueError("execution v2 requires equations: the numerical program is derived from them, never stored")
        signature = self._signature()
        if signature != self._saved_signature:
            self.raw["revision"] += 1
        self.validate(executable=executable, timeout=timeout)
        Model(self._document()).save(path)
        self._saved_signature = signature

    def relower(self, *, replay=None, executable=None, timeout=None):
        """Explicit recipe replay; a caller supplies compatible pass implementations.

        Callbacks run only while authoring, never in the saved runtime process.
        Unknown recipes fail rather than dropping instrumentation.
        """
        recipes = deepcopy(self.raw["passes"])
        replay = replay or {}
        for recipe in recipes:
            if recipe["id"] not in replay:
                raise ValueError(f"no compatible replay implementation: {recipe['id']}")
        # The program's references are what must still resolve. v1 checked a
        # serialized observation list here; v2 has none, so the check is
        # against the trace points the instructions actually name.
        known = {point["id"] for point in self.model.raw_model.get("trace_points", [])}
        for referenced in self.referenced_trace_points():
            if referenced not in known:
                raise ValueError(
                    f"replay target identity changed: trace point {referenced} is gone"
                )
        # The replayed passes rebuild the program; copying the old one first
        # would make every pass append a second time.
        fresh = lower(self.model, executable=executable or self.executable,
                      timeout=timeout if timeout is not None else self.timeout)
        for recipe in recipes:
            replay[recipe["id"]](
                fresh, self.model, **decode_pass_options(recipe.get("options", {}))
            )
        fresh.validate(executable=executable, timeout=timeout)
        return fresh


def encode_pass_options(options):
    """Typed pass options. An unrepresentable value is rejected here, not
    silently carried as an opaque blob the compiler cannot read back."""
    encoded = {}
    for key, value in options.items():
        if isinstance(value, bool):
            kind = "boolean"
        elif isinstance(value, int):
            kind = "integer"
        elif isinstance(value, float):
            kind = "real"
        elif isinstance(value, str):
            kind = "text"
        else:
            raise TypeError(
                f"pass option {key!r} has unsupported type {type(value).__name__}; "
                "options must be real, integer, boolean or text"
            )
        encoded[key] = {"kind": kind, "value": value}
    return encoded


def decode_pass_options(options):
    return {key: entry["value"] for key, entry in options.items()}


class Expressions:
    """Append-only program-local expression arena for one function.

    Operands name earlier nodes, so what this builds is acyclic by
    construction. Leaves are declared locals and literals; a model variable
    id is not a leaf, and the validator rejects one.
    """

    def __init__(self, program, function):
        self.nodes = (
            program.raw["program"]["functions"][function].setdefault("expressions", [])
        )

    def _add(self, node):
        self.nodes.append(node)
        return len(self.nodes) - 1

    def local(self, name):
        return self._add({"node": "local", "name": name})

    def real(self, value):
        return self._add({"node": "real", "value": float(value)})

    def integer(self, value):
        return self._add({"node": "integer", "value": int(value)})

    def boolean(self, value):
        return self._add({"node": "boolean", "value": bool(value)})

    def text(self, value):
        return self._add({"node": "text", "value": str(value)})

    def unary(self, op, operand):
        return self._add({"node": "unary", "op": op, "operand": operand})

    def binary(self, op, lhs, rhs):
        return self._add({"node": "binary", "op": op, "lhs": lhs, "rhs": rhs})

    def compare(self, op, lhs, rhs):
        return self._add({"node": "compare", "op": op, "lhs": lhs, "rhs": rhs})


class Builder:
    def __init__(self, program, pass_id, version, options):
        if any(p["id"] == pass_id for p in program.raw["passes"]):
            raise ValueError(f"duplicate execution pass: {pass_id}")
        self.program = program
        program.raw["passes"].append(
            {"id": pass_id, "version": str(version),
             "options": encode_pass_options(options)}
        )
        program.raw["revision"] += 1
        self.pass_id = pass_id
        program.open_passes.add(pass_id)

    def __enter__(self):
        return self

    def __exit__(self, kind, value, traceback):
        self.program.open_passes.discard(self.pass_id)
        # The pass boundary. The digest binds the program's references to the
        # model the pass just saw, so it is recomputed here and nowhere else.
        if kind is None:
            self.program.refresh_digest()
        return False

    def declare_csv_sink(self, *, key, filename, columns, metadata):
        """Declare a CSV sink.

        `columns` is a list of `{"name", "ty"}`. Name and type are paired, not
        two parallel lists: a count mismatch between them is the defect the
        paired form makes unrepresentable, and a guessed type is worse than a
        stated one.
        """
        sinks = self.program.raw["program"]["sinks"]
        if any(s["key"] == key or s["filename"] == filename for s in sinks):
            raise ValueError(f"duplicate CSV sink: {key}/{filename}")
        for column in columns:
            if set(column) != {"name", "ty"}:
                raise ValueError(
                    f"column {column!r} must state exactly a name and a type"
                )
        sinks.append(dict(key=key, filename=filename, columns=list(columns),
                          metadata=metadata))
        return key

    @contextmanager
    def at(self, function, index, *, body=None):
        """Insert at an explicit instruction position in any editable region.

        `function` names the scope: a result is declared there, so an emitter
        cannot leave an undeclared local behind.
        """
        target = self.program.function(function) if body is None else body
        yield Emitter(self.program, function, target, index)

    def before_return(self, function, *, body=None):
        target = self.program.function(function) if body is None else body
        return self.at(function, len(target), body=target)

    def replace(self, body, index, instruction):
        body[index] = deepcopy(instruction)

    def remove(self, body, index):
        return body.pop(index)


#: Result type of each value-producing instruction. `compute` is absent: what
#: it computes is the pass's to state, so `ty=` is required there.
RESULT_TYPES = {
    "snapshot.time": "real",
    "snapshot.sequence": "integer",
    "snapshot.phase": "text",
    "snapshot.value": "real",
}


class Emitter:
    def __init__(self, program, function, body, index):
        self.program, self.function = program, function
        self.body, self.index = body, index

    def argument(self, name):
        if name != "snapshot":
            raise ValueError(f"unknown lifecycle argument: {name}")
        return name

    def emit(self, op, *, ty=None, declare=True, **operands):
        # The implicit read-only snapshot is not a serialized mutable handle.
        operands.pop("snapshot", None)
        result = operands.get("result")
        if op in RESULT_TYPES or op == "compute":
            if result is None:
                taken = {local["name"] for local in self.program.locals_of(self.function)}
                taken |= {i.get("result") for i in self.body}
                n = 0
                while f"v{n}" in taken:
                    n += 1
                result = f"v{n}"
            operands["result"] = result
            if declare:
                if op == "compute":
                    if ty is None:
                        raise ValueError(
                            "compute must state the type it produces; the arena "
                            "is the pass's, so only the pass knows"
                        )
                else:
                    ty = RESULT_TYPES[op]
                self.program.declare(self.function, result, ty)
        elif ty is not None:
            raise ValueError(f"{op} produces no value, so it has no type")
        self.body.insert(self.index, {"op": op, **deepcopy(operands)})
        self.index += 1
        return result
