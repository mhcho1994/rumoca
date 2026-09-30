"""Rumoca Bitcode SDK.

Read, analyse and transform a compiled Modelica model without cloning Rumoca,
without Rust, and without matching any compiler version::

    from rumoca_bitcode import Model

    model = Model.load("motor.rbc")

    for connection in model.connections:
        print(connection)

    model.add_trace_point(some_variable, label="battery.pin.voltage")
    model.save("motor-traced.rbc")

The file format is the interface; this package is one convenient reader of it.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any, Iterable

from . import _cbor
from .dynamics import (
    Clock,
    ClockOwnership,
    Condition,
    ConnectorType,
    Delay,
    DiscreteBranch,
    DiscreteDefinition,
    EventTransaction,
    PreviousValue,
    Relation,
    Root,
    StructuredRoot,
    Terminal,
    TimeEvent,
    TransactionStep,
)
from .model import (
    ConnectionSet,
    FlowBalance,
    FlowTerm,
    MAGIC,
    VERSION,
    ArrayExpr,
    ArrayUpdate,
    Call,
    BinaryOp,
    Binder,
    BinderRef,
    BitcodeError,
    BuiltinCall,
    Comprehension,
    Component,
    Conditional,
    Connection,
    DiscreteRealEquation,
    Domain,
    Equation,
    Function,
    FunctionParameter,
    FunctionParameterRef,
    EquationFamily,
    FieldAccess,
    IndexExpr,
    InitialDiscreteValue,
    RangeExpr,
    RecordExpr,
    RecordField,
    Event,
    Expression,
    Literal,
    Provenance,
    Source,
    StringConversion,
    Span,
    SymbolContract,
    TimeRef,
    TracePoint,
    UnaryOp,
    Unsupported,
    ValueType,
    Variable,
    VariableRef,
)

#: The SDK's own release version. The bitcode version it reads is ``VERSION``.
__version__ = "0.2.0"

__all__ = [
    "Model",
    "Variable",
    "Equation",
    "Connection",
    "ConnectionSet",
    "FlowBalance",
    "FlowTerm",
    "Event",
    "TracePoint",
    "Component",
    "ValueType",
    "Source",
    "Span",
    "SymbolContract",
    "Provenance",
    "Expression",
    "Literal",
    "VariableRef",
    "TimeRef",
    "UnaryOp",
    "BinaryOp",
    "BuiltinCall",
    "StringConversion",
    "Conditional",
    "Domain",
    "DiscreteRealEquation",
    "InitialDiscreteValue",
    "Function",
    "FunctionParameter",
    "FunctionParameterRef",
    "Call",
    "Binder",
    "BinderRef",
    "EquationFamily",
    "ArrayExpr",
    "RecordExpr",
    "RecordField",
    "FieldAccess",
    "RangeExpr",
    "Comprehension",
    "IndexExpr",
    "ArrayUpdate",
    "Unsupported",
    "Relation",
    "Condition",
    "Root",
    "Clock",
    "ClockOwnership",
    "TimeEvent",
    "DiscreteDefinition",
    "DiscreteBranch",
    "EventTransaction",
    "TransactionStep",
    "PreviousValue",
    "Delay",
    "Terminal",
    "StructuredRoot",
    "ConnectorType",
    "BitcodeError",
    "Builder",
    "MAGIC",
    "VERSION",
]

_COORDINATE_KINDS = {
    "parameter",
    "input",
    "state",
    "derivative",
    "algebraic",
    "discrete_real",
    "discrete_value",
    "pre_state",
    "pre_algebraic",
    "pre_discrete_real",
    "pre_discrete_value",
}


class Model:
    """A compiled Modelica model loaded from bitcode."""

    def __init__(self, document: dict[str, Any]) -> None:
        magic = document.get("magic")
        if magic != MAGIC:
            raise BitcodeError(f"not a Rumoca Bitcode file: magic is {magic!r}")
        version = document.get("bitcode_version")
        if version != VERSION:
            raise BitcodeError(
                f"unsupported bitcode version {version}: this SDK reads version {VERSION}"
            )
        self._document = document
        self._raw = document["model"]
        self.producer: str = document.get("producer", "")
        self.name: str = self._raw["name"]

        self.sources: list[Source] = [
            Source(id=entry["id"], name=entry["name"], text=entry.get("text"))
            for entry in self._raw.get("sources", [])
        ]
        self.types: list[ValueType] = [
            ValueType(
                id=entry["id"],
                scalar=entry["scalar"],
                dimensions=tuple(entry.get("dimensions", [])),
                record_name=(entry.get("record") or {}).get("name"),
                fields=tuple(
                    RecordField(field["name"], field["value_type"])
                    for field in (entry.get("record") or {}).get("fields", [])
                ),
            )
            for entry in self._raw.get("types", [])
        ]
        self.components: list[Component] = [
            Component(id=entry["id"], path=entry["path"],
                      class_name=entry.get("class_name"))
            for entry in self._raw.get("components", [])
        ]
        self.variables: list[Variable] = [
            Variable(entry, self) for entry in self._raw.get("variables", [])
        ]
        # Domains and functions precede expressions: a binder, comprehension
        # or call node names one.
        self.domains: list[Domain] = [
            Domain(entry, self) for entry in self._raw.get("domains", [])
        ]
        self.functions: list[Function] = [
            Function(entry, self) for entry in self._raw.get("functions", [])
        ]
        self.expressions: list[Expression] = self._build_expressions()
        self.equations: list[Equation] = [
            Equation(entry, self) for entry in self._raw.get("equations", [])
        ]
        self.initial_equations: list[Equation] = [
            Equation(entry, self) for entry in self._raw.get("initial_equations", [])
        ]
        self.equation_families: list[EquationFamily] = [
            EquationFamily(entry, self)
            for entry in self._raw.get("equation_families", [])
        ]
        self.initial_equation_families: list[EquationFamily] = [
            EquationFamily(entry, self, initial=True)
            for entry in self._raw.get("initial_equation_families", [])
        ]
        self.discrete_real_equations: list[DiscreteRealEquation] = [
            DiscreteRealEquation(entry, self)
            for entry in self._raw.get("discrete_real_equations", [])
        ]
        self.initial_discrete_values: list[InitialDiscreteValue] = [
            InitialDiscreteValue(entry, self)
            for entry in self._raw.get("initial_discrete_values", [])
        ]
        self.connections: list[Connection] = [
            Connection(entry, self) for entry in self._raw.get("connections", [])
        ]
        self.connection_sets: list[ConnectionSet] = [
            ConnectionSet(entry, self)
            for entry in self._raw.get("connection_sets", [])
        ]
        self.events: list[Event] = [Event(entry, self) for entry in self._raw.get("events", [])]
        # Event, clock and owner tables. Ids are dense, so each list is
        # indexed by the id the artifact uses.
        def table(view, key):
            return [view(entry, self) for entry in self._raw.get(key, [])]

        self.relations: list[Relation] = table(Relation, "relations")
        self.conditions: list[Condition] = table(Condition, "conditions")
        self.clocks: list[Clock] = table(Clock, "clocks")
        self.clock_ownerships: list[ClockOwnership] = table(ClockOwnership, "clock_ownerships")
        self.roots: list[Root] = table(Root, "roots")
        self.time_events: list[TimeEvent] = table(TimeEvent, "time_events")
        self.discrete_definitions: list[DiscreteDefinition] = table(
            DiscreteDefinition, "discrete_definitions"
        )
        self.event_transactions: list[EventTransaction] = table(
            EventTransaction, "model_event_transactions"
        )
        self.previous_values: list[PreviousValue] = table(PreviousValue, "previous_values")
        self.delays: list[Delay] = table(Delay, "delays")
        self.terminals: list[Terminal] = table(Terminal, "terminals")
        self.structured_roots: list[StructuredRoot] = table(StructuredRoot, "structured_roots")
        self.connector_types: list[ConnectorType] = table(ConnectorType, "connector_types")
        self.trace_points: list[TracePoint] = [
            TracePoint(entry, self) for entry in self._raw.get("trace_points", [])
        ]

    # ── Loading and saving ───────────────────────────────────────────────────

    @classmethod
    def load(cls, path: str | Path) -> "Model":
        """Load an artifact, detecting CBOR or JSON automatically."""
        data = Path(path).read_bytes()
        return cls(decode(data))

    @classmethod
    def loads(cls, data: bytes) -> "Model":
        return cls(decode(data))

    @classmethod
    def empty(cls, name: str, producer: str = "rumoca-bitcode-sdk") -> "Model":
        """A valid, minimal artifact with nothing in it.

        The format calls itself an interchange format, and until this existed
        the only program that could produce one was the Rumoca compiler. A
        model with no variables and no equations is trivially valid, so a
        producer starts from something the validator already accepts and adds
        to it rather than assembling a document and hoping.
        """
        return cls({
            "magic": MAGIC,
            "bitcode_version": VERSION,
            "producer": producer,
            "model": {
                "name": name,
                # One source, so generated provenance has somewhere to point.
                # A span names a source by id, and an empty source table makes
                # every generated entity dangle — which the validator catches,
                # correctly and confusingly, as "source 0 does not exist".
                "sources": [{"id": 0, "name": f"<{producer}>"}],
                "types": [], "variables": [],
                "expressions": [], "equations": [], "initial_equations": [],
                "relations": [], "conditions": [], "clocks": [], "clock_ownerships": [], "roots": [],
                "events": [], "time_events": [], "connections": [],
                "components": [], "trace_points": [],
                "summary": {},
            },
        })

    @property
    def raw_model(self) -> dict[str, Any]:
        """The underlying document's model table.

        Public because a builder has to edit it, and a supported way to write
        an artifact beats every consumer reaching past a leading underscore —
        which is what the first builder did.
        """
        return self._raw

    def builder(self, pass_name: str, generation: str = "synthetic_residual"):
        """A [`Builder`] appending to this model on behalf of `pass_name`."""
        from .builder import Builder

        return Builder(model=self, pass_name=pass_name, generation=generation)

    @classmethod
    def link(cls, modules, *, name="LinkedModel", discard_execution=False):
        """Link namespace -> Model/path inputs without mutating or wiring them.

        Uses the installed native linker (RUMOCA selects the executable).
        Executable inputs require explicit discard_execution=True; lower and
        instrument the combined equations again afterwards.
        """
        from .linker import link
        return link(modules, name=name, discard_execution=discard_execution)

    @property
    def connectors(self):
        from .connectors import Connector
        return [Connector(raw, self) for raw in self._raw.get("connectors", [])]

    @property
    def has_execution(self) -> bool:
        """Whether the container carries a saved program (not a validity claim)."""
        return self._document.get("execution") is not None

    def validate(self, strict: bool = True, *, connections: bool = False) -> None:
        from .compiler import check_model
        check_model(self, strict=strict, connections=connections)

    def refresh(self) -> None:
        """Rebuild the typed views from the document, and the summary.

        A pass edits the document; the typed views are built once at load.
        Leaving them stale makes the model summarise itself with the counts it
        had before the pass ran, which the validator then rejects — that is
        how the need for this was found.
        """
        Model.__init__(self, self._document)
        self.recompute_summary()

    def reload_trace_points(self) -> None:
        """Rebuild the trace-point view from the raw document.

        A pass that adds observations edits the document; the typed views are
        built once at load. Without this the model keeps the list it was born
        with, and `recompute_summary` then writes a count the validator
        rejects --- which is how this was noticed.
        """
        self.trace_points = [
            TracePoint(entry, self)
            for entry in self._raw.get("trace_points", [])
        ]

    def save(self, path: str | Path, *, format: str | None = None) -> None:
        """Write the model back out.

        The summary the Rust validator checks is recomputed first, so an
        artifact saved after editing is accepted by ``rumoca compile-bitcode``.
        """
        self.recompute_summary()
        path = Path(path)
        if format is None:
            format = "json" if path.suffix == ".json" else "cbor"
        if format == "json":
            path.write_text(json.dumps(self._document, indent=1) + "\n")
        elif format == "cbor":
            path.write_bytes(_cbor.dumps(self._document))
        else:
            raise ValueError(f"unknown format {format!r}; use 'cbor' or 'json'")

    # ── Convenience views ────────────────────────────────────────────────────

    @property
    def states(self) -> list[Variable]:
        return [variable for variable in self.variables if variable.role == "state"]

    @property
    def parameters(self) -> list[Variable]:
        return [variable for variable in self.variables if variable.role in ("parameter", "constant")]

    @property
    def inputs(self) -> list[Variable]:
        return [variable for variable in self.variables if variable.role == "input"]

    @property
    def outputs(self) -> list[Variable]:
        return [variable for variable in self.variables if variable.role == "output"]

    @property
    def algebraics(self) -> list[Variable]:
        return [variable for variable in self.variables if variable.role == "algebraic"]

    @property
    def summary(self) -> dict[str, int]:
        return dict(self._raw.get("summary", {}))

    def variable(self, name: str) -> Variable:
        """Look a variable up by its flattened name."""
        for variable in self.variables:
            if variable.name == name:
                return variable
        raise KeyError(name)

    def connector_members(self, connector_path: str) -> list[Variable]:
        """Every variable belonging to one connector instance."""
        prefix = connector_path + "."
        return [
            variable
            for variable in self.variables
            if variable.name.startswith(prefix) and "." not in variable.name[len(prefix) :]
        ]

    # ── Transformation ───────────────────────────────────────────────────────

    def add_trace_point(
        self,
        variable: Variable,
        *,
        label: str | None = None,
        connection: Connection | None = None,
        quantity: str | None = None,
        added_by: str | None = None,
    ) -> TracePoint:
        """Request that ``variable`` be observed at runtime.

        Trace points are observation metadata. Adding one does not alter any
        equation, so instrumentation cannot change what the model computes.
        """
        entry: dict[str, Any] = {
            "id": len(self._raw.setdefault("trace_points", [])),
            "variable": variable.id,
            "label": label or variable.name,
        }
        if connection is not None:
            entry["connection"] = connection.id
        resolved_quantity = quantity or variable.quantity
        if resolved_quantity is not None:
            entry["quantity"] = resolved_quantity
        if variable.unit is not None:
            entry["unit"] = variable.unit
        if added_by is not None:
            entry["added_by"] = added_by
        self._raw["trace_points"].append(entry)
        trace = TracePoint(entry, self)
        self.trace_points.append(trace)
        return trace

    def recompute_summary(self) -> None:
        """Recompute the denormalised counts the validator checks."""

        def count(role: str) -> int:
            return sum(1 for variable in self.variables if variable.role == role)

        self._raw["summary"] = {
            "variables": len(self.variables),
            "states": count("state"),
            "parameters": count("parameter"),
            "constants": count("constant"),
            "inputs": count("input"),
            "outputs": count("output"),
            "algebraics": count("algebraic"),
            "discrete_reals": count("discrete_real"),
            "discrete_values": count("discrete_value"),
            "equations": len(self.equations),
            "initial_equations": len(self.initial_equations),
            "expressions": len(self.expressions),
            "relations": len(self._raw.get("relations", [])),
            "conditions": len(self._raw.get("conditions", [])),
            "clocks": len(self._raw.get("clocks", [])),
            "clock_ownerships": len(self._raw.get("clock_ownerships", [])),
            "roots": len(self._raw.get("roots", [])),
            "events": len(self.events),
            "time_events": len(self._raw.get("time_events", [])),
            "connections": len(self.connections),
            "connection_sets": len(self.connection_sets),
            "components": len(self.components),
            "trace_points": len(self.trace_points),
        }

    # ── Internals ────────────────────────────────────────────────────────────

    def _provenance(self, raw: dict) -> Provenance:
        origin = raw["origin"]
        kind = origin["kind"] if isinstance(origin, dict) else origin
        generation = origin.get("generation") if isinstance(origin, dict) else None
        span_raw = raw["span"]
        index = span_raw["source"]
        source = self.sources[index] if index < len(self.sources) else None
        span = Span(
            source=source,
            start=span_raw["start"],
            end=span_raw["end"],
            line=span_raw.get("line", 0),
            column=span_raw.get("column", 0),
        )
        return Provenance(origin=kind, generation=generation, span=span)

    def _build_expressions(self) -> list[Expression]:
        """Materialise the expression arena.

        Operands always reference strictly earlier nodes, so one forward pass
        builds the whole tree with no recursion and no cycle risk.
        """
        built: list[Expression] = []
        for entry in self._raw.get("expressions", []):
            node = entry["node"]
            kind = node["kind"]
            provenance = self._provenance(entry["provenance"])
            identifier = entry["id"]

            if kind == "literal":
                value = node["value"]
                built.append(Literal(identifier, provenance, value["kind"], value.get("value")))
            elif kind == "coordinate":
                coordinate = node["coordinate"]
                coordinate_kind = coordinate["kind"]
                if coordinate_kind == "time":
                    built.append(TimeRef(identifier, provenance))
                elif coordinate_kind in _COORDINATE_KINDS:
                    variable = self.variables[coordinate["variable"]]
                    built.append(VariableRef(identifier, provenance, coordinate_kind, variable))
                elif coordinate_kind == "function_parameter":
                    function, ordinal = coordinate["function"], coordinate["ordinal"]
                    params = (self.functions[function].parameters
                              if function < len(self.functions) else [])
                    name = params[ordinal].name if ordinal < len(params) else None
                    built.append(FunctionParameterRef(
                        identifier, provenance, function, ordinal, name))
                elif coordinate_kind == "binder":
                    # Carry the source name: a family body printed as
                    # `x[<binder 0.0>]` is unreadable next to `x[i]`.
                    domain, ordinal = coordinate["domain"], coordinate["ordinal"]
                    binders = self.domains[domain].binders if domain < len(self.domains) else []
                    name = binders[ordinal].name if ordinal < len(binders) else None
                    built.append(BinderRef(identifier, provenance, domain, ordinal, name))
                else:
                    built.append(
                        Unsupported(identifier, provenance, f"coordinate {coordinate_kind}")
                    )
            elif kind == "unary":
                built.append(
                    UnaryOp(identifier, provenance, node["op"], built[node["operand"]])
                )
            elif kind == "binary":
                built.append(
                    BinaryOp(
                        identifier,
                        provenance,
                        node["op"],
                        built[node["lhs"]],
                        built[node["rhs"]],
                    )
                )
            elif kind == "string_conversion":
                format = node["format"]
                built.append(StringConversion(identifier, provenance, built[node["value"]],
                    format["kind"], {key: built[value] for key, value in format.items()
                                     if key != "kind" and value is not None}))
            elif kind == "builtin":
                built.append(
                    BuiltinCall(
                        identifier,
                        provenance,
                        node["name"],
                        [built[index] for index in node["arguments"]],
                    )
                )
            elif kind == "conditional":
                branches = [
                    (built[branch["condition"]], built[branch["value"]])
                    for branch in node["branches"]
                ]
                built.append(
                    Conditional(identifier, provenance, branches, built[node["fallback"]])
                )
            elif kind == "call":
                function = node["function"]
                name = (self.functions[function].name
                        if function < len(self.functions) else None)
                built.append(Call(identifier, provenance, node["owner"], function,
                                  node["output"],
                                  [built[i] for i in node["arguments"]], name))
            elif kind == "array":
                built.append(ArrayExpr(identifier, provenance,
                                       [built[i] for i in node["elements"]]))
            elif kind == "record":
                built.append(RecordExpr(identifier, provenance,
                                        [built[i] for i in node["fields"]]))
            elif kind == "field":
                built.append(FieldAccess(identifier, provenance,
                                         built[node["base"]], node["field"]))
            elif kind == "range":
                step = node.get("step")
                built.append(RangeExpr(identifier, provenance, built[node["start"]],
                                       None if step is None else built[step],
                                       built[node["stop"]]))
            elif kind == "comprehension":
                built.append(Comprehension(identifier, provenance,
                                           node["domain"], built[node["body"]]))
            elif kind == "index":
                built.append(IndexExpr(identifier, provenance, built[node["base"]],
                                       _subscripts(node["subscripts"], built)))
            elif kind == "array_update":
                built.append(ArrayUpdate(identifier, provenance, built[node["base"]],
                                         built[node["value"]],
                                         _subscripts(node["subscripts"], built)))
            else:
                built.append(Unsupported(identifier, provenance, node.get("detail", kind)))
        return built

    def __repr__(self) -> str:
        return (
            f"<Model {self.name!r}: {len(self.variables)} variables, "
            f"{len(self.equations)} equations, {len(self.connections)} connections>"
        )


def _subscripts(raw: list[dict], built: list[Expression]) -> list[Expression | None]:
    """A whole-dimension `:` has no expression, and is reported as None."""
    return [
        None if entry["kind"] == "whole" else built[entry["expression"]]
        for entry in raw
    ]


def __getattr__(name: str):
    # Lazy, because `builder` imports nothing from here at module scope but a
    # reader of the package expects `rumoca_bitcode.Builder` to exist.
    if name == "Builder":
        from .builder import Builder

        return Builder
    raise AttributeError(name)


def decode(data: bytes) -> dict[str, Any]:
    """Decode an artifact, detecting the encoding from its first byte."""
    for byte in data:
        if byte not in (0x20, 0x09, 0x0A, 0x0D):
            first = byte
            break
    else:
        raise BitcodeError("empty bitcode artifact")
    if first == ord("{"):
        return json.loads(data.decode("utf-8"))
    return _cbor.loads(data)
