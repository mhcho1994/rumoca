#!/usr/bin/env python3
"""Build two open thermal modules, link, connect, instrument and optionally run.

Walkthrough: docs/combining-models.md. Python authors IR; Rumoca runs it.
"""
from __future__ import annotations

import argparse
from pathlib import Path

from rumoca_bitcode import Model
from rumoca_bitcode.compiler import invoke
from rumoca_bitcode.execution import lower
from synthesize_connector_csv import instrument_all_connectors


def thermal_body(initial: float, capacity: float, *, conductance=None) -> Model:
    model = Model.empty("ThermalBody")
    b = model.builder("tutorial.body")
    owner = b.add_component("body", type_name="tutorial.ThermalBody")
    port_type = b.add_connector_type("HeatPort", members=[
        {"name": "T", "scalar_type": "real", "kind": "potential",
         "unit": "K", "quantity": "ThermodynamicTemperature"},
        {"name": "Q_flow", "scalar_type": "real", "kind": "flow",
         "unit": "W", "quantity": "HeatFlowRate"},
    ])
    c = b.add_parameter("C", capacity, owner=owner, unit="J/K")
    temperature = b.add_state("T", initial, owner=owner, unit="K", causality="output")
    port = b.add_connector("port", owner=owner, type_id=port_type)
    tp, q = b.member(port, "T"), b.member(port, "Q_flow")
    b.add_derivative_equation(temperature, b.div(b.ref(q), b.ref(c)))
    if conductance is None:
        b.add_equation(b.sub(b.ref(tp), b.ref(temperature)))
    else:
        g = b.add_parameter("G", conductance, owner=owner, unit="W/K")
        b.add_equation(b.sub(b.ref(q), b.mul(b.ref(g), b.sub(b.ref(tp), b.ref(temperature)))))
    # Leave the port open: no singleton set and no implicit Q_flow = 0.
    b.finish()
    model.validate(connections=True)
    return model


def connect_bodies(model: Model) -> None:
    ports = {p.path: p.id for p in model.connectors}
    b = model.builder("tutorial.connect")
    b.add_connection_set([ports["hot.body.port"], ports["cold.body.port"]])
    b.finish()
    model.validate(connections=True)


def add_heat_integral(model: Model) -> None:
    """Equation pass: integrate heat entering the hot body; no feedback."""
    b = model.builder("tutorial.heat-integral")
    q = b.variable("hot.body.port.Q_flow")
    energy = b.add_state("heat_into_hot", 0.0, unit="J", causality="output")
    b.add_derivative_equation(energy, b.ref(q))
    b.finish()
    model.validate(connections=True)


def build_example(out: Path):
    # Refuse an existing directory, even empty, so saved runs stay separate.
    out.mkdir(parents=True, exist_ok=False)
    thermal_body(350.0, 2.0, conductance=1.0).save(out / "hot.rbc")
    thermal_body(300.0, 3.0).save(out / "cold.rbc")
    model = Model.link({"hot": out / "hot.rbc", "cold": out / "cold.rbc"},
                       name="ConnectedThermalBodies")
    model.save(out / "01-linked-open.rbc")
    connect_bodies(model)
    model.save(out / "02-connected.rbc")
    add_heat_integral(model)
    model.save(out / "03-equation-pass.rbc")
    # No observation list: the program demands what it needs, and
    # instrument_all_connectors is the producer of the trace points.
    program = lower(model)
    instrument_all_connectors(program, model)
    program.save(out / "04-executable.rbc")
    return model, program


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--run", action="store_true")
    args = parser.parse_args()
    out = args.out.resolve()
    build_example(out)
    if args.run:
        invoke("bitcode", "run", out / "04-executable.rbc", "--execution", "require",
               "--trace-root", out / "traces", "--stop", "5", "--publish-interval", "0.1",
               "--rtol", "1e-9", "--atol", "1e-11", "--result", out / "result.json")
    print(f"Artifacts: {out}")


if __name__ == "__main__":
    main()
