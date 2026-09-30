"""The worked external passes in ``examples/bitcode-passes`` run on current bitcode.

They are the pass author's guide, and nothing else exercised them: an example
that silently lags the format teaches the wrong thing. Each one runs on a model
with a sampled discrete-Real equation, which is the partition an example is
most likely to forget.
"""
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
RUMOCA = ROOT / "target/debug/rumoca"
EXAMPLES = ROOT / "examples/bitcode-passes"
SDK = ROOT / "packages/rumoca-bitcode"

MODEL = """
model Sampled
  constant Real samplePeriod = 0.1;
  parameter Real gain = 2.0;
  Real x(start = 1, fixed = true);
  discrete output Real held(start = 0.0);
equation
  der(x) = -x;
  when sample(0.0, samplePeriod) then
    held = gain * x;
  end when;
end Sampled;
"""


def run(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [*args],
        capture_output=True,
        text=True,
        timeout=120,
        env={"PYTHONPATH": str(SDK), "PATH": "/usr/bin:/bin"},
    )


@unittest.skipUnless(RUMOCA.exists(), "needs target/debug/rumoca")
class ExamplePasses(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.work = tempfile.TemporaryDirectory()
        work = Path(cls.work.name)
        source = work / "Sampled.mo"
        source.write_text(MODEL)
        cls.artifact = work / "Sampled.rbc"
        compiled = run(
            str(RUMOCA), "compile", str(source), "--model", "Sampled",
            "--emit-bitcode", str(cls.artifact),
        )
        assert compiled.returncode == 0, compiled.stderr

    @classmethod
    def tearDownClass(cls):
        cls.work.cleanup()

    def test_every_analysis_example_runs(self):
        for name in ("model_summary", "dependency_graph", "connector_graph", "model_check"):
            with self.subTest(example=name):
                result = run(sys.executable, str(EXAMPLES / f"{name}.py"), str(self.artifact))
                self.assertNotIn("Traceback", result.stderr, result.stderr)

    def test_model_check_counts_parameters_read_by_discrete_equations(self):
        result = run(sys.executable, str(EXAMPLES / "model_check.py"), str(self.artifact))
        self.assertNotIn("parameter `gain` is read by no equation", result.stdout, result.stdout)

    def test_the_rewriting_example_produces_a_runnable_artifact(self):
        traced = Path(self.work.name) / "traced.rbc"
        result = run(
            sys.executable, str(EXAMPLES / "trace_all.py"), str(self.artifact), "-o", str(traced),
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        simulated = run(
            str(RUMOCA), "compile-bitcode", str(traced), "--simulate", "--t-end", "0.5",
        )
        self.assertEqual(simulated.returncode, 0, simulated.stderr)
        self.assertIn("Simulation complete", simulated.stderr + simulated.stdout)

    def test_an_example_pass_runs_inside_the_compiler(self):
        source = Path(self.work.name) / "Sampled.mo"
        # `exec:` runs to the end of its value, so what follows it is a
        # separate `--pass`.
        result = run(
            str(RUMOCA), "compile", str(source), "--model", "Sampled",
            "--pass", f"default,exec:{sys.executable} {EXAMPLES / 'trace_all.py'}",
            "--pass", "fixpoint(fold-constants)",
            "--emit-bitcode", str(Path(self.work.name) / "inline.rbc"),
            "--verbose",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("pass exec:", result.stderr)
        sys.path.insert(0, str(SDK))
        from rumoca_bitcode import Model

        traced = Model.load(Path(self.work.name) / "inline.rbc")
        self.assertTrue(traced.trace_points, "the external pass's trace points survive the rebuild")

    def test_an_external_pass_that_breaks_the_model_is_named(self):
        source = Path(self.work.name) / "Sampled.mo"
        breaker = Path(self.work.name) / "breaker.py"
        breaker.write_text(
            "import sys\n"
            "from rumoca_bitcode import Model\n"
            "model = Model.load(sys.argv[1])\n"
            "model.raw_model['equations'][0]['residual'] = 10**6\n"
            "model.save(sys.argv[3])\n"
        )
        result = run(
            str(RUMOCA), "compile", str(source), "--model", "Sampled",
            "--pass", f"exec:{sys.executable} {breaker}",
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("breaker.py", result.stderr)


if __name__ == "__main__":
    unittest.main()
