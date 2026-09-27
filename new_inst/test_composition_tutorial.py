"""Keep the composition tutorial executable and check its physical result."""
import csv
import json
import math
from pathlib import Path
import tempfile
import unittest

from compose_models import build_example
from rumoca_bitcode.compiler import invoke
from synthesize_connector_csv import instrument_all_connectors


class CompositionTutorial(unittest.TestCase):
    def test_link_wire_passes_and_native_trajectory(self):
        with tempfile.TemporaryDirectory(prefix="rbc-composition-") as root:
            out = Path(root) / "example"
            model, program = build_example(out)
            self.assertEqual(len(model.connectors), 2)
            self.assertEqual(len(model.raw_model["connection_sets"]), 1)
            replayed = program.relower(replay={"example.connector-csv": instrument_all_connectors})
            self.assertEqual(replayed.raw["program"]["sinks"], program.raw["program"]["sinks"])
            invoke("bitcode", "run", out / "04-executable.rbc", "--execution", "require",
                   "--trace-root", out / "traces", "--stop", "5", "--publish-interval", "0.1",
                   "--rtol", "1e-9", "--atol", "1e-11", "--result", out / "result.json")
            result = json.loads((out / "result.json").read_text())
            values = dict(zip(result["names"], result["data"]))
            for name in ("hot.body.T", "cold.body.T", "heat_into_hot"):
                self.assertEqual(len(values[name]), 51)
            for i in range(51):
                factor = math.exp(-(1 / 2 + 1 / 3) * i / 10)
                self.assertAlmostEqual(values["hot.body.T"][i], 320 + 30 * factor, delta=1e-5)
                self.assertAlmostEqual(values["cold.body.T"][i], 320 - 20 * factor, delta=1e-5)
                self.assertAlmostEqual(values["heat_into_hot"][i], 60 * (factor - 1), delta=1e-5)
            streams = []
            for path in sorted((out / "traces").glob("*.csv")):
                with path.open(newline="") as stream:
                    streams.append(list(csv.DictReader(stream)))
            self.assertEqual([len(rows) for rows in streams], [51, 51])
            for i, (hot, cold) in enumerate(zip(*streams)):
                factor = math.exp(-(1 / 2 + 1 / 3) * i / 10)
                self.assertAlmostEqual(float(hot["T"]), 320 - 20 * factor, delta=1e-5)
                self.assertAlmostEqual(float(hot["T"]), float(cold["T"]), delta=1e-8)
                self.assertAlmostEqual(float(hot["Q_flow"]), -50 * factor, delta=1e-5)
                self.assertAlmostEqual(float(hot["Q_flow"]) + float(cold["Q_flow"]), 0, delta=1e-8)


if __name__ == "__main__":
    unittest.main()
