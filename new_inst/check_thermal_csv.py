#!/usr/bin/env python3
"""Independent CSV oracle for the proposed two-body connector-logging example.

This checker is runnable with the Python standard library. It does not generate
simulation data and does not import Rumoca. Feed it the *runtime-produced* trace
directory after implementing the target API in synthesize_connector_csv.py.

Required runtime manifest shape:
{
  "schema_version": 1,
  "dependency_digest": "sha1:...",
  "execution_revision": 2,
  "sinks": [{"filename": "connector-0000.csv", "metadata": {
      "connector": 0, "connector_path": "hot.port", "orientation": "outside",
      "members": [{"trace_point": 0, "name": "T", "unit": "K",
                   "kind": "potential"},
                  {"trace_point": 1, "name": "Q_flow", "unit": "W",
                   "kind": "flow"}]
  }}]
}

The wire artifact names identities only; the runtime resolves them against the
equation IR when it writes this manifest. That split is why the oracle can stay
Rumoca-free.
"""
from __future__ import annotations

import argparse
import csv
import json
import math
from pathlib import Path
import sys

EXPECTED_CONNECTORS = {"hot.port", "link.a", "link.b", "cold.port"}
COLUMNS = ["time_s", "publish_id", "phase", "T", "Q_flow"]


def reference(t: float, scale: float = 2.0) -> dict[str, tuple[float, float]]:
    """Analytic solution derived from the example's equations, not a solver."""
    if not math.isfinite(t) or t < 0:
        raise ValueError("time must be finite and nonnegative")
    if not math.isfinite(scale) or scale <= 0:
        raise ValueError("scale must be positive and finite")
    factor = math.exp(-scale * (1.0 / 2.0 + 1.0 / 3.0) * t)
    hot = 320.0 + 30.0 * factor
    cold = 320.0 - 20.0 * factor
    q = 50.0 * scale * factor
    return {"hot.port": (hot, -q), "link.a": (hot, q),
            "link.b": (cold, -q), "cold.port": (cold, q)}


def require_close(actual: float, expected: float, tolerance: float,
                  context: str) -> None:
    if not math.isfinite(actual) or abs(actual - expected) > tolerance:
        raise ValueError(f"{context}: got {actual}, expected {expected} +/- {tolerance}")


def verify(trace_root: Path, scale: float = 2.0,
           tolerance: float = 1e-4) -> dict[str, int]:
    if not math.isfinite(tolerance) or tolerance <= 0:
        raise ValueError("tolerance must be positive and finite")
    reference(0.0, scale)  # Validate scale even for a malformed/empty input.
    root = trace_root.resolve()
    manifest = json.loads((root / "manifest.json").read_text(encoding="utf-8"))
    if manifest.get("schema_version") != 1:
        raise ValueError("unsupported trace manifest schema")
    streams: dict[str, list[dict[str, str]]] = {}
    files: set[Path] = set()
    for sink in manifest["sinks"]:
        metadata = sink["metadata"]
        path = metadata["connector_path"]
        if path not in EXPECTED_CONNECTORS or path in streams:
            raise ValueError(f"unexpected or duplicate connector: {path}")
        fields = {m["name"]: m for m in metadata["members"]}
        if (fields.get("T", {}).get("unit") != "K" or
                fields.get("Q_flow", {}).get("unit") != "W" or
                fields.get("Q_flow", {}).get("kind") != "flow"):
            raise ValueError(f"wrong field metadata for {path}")
        filename = Path(sink["filename"])
        resolved = (root / filename).resolve()
        if filename.is_absolute() or not resolved.is_relative_to(root):
            raise ValueError(f"manifest path escapes trace directory: {filename}")
        if resolved in files:
            raise ValueError(f"two connectors share one file: {filename}")
        files.add(resolved)
        with resolved.open(newline="", encoding="utf-8") as stream:
            reader = csv.DictReader(stream)
            if reader.fieldnames != COLUMNS:
                raise ValueError(f"wrong CSV header for {path}: {reader.fieldnames}")
            streams[path] = list(reader)

    if set(streams) != EXPECTED_CONNECTORS:
        raise ValueError(f"missing connectors: {EXPECTED_CONNECTORS - set(streams)}")
    if {p.resolve() for p in root.glob("*.csv")} != files:
        raise ValueError("unlisted or missing CSV files in trace directory")

    # Publication policy for this event-free fixture: t=0,0.1,...,5, 51 records.
    for path, rows in streams.items():
        if len(rows) != 51:
            raise ValueError(f"{path}: expected 51 published records, got {len(rows)}")
        for index, row in enumerate(rows):
            if int(row["publish_id"]) != index:
                raise ValueError(f"{path}: noncanonical publication sequence")
            expected_phase = "initial" if index == 0 else "sample"
            if row["phase"] != expected_phase:
                raise ValueError(f"{path}: unexpected phase {row['phase']}")
            t = float(row["time_s"])
            require_close(t, index / 10.0, 1e-10, f"{path}/time[{index}]")
            temperature, flow = reference(t, scale)[path]
            require_close(float(row["T"]), temperature, tolerance, f"{path}/T[{index}]")
            require_close(float(row["Q_flow"]), flow, tolerance, f"{path}/Q[{index}]")

    for index in range(51):
        hot, a, b, cold = [streams[p][index] for p in
                           ("hot.port", "link.a", "link.b", "cold.port")]
        require_close(float(hot["T"]), float(a["T"]), tolerance, "hot connection T")
        require_close(float(b["T"]), float(cold["T"]), tolerance, "cold connection T")
        require_close(float(hot["Q_flow"]) + float(a["Q_flow"]), 0.0,
                      2 * tolerance, "hot connection flow balance")
        require_close(float(b["Q_flow"]) + float(cold["Q_flow"]), 0.0,
                      2 * tolerance, "cold connection flow balance")
        require_close(2.0 * float(hot["T"]) + 3.0 * float(cold["T"]),
                      1600.0, 5 * tolerance, "stored energy")
    return {"connectors": 4, "rows_per_connector": 51, "total_rows": 204}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace_root", type=Path)
    parser.add_argument("--conductance-scale", type=float, default=2.0)
    parser.add_argument("--tolerance", type=float, default=1e-4)
    args = parser.parse_args()
    result = verify(args.trace_root, args.conductance_scale, args.tolerance)
    print(json.dumps(result, indent=2))
    print("Connector traces match the analytic solution and conservation checks.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, KeyError, TypeError, csv.Error) as exc:
        print(f"Trace verification failed: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
