"""Domain evidence must come from reached native operations, not guessed zeros."""
from pathlib import Path
import tempfile
import unittest

from rumoca_bitcode.execution import lower
from modelsan.analysis.context import AnalysisContext
from modelsan.backends.rumoca import RumocaBackend
from modelsan.findings.finding import Severity
from modelsan.fuzz.testcase import NOMINAL, TestCase as Case
from modelsan.runtime.observations import ExpressionObservation
from modelsan.sanitizers.domain import DomainSan
from test_current_bitcode import decay, RUMOCA


class NativeDomain(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.path = Path(self.tmp.name) / "program.rbc"
        self.model = decay()
        self.backend = RumocaBackend(RUMOCA, t_end=.2)
        self.addCleanup(self.backend.close)

    def run_program(self, rhs, case=NOMINAL):
        """Build `der(x) = rhs(b, x, k)` and run it through the native backend.

        The derivative is expressed as an *equation*: execution IR v2 derives
        the numerical program at load, so there is no serialized register
        program to edit. The native domain diagnostics are the same either way
        — that is the point of the test.
        """
        from rumoca_bitcode import Model
        self.model = Model.empty("Domain")
        b = self.model.builder("test")
        x, k = b.add_state("x", 2.), b.add_parameter("k", 1.)
        self.model.raw_model["variables"][k]["tunable"] = True
        b.add_derivative_equation(x, rhs(b, x, k))
        b.finish()
        program = lower(self.model)
        program.save(self.path)
        original = self.path.read_bytes()
        self.assertIsNone(self.backend.prepare_from_artifact(self.path))
        result = self.backend.run(case)
        self.assertEqual(original, self.path.read_bytes())
        findings = DomainSan().observe(result.observations, self.model,
                                       AnalysisContext(self.model), case)
        return result, findings

    @staticmethod
    def reciprocal(b, k):
        return b.divide(b.real(1.), b.ref(k))

    def test_zero_denominator_is_recorded_before_first_physical_sample(self):
        result, findings = self.run_program(
            lambda b, x, k: self.reciprocal(b, k), Case(parameters={"k": 0.}))
        self.assertFalse(result.ok)
        self.assertIsNone(result.trace)
        self.assertEqual([f.kind for f in findings], ["division-out-of-domain"])
        self.assertEqual(findings[0].evidence["operand_value"], 0.)
        self.assertEqual(findings[0].canonical_anchors, [])
        self.assertTrue(findings[0].backend_anchors)
        self.assertEqual(findings[0].evidence["coordinates"], "internal-evaluation")

    def test_zero_on_an_inactive_branch_is_not_a_violation(self):
        def guarded(b, x, k):
            live = b.binary("not_equal", b.ref(k), b.real(0.), value_type=b.scalar_type("boolean"))
            return b.conditional([(live, self.reciprocal(b, k))], b.real(0.))
        result, findings = self.run_program(guarded, Case(parameters={"k": 0.}))
        self.assertTrue(result.ok, result.failure)
        self.assertEqual(findings, [])
        self.assertEqual(result.trace.final_state["x"], 2.)
        self.assertEqual(result.backend_metadata["domain_diagnostics"]["faults"], [])

    def test_nonzero_denominator_is_not_a_violation(self):
        result, findings = self.run_program(lambda b, x, k: self.reciprocal(b, k))
        self.assertTrue(result.ok, result.failure)
        self.assertAlmostEqual(result.trace.final_state["x"], 2.2)
        self.assertEqual(findings, [])

    def test_recovered_internal_evaluation_is_informational(self):
        def recovered(b, x, k):
            quotient = self.reciprocal(b, k)
            big = b.binary("greater", quotient, b.real(1.), value_type=b.scalar_type("boolean"))
            return b.conditional([(big, b.real(1.))], b.ref(k))
        result, findings = self.run_program(recovered, Case(parameters={"k": 0.}))
        self.assertTrue(result.ok, result.failure)
        self.assertEqual([f.kind for f in findings], ["division-domain-trial"])
        self.assertEqual(findings[0].severity, Severity.INFO)

    def test_scope_is_reset_between_failed_and_successful_runs(self):
        result, _ = self.run_program(
            lambda b, x, k: self.reciprocal(b, k), Case(parameters={"k": 0.}))
        self.assertFalse(result.ok)
        result = self.backend.run(NOMINAL)
        self.assertTrue(result.ok, result.failure)
        self.assertEqual(list(result.observations.of(ExpressionObservation)), [])
        self.assertEqual(result.backend_metadata["domain_diagnostics"]["faults"], [])

    def test_sqrt_negative_argument_is_reported(self):
        result, findings = self.run_program(
            lambda b, x, k: b.builtin("sqrt", [b.real(-1.)]))
        self.assertFalse(result.ok)
        self.assertEqual([f.kind for f in findings], ["sqrt-out-of-domain"])

    def test_equation_input_uses_the_same_native_diagnostics(self):
        from rumoca_bitcode import Model
        model = Model.empty("Reciprocal")
        b = model.builder("test")
        x, p = b.add_state("x", 2.), b.add_parameter("p", 1.)
        model.raw_model["variables"][p]["tunable"] = True
        b.add_derivative_equation(x, b.div(b.real(1.), b.ref(p)))
        b.finish()
        model.save(self.path)
        self.assertIsNone(self.backend.prepare_from_artifact(self.path))
        result = self.backend.run(Case(parameters={"p": 0.}))
        self.assertFalse(result.ok)
        findings = DomainSan().observe(result.observations, model, AnalysisContext(model), NOMINAL)
        self.assertEqual([f.kind for f in findings], ["division-out-of-domain"])

    def test_diagnostics_preserve_ordinary_native_trajectory(self):
        import json
        from rumoca_bitcode.compiler import invoke
        program = lower(decay())
        program.save(self.path)
        ordinary = Path(self.tmp.name) / "ordinary.json"
        invoke("bitcode", "run", self.path, "--stop", ".2",
               "--trace-root", Path(self.tmp.name) / "ordinary-traces", "--result", ordinary,
               executable=RUMOCA)
        self.assertIsNone(self.backend.prepare_from_artifact(self.path))
        observed = self.backend.run(NOMINAL)
        self.assertTrue(observed.ok, observed.failure)
        payload = json.loads(ordinary.read_text())
        self.assertEqual(observed.trace.times, payload["times"])
        for actual, expected in zip(observed.trace.columns["x"], payload["data"][0]):
            self.assertAlmostEqual(actual, expected, places=12)
