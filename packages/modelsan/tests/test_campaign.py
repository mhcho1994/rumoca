"""Campaign reporting must distinguish a witness from absent execution."""
import json
from pathlib import Path
import tempfile
import unittest

from modelsan.backends.base import ExecutionResult, ExecutionStatus, Trace
from modelsan.campaign import load_campaign, run_case
from modelsan.contracts.trace import Equality
from modelsan.instrumentation.capability import Capability
from modelsan.runtime.anchors import BackendAnchor, CanonicalAnchor, EntityKind
from modelsan.runtime.failures import FailureKind
from modelsan.runtime.observations import ObservationStream, SimulationAbort, SimulationEnd, VariableObservation


class Backend:
    name = 'test-backend'
    capabilities = frozenset({Capability.OBSERVE_VARIABLE, Capability.OBSERVE_FAILURE})

    def __init__(self, result, preparation=None):
        self.result, self.preparation, self.closed = result, preparation, False

    def prepare(self, *args):
        return self.preparation

    def run(self, *args):
        return self.result

    def close(self):
        self.closed = True


def result(values):
    observations = ObservationStream()
    for name, value in values.items():
        observations.add(VariableObservation(time=0, value=value, backend=BackendAnchor('test-backend', name)))
    observations.add(SimulationEnd(completed=True))
    return ExecutionResult('test-backend', ExecutionStatus.SUCCESS, observations=observations,
                           trace=Trace([0], {k:[v] for k,v in values.items()}))


class CampaignTests(unittest.TestCase):
    def setUp(self):
        self.contract = Equality(contract_id='roundtrip', origin='public API', actual='actual', expected='original')

    def test_real_runtime_observer_is_wired_through_pipeline(self):
        for actual, expected_status in [(300, 'no-violation-observed'), (327, 'violated')]:
            backend = Backend(result({'actual':actual, 'original':300}))
            report = run_case(backend, 'unused.mo', 'ArbitraryModel', [self.contract])
            self.assertEqual(report['status'], expected_status)
            self.assertTrue(backend.closed)
            json.dumps(report, allow_nan=False)

    def test_missing_signal_is_coverage_not_clean(self):
        backend = Backend(result({'actual':300}))
        report = run_case(backend, '', 'M', [self.contract])
        self.assertEqual(report['status'], 'unobserved')
        self.assertIn('behavior.contract.roundtrip', report['coverage'])

    def test_canonical_numeric_failure_does_not_require_source_model(self):
        run = result({})
        run.observations.observations.insert(0, VariableObservation(time=0, value=float('inf'),
            canonical=CanonicalAnchor(EntityKind.VARIABLE, 0, 'actual')))
        report = run_case(Backend(run), '', 'M', [])
        self.assertEqual(report['status'], 'violation')
        finding = next(f for f in report['findings'] if f['sanitizer']=='numeric')
        self.assertTrue(finding['canonical_anchors'])
        self.assertFalse(finding['source_locations'])

    def test_build_failure_and_timeout_do_not_get_detection_credit(self):
        error = ExecutionResult.backend_error('test-backend', 'unsupported feature')
        report = run_case(Backend(None, preparation=error), '', 'M', [self.contract])
        self.assertEqual(report['status'], 'blocked')
        timeout = ExecutionResult('test-backend', ExecutionStatus.TIMEOUT)
        report = run_case(Backend(timeout), '', 'M', [self.contract])
        self.assertEqual(report['status'], 'inconclusive')

    def test_preparation_failure_preserves_completed_static_findings(self):
        from modelsan.pipeline import Pipeline
        from modelsan.sanitizers.registry import SanitizerRegistry
        from modelsan.findings.finding import Finding, Severity
        class StaticProbe:
            name = 'probe'
            def analyze(self, model, context):
                return [Finding('probe', 'static-witness', Severity.MEDIUM)]
        registry = SanitizerRegistry()
        registry.register(StaticProbe())
        failure = ExecutionResult.backend_error('test-backend', 'cannot instrument')
        outcome = Pipeline(registry, Backend(None, preparation=failure)).run(None, '', 'M')
        self.assertEqual(outcome.database.bugs[0].findings[0].kind, 'static-witness')
        self.assertIn('execution.run0', outcome.coverage)

    def test_external_abort_is_not_a_high_severity_model_bug(self):
        data = ObservationStream()
        data.add(SimulationAbort(kind=FailureKind.ABORTED, reason='killed by signal'))
        interrupted = ExecutionResult('test-backend', ExecutionStatus.ABORTED, observations=data)
        report = run_case(Backend(interrupted), '', 'M', [self.contract])
        self.assertEqual(report['status'], 'inconclusive')
        self.assertFalse(any(f['severity']=='high' for f in report['findings']))

    def test_manifest_requires_unique_cases_and_valid_contracts(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root/'M.mo').write_text('model M end M;')
            case = {'model':'M', 'contracts':[]}
            manifest = root/'campaign.json'
            for data in [dict(schema=1, source='M.mo', cases=[]),
                         dict(schema=1, source='M.mo', cases=[case,case]),
                         dict(schema=1, source='M.mo', cases=[{'model':'M','contracts':[{'kind':'unknown'}]}])]:
                manifest.write_text(json.dumps(data))
                with self.assertRaises(ValueError):
                    load_campaign(manifest)


if __name__ == '__main__':
    unittest.main()
