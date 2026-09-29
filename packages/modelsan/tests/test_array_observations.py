"""Array variables are observed per element and checked against their bounds.

The solver publishes `w[1]`, `w[2]`, ... while a trace point and a declared
bound name `w`. Matching by the variable's name alone published nothing, made
the backend refuse the run ("missing requested columns"), and left every
array bound unchecked.
"""
from pathlib import Path
import subprocess
import tempfile
import unittest

from rumoca_bitcode import Model
from modelsan.backends.rumoca import RumocaBackend, variable_of
from modelsan.pipeline import Pipeline
from modelsan.sanitizers.range import RangeSan
from modelsan.sanitizers.registry import SanitizerRegistry

ROOT = Path(__file__).resolve().parents[3]
RUMOCA = ROOT / "target/debug/rumoca"

SOURCE = """
model ArrayBounds
  Real x[3](each start = 1, each fixed = true);
  Real y[3](each max = 2.9);
  Real w[3](max = {5, 2.5, 5});
equation
  der(x) = -x;
  y = 2 .+ x;
  w = {2, 2, 2} + x;
end ArrayBounds;
"""


class ArrayObservations(unittest.TestCase):
    def test_an_element_column_belongs_to_its_variable(self):
        self.assertEqual(variable_of("w[2]"), "w")
        self.assertEqual(variable_of("w"), "w")
        self.assertEqual(variable_of("a.b[1,2]"), "a.b")
        # A variable inside an array of components keeps its own subscript.
        self.assertEqual(variable_of("a.TF1[1].x[2]"), "a.TF1[1].x")

    @unittest.skipUnless(RUMOCA.exists(), "needs a built rumoca")
    def test_each_element_is_checked_against_its_bound(self):
        with tempfile.TemporaryDirectory() as work:
            source = Path(work) / "ArrayBounds.mo"
            source.write_text(SOURCE)
            artifact = Path(work) / "array.rbc"
            subprocess.run([str(RUMOCA), "compile", str(source), "--emit-bitcode", str(artifact)],
                           check=True, capture_output=True)
            registry = SanitizerRegistry()
            registry.register(RangeSan())
            backend = RumocaBackend(str(RUMOCA), t_end=0.1, timeout=60, source_roots=(), dt=0.01)
            try:
                outcome = Pipeline(registry, backend).run(Model.load(artifact), str(artifact), "A")
            finally:
                backend.close()
            broken = sorted((f.evidence["variable"], f.evidence["limit"])
                            for bug in outcome.database.bugs for f in bug.findings)
            self.assertEqual(broken, [("w[2]", 2.5), ("y[1]", 2.9), ("y[2]", 2.9), ("y[3]", 2.9)])
