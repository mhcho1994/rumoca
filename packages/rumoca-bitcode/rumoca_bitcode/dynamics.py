"""Typed views of the event, clock and owner tables of an artifact.

Everything the compiler exports has a typed view here, so a pass never has to
reach into ``Model.raw_model`` to read a section: relations and the
conditions built from them, the roots that watch them, clocks and the
variables they own, scheduled time events, discrete definitions, event
transactions, ``previous``/``delay``/``terminal`` owners and structured
roots. Each view wraps one raw entry and resolves its ids through the model
it came from.
"""
from __future__ import annotations

from fractions import Fraction
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:  # pragma: no cover
    from . import Model
    from .model import Domain, Expression, Provenance, Variable


class _View:
    """One raw table entry, read through the model that owns it."""

    __slots__ = ("_raw", "_model")

    def __init__(self, raw: dict[str, Any], model: "Model") -> None:
        self._raw = raw
        self._model = model

    @property
    def raw(self) -> dict[str, Any]:
        """The entry as the artifact stores it."""
        return self._raw

    @property
    def source(self) -> "Provenance":
        return self._model._provenance(self._raw["provenance"])

    def _expression(self, key: str) -> "Expression":
        return self._model.expressions[self._raw[key]]

    def _variable(self, id: int) -> "Variable":
        return self._model.variables[id]

    def __repr__(self) -> str:
        return f"{type(self).__name__}({self._raw.get('id', '')})"


class Relation(_View):
    """A comparison the runtime watches for sign changes (MLS §8.5)."""

    __slots__ = ()
    id = property(lambda self: self._raw["id"])

    @property
    def expression(self) -> "Expression":
        return self._expression("expression")


class Condition(_View):
    """A node of an event condition: `initial()`, a relation, a Boolean
    expression, `not`/`and`/`or`, an edge over two conditions, or a clock
    tick. ``kind`` names which; the operands are resolved accordingly."""

    __slots__ = ()
    id = property(lambda self: self._raw["id"])
    kind = property(lambda self: self._raw["node"]["kind"])

    @property
    def relation(self) -> Relation | None:
        node = self._raw["node"]
        return self._model.relations[node["relation"]] if "relation" in node else None

    @property
    def expression(self) -> "Expression | None":
        node = self._raw["node"]
        return self._model.expressions[node["expression"]] if "expression" in node else None

    @property
    def operands(self) -> list["Condition"]:
        """The conditions this node combines, in order (empty for a leaf)."""
        node = self._raw["node"]
        return [
            self._model.conditions[node[key]]
            for key in ("operand", "lhs", "rhs")
            if key in node
        ]

    @property
    def clock(self) -> "Clock | None":
        node = self._raw["node"]
        return self._model.clocks[node["clock"]] if "clock" in node else None

    @property
    def detail(self) -> str | None:
        """For an ``unsupported`` node, what the exporter could not carry."""
        return self._raw["node"].get("detail")


class Root(_View):
    """A zero-crossing function: a relation watched while its activation holds."""

    __slots__ = ()
    id = property(lambda self: self._raw["id"])

    @property
    def relation(self) -> Relation:
        return self._model.relations[self._raw["relation"]]

    @property
    def activation(self) -> Condition:
        return self._model.conditions[self._raw["activation"]]


def _rational(raw: dict[str, str]) -> Fraction:
    return Fraction(int(raw["numerator"]), int(raw["denominator"]))


class Clock(_View):
    """A synchronous clock (MLS §16): periodic with an exact rational period
    and phase, or triggered by a condition."""

    __slots__ = ()
    id = property(lambda self: self._raw["id"])
    kind = property(lambda self: self._raw["node"]["kind"])

    @property
    def period(self) -> Fraction | None:
        node = self._raw["node"]
        return _rational(node["period"]) if "period" in node else None

    @property
    def phase(self) -> Fraction | None:
        node = self._raw["node"]
        return _rational(node["phase"]) if "phase" in node else None

    @property
    def anchor(self) -> str | None:
        """``absolute`` or ``simulation_start`` for a periodic clock."""
        return self._raw["node"].get("anchor")

    @property
    def condition(self) -> Condition | None:
        node = self._raw["node"]
        return self._model.conditions[node["condition"]] if "condition" in node else None


class ClockOwnership(_View):
    """A variable that belongs to a clock partition (``sampled`` when it is the
    sample of a continuous value)."""

    __slots__ = ()
    sampled = property(lambda self: bool(self._raw["sampled"]))

    @property
    def variable(self) -> "Variable":
        return self._variable(self._raw["variable"])

    @property
    def clock(self) -> Clock:
        return self._model.clocks[self._raw["clock"]]


class TimeEvent(_View):
    """A scheduled event: at an exact rational time, or at a deadline the model
    computes."""

    __slots__ = ()
    id = property(lambda self: self._raw["id"])
    kind = property(lambda self: self._raw["schedule"]["kind"])

    @property
    def time(self) -> Fraction | None:
        schedule = self._raw["schedule"]
        if schedule["kind"] != "static":
            return None
        return Fraction(schedule["numerator"], schedule["denominator"])

    @property
    def deadline(self) -> "Expression | None":
        schedule = self._raw["schedule"]
        return self._model.expressions[schedule["deadline"]] if "deadline" in schedule else None


class DiscreteBranch(_View):
    """One `when` branch of a discrete definition, or its always-active form."""

    __slots__ = ()
    kind = property(lambda self: self._raw["activation"]["kind"])

    @property
    def trigger(self) -> Condition | None:
        activation = self._raw["activation"]
        return self._model.conditions[activation["trigger"]] if "trigger" in activation else None

    @property
    def guard(self) -> Condition | None:
        activation = self._raw["activation"]
        return self._model.conditions[activation["guard"]] if "guard" in activation else None

    @property
    def values(self) -> list["Expression"]:
        return [self._model.expressions[value] for value in self._raw["values"]]


class DiscreteDefinition(_View):
    """What defines one group of discrete variables, branch by branch."""

    __slots__ = ()

    @property
    def targets(self) -> list["Variable"]:
        return [self._variable(target) for target in self._raw["targets"]]

    @property
    def branches(self) -> list[DiscreteBranch]:
        return [DiscreteBranch(branch, self._model) for branch in self._raw["branches"]]


class TransactionStep(_View):
    """One guarded step of an event transaction and the values it assigns."""

    __slots__ = ()

    @property
    def trigger(self) -> Condition:
        return self._model.conditions[self._raw["trigger"]]

    @property
    def guard(self) -> Condition:
        return self._model.conditions[self._raw["guard"]]

    @property
    def clock(self) -> Clock | None:
        clock = self._raw.get("clock")
        return self._model.clocks[clock] if clock is not None else None

    @property
    def definitions(self) -> list[tuple["Variable", "Expression"]]:
        """(target, value) pairs, in assignment order."""
        return [
            (self._variable(entry["target"]), self._model.expressions[entry["value"]])
            for entry in self._raw["definitions"]
        ]


class EventTransaction(_View):
    """An algorithm section executed atomically at events (MLS §11.1.2)."""

    __slots__ = ()

    @property
    def targets(self) -> list["Variable"]:
        return [self._variable(target) for target in self._raw["targets"]]

    @property
    def steps(self) -> list[TransactionStep]:
        return [TransactionStep(step, self._model) for step in self._raw["steps"]]


class PreviousValue(_View):
    """A `previous(v)` owner on a clock (MLS §16.4)."""

    __slots__ = ()

    @property
    def variable(self) -> "Variable":
        return self._variable(self._raw["variable"])

    @property
    def clock(self) -> Clock:
        return self._model.clocks[self._raw["clock"]]


class Delay(_View):
    """A `delay(expr, delayTime[, delayMax])` owner (MLS §3.7.4.1).

    ``kind`` is ``parameter`` for a delay time fixed by a parameter, or
    ``bounded`` for a time the model computes, bounded by ``maximum``."""

    __slots__ = ()
    kind = property(lambda self: self._raw["delay"]["kind"])

    @property
    def expression(self) -> "Expression":
        return self._expression("source")

    @property
    def delay_time(self) -> "Expression":
        delay = self._raw["delay"]["delay_time"]
        expression = delay["expression"] if isinstance(delay, dict) else delay
        return self._model.expressions[expression]

    @property
    def delay_time_value(self) -> float | None:
        """The parameter's value for a ``parameter`` delay."""
        delay = self._raw["delay"]["delay_time"]
        return delay.get("value") if isinstance(delay, dict) else None

    @property
    def maximum(self) -> float | None:
        maximum = self._raw["delay"].get("maximum")
        return maximum["value"] if maximum else None


class Terminal(_View):
    """A `terminal()` owner; it has only its provenance."""

    __slots__ = ()


class StructuredRoot(_View):
    """A zero-crossing function repeated over a structured index domain."""

    __slots__ = ()

    @property
    def domain(self) -> "Domain":
        return self._model.domains[self._raw["domain"]]

    @property
    def expression(self) -> "Expression":
        return self._expression("expression")


class ConnectorType(_View):
    """A connector class: its members, each a potential, flow or stream."""

    __slots__ = ()
    id = property(lambda self: self._raw["id"])
    name = property(lambda self: self._raw["name"])
    flow_convention = property(lambda self: self._raw["flow_convention"])

    @property
    def members(self) -> list[dict[str, Any]]:
        """Each member's ``name``, ``scalar_type``, ``unit``, ``quantity`` and
        ``kind`` (``potential``, ``flow`` or ``stream``)."""
        return list(self._raw["members"])

    @property
    def source(self) -> "Provenance":  # connector types carry no provenance
        raise AttributeError("a connector type has no source provenance")
