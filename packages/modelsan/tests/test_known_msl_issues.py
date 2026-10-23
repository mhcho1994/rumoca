"""Opt-in end-to-end known-issue regression targets, including passing controls.

MODELSAN_MSL_TESTS=1 enables actual compiler/runtime executions. Both backend
results remain separate; this is not a corpus recall score or a parity test.
"""
import os
from pathlib import Path
import shutil
import tempfile
import unittest

from modelsan.backends.openmodelica import OpenModelicaBackend
from modelsan.backends.rumoca import RumocaBackend
from modelsan.backends.rumoca_source import RumocaSourceBackend
from modelsan.campaign import load_campaign, run_case

ROOT = Path(__file__).resolve().parents[3]
MSL = ROOT/'target/msl/ModelicaStandardLibrary-4.1.0'
RUMOCA = Path(os.environ.get('RUMOCA', ROOT/'target/debug/rumoca'))
CAMPAIGN = ROOT/'examples/modelsan/known-msl-issues.json'
ENABLED = os.environ.get('MODELSAN_MSL_TESTS') == '1'


@unittest.skipUnless(ENABLED, 'set MODELSAN_MSL_TESTS=1 to run actual-library gates')
class KnownIssueDetection(unittest.TestCase):
    def run_selected(self, backend_factory, names):
        source, cases = load_campaign(CAMPAIGN)
        return {name.split('.')[-1]: run_case(backend_factory(), source, name, contracts)
                for name, contracts in cases if name.split('.')[-1] in names}

    @classmethod
    def setUpClass(cls):
        if not (MSL/'Modelica 4.1.0/package.mo').is_file():
            raise RuntimeError('known-issue gate requires cached MSL 4.1.0')

    def assert_detection_matrix(self, reports):
        self.assertEqual(set(reports), {'GasControl','GasRoundTrip','WaterRoundTrip',
            'PulseControl','PulsePast','Delay','DelayControl','Quantization',
            'QuantizationControl','MoistAirFull','MoistAirReduced'})
        for name in ['PulsePast','Delay','Quantization','GasRoundTrip','WaterRoundTrip']:
            self.assertEqual(reports[name]['status'], 'violated', reports[name])
            self.assertTrue(any(f['sanitizer']=='behavior' and f['kind']=='contract-violation'
                                for f in reports[name]['findings']))
        for name in ['GasRoundTrip','WaterRoundTrip']:
            witness = next(f for f in reports[name]['findings'] if f['sanitizer']=='behavior')
            self.assertAlmostEqual(witness['evidence']['expected'], 300)
        quantizer = next(f for f in reports['Quantization']['findings'] if f['sanitizer']=='behavior')
        self.assertEqual(quantizer['evidence']['observed_count_lower_bound'], 5)
        for name in ['PulseControl','DelayControl','QuantizationControl','GasControl','MoistAirFull']:
            self.assertEqual(reports[name]['status'], 'no-violation-observed', reports[name])
        full_trace = reports['MoistAirFull']['executions'][0]['trace']
        self.assertIsNotNone(full_trace)
        for value in full_trace['columns']['actual']:
            self.assertAlmostEqual(value, 300, delta=1e-6)
        reduced = reports['MoistAirReduced']
        expected = 'execution-failed' if reduced['backend']=='openmodelica' else 'model-rejected'
        self.assertEqual(reduced['status'], expected, reduced)
        failure = reduced['executions'][0]['failure']['raw'].lower()
        self.assertIn('index', failure)
        self.assertTrue(any(word in failure for word in ['dim_size','bound','length','size']))
        self.assertTrue(any(f['sanitizer'] in ('solver','domain') and f['severity']=='high'
                            for f in reduced['findings']))
        if reduced['backend'] != 'openmodelica':
            proof = next(f for f in reduced['findings'] if f['kind']=='compile-time-array-bounds')
            self.assertEqual(proof['evidence']['compiler_diagnostic']['code'], 'EF032')
            self.assertTrue(proof['source_locations'])
        for report in reports.values():
            self.assertFalse(any(f['kind']=='contract-unobserved' for f in report['findings']))

    def all_cases(self, factory):
        _, cases = load_campaign(CAMPAIGN)
        return self.run_selected(factory, {name.split('.')[-1] for name, _ in cases})

    def test_native_known_issue_detections_and_controls(self):
        self.assertTrue(RUMOCA.is_file(), 'requires native Rumoca binary')
        with tempfile.TemporaryDirectory() as cache:
            reports = self.all_cases(lambda: RumocaSourceBackend(str(RUMOCA), source_roots=[MSL],
                cache_dir=cache, t_end=1, dt=.0025, timeout=90, freeze_parameters=True))
        self.assert_detection_matrix(reports)

    def test_native_artifact_known_issue_detections_and_controls(self):
        self.assertTrue(RUMOCA.is_file(), 'requires native Rumoca binary')
        with tempfile.TemporaryDirectory() as cache:
            reports = self.all_cases(lambda: RumocaBackend(str(RUMOCA), source_roots=[MSL],
                cache_dir=cache, t_end=1, dt=.0025, timeout=90, freeze_parameters=True))
        self.assert_detection_matrix(reports)
        for name in ['PulsePast','Delay','Quantization','GasRoundTrip','WaterRoundTrip']:
            finding = next(f for f in reports[name]['findings'] if f['sanitizer']=='behavior')
            self.assertTrue(finding['canonical_anchors'])

    def test_omc_known_issue_detections_and_controls(self):
        self.assertIsNotNone(shutil.which('omc'), 'requires OpenModelica')
        libraries = [str(MSL/'ModelicaServices 4.1.0/package.mo'), str(MSL/'Complex.mo'),
                     str(MSL/'Modelica 4.1.0/package.mo')]
        reports = self.all_cases(lambda: OpenModelicaBackend(libraries, t_end=1, timeout=90,
                    number_of_intervals=400))
        self.assert_detection_matrix(reports)


if __name__ == '__main__':
    unittest.main()
