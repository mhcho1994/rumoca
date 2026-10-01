"""`modelsan check`: one command per model, like `gcc -fsanitize=`."""
from pathlib import Path
import os
import subprocess
import sys
import tempfile
import unittest

from modelsan.check import GROUPS, selected

ROOT = Path(__file__).resolve().parents[3]
RUMOCA = ROOT / "target/debug/rumoca"
TANK = """
model Tank
  parameter Real A = 1 "Tank area";
  parameter Real k = 0.5 "Outflow coefficient";
  Real h(start = 1, fixed = true, min = 0) "Level";
  Real q "Outflow";
equation
  q = k * sqrt(h);
  A * der(h) = 0.2 - q - 0.6;
end Tank;
"""


class Selection(unittest.TestCase):
    def test_groups_and_names_expand_in_order_without_duplicates(self):
        self.assertEqual(selected("default"), GROUPS["default"])
        self.assertEqual(selected("domain,range,domain"), ["domain", "range"])

    def test_a_leading_minus_removes_one(self):
        names = selected("all,-network")
        self.assertNotIn("network", names)
        self.assertEqual(len(names), len(GROUPS["all"]) - 1)

    def test_an_unknown_name_is_refused(self):
        with self.assertRaisesRegex(ValueError, "unknown sanitizer `bogus`"):
            selected("domain,bogus")


@unittest.skipUnless(RUMOCA.exists(), "needs target/debug/rumoca")
class Command(unittest.TestCase):
    def run_check(self, *extra):
        with tempfile.TemporaryDirectory() as work:
            source = Path(work) / "Tank.mo"
            source.write_text(TANK.lstrip())
            return subprocess.run(
                [sys.executable, "-m", "modelsan.cli", "check", str(source), "--model", "Tank",
                 "--rumoca", str(RUMOCA), "--stop-time", "5", *extra],
                capture_output=True, text=True, timeout=600, cwd=work,
                env={**os.environ, "PYTHONPATH": os.pathsep.join(
                    str(ROOT / "packages" / name) for name in ("rumoca-bitcode", "modelsan"))})

    def test_findings_print_as_diagnostics_and_exit_one(self):
        result = self.run_check()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("Tank.mo:2: medium: [singularity] vanishing-coefficient", result.stdout)
        self.assertIn("[solver] simulation-failure: simulation failed: non-finite", result.stdout)

    def test_a_model_that_does_not_compile_exits_two(self):
        result = self.run_check("--model", "Missing")
        self.assertEqual(result.returncode, 2)


if __name__ == "__main__":
    unittest.main()
