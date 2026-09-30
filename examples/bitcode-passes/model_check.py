#!/usr/bin/env python3
"""Static analysis for modelling bugs.

    python model_check.py motor.rbc

Each check below looks for something that is *legal Modelica and compiles
cleanly*, but is usually not what the author meant. The compiler cannot reject
these — a dangling connector is a well-formed model — so they live in a
separate analysis that reports rather than refuses.

Findings carry source locations because a finding the user cannot act on is
not worth reporting.
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass, field

from rumoca_bitcode import BinaryOp, Literal, Model, UnaryOp, Unsupported, VariableRef


@dataclass
class Finding:
    check: str
    severity: str
    message: str
    where: str
    detail: str = ""

    def __str__(self) -> str:
        detail = f"\n      {self.detail}" if self.detail else ""
        return f"  [{self.severity}] {self.check}: {self.message}\n      at {self.where}{detail}"


@dataclass
class Report:
    findings: list[Finding] = field(default_factory=list)

    def add(self, *args, **kwargs) -> None:
        self.findings.append(Finding(*args, **kwargs))


# ── Checks ───────────────────────────────────────────────────────────────────


def check_unconnected_connectors(model: Model, report: Report) -> None:
    """A connector member that joins no connection.

    Physically this is a port left dangling: an unconnected `flow` member is
    forced to zero, so no current, torque or mass ever crosses it. That is
    occasionally deliberate and usually a wiring mistake.
    """
    by_connector: dict[str, list] = {}
    for variable in model.variables:
        if variable.is_connector_member:
            by_connector.setdefault(variable.name.rsplit(".", 1)[0], []).append(variable)

    for connector, members in sorted(by_connector.items()):
        if any(member.connected for member in members):
            continue
        flows = [m for m in members if m.quantity == "flow"]
        report.add(
            "unconnected-connector",
            "warning",
            f"connector `{connector}` participates in no connection",
            str(members[0].source.span),
            f"its flow member(s) {', '.join(m.name.rsplit('.', 1)[1] for m in flows)} "
            f"are therefore held at zero" if flows else "",
        )


def check_unused_parameters(model: Model, report: Report) -> None:
    """A parameter no equation reads.

    **Read the caveats before trusting this one.** At DAE level "unused"
    conflates three different situations, and only the first is a bug:

    1. Genuinely dead configuration, or a term the author forgot to use.
    2. A parameter whose value was constant-folded into another binding. The
       symbolic reference is gone by DAE, so the use is invisible here even
       though it is real. MSL's `source.startTime` is this case.
    3. A structural switch consumed by *instantiation* rather than by an
       equation — `Real x if useHeatPort` decides whether a component exists.
       It can never appear in an equation by construction. MSL's
       `resistor.useHeatPort` is this case.

    Cases 2 and 3 are not defects in the model; they are this check running at
    the wrong IR level. Distinguishing them properly needs Flat, where bindings
    are still symbolic and conditional components are still visible. Until then
    a Boolean parameter is reported as a note rather than a warning, because
    case 3 is overwhelmingly Boolean.
    """
    read: set[int] = set()
    for equation in model.equations + model.initial_equations:
        read.update(variable.id for variable in equation.reads)
    # Sampled and held discrete-Real equations are a separate partition from
    # the continuous ones, and read parameters just the same.
    for equation in model.discrete_real_equations:
        read.update(variable.id for variable in equation.reads)
    for initial in model.initial_discrete_values:
        read.update(ref.id for ref in initial.value.variables())
    for event in model.events:
        if event.value is not None:
            read.update(ref.id for ref in event.value.variables())
    # A parameter may also be consumed by another variable's start/binding.
    for variable in model.variables:
        for attribute in (variable.start, variable.binding, variable.minimum, variable.maximum):
            if attribute is not None:
                read.update(ref.id for ref in attribute.variables())

    for parameter in model.parameters:
        if parameter.id in read:
            continue
        structural = parameter.type.scalar == "boolean"
        note = (
            "Boolean parameter: likely a conditional-instantiation switch "
            "consumed at compile time, not a defect"
            if structural
            else "may also be constant-folded into another binding rather than dead"
        )
        detail = f"{parameter.description}; {note}" if parameter.description else note
        report.add(
            "unused-parameter",
            "note" if structural else "warning",
            f"parameter `{parameter.name}` is read by no equation",
            str(parameter.source.span),
            detail,
        )


def unit_of(expression) -> str | None:
    """Bottom-up unit inference, deliberately partial.

    Only additive structure is checked, because that is where a unit error is
    unambiguous: `a + b` requires `a` and `b` to have the same unit, whatever
    that unit is. Products and quotients create derived units this schema does
    not model, so they report `None` rather than a guess.
    """
    if isinstance(expression, VariableRef):
        # der(x) has the unit of x per second, which we do not model; treat it
        # as unknown rather than claim x's unit.
        return None if expression.is_derivative else expression.variable.unit
    if isinstance(expression, UnaryOp):
        return unit_of(expression.operand)
    if isinstance(expression, BinaryOp) and expression.op in ("add", "subtract"):
        left = unit_of(expression.lhs)
        return left if left == unit_of(expression.rhs) else None
    return None


def check_unit_consistency(model: Model, report: Report) -> None:
    """Addition or subtraction of two differently-united quantities."""
    for equation in model.equations:
        for node in equation.residual.walk():
            if not isinstance(node, BinaryOp) or node.op not in ("add", "subtract"):
                continue
            left, right = unit_of(node.lhs), unit_of(node.rhs)
            if left and right and left != right:
                report.add(
                    "unit-mismatch",
                    "error",
                    f"`{node.op}` combines [{left}] and [{right}]",
                    str(equation.source.span),
                    f"{node.lhs!r} [{left}]  {node.op}  {node.rhs!r} [{right}]",
                )


def check_states_without_start(model: Model, report: Report) -> None:
    """A continuous state whose start value the compiler supplied, not the author.

    Testing `start is None` does not work: DAE lowering materialises a default
    start for every state, so the field is always populated. What distinguishes
    them is provenance — an author-written start is `source`, a language default
    is `generated/default_start`. This is the check that would have been
    silently dead without that distinction.
    """
    for state in model.states:
        start = state.start
        if start is None or start.provenance.generation == "default_start":
            report.add(
                "state-without-start",
                "note",
                f"state `{state.name}` has no declared start; it initializes to 0 by language default",
                str(state.source.span),
            )


def check_static_event_conditions(model: Model, report: Report) -> None:
    """An event condition that cannot change during simulation.

    A relation over only parameters and constants is decided before the solve
    begins, so the event either never fires or fires immediately. Usually the
    author meant to compare against a state.
    """
    for relation in model._raw.get("relations", []):
        expression = model.expressions[relation["expression"]]
        variables = expression.variables()
        if not variables:
            continue
        if all(v.is_parameter for v in variables):
            provenance = model._provenance(relation["provenance"])
            report.add(
                "static-event-condition",
                "warning",
                "event condition depends only on parameters, so it cannot change during the solve",
                str(provenance.span),
                f"reads {', '.join(v.name for v in variables)}",
            )


def check_unread_variables(model: Model, report: Report) -> None:
    """A non-output variable that no equation reads.

    It is still solved for, costing work, and nothing consumes the answer.
    """
    read: set[int] = set()
    for equation in model.equations + model.initial_equations:
        read.update(v.id for v in equation.reads)
        read.update(v.id for v in equation.reads_derivative)

    for variable in model.variables:
        if variable.role not in ("algebraic",) or variable.causality == "output":
            continue
        if variable.id not in read:
            report.add(
                "unread-variable",
                "note",
                f"`{variable.name}` is computed but never read",
                str(variable.source.span),
            )


def check_incomplete_artifact(model: Model, report: Report) -> None:
    """The artifact could not represent part of the model.

    Reported first: every other check below it is working from a partial
    picture, and silence would be misleading.
    """
    unsupported = [
        node
        for equation in model.equations
        for node in equation.residual.walk()
        if isinstance(node, Unsupported)
    ]
    if unsupported:
        report.add(
            "incomplete-artifact",
            "error",
            f"{len(unsupported)} expression(s) could not be represented in bitcode v1",
            str(unsupported[0].provenance.span),
            "other findings are based on a partial model",
        )


CHECKS = [
    check_incomplete_artifact,
    check_unit_consistency,
    check_unconnected_connectors,
    check_unused_parameters,
    check_static_event_conditions,
    check_states_without_start,
    check_unread_variables,
]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", nargs="+")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--only", help="run one check by name")
    parser.add_argument(
        "--min-severity",
        choices=["note", "warning", "error"],
        default="note",
    )
    args = parser.parse_args()

    rank = {"note": 0, "warning": 1, "error": 2}
    total = 0
    payload = []

    for path in args.input:
        model = Model.load(path)
        report = Report()
        for check in CHECKS:
            if args.only and check.__name__ != f"check_{args.only.replace('-', '_')}":
                continue
            check(model, report)
        findings = [
            finding
            for finding in report.findings
            if rank[finding.severity] >= rank[args.min_severity]
        ]
        total += len(findings)
        if args.json:
            payload.append(
                {"model": model.name, "findings": [vars(f) for f in findings]}
            )
        else:
            print(f"{model.name} ({path}): {len(findings)} finding(s)")
            for finding in findings:
                print(finding)
            print()

    if args.json:
        print(json.dumps(payload, indent=2))
    return 1 if total else 0


if __name__ == "__main__":
    sys.exit(main())
