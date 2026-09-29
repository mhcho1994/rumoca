"""RangeSan — a variable leaving the range its own declaration promised.

1. Bug class    the model states `min`/`max` and computes outside them. Unlike a
                crash this is a *silent* wrong answer: the run completes and the
                trajectory is simply invalid.
2. Overlap      distinct. NumericSan needs the value to be non-finite; DomainSan
                needs a restricted operation. A mass fraction reaching -1.3e-1 is
                neither, and is still wrong.
3. Signal       runtime: an observed value outside a declared bound.
4. Needs        OBSERVE_VARIABLE. Bounds come from DAE metadata, already in the
                artifact. Canonical identity is *not* required — see below.
5. Transform    no.
6. Fuzzing      hints: the bounds themselves are the values worth trying.
7. Signature    the anchored variable plus which bound broke. Canonical where
                available, backend-namespaced otherwise.

**Backend-only anchoring is deliberate.** OpenModelica reports variable names,
not DAE ids. Discarding a real violation because the identity is weaker would
throw away bug-finding capability; inventing a DAE id would corrupt identity
semantics for everything downstream. So the finding is kept and labelled
backend-only, and the bound is matched by name.
"""

from __future__ import annotations

from ..analysis.context import AnalysisContext
from ..findings.location import locate
from ..findings.finding import Finding, Severity, SourceLocation
from ..fuzz.hints import FuzzHint
from ..fuzz.testcase import TestCase
from ..instrumentation.capability import Capability
from .base import is_cosmetic
from ..runtime.anchors import EntityKind
from ..runtime.observations import ObservationStream, VariableObservation


#: OMC and Modelica both spell "merely positive" as a denormal lower bound.
#: Probing there asks whether the model survives 1e-308, which is a question
#: about floating point, not about the domain the model declared. It produced
#: ~60 findings of the form `PI.T=2.22507e-308` before being filtered.
DENORMAL = 1e-300


def _literal(expression) -> float | None:
    value = getattr(expression, "value", None) if expression is not None else None
    return value if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def _elements(expression) -> list | None:
    """The literal elements of a one-dimensional array bound, or None."""
    elements = getattr(expression, "elements", None) if expression is not None else None
    if not elements:
        return None
    values = [_literal(element) for element in elements]
    return values if all(value is not None for value in values) else None


class RangeSan:
    name = "range"

    #: Notably does *not* require CANONICAL_IDENTITY: a named observation and a
    #: declared bound are enough to establish the violation.
    requires = {
        "runtime": frozenset({Capability.OBSERVE_VARIABLE,
                              Capability.CANONICAL_MODEL}),
        "hints": frozenset({Capability.CANONICAL_MODEL}),
    }

    def __init__(self, relative_tolerance: float = 1e-9) -> None:
        # Bounds are enforced by an integrator only approximately, so a
        # violation below its own tolerance is not a model defect.
        self.relative_tolerance = relative_tolerance

    def _bounds(self, model):
        """{name: (min, max, variable)} — keyed by name so a backend-anchored
        observation can be matched without a canonical id."""
        found = {}
        for variable in model.variables:
            low, high = _literal(variable.minimum), _literal(variable.maximum)
            if low is not None or high is not None:
                found[variable.name] = (low, high, variable)
        return found

    @staticmethod
    def _array_bounds(model):
        """{name: (min elements, max elements, variable)} for literal array
        bounds, e.g. `Real w[3](max = {5, 2.5, 5})`."""
        found = {}
        for variable in model.variables:
            low, high = _elements(variable.minimum), _elements(variable.maximum)
            if low is not None or high is not None:
                found[variable.name] = (low, high, variable)
        return found

    @staticmethod
    def _element_bound(name, bounds, arrays):
        """The bound on one element column, `y[2]`.

        Observations are published per element while bounds are declared per
        variable; matching only whole names skipped every array element. A
        scalar bound (`each max = 2.9`) holds for every element, a literal
        array bound element-wise. Other shapes are left unchecked, not guessed.
        """
        if not (name.endswith("]") and "[" in name):
            return None
        base, subscript = name[: name.index("[")], name[name.index("[") + 1:-1]
        if base in bounds:
            return bounds[base]
        if base not in arrays or not subscript.isdigit():
            return None
        low, high, variable = arrays[base]
        index = int(subscript) - 1
        pick = (lambda items: items[index] if items is not None
                and 0 <= index < len(items) else None)
        low, high = pick(low), pick(high)
        return (low, high, variable) if low is not None or high is not None else None

    def hints(self, model, context: AnalysisContext) -> list[FuzzHint]:
        found = []
        for _, (low, high, variable) in self._bounds(model).items():
            if not variable.is_parameter or is_cosmetic(variable.name):
                continue
            values = tuple(v for v in (low, high)
                           if v is not None and (v == 0.0 or abs(v) > DENORMAL))
            if values:
                found.append(FuzzHint(
                    target=variable.name, values=values,
                    reason="the parameter\'s own declared bound",
                    source=self.name, variable_ids=(variable.id,)))
        return found

    def observe(self, stream: ObservationStream, model, context: AnalysisContext,
                testcase: TestCase) -> list[Finding]:
        bounds = self._bounds(model)
        arrays = self._array_bounds(model)
        worst: dict[tuple[str, str], Finding] = {}

        for observation in stream.of(VariableObservation):
            name = observation.label
            entry = bounds.get(name) or self._element_bound(name, bounds, arrays)
            if entry is None:
                continue
            low, high, variable = entry
            for bound, limit, broken in (("min", low, lambda v, l: v < l),
                                         ("max", high, lambda v, l: v > l)):
                if limit is None or not broken(observation.value, limit):
                    continue
                excess = abs(observation.value - limit)
                if excess <= self.relative_tolerance * max(abs(limit), 1.0):
                    continue
                key = (name, bound)
                previous = worst.get(key)
                if previous and previous.evidence["excess"] >= excess:
                    continue
                worst[key] = Finding(
                    sanitizer=self.name,
                    kind="below-min" if bound == "min" else "above-max",
                    severity=Severity.MEDIUM,
                    # Whichever identity the observation actually carried. The
                    # canonical one is only asserted when the producer knew it.
                    canonical_anchors=([observation.canonical]
                                       if observation.canonical else []),
                    backend_anchors=([observation.backend]
                                     if observation.backend else []),
                    source_locations=self._location(variable),
                    phase=observation.phase,
                    time=observation.time,
                    test_case=testcase,
                    evidence={"variable": name, "bound": bound, "limit": limit,
                              "value": observation.value, "excess": excess,
                              "anchor_quality": observation.anchor_quality.value},
                )
        return list(worst.values())

    @staticmethod
    def _location(variable) -> list[SourceLocation]:
        return locate(variable)
