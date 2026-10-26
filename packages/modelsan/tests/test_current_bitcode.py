"""Native regressions for current equation and executable RBC consumers."""
from copy import deepcopy
import math
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from rumoca_bitcode import Model
from rumoca_bitcode.execution import lower
from modelsan.backends.rumoca import RumocaBackend
from modelsan.backends.base import ExecutionStatus
from modelsan.fuzz.testcase import NOMINAL, TestCase as Case
from modelsan.network import build
from modelsan.analysis.context import AnalysisContext
from modelsan.sanitizers import NetworkSan, NumericSan, SanitizerRegistry
from modelsan.pipeline import Pipeline
from modelsan.instrumentation.capability import Capability
from modelsan.backends.rumoca_execution import PASS_NAME, instrument
from modelsan.runtime.failures import FailureKind
from modelsan.sanitizers import AssertSan

ROOT = Path(__file__).resolve().parents[3]
RUMOCA = str(ROOT / "target/debug/rumoca")


def decay():
    model = Model.empty("Decay")
    b = model.builder("test.current")
    x = b.add_state("x", 2.0)
    k = b.add_parameter("k", 1.0)
    model.raw_model["variables"][k]["tunable"] = True
    b.add_derivative_equation(x, b.sub(b.real(0), b.mul(b.ref(k), b.ref(x))))
    b.finish()
    return model


def user_logger(program):
    with program.builder("test.user-logging") as b:
        sink = b.declare_csv_sink(
            key="user", filename="user.csv",
            columns=[{"name": "time", "ty": "real"}], metadata={"members": []})
        with b.before_return("run_start") as ir:
            ir.emit("csv.open", sink=sink)
        with b.before_return("publish") as ir:
            ir.emit("csv.write_row", sink=sink, values=[ir.emit("snapshot.time")])
        with b.before_return("run_finish") as ir:
            ir.emit("csv.close", sink=sink)


class CurrentBitcode(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="modelsan-current-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.backend = RumocaBackend(RUMOCA, t_end=0.2, timeout=10)
        self.addCleanup(self.backend.close)

    def test_saved_program_is_run_without_relowering(self):
        program = lower(decay())
        user_logger(program)
        original = deepcopy(program.raw)
        artifact = self.root / "program.rbc"
        program.save(artifact)
        before = artifact.read_bytes()
        self.assertIsNone(self.backend.prepare_from_artifact(artifact))
        prepared = Model.load(self.backend._artifact)._document["execution"]
        # The derived program is not on the wire, so there is nothing to
        # compare there; what must survive is the authored program.
        self.assertNotIn("numerical", prepared)
        self.assertEqual(prepared["passes"][:-1], original["passes"])
        self.assertEqual(prepared["program"]["sinks"][:-1],
                         original["program"]["sinks"])
        for name, function in original["program"]["functions"].items():
            body = function["body"]
            self.assertEqual(
                prepared["program"]["functions"][name]["body"][:len(body)], body)
        result = self.backend.run(NOMINAL)
        self.assertEqual(result.status, ExecutionStatus.SUCCESS, result.failure)
        self.assertAlmostEqual(result.trace.final_state["x"], 2 * math.exp(-0.2),
                               delta=1e-5)
        self.assertEqual(artifact.read_bytes(), before)
        self.assertTrue((Path(result.backend_metadata["trace_root"]) / "user.csv").exists())
        second = self.backend.run(NOMINAL)
        self.assertTrue(second.ok, second.failure)
        self.assertNotEqual(second.backend_metadata["trace_root"], result.backend_metadata["trace_root"])
        self.assertEqual(second.trace.columns, result.trace.columns)

    def test_equation_artifacts_and_parameter_fuzzing_still_work(self):
        path = self.root / "equations.rbc"
        decay().save(path)
        self.assertIsNone(self.backend.prepare_from_artifact(path))
        result = self.backend.run(Case(parameters={"k": 2.0}))
        self.assertTrue(result.ok, result.failure)
        self.assertAlmostEqual(result.trace.final_state["x"], 2 * math.exp(-0.4), delta=1e-5)

    def test_integer_binding_keeps_tunable_array_dependency(self):
        import subprocess
        source = self.root / 'Dependent.mo'
        source.write_text('model Dependent parameter Real a[2]={1,2}; '
            'parameter Integer k=if a[2]>0 then 3 else 1; Real y=k+time; end Dependent;')
        artifact = self.root / 'dependent.rbc'
        subprocess.run([RUMOCA, 'compile', str(source), '--model', 'Dependent',
            '--no-fold-parameter-bindings', '--pass', 'none', '--emit-bitcode', str(artifact)],
            check=True, capture_output=True, timeout=20)
        self.assertIsNone(self.backend.prepare_from_artifact(artifact))
        for value, expected in [(2.0,3.2), (-1.0,1.2), (2.0,3.2)]:
            result = self.backend.run(Case(parameters={'a[2]': value}))
            self.assertTrue(result.ok, result.failure)
            self.assertAlmostEqual(result.trace.final_state['y'], expected)

    def test_saved_frozen_artifact_rejects_overrides_without_backend_profile(self):
        import subprocess
        source = self.root / 'Frozen.mo'
        source.write_text('function iterate input Real seed; output Real y; '
            'algorithm y:=seed; while y<3 loop y:=y+1; end while; end iterate; '
            'model Frozen parameter Real seed=1; Real actual=iterate(seed); end Frozen;')
        artifact = self.root / 'frozen.rbc'
        subprocess.run([RUMOCA, 'compile', str(source), '--model', 'Frozen',
            '--freeze-parameters', '--pass', 'none', '--emit-bitcode', str(artifact)],
            check=True, capture_output=True, timeout=20)
        seed = next(v for v in Model.load(artifact).raw_model['variables'] if v['name']=='seed')
        self.assertFalse(seed['tunable'])
        self.assertIsNone(self.backend.prepare_from_artifact(artifact))
        result = self.backend.run(NOMINAL)
        self.assertTrue(result.ok, result.failure)
        self.assertEqual(result.trace.final_state['actual'], 3.0)
        result = self.backend.run(Case(parameters={'seed': 4.0}))
        self.assertEqual(result.status, ExecutionStatus.BACKEND_ERROR)
        self.assertIn('not tunable', result.failure.raw)

    def test_fixed_parameter_profile_rejects_overrides_before_launch(self):
        path = self.root / 'equations.rbc'
        decay().save(path)
        self.backend.freeze_parameters = True
        self.assertIsNone(self.backend.prepare_from_artifact(path))
        with patch('modelsan.backends.rumoca.execute') as invoke:
            result = self.backend.run(Case(parameters={'k': 2.0}))
        self.assertEqual(result.status, ExecutionStatus.BACKEND_ERROR)
        self.assertIn('recompilation', result.failure.raw)
        invoke.assert_not_called()

    def test_source_compile_keeps_the_temporary_artifact_alive(self):
        source = ROOT / "new_inst/link_fixtures/Decay.mo"
        self.assertIsNone(self.backend.prepare(str(source), "LinkDecay"))
        self.assertTrue(self.backend.run(NOMINAL).ok)

    def test_unsupported_executable_overrides_are_backend_errors(self):
        path = self.root / "program.rbc"
        lower(decay()).save(path)
        self.assertIsNone(self.backend.prepare_from_artifact(path))
        for case in [Case(parameters={"missing": 2}), Case(initial_values={"k": 3}),
                     Case(solver_options={"rtol": 0.1}), Case(input_trajectory=[(0, 1)])]:
            result = self.backend.run(case)
            self.assertEqual(result.status, ExecutionStatus.BACKEND_ERROR)
            self.assertFalse(result.informative)

    def test_stale_missing_or_unsupported_program_is_not_a_model_failure(self):
        for corruption in ("stale", "unknown-trace-point", "unsupported"):
            with self.subTest(corruption=corruption):
                program = lower(decay())
                user_logger(program)
                if corruption == "stale":
                    observed = program.model.variable("x")
                    point = program.model.add_trace_point(observed, label="x")
                    program.function("publish").append(
                        {"op": "snapshot.value", "result": "zz",
                         "trace_point": point.id})
                    program.declare("publish", "zz", "real")
                    program.refresh_digest()
                    program.model.raw_model["variables"][observed.id][
                        "causality"] = "output"
                elif corruption == "unknown-trace-point":
                    program.function("publish").append(
                        {"op": "snapshot.value", "result": "zz",
                         "trace_point": 999999})
                    program.declare("publish", "zz", "real")
                else:
                    program.function("publish").append({"op": "no-such-operation"})
                path = self.root / f"{corruption}.rbc"
                Model(program._document()).save(path)
                prepared = self.backend.prepare_from_artifact(path)
                self.assertIsNotNone(prepared)
                self.assertEqual(prepared.status, ExecutionStatus.BACKEND_ERROR)
                self.assertEqual(self.backend.run(NOMINAL).status, ExecutionStatus.BACKEND_ERROR)

    def test_selected_compiler_not_environment_is_used(self):
        path = self.root / "program.rbc"
        lower(decay()).save(path)
        with patch.dict("os.environ", {"RUMOCA": "/nonexistent/compiler"}):
            self.assertIsNone(self.backend.prepare_from_artifact(path))
            self.assertTrue(self.backend.run(NOMINAL).ok)

    def test_native_assertion_failure_is_still_a_legitimate_runtime_failure(self):
        program = lower(decay())
        with program.builder("test.assertion") as b:
            arena = program.expressions("publish")
            with b.before_return("publish") as ir:
                ok = ir.emit("compute", ty="boolean", expr=arena.compare(
                    "Gt", arena.real(0.0), arena.real(1.0)))
                message = ir.emit("compute", ty="text",
                                  expr=arena.text("intentional runtime failure"))
                ir.emit("assert", condition=ok, message=message)
        path = self.root / "assert.rbc"
        program.save(path)
        self.assertIsNone(self.backend.prepare_from_artifact(path))
        result = self.backend.run(NOMINAL)
        self.assertEqual(result.status, ExecutionStatus.FAILED, result.failure)
        self.assertIn("intentional runtime failure", result.failure.raw)
        self.assertTrue(result.informative)
        self.assertEqual(result.failure.kind, FailureKind.ASSERTION_VIOLATED)
        self.assertEqual(len(AssertSan().observe(result.observations, program.model,
                                               AnalysisContext(program.model), NOMINAL)), 1)

    def test_pipeline_accepts_executable_path_and_plans_after_prepare(self):
        path = self.root / "program.rbc"
        model = decay()
        lower(model).save(path)
        registry = SanitizerRegistry()
        registry.register(NumericSan())
        outcome = Pipeline(registry, self.backend).run(model, str(path), model.name)
        self.assertTrue(outcome.baseline.ok, outcome.baseline.failure)
        self.assertIn(Capability.CANONICAL_MODEL, outcome.plan.available)

    def test_execution_observation_pass_is_replayable(self):
        program = lower(decay())
        instrument(program, program.model, variable_ids=[0])
        fresh = program.relower(replay={PASS_NAME: instrument})
        self.assertEqual(fresh.raw["program"]["sinks"],
                         program.raw["program"]["sinks"])

    def test_no_silent_missing_trace_or_stale_run(self):
        path = self.root / "program.rbc"
        lower(decay()).save(path)
        self.assertIsNone(self.backend.prepare_from_artifact(path))
        with patch("modelsan.backends.rumoca.execute") as run:
            run.return_value.returncode = 0
            run.return_value.stdout = ""
            run.return_value.stderr = ""
            self.assertEqual(self.backend.run(NOMINAL).status, ExecutionStatus.BACKEND_ERROR)
        # Change an identity the prepared program references. The v2 digest
        # covers those, not the whole model, so renaming the model would
        # correctly leave the program runnable.
        artifact = Model.load(self.backend._artifact)
        observed = artifact.raw_model["trace_points"][0]["variable"]
        artifact.raw_model["variables"][observed]["causality"] = "output"
        artifact.save(self.backend._artifact)
        result = self.backend.run(NOMINAL)
        self.assertEqual(result.status, ExecutionStatus.BACKEND_ERROR)
        self.assertIn("stale execution", result.failure.raw)

    def test_legacy_graphs_remain_inspectable_and_declarations_never_fallback(self):
        from types import SimpleNamespace
        from modelsan.network.graph import Node
        legacy = SimpleNamespace(components=[], variables=[], connection_sets=[Node(
            id=0, connectors=("a.p", "b.p"))])
        net = build(legacy)
        self.assertEqual(net.identity_source, "legacy-paths")
        self.assertEqual(net.nodes[0].components, ("a", "b"))
        self.assertEqual(Node(id=1, connectors=("port", "a.p")).boundary, ("port",))
        legacy.connectors = [SimpleNamespace(path="a.p", owner=99)]
        with self.assertRaisesRegex(ValueError, "owner"):
            build(legacy)

    def test_linked_connector_observations_reach_network_sanitizer(self):
        def body(initial, contact=False):
            model = Model.empty("Body")
            b = model.builder("test.body")
            owner = b.add_component("body")
            ty = b.add_connector_type("HeatPort", members=[
                {"name": "T", "scalar_type": "real", "kind": "potential",
                 "unit": "K", "quantity": "ThermodynamicTemperature"},
                {"name": "Q", "scalar_type": "real", "kind": "flow",
                 "unit": "W", "quantity": "HeatFlowRate"},
            ])
            t = b.add_state("T", initial, owner=owner)
            port = b.add_connector("port", owner=owner, type_id=ty)
            pt, q = b.member(port, "T"), b.member(port, "Q")
            b.add_derivative_equation(t, b.ref(q))
            law = b.sub(b.ref(pt), b.ref(t))
            b.add_equation(b.sub(b.ref(q), law) if contact else law)
            b.finish()
            return model
        model = Model.link({"hot": body(350, True), "cold": body(300)})
        b = model.builder("wire")
        b.add_connection_set([0, 1])
        b.finish()
        path = self.root / "linked.rbc"
        lower(model).save(path)
        self.assertIsNone(self.backend.prepare_from_artifact(path))
        self.assertIn(Capability.OBSERVE_CONNECTOR, self.backend.capabilities)
        sanitizer = NetworkSan()
        context = AnalysisContext(model)
        result = self.backend.run(NOMINAL, sanitizer.requests(model, context))
        self.assertTrue(result.ok, result.failure)
        self.assertEqual(len(result.trace.columns), 6)
        self.assertEqual(sanitizer.observe(result.observations, model, context, NOMINAL), [])
        # Ensure the new transport does not suppress a real conservation error.
        q = b.variable("hot.body.port.Q")
        for observation in result.observations.observations:
            if getattr(observation, "canonical", None) and observation.canonical.dae_id == q:
                observation.value += 100
        findings = sanitizer.observe(result.observations, model, context, NOMINAL)
        self.assertIn("network-conservation-violated", [f.kind for f in findings])
        registry = SanitizerRegistry()
        registry.register(sanitizer)
        pipeline = Pipeline(registry, self.backend)
        outcome = pipeline.run(model, str(path), model.name)
        self.assertTrue(outcome.baseline.ok, outcome.baseline.failure)
        self.assertTrue(outcome.plan.can_run("network"))
        self.assertTrue(outcome.plan.satisfied)

    def test_declared_ports_use_ids_not_variable_name_prefixes(self):
        model = Model.empty("Ports")
        b = model.builder("test.ids")
        ty = b.add_connector_type("Port", members=[
            {"name": "v", "scalar_type": "real", "kind": "potential"},
            {"name": "i", "scalar_type": "real", "kind": "flow"},
        ])
        owners = [b.add_component(name) for name in ("a", "b")]
        ports = [b.add_connector("nested.port", owner=owner, type_id=ty) for owner in owners]
        b.add_connection_set(ports)
        for v in model.raw_model["variables"]:
            v["name"] = f"opaque_{v['id']}"
        model.refresh()
        model.validate(connections=True)
        net = build(model)
        self.assertEqual(net.nodes[0].components, ("a", "b"))
        self.assertEqual(net.neighbours("a"), ["b"])
        self.assertEqual([len(p.potentials) for p in net.ports], [1, 1])
        self.assertEqual([len(p.flows) for p in net.ports], [1, 1])
        self.assertEqual([p.owner_id for p in net.ports], owners)
        self.assertEqual([p.connector_id for p in net.ports], ports)
        self.assertEqual(net.identity_source, "declared-ids")
        linked = Model.link({"left": model, "right": model})
        net = build(linked)
        self.assertEqual(net.neighbours("right.a"), ["right.b"])
        self.assertEqual([len(p.flows) for p in net.ports], [1] * 4)

    def test_open_ports_are_not_zero_flow_singletons(self):
        model = Model.empty("Open")
        b = model.builder("open")
        ty = b.add_connector_type("Port", members=[
            {"name": "q", "scalar_type": "real", "kind": "flow"}])
        owner = b.add_component("owner")
        b.add_connector("nested.port", owner=owner, type_id=ty, orientation="inside")
        b.finish()
        net = build(model)
        self.assertFalse(net.absent)
        self.assertEqual(net.nodes, [])
        self.assertIsNone(net.ports[0].node)
        self.assertEqual(net.ports[0].flows[0][1], -1)
        self.assertEqual(NetworkSan().analyze(model, AnalysisContext(model)), [])
        b.add_connection_set([0])
        net = build(model)
        self.assertTrue(net.nodes[0].unconnected)
        findings = NetworkSan().analyze(model, AnalysisContext(model))
        finding = next(f for f in findings if f.kind == "network-port-unconnected")
        self.assertEqual(finding.evidence["component"], "owner")

    def test_multifield_laws_cannot_cancel_each_others_violation(self):
        from modelsan.runtime.anchors import CanonicalAnchor, EntityKind
        from modelsan.runtime.observations import ObservationStream, VariableObservation
        model = Model.empty("Multifield")
        b = model.builder("multifield")
        ty = b.add_connector_type("Port", members=[
            {"name": "q1", "scalar_type": "real", "kind": "flow"},
            {"name": "q2", "scalar_type": "real", "kind": "flow"},
        ])
        ports = [b.add_connector("p", owner=b.add_component(name), type_id=ty)
                 for name in ("a", "b")]
        b.add_connection_set(ports)
        b.finish()
        stream = ObservationStream()
        for variable, value in zip(model.variables, (1, -1, 0, 0)):
            stream.add(VariableObservation(time=0, value=value,
                canonical=CanonicalAnchor(EntityKind.VARIABLE, variable.id, variable.name)))
        san = NetworkSan()
        findings = san.observe(stream, model, AnalysisContext(model), NOMINAL)
        self.assertEqual([f.kind for f in findings], ["network-conservation-violated"] * 2)
        stream.observations.pop()
        findings = san.observe(stream, model, AnalysisContext(model), NOMINAL)
        self.assertEqual([f.kind for f in findings], ["network-observation-incomplete"])

    def test_csv_nan_survives_transport_and_bad_coordinates_are_rejected(self):
        from modelsan.backends.rumoca_execution import read_trace
        target = decay().states[0]
        path = self.root / "observations.csv"
        path.write_text("time_s,publish_id,phase,v0\n0,0,initial,nan\n")
        times, values = read_trace(path, [target])
        self.assertEqual(times, [0])
        self.assertTrue(math.isnan(values["x"][0]))
        stream = self.backend._stream(times, values)
        self.assertTrue(NumericSan().observe(stream, decay(), AnalysisContext(decay()), NOMINAL))
        for row in ["0,7,initial,1", "nan,0,initial,1", "0,0,initial", "0,0,initial,1,extra"]:
            path.write_text(f"time_s,publish_id,phase,v0\n{row}\n")
            with self.assertRaises(ValueError):
                read_trace(path, [target])

    def test_unknown_requests_and_missing_tool_cannot_be_clean_runs(self):
        from modelsan.instrumentation.request import InstrumentationRequest
        from modelsan.runtime.anchors import CanonicalAnchor, EntityKind
        path = self.root / "program.rbc"
        lower(decay()).save(path)
        self.assertIsNone(self.backend.prepare_from_artifact(path))
        for request in [InstrumentationRequest(Capability.OBSERVE_EXPRESSION),
                        InstrumentationRequest(Capability.OBSERVE_VARIABLE,
                            CanonicalAnchor(EntityKind.VARIABLE, 999)),
                        InstrumentationRequest(Capability.OBSERVE_VARIABLE,
                            CanonicalAnchor(EntityKind.EQUATION, 0))]:
            result = self.backend.run(NOMINAL, [request])
            self.assertEqual(result.status, ExecutionStatus.BACKEND_ERROR)
        self.backend.executable = "/nonexistent/compiler"
        self.assertEqual(self.backend.prepare_from_artifact(path).status, ExecutionStatus.BACKEND_ERROR)
        self.assertEqual(self.backend.run(NOMINAL).status, ExecutionStatus.BACKEND_ERROR)

    def test_existing_sink_collision_is_refused_without_overwriting(self):
        from modelsan.backends.rumoca_execution import FILENAME
        program = lower(decay())
        user_logger(program)
        program.raw["program"]["sinks"][0]["filename"] = FILENAME
        path = self.root / "collision.rbc"
        program.save(path)
        before = path.read_bytes()
        result = self.backend.prepare_from_artifact(path)
        self.assertEqual(result.status, ExecutionStatus.BACKEND_ERROR)
        self.assertIn("duplicate CSV sink", result.failure.raw)
        self.assertEqual(path.read_bytes(), before)

    def test_unsynchronized_network_evidence_is_not_index_aligned(self):
        from modelsan.network.observations import Samples
        from modelsan.runtime.anchors import CanonicalAnchor, EntityKind
        from modelsan.runtime.observations import ObservationStream, VariableObservation
        stream = ObservationStream()
        stream.add(VariableObservation(time=0, value=1,
            canonical=CanonicalAnchor(EntityKind.VARIABLE, 0)))
        stream.add(VariableObservation(time=1, value=-1,
            canonical=CanonicalAnchor(EntityKind.VARIABLE, 1)))
        samples = Samples(stream, [0, 1])
        self.assertIn("unsynchronized", samples.reason)
        self.assertIsNone(samples.series(0))
        stream.observations[-1].time = 0
        self.assertFalse(Samples(stream, [0, 1]).reason)
        stream.add(stream.observations[-1])
        self.assertIn("duplicate", Samples(stream, [0, 1]).reason)


if __name__ == "__main__":
    unittest.main()
