"""Every table the compiler exports reads through a typed SDK view.

A pass author who has to fall back to ``Model.raw_model`` for a section is
using an unstable, undocumented shape; this test compiles models that fill
the event, clock and owner tables and walks each one through its view.
"""
from fractions import Fraction
from pathlib import Path
import subprocess
import tempfile
import unittest

from rumoca_bitcode import Model

ROOT = Path(__file__).resolve().parents[3]
RUMOCA = ROOT / "target/debug/rumoca"

EVENTS = """
model Events
  parameter Real d = 0.2;
  Real x(start = 1, fixed = true);
  Real y;
  discrete Real count(start = 0, fixed = true);
  Boolean hit(start = false, fixed = true);
equation
  der(x) = -x + 0.1 * delay(x, d);
  y = x - 0.5;
  when y < 0 then
    count = pre(count) + 1;
  end when;
  when time >= 0.3 then
    hit = true;
  end when;
  when x < 0.2 then
    reinit(x, 1.0);
  end when;
  assert(x < 10, "x grew", AssertionLevel.warning);
  when time > 0.25 then
    terminate("done");
  end when;
end Events;
"""

CLOCKED = """
model Clocked
  Clock c = Clock(1, 10);
  Real x(start = 1, fixed = true);
  Real s;
  Real u(start = 0);
equation
  der(x) = -x + hold(u);
  when c then
    s = sample(x);
    u = previous(u) + 0.1 * s;
  end when;
end Clocked;
"""


def compile_model(work: Path, name: str, text: str) -> Model:
    source = work / f"{name}.mo"
    source.write_text(text)
    artifact = work / f"{name}.rbc"
    result = subprocess.run(
        [str(RUMOCA), "compile", str(source), "--model", name, "--emit-bitcode", str(artifact)],
        capture_output=True, text=True, timeout=300,
    )
    assert result.returncode == 0, result.stderr
    return Model.load(artifact)


TABLES = {
    "relations": "relations",
    "conditions": "conditions",
    "clocks": "clocks",
    "clock_ownerships": "clock_ownerships",
    "roots": "roots",
    "time_events": "time_events",
    "discrete_definitions": "discrete_definitions",
    "model_event_transactions": "event_transactions",
    "previous_values": "previous_values",
    "delays": "delays",
    "terminals": "terminals",
    "structured_roots": "structured_roots",
    "connector_types": "connector_types",
}


def walk(model: Model) -> None:
    """Touch every accessor, so a shape mismatch raises."""
    for relation in model.relations:
        relation.expression, relation.source
    for condition in model.conditions:
        condition.kind, condition.relation, condition.expression
        condition.operands, condition.clock, condition.detail
    for root in model.roots:
        root.relation, root.activation
    for clock in model.clocks:
        clock.period, clock.phase, clock.anchor, clock.condition
    for owned in model.clock_ownerships:
        owned.variable, owned.clock, owned.sampled
    for event in model.time_events:
        event.time, event.deadline
    for definition in model.discrete_definitions:
        definition.targets
        for branch in definition.branches:
            branch.kind, branch.trigger, branch.guard, branch.values
    for transaction in model.event_transactions:
        transaction.targets
        for step in transaction.steps:
            step.trigger, step.guard, step.clock, step.definitions
    for previous in model.previous_values:
        previous.variable, previous.clock
    for delay in model.delays:
        delay.kind, delay.expression, delay.delay_time, delay.delay_time_value, delay.maximum
    for terminal in model.terminals:
        terminal.source
    for root in model.structured_roots:
        root.domain, root.expression
    for connector in model.connector_types:
        connector.name, connector.members, connector.flow_convention
    for event in model.events:
        event.trigger, event.guard, event.message, event.level, event.state, event.value


@unittest.skipUnless(RUMOCA.exists(), "needs target/debug/rumoca")
class TypedTables(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.work = tempfile.TemporaryDirectory()
        work = Path(cls.work.name)
        cls.events = compile_model(work, "Events", EVENTS)
        cls.clocked = compile_model(work, "Clocked", CLOCKED)

    @classmethod
    def tearDownClass(cls):
        cls.work.cleanup()

    def test_every_table_has_a_view_of_the_same_length(self):
        for model in (self.events, self.clocked):
            for key, attribute in TABLES.items():
                with self.subTest(model=model.name, table=key):
                    self.assertEqual(
                        len(getattr(model, attribute)), len(model.raw_model.get(key, []))
                    )

    def test_every_accessor_reads_its_table(self):
        walk(self.events)
        walk(self.clocked)

    def test_the_tables_these_models_fill_are_not_empty(self):
        for attribute in ("relations", "conditions", "roots", "delays", "discrete_definitions"):
            with self.subTest(table=attribute):
                self.assertTrue(getattr(self.events, attribute))
        for attribute in ("clocks", "clock_ownerships", "previous_values"):
            with self.subTest(table=attribute):
                self.assertTrue(getattr(self.clocked, attribute))

    def test_a_periodic_clock_reads_as_an_exact_rational(self):
        periods = {clock.period for clock in self.clocked.clocks if clock.kind == "periodic"}
        self.assertIn(Fraction(1, 10), periods)

    def test_the_delay_carries_its_parameter_value(self):
        (delay,) = self.events.delays
        self.assertEqual(delay.kind, "parameter")
        self.assertEqual(delay.delay_time_value, 0.2)

    def test_assert_and_terminate_expose_their_message(self):
        kinds = {event.kind: event for event in self.events.events}
        self.assertIsNotNone(kinds["assert"].message)
        self.assertIsNotNone(kinds["assert"].level)
        self.assertIsNotNone(kinds["terminate"].message)
        self.assertIsNotNone(kinds["reinitialize"].state)
        for event in self.events.events:
            self.assertIsNotNone(event.trigger.kind)


if __name__ == "__main__":
    unittest.main()
