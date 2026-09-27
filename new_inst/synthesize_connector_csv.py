#!/usr/bin/env python3
"""Runnable acceptance example for the writable equation/execution SDK.

Requires this fork's public scalar execution v1 profile and native RK45 backend.

The script never creates CSV rows in Python. It inserts serialized executable
IR operations that the Rumoca runtime must execute, including in a fresh process.
The native `rumoca bitcode run` command consumes the saved executable artifact.

Examples:
    python synthesize_connector_csv.py --out build/demo
    python synthesize_connector_csv.py --out build/demo --run
"""
from __future__ import annotations

import argparse
import math
import os
from pathlib import Path
import subprocess
import sys
from typing import Any, TYPE_CHECKING

if TYPE_CHECKING:
    from rumoca_bitcode import Model
    from rumoca_bitcode.execution import Program


#: Connector member scalar type -> the program's value type. `string` is
#: spelled `text` in the program vocabulary; the two names are not a synonym
#: pair to guess between, so the map is written out.
COLUMN_TYPES = {"real": "real", "integer": "integer",
                "boolean": "boolean", "string": "text"}


def positive_finite(text: str) -> float:
    value = float(text)
    if not math.isfinite(value) or value <= 0:
        raise argparse.ArgumentTypeError("expected a positive finite number")
    return value


def synthesize() -> tuple[Model, dict[str, Any]]:
    """Build all equations, variables, types, components and connections."""
    from rumoca_bitcode import Model

    model = Model.empty("TwoThermalBodies")
    b = model.builder("example.synthesize")

    port_type = b.add_connector_type(
        "demo.HeatPort",
        members=[
            {"name": "T", "scalar_type": "real", "unit": "K",
             "quantity": "ThermodynamicTemperature", "kind": "potential"},
            {"name": "Q_flow", "scalar_type": "real", "unit": "W",
             "quantity": "HeatFlowRate", "kind": "flow"},
        ],
        flow_convention="positive_into_owner",
    )
    hot = b.add_component("hot", type_name="demo.HeatCapacity")
    link = b.add_component("link", type_name="demo.ThermalConductor")
    cold = b.add_component("cold", type_name="demo.HeatCapacity")

    ch = b.add_parameter("C", owner=hot, value=2.0, unit="J/K")
    cc = b.add_parameter("C", owner=cold, value=3.0, unit="J/K")
    g = b.add_parameter("G", owner=link, value=1.0, unit="W/K")

    # Causality is separate from state role. Give the runtime real observables.
    th = b.add_variable("T", owner=hot, role="state", scalar_type="real",
                        unit="K", causality="output")
    tc = b.add_variable("T", owner=cold, role="state", scalar_type="real",
                        unit="K", causality="output")
    hp = b.add_connector("port", owner=hot, type_id=port_type)
    la = b.add_connector("a", owner=link, type_id=port_type)
    lb = b.add_connector("b", owner=link, type_id=port_type)
    cp = b.add_connector("port", owner=cold, type_id=port_type)

    # Member lookup returns semantic variable IDs, not strings or storage slots.
    ht, hq = b.member(hp, "T"), b.member(hp, "Q_flow")
    at, aq = b.member(la, "T"), b.member(la, "Q_flow")
    bt, bq = b.member(lb, "T"), b.member(lb, "Q_flow")
    ct, cq = b.member(cp, "T"), b.member(cp, "Q_flow")
    r = b.ref

    # Two explicit initial constraints; a start guess is not used as a constraint.
    b.add_initial_equation(b.sub(r(th), b.real(350.0)))
    b.add_initial_equation(b.sub(r(tc), b.real(300.0)))
    b.add_equation(b.sub(r(ht), r(th)))
    b.add_equation(b.sub(r(ct), r(tc)))
    b.add_derivative_equation(th, b.div(r(hq), r(ch)))
    b.add_derivative_equation(tc, b.div(r(cq), r(cc)))

    # The conductor has no heat storage.
    law = b.add_equation(
        b.sub(r(aq), b.mul(r(g), b.sub(r(at), r(bt))))
    )
    b.add_equation(b.add(r(aq), r(bq)))

    # Each helper must add BOTH semantic connection metadata and its equations.
    # These two disjoint sets have equal potentials and one zero-sum flow law.
    # Do not later add the connection equations a second time during lowering.
    b.add_connection_set([hp, la])
    b.add_connection_set([lb, cp])

    model.refresh()
    model.validate(strict=True)
    return model, {"law": law, "g": g, "aq": aq, "at": at, "bt": bt}


def scale_conductor_equation(model: Model, handles: dict[str, Any],
                             scale: float) -> None:
    """An equation-IR pass: change the constitutive equation, not a runtime value."""
    b = model.builder("example.scale-conductor-equation")
    r = b.ref
    effective_g = b.mul(b.real(scale), r(handles["g"]))
    rewritten = b.sub(
        r(handles["aq"]),
        b.mul(effective_g, b.sub(r(handles["at"]), r(handles["bt"]))),
    )
    b.rewrite_equation(handles["law"], rewritten)
    model.refresh()
    model.validate(strict=True)


def instrument_all_connectors(program: Program, model: Model) -> None:
    """Execution-IR pass. No connector names or physics appear in this logger.

    The pass registers a trace point per connector member on the *model* and
    references it from the instructions: the trace point is the observation
    request, and the program is what demands it.
    """
    from rumoca_bitcode.execution import observe_connector_members

    with program.builder("example.connector-csv") as b:
        points = observe_connector_members(model)
        prepared: list[tuple[Any, tuple[Any, ...]]] = []
        sink_paths: dict[Any, str] = {}
        connectors = sorted(model.connectors, key=lambda c: str(c.id))
        if not connectors:
            raise ValueError("the example must contain observable connectors")

        for index, connector in enumerate(connectors):
            members = tuple(connector.members)  # Stable declaration order.
            if not members:
                raise ValueError(f"connector {connector.id} has no members")
            for member in members:
                if member.shape or member.scalar_type not in {
                    "real", "integer", "boolean", "string"
                }:
                    raise ValueError(
                        f"unsupported CSV member {connector.id}/{member.name}; "
                        "must not silently omit it"
                    )
                if member.kind == "stream":
                    raise ValueError("stream observation semantics are not in this demo")

            sink = b.declare_csv_sink(
                key=f"connector-csv:{connector.id}",  # Unique, duplicate pass => reject.
                filename=f"connector-{index:04d}.csv",  # Relative, collision-free.
                columns=[{"name": "time_s", "ty": "real"},
                         {"name": "publish_id", "ty": "integer"},
                         {"name": "phase", "ty": "text"},
                         *[{"name": m.name, "ty": COLUMN_TYPES[m.scalar_type]}
                           for m in members]],
                # Identity only. unit/kind/variable are reachable from the trace
                # point, and the runtime denormalizes them into manifest.json.
                metadata={
                    "connector": int(connector.id),
                    "orientation": connector.orientation,
                    "members": [
                        {"trace_point": points[f"{connector.path}.{m.name}"]}
                        for m in members
                    ],
                },
            )
            prepared.append((sink, members))
            sink_paths[sink] = connector.path

        # These are ordinary serializable functions/blocks, not Python callbacks.
        # before_return is convenience for the single-exit lifecycle functions.
        with b.before_return("run_start") as ir:
            for sink, _ in prepared:
                ir.emit("csv.open", sink=sink)

        with b.before_return("publish") as ir:
            snapshot = ir.argument("snapshot")
            time = ir.emit("snapshot.time", snapshot=snapshot)
            sequence = ir.emit("snapshot.sequence", snapshot=snapshot)
            phase = ir.emit("snapshot.phase", snapshot=snapshot)
            for sink, members in prepared:
                values = []
                for member in members:
                    values.append(ir.emit(
                        "snapshot.value",
                        snapshot=snapshot,
                        trace_point=points[f"{sink_paths[sink]}.{member.name}"],
                    ))
                # Column order is owned here: this list is the row.
                ir.emit("csv.write_row", sink=sink,
                        values=[time, sequence, phase, *values])

        with b.before_return("run_finish") as ir:
            for sink, _ in prepared:
                ir.emit("csv.close", sink=sink)

    # Includes effect, reference, type, lifecycle and derivation validation.
    program.validate(strict=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=Path("build/connector-demo"))
    parser.add_argument("--conductance-scale", type=positive_finite, default=2.0)
    parser.add_argument("--run", action="store_true")
    parser.add_argument("--rumoca", default=os.environ.get("RUMOCA", "rumoca"))
    args = parser.parse_args()
    # Use the selected compiler for both authoring checks/lowering and runtime.
    os.environ["RUMOCA"] = args.rumoca
    out = args.out.resolve()
    # Never overwrite a previous run or silently combine old and new traces.
    if out.exists() and any(out.iterdir()):
        parser.error(f"output directory is not empty: {out}")
    out.mkdir(parents=True, exist_ok=True)

    from rumoca_bitcode.execution import lower, observe_connector_members

    model, handles = synthesize()
    model.save(out / "01-original.rbc")
    scale_conductor_equation(model, handles, args.conductance_scale)
    model.save(out / "02-equation-rewritten.rbc")

    # Trace points are registered by the logging pass below, which is the
    # producer of observation requests; lowering takes no observation list.
    program = lower(model)
    program.validate(strict=True)
    program.save(out / "03-executable.rbc", include_equations=True)
    instrument_all_connectors(program, model)
    artifact = out / "04-instrumented.rbc"
    program.save(artifact, include_equations=True)
    print(f"Wrote instrumented artifact: {artifact}")

    if args.run:
        # Execute the saved execution IR exactly; do not silently
        # re-lower equations and discard instrumentation. Fresh native process.
        subprocess.run(
            [args.rumoca, "bitcode", "run", str(artifact),
             "--execution", "require", "--start", "0", "--stop", "5",
             "--publish-interval", "0.1", "--rtol", "1e-9", "--atol", "1e-11",
             "--trace-root", str(out / "traces")],
            check=True,
        )
        print(f"Runtime traces: {out / 'traces'}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ImportError as exc:
        print(
            "This acceptance example requires this fork's dual-IR SDK helpers. "
            "It is not a compatibility script for an unmodified Rumoca install.\n"
            f"Details: {exc}", file=sys.stderr,
        )
        raise SystemExit(2) from exc
    except (OSError, ValueError, subprocess.CalledProcessError) as exc:
        print(f"Connector example failed: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
