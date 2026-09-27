"""Native, fresh-process acceptance tests (requires built rumoca via RUMOCA).

Run: PYTHONPATH=packages/rumoca-bitcode:new_inst RUMOCA=target/debug/rumoca
     python3 -m unittest discover -s new_inst -p test_execution.py -v
"""
from copy import deepcopy
import csv
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from rumoca_bitcode import Model
from rumoca_bitcode.compiler import compiler, invoke
from rumoca_bitcode.execution import Program, lower, observe_connector_members
from synthesize_connector_csv import synthesize, scale_conductor_equation, instrument_all_connectors
from check_thermal_csv import verify


#: A sink that is not connector instrumentation names no connector. `members`
#: is always present: it is the same trace-point identity a `snapshot.value`
#: names, so a sink's references are visible without reading its instructions.
EMPTY_METADATA = {"members": []}


class ExecutionAcceptance(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        model, handles = synthesize()
        cls.base = deepcopy(model._document)
        cls.handles = handles

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="rbc-execution-test-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.model = Model(deepcopy(self.base))

    def program(self, scale=2):
        scale_conductor_equation(self.model, self.handles, scale)
        # No observation list: lowering records a profile, and the program
        # says what it needs (D2).
        return lower(self.model)

    def run_program(self, p, label="run"):
        artifact = self.root / f"{label}.rbc"
        p.save(artifact)
        # Only a native CLI process reads the artifact; no Python pass module.
        result = self.root / f"{label}.json"
        invoke("bitcode", "run", artifact, "--execution", "require", "--trace-root", self.root / label, "--result", result)
        return json.loads(result.read_text())

    def test_scales_fresh_process_and_neutrality(self):
        for scale in (1, 2):
            with self.subTest(scale=scale):
                p = self.program(scale)
                baseline = self.run_program(p, f"baseline-{scale}")
                instrument_all_connectors(p, self.model)
                logged = self.run_program(p, f"logged-{scale}")
                self.assertEqual(baseline, logged)
                self.assertEqual(verify(self.root / f"logged-{scale}", scale)["total_rows"], 204)

    def test_unreferenced_equation_edit_is_not_stale(self):
        """D3. The digest covers what the program references, so rewriting a
        residual the program does not observe leaves it runnable."""
        p = self.program(1)
        instrument_all_connectors(p, self.model)
        before = p.raw["dependency_digest"]
        scale_conductor_equation(self.model, self.handles, 2)
        p.validate()
        self.assertEqual(p.raw["dependency_digest"], before)
        self.run_program(p)
        verify(self.root / "run", 2)

    def test_referenced_identity_change_is_stale_and_names_the_point(self):
        """D3. Retyping a variable a trace point names invalidates the
        program, and the diagnostic says which point."""
        p = self.program(1)
        instrument_all_connectors(p, self.model)
        observed = self.model.raw_model["trace_points"][0]["variable"]
        # Causality, not unit: a connector member's unit is also declared by
        # its connector type, so changing it would fail a different check.
        self.model.raw_model["variables"][observed]["causality"] = "output"
        with self.assertRaisesRegex(ValueError, "stale execution"):
            p.validate()
        with self.assertRaisesRegex(ValueError, "no compatible replay"):
            p.relower()
        fresh = p.relower(replay={"example.connector-csv": instrument_all_connectors})
        self.run_program(fresh)

    def test_raw_mutation_stale_at_native_boundary(self):
        p = self.program()
        instrument_all_connectors(p, self.model)
        artifact = self.root / "raw.json"
        p.save(artifact)
        document = Model.load(artifact)
        observed = document.raw_model["trace_points"][0]["variable"]
        document.raw_model["variables"][observed]["causality"] = "output"
        document.save(artifact)
        with self.assertRaisesRegex(ValueError, "stale execution"):
            invoke("bitcode", "check-execution", artifact)
        with self.assertRaisesRegex(ValueError, "stale execution"):
            invoke("bitcode", "run", artifact, "--trace-root", self.root / "forbidden")
        self.assertFalse((self.root / "forbidden").exists())

    def test_duplicate_pass_rejected(self):
        p = self.program()
        instrument_all_connectors(p, self.model)
        with self.assertRaisesRegex(ValueError, "duplicate execution pass"):
            instrument_all_connectors(p, self.model)

    def test_no_default_observation_and_helper_is_the_producer(self):
        """D2. Lowering observes nothing; the logging helper creates the
        trace points, and an empty program is a valid zero-observation
        artifact rather than an error."""
        p = lower(self.model)
        self.assertEqual(p.referenced_trace_points(), set())
        self.assertEqual(self.model.raw_model.get("trace_points", []), [])
        p.validate()
        instrument_all_connectors(p, self.model)
        points = self.model.raw_model["trace_points"]
        self.assertEqual(len(points), 8)
        self.assertEqual(p.referenced_trace_points(), {point["id"] for point in points})
        self.assertTrue(all(
            point["added_by"].endswith("observe_connector_members") for point in points
        ))

    def test_effect_order_and_csv_quoting(self):
        p = self.program()
        with p.builder("test.ordered-effects") as b:
            sink = b.declare_csv_sink(
                key="ordered", filename="ordered.csv",
                columns=[{"name": 'value,"quoted"', "ty": "real"}],
                metadata=EMPTY_METADATA)
            arena = p.expressions("publish")
            with b.before_return("run_start") as ir:
                ir.emit("csv.open", sink=sink)
            with b.before_return("publish") as ir:
                for n in (2.0, 1.0):
                    value = ir.emit("compute", ty="real", expr=arena.real(n))
                    ir.emit("csv.write_row", sink=sink, values=[value])
            with b.before_return("run_finish") as ir:
                ir.emit("csv.close", sink=sink)
        self.run_program(p)
        with (self.root / "run" / "ordered.csv").open(newline="") as stream:
            rows = list(csv.reader(stream))
        self.assertEqual(rows[0], ['value,"quoted"'])
        self.assertEqual([float(row[0]) for row in rows[1:]], [2.0, 1.0] * 51)

    def test_logging_pass_computes_a_derived_column(self):
        """D4. `compute` earns its place: a pass derives a column the model
        never declares, from two observed members, in typed arithmetic."""
        p = self.program()
        instrument_all_connectors(p, self.model)
        points = observe_connector_members(self.model)
        with p.builder("test.power") as b:
            sink = b.declare_csv_sink(
                key="power", filename="power.csv",
                columns=[{"name": "T", "ty": "real"},
                         {"name": "Q_flow", "ty": "real"},
                         {"name": "power", "ty": "real"}],
                metadata={"connector": 0, "orientation": "outside", "members": [
                    {"trace_point": points["hot.port.T"]},
                    {"trace_point": points["hot.port.Q_flow"]}]})
            arena = p.expressions("publish")
            with b.before_return("run_start") as ir:
                ir.emit("csv.open", sink=sink)
            with b.before_return("publish") as ir:
                temperature = ir.emit("snapshot.value",
                                      trace_point=points["hot.port.T"])
                flow = ir.emit("snapshot.value",
                               trace_point=points["hot.port.Q_flow"])
                power = ir.emit("compute", ty="real", expr=arena.binary(
                    "Mul", arena.local(temperature), arena.local(flow)))
                ir.emit("csv.write_row", sink=sink,
                        values=[temperature, flow, power])
            with b.before_return("run_finish") as ir:
                ir.emit("csv.close", sink=sink)
        self.run_program(p)
        with (self.root / "run" / "power.csv").open(newline="") as stream:
            rows = list(csv.DictReader(stream))
        self.assertEqual(len(rows), 51)
        for row in rows:
            self.assertAlmostEqual(
                float(row["power"]),
                float(row["T"]) * float(row["Q_flow"]),
                delta=1e-9)

    def test_effects_are_not_reordered_across_publish(self):
        """Section 5. Ordering is owned by the program: publication sequence
        is row order, and no validator normalization may permute it."""
        p = self.program()
        instrument_all_connectors(p, self.model)
        with p.builder("test.sequence") as b:
            sink = b.declare_csv_sink(
                key="sequence", filename="sequence.csv",
                columns=[{"name": "publish_id", "ty": "integer"}],
                metadata=EMPTY_METADATA)
            with b.before_return("run_start") as ir:
                ir.emit("csv.open", sink=sink)
            with b.before_return("publish") as ir:
                ir.emit("csv.write_row", sink=sink,
                        values=[ir.emit("snapshot.sequence")])
            with b.before_return("run_finish") as ir:
                ir.emit("csv.close", sink=sink)
        before = deepcopy(p.raw["program"]["functions"])
        p.validate()
        self.assertEqual(p.raw["program"]["functions"], before)
        self.run_program(p)
        with (self.root / "run" / "sequence.csv").open(newline="") as stream:
            rows = list(csv.reader(stream))
        self.assertEqual([int(row[0]) for row in rows[1:]], list(range(51)))

    def test_unsupported_shapes_and_connector_metadata_rejected(self):
        b = self.model.builder("test.unsupported")
        with self.assertRaisesRegex(ValueError, "unsupported-feature"):
            b.add_connector_type("ArrayPort", members=[{"name": "x", "scalar_type": "real", "kind": "flow", "shape": [2]}])
        self.model.raw_model["connectors"][0]["members"][0]["variable"] = 999999
        with self.assertRaisesRegex(ValueError, "connector"):
            self.model.validate()

    def test_invalid_operations_types_and_lifecycles(self):
        p = self.program()
        instrument_all_connectors(p, self.model)
        original = deepcopy(p.raw)
        mutations = [
            lambda: p.function("run_start").append({"op": "snapshot.time", "result": "illegal"}),
            lambda: p.function("publish").append({"op": "assert", "condition": "undefined", "message": "bad"}),
            lambda: p.function("run_finish").clear(),
            lambda: p.raw["program"]["sinks"][0].update(filename="../escape.csv"),
            lambda: p.function("publish").append({"op": "unknown.operation"}),
        ]
        for mutate in mutations:
            p.raw = deepcopy(original)
            mutate()
            with self.assertRaises(ValueError):
                p.validate()

    def test_each_name_and_type_defect_has_its_own_code(self):
        """D4. A caller matches the code, not the message text."""
        p = self.program()
        instrument_all_connectors(p, self.model)
        original = deepcopy(p.raw)

        def undeclared_local(p):
            sink = p.raw["program"]["sinks"][0]
            p.function("publish").append({
                "op": "csv.write_row", "sink": sink["key"],
                "values": ["absent"] * len(sink["columns"])})

        def type_mismatch(p):
            arena = p.expressions("publish")
            p.declare("publish", "wrong", "integer")
            p.function("publish").append(
                {"op": "compute", "result": "wrong", "expr": arena.real(1.0)})

        def text_in_arithmetic(p):
            arena = p.expressions("publish")
            p.declare("publish", "bad", "real")
            phase = arena.local("v2")
            p.function("publish").append({
                "op": "compute", "result": "bad",
                "expr": arena.binary("Mul", phase, phase)})

        def branch_scoped_read(p):
            """Assigned in one arm only, read after the join.

            EX2-023 was reserved for a write from outside to a local declared
            in an inner scope. Declarations are function-scoped and `if`
            carries none of its own, so that shape does not exist; what a
            branch can actually do is leave a local unassigned on one path,
            and reading it is EX2-020.
            """
            arena = p.expressions("publish")
            p.declare("publish", "flag", "boolean")
            p.declare("publish", "inner", "real")
            p.declare("publish", "used", "real")
            p.function("publish").append({
                "op": "compute", "result": "flag",
                "expr": arena.compare("Ge", arena.real(1.0), arena.real(0.0))})
            p.function("publish").append({
                "op": "if", "condition": "flag",
                "then_body": [{"op": "snapshot.time", "result": "inner"}],
                "else_body": []})
            p.function("publish").append({
                "op": "compute", "result": "used", "expr": arena.local("inner")})

        for expected, mutate in (
            ("EX2-020", undeclared_local),
            ("EX2-021", type_mismatch),
            ("EX2-022", text_in_arithmetic),
            ("EX2-020", branch_scoped_read),
        ):
            with self.subTest(code=expected):
                p.raw = deepcopy(original)
                mutate(p)
                with self.assertRaisesRegex(ValueError, expected):
                    p.validate()

    def test_operator_enums_round_trip_and_a_typo_is_a_parse_error(self):
        """D4. Operators are enums on the wire, so a misspelling cannot reach
        the runtime."""
        p = self.program()
        instrument_all_connectors(p, self.model)
        arena = p.expressions("publish")
        p.declare("publish", "ok", "real")
        p.function("publish").append({
            "op": "compute", "result": "ok",
            "expr": arena.unary("Neg", arena.real(1.0))})
        p.validate()
        path = self.root / "operators.rbc"
        p.save(path)
        document = Model.load(path)
        nodes = document._document["execution"]["program"]["functions"]["publish"]["expressions"]
        self.assertEqual(nodes[-1], {"node": "unary", "op": "Neg", "operand": len(nodes) - 2})
        nodes[-1]["op"] = "Negg"
        broken = self.root / "broken.rbc"
        document.save(broken)
        # A misspelled operator cannot become a runtime surprise: it is not a
        # value of the enum, so the artifact does not parse.
        with self.assertRaisesRegex(ValueError, "unknown variant"):
            invoke("bitcode", "check-execution", broken)

    def test_recursive_call_is_rejected(self):
        """D5. Termination is by acyclicity of the call graph."""
        p = self.program()
        instrument_all_connectors(p, self.model)
        p.add_function("publish:loop")
        p.function("publish:loop").append({"op": "call", "function": "publish:loop"})
        p.function("publish").append({"op": "call", "function": "publish:loop"})
        with self.assertRaisesRegex(ValueError, "EX2-030"):
            p.validate()

    def test_v1_artifact_is_rejected_with_a_stable_code(self):
        """D1. No adapter for a superseded wire version (SPEC_0007)."""
        p = self.program()
        instrument_all_connectors(p, self.model)
        p.raw["version"] = 1
        with self.assertRaisesRegex(ValueError, "EX2-001"):
            p.validate()

    def test_backend_and_no_overwrite(self):
        p = self.program()
        path = self.root / "artifact.rbc"
        p.save(path)
        with self.assertRaisesRegex(ValueError, "unsupported execution mode/backend"):
            invoke("bitcode", "run", path, "--backend", "other", "--trace-root", self.root / "no")
        self.run_program(p)
        with self.assertRaisesRegex(ValueError, "trace directory must be new"):
            invoke("bitcode", "run", path, "--trace-root", self.root / "run")
        with self.assertRaisesRegex(ValueError, "discard executable edits"):
            invoke("compile-bitcode", path)

    def test_failed_run_writes_no_data_rows(self):
        """A run that cannot produce a state writes headers and nothing else.

        The failure is caused through the equations, because v2 has no
        serialized derived program to corrupt.
        """
        # A zero heat capacity makes der(hot.T) = Q/0 non-finite.
        capacity = next(v for v in self.model.raw_model["variables"]
                        if v["name"] == "hot.C")
        binding = self.model.raw_model["expressions"][capacity["binding"]]
        binding["node"]["value"]["value"] = 0.0
        p = self.program()
        instrument_all_connectors(p, self.model)
        artifact = self.root / "failure.rbc"
        p.save(artifact)
        with self.assertRaisesRegex(ValueError, "execution-failure"):
            invoke("bitcode", "run", artifact, "--trace-root", self.root / "failed")
        for path in (self.root / "failed").glob("*.csv"):
            self.assertEqual(len(path.read_text().splitlines()), 1)

    def test_derived_program_is_not_on_the_wire(self):
        """D1. One lowering's output for one backend is not a public artifact.

        v1 serialized storage, rows, projections and observations beside the
        authored program; v2 rebuilds them at load, so none of those names
        appear anywhere in the saved execution section.
        """
        p = self.program()
        instrument_all_connectors(p, self.model)
        for suffix in (".rbc", ".json"):
            with self.subTest(format=suffix):
                path = self.root / f"wire{suffix}"
                p.save(path)
                execution = Model.load(path)._document["execution"]
                self.assertEqual(set(execution), {
                    "version", "lowering", "dependency_digest", "revision",
                    "passes", "program"})
                text = json.dumps(execution)
                for derived in ("storage", "residual", "derivatives",
                                "initialization", "algebraic_blocks",
                                "initial_blocks", "observations"):
                    self.assertNotIn(f'"{derived}"', text)

    def test_program_round_trips_across_every_wire_form(self):
        """D1. One wire form, three encodings of it (TRP-002)."""
        p = self.program()
        instrument_all_connectors(p, self.model)
        cbor, js = self.root / "rt.rbc", self.root / "rt.json"
        p.save(cbor)
        invoke("bitcode", "convert", cbor, "--output", js)
        back = self.root / "back.rbc"
        invoke("bitcode", "convert", js, "--output", back)
        self.assertEqual(
            Model.load(cbor)._document["execution"],
            Model.load(back)._document["execution"],
        )
        # The text profile is a declared subset: it refuses an artifact whose
        # program it cannot represent rather than writing a lossy listing that
        # would assemble back into a different artifact.
        with self.assertRaisesRegex(ValueError, "text profile does not carry execution"):
            invoke("bitcode", "emit-text", cbor, "--output", self.root / "rt.txt")
        self.assertIn("run_start", invoke("bitcode", "disasm", cbor))

    def test_computation_call_branch_and_assert(self):
        p = self.program()
        instrument_all_connectors(p, self.model)
        with p.builder("test.check") as b:
            helper = p.add_function("publish:check")
            arena = p.expressions(helper)
            with b.before_return(helper) as ir:
                t = ir.emit("snapshot.time")
                nonnegative = ir.emit(
                    "compute", ty="boolean",
                    expr=arena.compare("Ge", arena.local(t), arena.real(0.0)))
                message = ir.emit("snapshot.phase")
                ir.emit("if", condition=nonnegative, then_body=[], else_body=[
                    {"op": "assert", "condition": nonnegative, "message": message}])
            with b.before_return("publish") as ir:
                ir.emit("call", function="publish:check")
        self.run_program(p)
        verify(self.root / "run")

    def test_missing_replay_target_rejected(self):
        """The replay target is the trace point the program names (D2)."""
        p = self.program()
        instrument_all_connectors(p, self.model)
        self.model.raw_model["trace_points"].pop()
        self.model.reload_trace_points()
        with self.assertRaisesRegex(ValueError, "replay target identity changed"):
            p.relower(replay={"example.connector-csv": instrument_all_connectors})

    def test_three_way_connection_one_flow_sum(self):
        model = Model.empty("Nary")
        b = model.builder("test.nary")
        t = b.add_connector_type("Port", members=[{"name": "v", "scalar_type": "real", "kind": "potential"}, {"name": "i", "scalar_type": "real", "kind": "flow"}])
        ports = [b.add_connector("p", owner=b.add_component(n), type_id=t) for n in ("a", "b", "c")]
        b.add_connection_set(ports)
        model.refresh()
        model.validate()
        self.assertEqual(len(model.equations), 3)
        self.assertEqual(len(model.raw_model["connection_sets"][0]["balances"]), 1)
        self.assertEqual(len(model.raw_model["connection_sets"][0]["balances"][0]["terms"]), 3)

    def test_state_equation_removal_is_atomic_and_reindexes(self):
        model = Model.empty("Removal")
        b = model.builder("test.add")
        a, z = b.add_state("a", 1), b.add_state("z", 2)
        ea = b.add_derivative_equation(a, b.unary("negate", b.ref(a)))
        b.add_derivative_equation(z, b.unary("negate", b.ref(z)))
        model.refresh()
        before = deepcopy(model.raw_model)
        with self.assertRaisesRegex(ValueError, "still referenced"):
            b.remove(variables=[a])
        self.assertEqual(before, model.raw_model)
        mapping = b.remove(variables=[a], equations=[ea])
        self.assertEqual(mapping["variables"], {z: 0})
        self.assertEqual([v.name for v in model.states], ["z"])
        self.assertEqual(len(model.equations), 1)
        model.validate()


if __name__ == "__main__":
    unittest.main()
