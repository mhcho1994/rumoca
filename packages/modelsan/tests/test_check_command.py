"""`modelsan check`: one command per model, like `gcc -fsanitize=`."""
from pathlib import Path
import os
import subprocess
import sys
import tempfile
import unittest
import json
import time
from unittest.mock import patch

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
    def check_source(self, source_text, *extra):
        with tempfile.TemporaryDirectory() as work:
            source = Path(work) / 'Probe.mo'
            source.write_text(source_text)
            report = Path(work) / 'report.json'
            result = subprocess.run([sys.executable, '-m', 'modelsan.cli', 'check',
                str(source), '--model', 'Probe', '--rumoca', str(RUMOCA),
                '--json', str(report), *extra], capture_output=True, text=True,
                timeout=30)
            return result, json.loads(report.read_text()) if report.exists() else None

    def test_runtime_refusal_is_incomplete_even_without_solver_sanitizer(self):
        result, report = self.check_source('model Probe input Real u; Real x=u; end Probe;',
                                          '-fsanitize=numeric', '--stop-time', '.1')
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertEqual(report['status'], 'incomplete')
        self.assertIn('execution.run0', report['not_checked'])
        self.assertEqual(report['executions'][0]['status'], 'failed')

    def test_static_selection_does_not_simulate_an_unbound_input(self):
        result, report = self.check_source('model Probe input Real u; Real x=u; end Probe;',
                                          '-fsanitize=dimension')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(report['mode'], 'static')
        self.assertEqual(report['executions'], [])
        self.assertEqual(report['not_checked'], {})

    def test_static_group_never_prepares_backend(self):
        from modelsan.cli import main
        with tempfile.TemporaryDirectory() as work:
            source = Path(work) / 'Probe.mo'
            source.write_text('model Probe input Real u; Real x=u; end Probe;')
            with patch('modelsan.backends.rumoca.RumocaBackend',
                       side_effect=AssertionError('static checks must not prepare a simulator')):
                status = main(['check', str(source), '--model', 'Probe', '--rumoca', str(RUMOCA),
                               '-fsanitize=static'])
            self.assertIn(status, (0, 1, 2))  # network metadata may be unavailable

    @unittest.skipUnless(os.name == 'posix', 'executable process-group probe')
    def test_timeout_bounds_compiler_and_descendants(self):
        with tempfile.TemporaryDirectory() as work:
            compiler = Path(work) / 'compiler'
            compiler.write_text('#!/usr/bin/env python3\nimport subprocess,sys,time\n'
                'subprocess.Popen([sys.executable,"-c","import time; time.sleep(30)"])\n'
                'time.sleep(30)\n')
            compiler.chmod(0o700)
            started = time.monotonic()
            result, _ = self.check_source('model Probe end Probe;', '--rumoca', str(compiler),
                                          '--timeout', '.2')
            self.assertEqual(result.returncode, 2)
            self.assertIn('compilation exceeded', result.stderr)
            self.assertLess(time.monotonic()-started, 5)

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
