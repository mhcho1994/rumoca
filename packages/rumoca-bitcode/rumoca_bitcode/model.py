"""Object model for Rumoca Bitcode.

A pass author works with these classes, never with raw dictionaries::

    from rumoca_bitcode import Model

    model = Model.load("motor.rbc")
    for variable in model.variables:
        print(variable.name, variable.role, variable.unit)

Objects are thin views over the decoded document, so an edit through a helper
method is reflected when the model is saved. Editing the underlying ``raw``
dictionaries directly is allowed but unsupported: use the helpers, then call
:meth:`Model.save`, which recomputes the summary the validator checks.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterator

from . import _cbor

MAGIC = "RUMOCA-RBC"
VERSION = 2


class BitcodeError(Exception):
    """The artifact is not bitcode this SDK can read."""


# ── Provenance ───────────────────────────────────────────────────────────────


@dataclass(frozen=True)
class Source:
    """One source file referenced by the model."""

    id: int
    name: str
    text: str | None = None

    @property
    def short_name(self) -> str:
        return Path(self.name).name


@dataclass(frozen=True)
class Span:
    """A resolved source location.

    ``line`` and ``column`` are 1-based and precomputed by the exporter, so
    reporting ``Motor.mo:52`` never requires the source text.
    """

    source: Source | None
    start: int
    end: int
    line: int
    column: int

    def __str__(self) -> str:
        name = self.source.short_name if self.source else "<unknown>"
        if self.line:
            return f"{name}:{self.line}:{self.column}"
        return f"{name}@{self.start}"

    def text(self) -> str | None:
        """The exact source text this span covers, if the artifact embeds it."""
        if self.source is None or self.source.text is None:
            return None
        return self.source.text[self.start : self.end]


@dataclass(frozen=True)
class Provenance:
    """Why an object exists and where it came from."""

    origin: str
    """``"source"`` or ``"generated"``."""
    generation: str | None
    """For generated objects, the lowering kind, e.g. ``"connection_equation"``."""
    span: Span

    @property
    def is_generated(self) -> bool:
        return self.origin == "generated"

    def __str__(self) -> str:
        if self.generation:
            return f"{self.span} ({self.generation})"
        return str(self.span)


# ── Expressions ──────────────────────────────────────────────────────────────


class Expression:
    """Base class for the expression tree."""

    __slots__ = ("id", "provenance")

    def __init__(self, id: int, provenance: Provenance) -> None:
        self.id = id
        self.provenance = provenance

    def children(self) -> list["Expression"]:
        return []

    def walk(self) -> Iterator["Expression"]:
        """Yield this node and every descendant, parents first."""
        yield self
        for child in self.children():
            yield from child.walk()

    def variables(self) -> list["Variable"]:
        """Every variable this expression reads, in first-appearance order."""
        seen: dict[int, Variable] = {}
        for node in self.walk():
            if isinstance(node, VariableRef):
                seen.setdefault(node.variable.id, node.variable)
        return list(seen.values())


class BinderRef(Expression):
    """The iteration index of an equation family: the ``i`` in ``x[i] = i``.

    It is not a model variable, so it is deliberately not a `VariableRef`: a
    pass counting what an equation reads must not count a loop index as an
    unknown.
    """

    __slots__ = ("domain", "ordinal", "name")

    def __init__(self, id, provenance, domain: int, ordinal: int,
                 name: str | None = None) -> None:
        super().__init__(id, provenance)
        self.domain = domain
        self.ordinal = ordinal
        self.name = name

    def __repr__(self) -> str:
        return self.name or f"<binder {self.domain}.{self.ordinal}>"


class ArrayExpr(Expression):
    """An array construction ``{a, b, c}``."""

    __slots__ = ("elements",)

    def __init__(self, id, provenance, elements: list[Expression]) -> None:
        super().__init__(id, provenance)
        self.elements = elements

    def children(self) -> list[Expression]:
        return list(self.elements)

    def __repr__(self) -> str:
        return "{" + ", ".join(repr(e) for e in self.elements) + "}"


class RecordExpr(Expression):
    """A record construction, one operand per field in declaration order."""

    __slots__ = ("fields",)

    def __init__(self, id, provenance, fields: list[Expression]) -> None:
        super().__init__(id, provenance)
        self.fields = fields

    def children(self) -> list[Expression]:
        return list(self.fields)

    def __repr__(self) -> str:
        return "record(" + ", ".join(repr(f) for f in self.fields) + ")"


class FieldAccess(Expression):
    """``base.field``, by ordinal in the record's declaration order."""

    __slots__ = ("base", "ordinal")

    def __init__(self, id, provenance, base: Expression, ordinal: int) -> None:
        super().__init__(id, provenance)
        self.base = base
        self.ordinal = ordinal

    def children(self) -> list[Expression]:
        return [self.base]

    def __repr__(self) -> str:
        return f"{self.base!r}.[{self.ordinal}]"


class RangeExpr(Expression):
    """``start:step:stop``. ``step`` is None when it was not written."""

    __slots__ = ("start", "step", "stop")

    def __init__(self, id, provenance, start, step, stop) -> None:
        super().__init__(id, provenance)
        self.start = start
        self.step = step
        self.stop = stop

    def children(self) -> list[Expression]:
        return [e for e in (self.start, self.step, self.stop) if e is not None]

    def __repr__(self) -> str:
        middle = "" if self.step is None else f"{self.step!r}:"
        return f"{self.start!r}:{middle}{self.stop!r}"


class Comprehension(Expression):
    """``{body for i in ...}`` over an iteration domain."""

    __slots__ = ("domain", "body")

    def __init__(self, id, provenance, domain: int, body: Expression) -> None:
        super().__init__(id, provenance)
        self.domain = domain
        self.body = body

    def children(self) -> list[Expression]:
        return [self.body]

    def __repr__(self) -> str:
        return f"{{{self.body!r} for &{self.domain}}}"


class IndexExpr(Expression):
    """``base[s1, s2, ...]``. A ``:`` subscript appears as None."""

    __slots__ = ("base", "subscripts")

    def __init__(self, id, provenance, base: Expression,
                 subscripts: list[Expression | None]) -> None:
        super().__init__(id, provenance)
        self.base = base
        self.subscripts = subscripts

    def children(self) -> list[Expression]:
        return [self.base] + [s for s in self.subscripts if s is not None]

    def __repr__(self) -> str:
        inner = ", ".join(":" if s is None else repr(s) for s in self.subscripts)
        return f"{self.base!r}[{inner}]"


class ArrayUpdate(Expression):
    """``base`` with ``subscripts`` replaced by ``value``."""

    __slots__ = ("base", "value", "subscripts")

    def __init__(self, id, provenance, base, value, subscripts) -> None:
        super().__init__(id, provenance)
        self.base = base
        self.value = value
        self.subscripts = subscripts

    def children(self) -> list[Expression]:
        return [self.base, self.value] + [s for s in self.subscripts if s is not None]

    def __repr__(self) -> str:
        inner = ", ".join(":" if s is None else repr(s) for s in self.subscripts)
        return f"({self.base!r} with [{inner}] := {self.value!r})"


class FunctionParameterRef(Expression):
    """A function's formal parameter, read inside that function's body.

    Not a model variable, so deliberately not a `VariableRef`: a pass counting
    what an equation reads must not count a callee's parameter as an unknown.
    """

    __slots__ = ("function", "ordinal", "name")

    def __init__(self, id, provenance, function: int, ordinal: int,
                 name: str | None = None) -> None:
        super().__init__(id, provenance)
        self.function = function
        self.ordinal = ordinal
        self.name = name

    def __repr__(self) -> str:
        return self.name or f"<parameter {self.function}.{self.ordinal}>"


class Call(Expression):
    """One result projection of a function call.

    `owner` is the id of the call occurrence: two projections of `(a, b) = f(x)`
    share it, so a consumer knows that is one evaluation of `f` and not two.
    """

    __slots__ = ("owner", "function", "output", "arguments", "name")

    def __init__(self, id, provenance, owner: int, function: int, output: int,
                 arguments: list[Expression], name: str | None = None) -> None:
        super().__init__(id, provenance)
        self.owner = owner
        self.function = function
        self.output = output
        self.arguments = arguments
        self.name = name

    def children(self) -> list[Expression]:
        return list(self.arguments)

    def __repr__(self) -> str:
        callee = self.name or f"<function {self.function}>"
        return f"{callee}({', '.join(repr(a) for a in self.arguments)})"


class Literal(Expression):
    __slots__ = ("kind", "value")

    def __init__(self, id, provenance, kind: str, value: Any) -> None:
        super().__init__(id, provenance)
        self.kind = kind
        self.value = value

    def __repr__(self) -> str:
        return repr(self.value) if self.kind == "string" else f"{self.value}"


class VariableRef(Expression):
    """A leaf naming a model quantity.

    ``kind`` distinguishes ``x`` from ``der(x)`` from ``pre(x)``; they are
    different coordinates of the same variable.
    """

    __slots__ = ("kind", "variable")

    def __init__(self, id, provenance, kind: str, variable: "Variable") -> None:
        super().__init__(id, provenance)
        self.kind = kind
        self.variable = variable

    @property
    def is_derivative(self) -> bool:
        return self.kind == "derivative"

    @property
    def is_previous(self) -> bool:
        return self.kind.startswith("pre_")

    def __repr__(self) -> str:
        if self.is_derivative:
            return f"der({self.variable.name})"
        if self.is_previous:
            return f"pre({self.variable.name})"
        return self.variable.name


class TimeRef(Expression):
    __slots__ = ()

    def __repr__(self) -> str:
        return "time"


class UnaryOp(Expression):
    __slots__ = ("op", "operand")

    def __init__(self, id, provenance, op: str, operand: Expression) -> None:
        super().__init__(id, provenance)
        self.op = op
        self.operand = operand

    def children(self) -> list[Expression]:
        return [self.operand]

    _SYMBOLS = {"negate": "-", "not": "not "}

    def __repr__(self) -> str:
        return f"{self._SYMBOLS.get(self.op, self.op + ' ')}{self.operand!r}"


class BinaryOp(Expression):
    __slots__ = ("op", "lhs", "rhs")

    def __init__(self, id, provenance, op: str, lhs: Expression, rhs: Expression) -> None:
        super().__init__(id, provenance)
        self.op = op
        self.lhs = lhs
        self.rhs = rhs

    def children(self) -> list[Expression]:
        return [self.lhs, self.rhs]

    _SYMBOLS = {
        "add": "+", "subtract": "-", "multiply": "*", "divide": "/", "power": "^",
        "equal": "==", "not_equal": "<>", "less": "<", "less_equal": "<=",
        "greater": ">", "greater_equal": ">=", "and": "and", "or": "or",
        "elementwise_add": ".+", "elementwise_subtract": ".-",
        "elementwise_multiply": ".*", "elementwise_divide": "./",
        "elementwise_power": ".^",
    }

    def __repr__(self) -> str:
        # Infix, fully parenthesised: a report a human reads should look like
        # the model they wrote, and explicit parentheses beat guessing at
        # precedence.
        return f"({self.lhs!r} {self._SYMBOLS.get(self.op, self.op)} {self.rhs!r})"


class Conditional(Expression):
    """``if c1 then v1 elseif c2 then v2 else fallback``."""

    __slots__ = ("branches", "fallback")

    def __init__(self, id, provenance, branches, fallback: Expression) -> None:
        super().__init__(id, provenance)
        self.branches = branches
        self.fallback = fallback

    def children(self) -> list[Expression]:
        out: list[Expression] = []
        for condition, value in self.branches:
            out.extend((condition, value))
        out.append(self.fallback)
        return out

    def __repr__(self) -> str:
        arms = " ".join(
            f"if {condition!r} then {value!r}" for condition, value in self.branches
        )
        return f"({arms} else {self.fallback!r})"


class BuiltinCall(Expression):
    """A pure built-in: `sqrt(x)`, `log(x)`, `min(a, b)`, ...

    `name` is the Modelica spelling, so a consumer matches on `"sqrt"` rather
    than on an ordinal that could shift between compiler versions.
    """

    __slots__ = ("name", "arguments")

    def __init__(self, id, provenance, name: str, arguments) -> None:
        super().__init__(id, provenance)
        self.name = name
        self.arguments = arguments

    def children(self) -> list[Expression]:
        return list(self.arguments)

    def __repr__(self) -> str:
        return f"{self.name}({', '.join(repr(a) for a in self.arguments)})"


class StringConversion(Expression):
    """Predefined scalar-to-String conversion with explicit format operands."""

    __slots__ = ("value", "format_kind", "format_operands")

    def __init__(self, id, provenance, value, format_kind, format_operands):
        super().__init__(id, provenance)
        self.value, self.format_kind, self.format_operands = value, format_kind, format_operands

    def children(self):
        return [self.value, *self.format_operands.values()]

    def __repr__(self):
        return f"String({self.value!r})"


class Unsupported(Expression):
    """A node bitcode v2 could not represent.

    Its presence means the artifact does not fully describe the model. A pass
    that cares about completeness should refuse rather than assume a value.
    """

    __slots__ = ("detail",)

    def __init__(self, id, provenance, detail: str) -> None:
        super().__init__(id, provenance)
        self.detail = detail

    def __repr__(self) -> str:
        return f"Unsupported({self.detail!r})"


# ── Model objects ────────────────────────────────────────────────────────────


@dataclass(frozen=True)
class SymbolContract:
    """What a declaration promises about a symbol.

    Four fields that are routinely confused, and are not the same thing:

    ``variability``
        May the value change, and when. A ``constant`` may not change at all;
        a ``parameter`` may be set before translation but not during.
    ``is_final``
        May a *modifier* override this declaration. It says nothing about what
        the binding depends on: ``final parameter d = p`` cannot be overridden
        and is still reachable by setting ``p``.
    ``is_protected``
        Who may see it. Visibility is not immutability.
    ``evaluate``
        The MLS §18.3 hint that a value may be substituted at translation time.
        A hint, not a guarantee, and not equivalent to ``constant``.
    """

    variability: str
    is_final: bool = False
    is_protected: bool = False
    evaluate: bool = False
    structural: bool = False
    effective_value: float | None = None
    binding_depends_on: tuple[int, ...] = ()
    binding_from_modification: bool = False
    declared_in: str | None = None

    @property
    def is_constant(self) -> bool:
        return self.variability == "constant"


@dataclass(frozen=True)
class RecordField:
    """One field of a record type."""

    name: str
    value_type: int

    def __repr__(self) -> str:
        return f"<field {self.name}: ${self.value_type}>"


@dataclass(frozen=True)
class ValueType:
    id: int
    scalar: str
    dimensions: tuple[int, ...]
    #: Name of the record type, when `scalar` is ``"record"``.
    record_name: str | None = None
    #: Fields in declaration order, when `scalar` is ``"record"``.
    fields: tuple[RecordField, ...] = ()

    @property
    def is_record(self) -> bool:
        return self.scalar == "record"

    @property
    def is_scalar(self) -> bool:
        return not self.dimensions and not self.is_record

    def __str__(self) -> str:
        base = self.record_name if self.is_record and self.record_name else self.scalar
        if not self.dimensions:
            return base
        return f"{base}[{','.join(str(d) for d in self.dimensions)}]"


@dataclass(frozen=True)
class Component:
    """A component instance, identified by its flattened path."""

    id: int
    path: str
    class_name: str | None = None
    """The class this instance is of.

    Without it a connection graph can say that a node joins `L.n` to `Ro.p`
    and cannot say that it joins an inductor to a resistor, which is most of
    what the graph is for. `None` where the instance declares no variable of
    its own --- `Ground` is only a pin --- rather than where the class is
    unknown in principle.
    """

    def __str__(self) -> str:
        return self.path


@dataclass(frozen=True)
class FlowTerm:
    """One signed member of a connection set's conservation law."""

    variable: "Variable"
    negated: bool = False

    @property
    def sign(self) -> int:
        return -1 if self.negated else 1

    def __str__(self) -> str:
        return f"{'-' if self.negated else '+'}{self.variable.name}"


@dataclass(frozen=True)
class FlowBalance:
    """One conservation law: the signed terms sum to zero."""

    terms: tuple
    equation: int | None = None

    def __str__(self) -> str:
        return " + ".join(str(term) for term in self.terms) + " = 0"


class ConnectionSet:
    """One node of the connection graph (MLS §9.2).

    A `connect` is written pairwise; the object it creates is n-ary. Three pins
    wired together share **one** potential and **one** conservation law, and
    the balance over them is not expressible as three pairs. This is the unit
    every question about a network is asked about: what is equated here, what
    is conserved here, and which components meet here.
    """

    __slots__ = ("_raw", "_model")

    def __init__(self, raw: dict, model: "Model") -> None:
        self._raw = raw
        self._model = model

    id = property(lambda self: self._raw["id"])
    connectors = property(lambda self: tuple(self._raw.get("connectors", ())))
    unconnected = property(lambda self: bool(self._raw.get("unconnected", False)))
    potential_equations = property(lambda self: tuple(self._raw.get("potential_equations", ())))

    @property
    def potentials(self) -> tuple["Variable", ...]:
        return tuple(self._model.variables[i]
                     for i in self._raw.get("potentials", ()))

    @property
    def balances(self) -> tuple["FlowBalance", ...]:
        """The conservation laws at this node, one per flow member kind.

        A MultiBody frame conserves a force *and* a torque here: separate
        sums, separate equations, and a reader who merges them is looking at
        a unit mismatch that is not there.
        """
        return tuple(
            FlowBalance(
                equation=entry.get("equation"),
                terms=tuple(FlowTerm(self._model.variables[term["variable"]],
                                     bool(term.get("negated", False)))
                            for term in entry.get("terms", ())),
            )
            for entry in self._raw.get("balances", ())
        )

    @property
    def flows(self) -> tuple[FlowTerm, ...]:
        """Every flow member, across all balances. Order-preserving."""
        return tuple(term for balance in self.balances
                     for term in balance.terms)

    @property
    def components(self) -> tuple[str, ...]:
        """The owning instances, one per connector, deduplicated in order."""
        seen: list[str] = []
        for connector in self.connectors:
            owner = connector.rpartition(".")[0] or connector
            if owner not in seen:
                seen.append(owner)
        return tuple(seen)

    @property
    def degree(self) -> int:
        return len(self.connectors)

    def __repr__(self) -> str:
        return (f"<ConnectionSet {self.id}: {' -- '.join(self.connectors)}"
                f"{' (unconnected)' if self.unconnected else ''}>")


class Variable:
    """One model variable in any role."""

    __slots__ = ("_raw", "_model")

    def __init__(self, raw: dict, model: "Model") -> None:
        self._raw = raw
        self._model = model

    id = property(lambda self: self._raw["id"])
    name = property(lambda self: self._raw["name"])
    role = property(lambda self: self._raw["role"])
    causality = property(lambda self: self._raw["causality"])
    scalar_count = property(lambda self: self._raw["scalar_count"])
    unit = property(lambda self: self._raw.get("unit"))
    description = property(lambda self: self._raw.get("description"))
    fixed = property(lambda self: self._raw.get("fixed"))
    tunable = property(lambda self: self._raw.get("tunable", False))
    from_source = property(lambda self: self._raw.get("from_source", False))

    @property
    def contract(self) -> SymbolContract | None:
        """What the declaration promises, or None on an older artifact."""
        raw = self._raw.get("contract")
        if raw is None:
            return None
        return SymbolContract(
            variability=raw["variability"],
            is_final=raw.get("is_final", False),
            is_protected=raw.get("is_protected", False),
            evaluate=raw.get("evaluate", False),
            structural=raw.get("structural", False),
            effective_value=raw.get("effective_value"),
            binding_depends_on=tuple(raw.get("binding_depends_on", [])),
            binding_from_modification=raw.get("binding_from_modification", False),
            declared_in=raw.get("declared_in"),
        )

    @property
    def type(self) -> ValueType:
        return self._model.types[self._raw["value_type"]]

    @property
    def source(self) -> Provenance:
        return self._model._provenance(self._raw["declaration"])

    @property
    def component(self) -> Component | None:
        index = self._raw.get("component")
        return None if index is None else self._model.components[index]

    @property
    def is_state(self) -> bool:
        return self.role == "state"

    @property
    def is_parameter(self) -> bool:
        return self.role in ("parameter", "constant")

    @property
    def is_connector_member(self) -> bool:
        return self._raw.get("connector") is not None

    @property
    def physical_quantity(self) -> str | None:
        """The declared MLS §4.8 `quantity` — `"Mass"`, `"Resistance"`.

        The semantic identity of what this variable measures. Distinct from
        `quantity`, which is the *connector* role (potential/flow/stream); the
        names are unfortunately close because Modelica uses the word for both.
        """
        return self._raw.get("physical_quantity")

    @property
    def declaring_class(self) -> str | None:
        """Fully qualified Modelica class that declared this variable.

        `physical_quantity` says what the variable measures; this says what
        declared it. `Resistance` holds for a passive resistor and for a
        negative-impedance converter alike, so a rule that applies to only one
        of them must match here.
        """
        return self._raw.get("declaring_class")

    @property
    def quantity(self) -> str | None:
        """``"potential"``, ``"flow"`` or ``"stream"`` for a connector member."""
        connector = self._raw.get("connector")
        return None if connector is None else connector["quantity"]

    @property
    def connected(self) -> bool:
        connector = self._raw.get("connector")
        return bool(connector and connector.get("connected"))

    def _expression(self, key: str) -> Expression | None:
        index = self._raw.get(key)
        return None if index is None else self._model.expressions[index]

    start = property(lambda self: self._expression("start"))
    binding = property(lambda self: self._expression("binding"))
    minimum = property(lambda self: self._expression("min"))
    maximum = property(lambda self: self._expression("max"))

    def __repr__(self) -> str:
        unit = f" [{self.unit}]" if self.unit else ""
        return f"<Variable {self.id} {self.name} {self.role}{unit}>"


class Equation:
    """A residual equation: the model asserts ``residual == 0``."""

    __slots__ = ("_raw", "_model")

    def __init__(self, raw: dict, model: "Model") -> None:
        self._raw = raw
        self._model = model

    id = property(lambda self: self._raw["id"])

    @property
    def residual(self) -> Expression:
        return self._model.expressions[self._raw["residual"]]

    @property
    def source(self) -> Provenance:
        return self._model._provenance(self._raw["provenance"])

    @property
    def reads(self) -> list[Variable]:
        """Variables this equation reads.

        Precomputed by the compiler's own dependency projection, which resolves
        function calls and array selection correctly. Prefer this over walking
        the expression tree yourself.
        """
        return [self._model.variables[i] for i in self._raw.get("reads", [])]

    @property
    def reads_derivative(self) -> list[Variable]:
        """States whose derivative this equation reads."""
        return [self._model.variables[i] for i in self._raw.get("reads_derivative", [])]

    @property
    def reads_previous(self) -> list["Variable"]:
        """Variables whose left limit ``pre(v)`` this reads (MLS §3.7.5).

        Separate from `reads` because `pre(v)` is the value `v` held at event
        entry — a known. A dependency analysis wants both; a *matching*
        analysis must use only `reads`, or an equation reading `pre(x)` looks
        able to determine `x`.
        """
        return [self._model.variables[i]
                for i in self._raw.get("reads_previous", [])]


    def __repr__(self) -> str:
        return f"<Equation {self.id} at {self.source.span}>"


@dataclass(frozen=True)
class FunctionParameter:
    """One formal parameter of a function declaration."""

    name: str
    value_type: int


class Function:
    """A function declaration, without its body.

    Bitcode v2 carries the *signature* — what a call site needs — and not the
    body, which is a separate IR of SSA definitions, loop transitions and
    external interfaces. `has_body` says which, so an absent body is never
    mistaken for an empty one.
    """

    __slots__ = ("_raw", "_model")

    def __init__(self, raw: dict, model: "Model") -> None:
        self._raw = raw
        self._model = model

    id = property(lambda self: self._raw["id"])
    name = property(lambda self: self._raw["name"])

    @property
    def parameters(self) -> list[FunctionParameter]:
        return [FunctionParameter(p["name"], p["value_type"])
                for p in self._raw.get("parameters", [])]

    @property
    def results(self) -> list[ValueType]:
        return [self._model.types[t] for t in self._raw.get("results", [])]

    @property
    def is_external(self) -> bool:
        """An MLS §12.9 foreign body: there is no Modelica body to carry."""
        return self._raw.get("body", {}).get("kind") == "external"

    @property
    def external_symbol(self) -> str | None:
        return self._raw.get("body", {}).get("symbol")

    @property
    def has_body(self) -> bool:
        """False for every Modelica function: v1 elides bodies."""
        return self.is_external

    @property
    def inline(self) -> str:
        return self._raw.get("inline", "unstated")

    @property
    def source(self) -> Provenance:
        return self._model._provenance(self._raw["declaration"])

    def __repr__(self) -> str:
        return (f"<Function {self.name}({len(self.parameters)} args)"
                f"{' external' if self.is_external else ''}>")


@dataclass(frozen=True)
class Binder:
    """One iteration axis: the ``i in 1:3`` of a ``for`` equation."""

    id: int
    name: str
    lower: int
    upper: int
    step: int

    @property
    def extent(self) -> int:
        if self.step == 0:
            return 0
        return max(0, (self.upper - self.lower) // self.step + 1)

    def __str__(self) -> str:
        if self.step == 1:
            return f"{self.name} in {self.lower}:{self.upper}"
        return f"{self.name} in {self.lower}:{self.step}:{self.upper}"


class Domain:
    """The iteration domain an equation family or comprehension ranges over."""

    __slots__ = ("_raw", "_model")

    def __init__(self, raw: dict, model: "Model") -> None:
        self._raw = raw
        self._model = model

    id = property(lambda self: self._raw["id"])
    scalar_count = property(lambda self: self._raw["scalar_count"])
    extents = property(lambda self: tuple(self._raw.get("extents", [])))

    @property
    def binders(self) -> list[Binder]:
        return [
            Binder(id=b["id"], name=b["display_name"],
                   lower=b["lower"], upper=b["upper"], step=b["step"])
            for b in self._raw.get("binders", [])
        ]

    @property
    def parent(self) -> "Domain | None":
        index = self._raw.get("parent")
        return None if index is None else self._model.domains[index]

    @property
    def source(self) -> Provenance:
        return self._model._provenance(self._raw["provenance"])

    def __repr__(self) -> str:
        return f"<Domain {self.id}: for {', '.join(str(b) for b in self.binders)}>"


class EquationFamily:
    """An array or ``for`` equation: one symbolic body standing for N rows.

    `scalar_rows` is the number a balance or matching analysis must count. A
    family is *not* one equation — treating it as one is what made array
    models look under-constrained by a factor of their extent.
    """

    __slots__ = ("_raw", "_model", "initial")

    def __init__(self, raw: dict, model: "Model", initial: bool = False) -> None:
        self._raw = raw
        self._model = model
        self.initial = initial

    id = property(lambda self: self._raw["id"])
    scalar_rows = property(lambda self: self._raw["scalar_rows"])
    extents = property(lambda self: tuple(self._raw.get("extents", [])))

    @property
    def domain(self) -> Domain:
        return self._model.domains[self._raw["domain"]]

    @property
    def bodies(self) -> list[Expression]:
        return [self._model.expressions[i] for i in self._raw.get("bodies", [])]

    @property
    def reads(self) -> list["Variable"]:
        """Variables the family's rows read.

        From the compiler's own scalar coordinate projection, like
        `Equation.reads`. Walking `bodies` instead misses array and record
        selections, and a missing incidence edge reads as a matching failure —
        a defect reported against a model that does not have one.
        """
        return [self._model.variables[i] for i in self._raw.get("reads", [])]

    @property
    def reads_derivative(self) -> list["Variable"]:
        return [self._model.variables[i]
                for i in self._raw.get("reads_derivative", [])]

    @property
    def reads_previous(self) -> list["Variable"]:
        """Variables whose left limit ``pre(v)`` this reads (MLS §3.7.5).

        Separate from `reads` because `pre(v)` is the value `v` held at event
        entry — a known. A dependency analysis wants both; a *matching*
        analysis must use only `reads`, or an equation reading `pre(x)` looks
        able to determine `x`.
        """
        return [self._model.variables[i]
                for i in self._raw.get("reads_previous", [])]


    @property
    def scalar_view(self) -> str:
        return self._raw.get("scalar_view", {}).get("kind", "")

    @property
    def source(self) -> Provenance:
        return self._model._provenance(self._raw["provenance"])

    def __repr__(self) -> str:
        return (f"<EquationFamily {self.id}: {self.scalar_rows} rows "
                f"over {self.domain!r}>")


class DiscreteRealEquation:
    """An MLS Appendix B.1b coupled discrete-Real equation.

    These determine the discrete-Real unknowns — a sampled hold, a mean held
    between events — and are a *different partition* from the continuous
    residuals. Counting them alongside `Model.equations` makes every model with
    a `sample` look over-constrained; leaving both them and their unknowns out
    is what keeps the books straight.
    """

    __slots__ = ("_raw", "_model")

    def __init__(self, raw: dict, model: "Model") -> None:
        self._raw = raw
        self._model = model

    id = property(lambda self: self._raw["id"])

    @property
    def residual(self) -> Expression:
        return self._model.expressions[self._raw["residual"]]

    @property
    def activation(self) -> str:
        """``"always"`` or ``"when"``."""
        return self._raw.get("activation", {}).get("kind", "always")

    @property
    def reads(self) -> list["Variable"]:
        return [self._model.variables[i] for i in self._raw.get("reads", [])]

    @property
    def reads_derivative(self) -> list["Variable"]:
        return [self._model.variables[i]
                for i in self._raw.get("reads_derivative", [])]

    @property
    def reads_previous(self) -> list["Variable"]:
        """Variables whose left limit ``pre(v)`` this reads (MLS §3.7.5).

        Separate from `reads` because `pre(v)` is the value `v` held at event
        entry — a known. A dependency analysis wants both; a *matching*
        analysis must use only `reads`, or an equation reading `pre(x)` looks
        able to determine `x`.
        """
        return [self._model.variables[i]
                for i in self._raw.get("reads_previous", [])]


    @property
    def source(self) -> Provenance:
        return self._model._provenance(self._raw["provenance"])

    def __repr__(self) -> str:
        return f"<DiscreteRealEquation {self.id} ({self.activation})>"


class InitialDiscreteValue:
    """What a discrete variable is set to at the initialization instant."""

    __slots__ = ("_raw", "_model")

    def __init__(self, raw: dict, model: "Model") -> None:
        self._raw = raw
        self._model = model

    @property
    def target(self) -> "Variable":
        return self._model.variables[self._raw["target"]]

    @property
    def value(self) -> Expression:
        return self._model.expressions[self._raw["value"]]

    @property
    def source(self) -> Provenance:
        return self._model._provenance(self._raw["provenance"])

    def __repr__(self) -> str:
        return f"<InitialDiscreteValue {self.target.name} = {self.value!r}>"


class Connection:
    """One ``connect(...)`` relationship.

    A connection is symmetric: ``left`` and ``right`` are endpoints of an
    equality (for a potential quantity) or of a conservation law (for a flow
    quantity). It is not a directional message.
    """

    __slots__ = ("_raw", "_model")

    def __init__(self, raw: dict, model: "Model") -> None:
        self._raw = raw
        self._model = model

    id = property(lambda self: self._raw["id"])
    quantity = property(lambda self: self._raw["quantity"])
    left_connector = property(lambda self: self._raw["left_connector"])
    right_connector = property(lambda self: self._raw["right_connector"])

    @property
    def left(self) -> Variable:
        return self._model.variables[self._raw["left"]]

    @property
    def right(self) -> Variable:
        return self._model.variables[self._raw["right"]]

    @property
    def members(self) -> list[Variable]:
        return [self.left, self.right]

    @property
    def source(self) -> Provenance:
        return self._model._provenance(self._raw["provenance"])

    @property
    def is_flow(self) -> bool:
        return self.quantity == "flow"

    def __repr__(self) -> str:
        return f"<Connection {self.id} {self.left_connector} <-> {self.right_connector} ({self.quantity})>"


class Event:
    """An action performed when an event fires."""

    __slots__ = ("_raw", "_model")

    def __init__(self, raw: dict, model: "Model") -> None:
        self._raw = raw
        self._model = model

    id = property(lambda self: self._raw["id"])
    kind = property(lambda self: self._raw["action"]["kind"])

    @property
    def source(self) -> Provenance:
        return self._model._provenance(self._raw["provenance"])

    @property
    def state(self) -> Variable | None:
        """For a ``reinitialize`` action, the state it resets."""
        action = self._raw["action"]
        if action["kind"] != "reinitialize":
            return None
        return self._model.variables[action["state"]]

    @property
    def value(self) -> Expression | None:
        action = self._raw["action"]
        index = action.get("value") if action["kind"] == "reinitialize" else None
        return None if index is None else self._model.expressions[index]

    def __repr__(self) -> str:
        return f"<Event {self.id} {self.kind} at {self.source.span}>"


class TracePoint:
    """A request to observe a value at runtime."""

    __slots__ = ("_raw", "_model")

    def __init__(self, raw: dict, model: "Model") -> None:
        self._raw = raw
        self._model = model

    id = property(lambda self: self._raw["id"])
    label = property(lambda self: self._raw["label"])
    unit = property(lambda self: self._raw.get("unit"))
    quantity = property(lambda self: self._raw.get("quantity"))
    added_by = property(lambda self: self._raw.get("added_by"))

    @property
    def variable(self) -> Variable:
        return self._model.variables[self._raw["variable"]]

    @property
    def connection_set(self) -> "ConnectionSet | None":
        """The node this observation belongs to.

        Distinct from `connection`, which names one pairwise `connect`. A flow
        observation belongs to the node, and attributing it to one of the
        edges that happen to form that node would name an arbitrary one.
        """
        index = self._raw.get("connection_set")
        if index is None:
            return None
        sets = self._model.connection_sets
        return sets[index] if index < len(sets) else None

    @property
    def connection(self) -> Connection | None:
        index = self._raw.get("connection")
        return None if index is None else self._model.connections[index]

    def __repr__(self) -> str:
        return f"<TracePoint {self.id} {self.label!r}>"
