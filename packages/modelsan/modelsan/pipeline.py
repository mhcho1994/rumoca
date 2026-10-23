"""Wiring the layers together. The only component that knows all of them exist.

The ordering here encodes two rules:

**Coverage is resolved before anything runs.** The plan records, per sanitizer
component, whether it can operate and why not — so a finding count is always
interpretable. "No finding" from a sanitizer the planner marked unsupported does
not mean the model was clean.

**A failed execution is still judged.** Results are passed to sanitizers whether
or not they produced a trajectory, because the failure itself is an observation
and SolverSan exists to read it.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from types import SimpleNamespace

from .analysis.context import AnalysisContext
from .backends.base import ExecutionResult, ExecutionStatus
from .findings.deduplicate import BugDatabase
from .findings.finding import Finding
from .findings.signature import attach
from .fuzz.hints import FuzzHint, merge
from .fuzz.testcase import NOMINAL, TestCase
from .instrumentation.planner import CapabilityPlanner, Plan
from .instrumentation.capability import Capability
from .sanitizers.registry import SanitizerRegistry


@dataclass
class RunOutcome:
    """Everything one campaign over one model produced, including what it could
    not look at."""

    model_name: str
    plan: Plan | None = None
    baseline: ExecutionResult | None = None
    results: list[ExecutionResult] = field(default_factory=list)
    database: BugDatabase = field(default_factory=BugDatabase)
    hints: list[FuzzHint] = field(default_factory=list)
    note: str = ""

    @property
    def coverage(self) -> dict[str, str]:
        """Sanitizer components that did not run, and why. Never empty silently."""
        gaps = self.plan.skipped_sanitizers() if self.plan else {}
        for bug in self.database.bugs:
            for finding in bug.findings:
                if finding.kind == "contract-unobserved":
                    key = f"behavior.contract.{finding.evidence['contract_id']}"
                    gaps[key] = finding.evidence["reason"]
        for index, result in enumerate(self.results):
            if not result.ok:
                gaps[f"execution.run{index}"] = (
                    f"{result.status.value} during {result.phase.value}: "
                    f"{result.failure.message if result.failure else 'execution did not complete'}")
            evidence = result.backend_metadata.get("domain_diagnostics", {})
            if evidence.get("available") is False:
                gaps[f"domain.failure.run{index}"] = "native diagnostics unavailable for this run"
            missing = evidence.get("unobserved_evaluations", 0)
            if missing or evidence.get("truncated"):
                gaps[f"domain.failure.run{index}"] = (
                    f"partial native coverage: {missing} unobserved evaluations; "
                    f"fault limit reached={bool(evidence.get('truncated'))}")
            solver = evidence.get("solver", {})
            omitted = solver.get("omitted", 0)
            large = sum(r.get("values_omitted", False) for r in solver.get("records", []))
            if omitted or large:
                gaps[f"solver.telemetry.run{index}"] = f"bounded native diagnostics: {omitted} records and {large} matrices omitted"
        return gaps

    @property
    def executed(self) -> int:
        return len(self.results)


class Pipeline:
    def __init__(self, registry: SanitizerRegistry, backend) -> None:
        self.registry = registry
        self.backend = backend

    def collect_hints(self, model, context: AnalysisContext) -> list[FuzzHint]:
        hints: list[FuzzHint] = []
        for sanitizer in self.registry.hint_providers():
            hints.extend(sanitizer.hints(model, context))
        return merge(hints)

    def run_static(self, model) -> RunOutcome:
        """Analyse the artifact without preparing or launching an execution backend."""
        from .network import build
        context = AnalysisContext(model)
        available = {Capability.CANONICAL_MODEL, Capability.CANONICAL_IDENTITY}
        if not build(model).absent:
            available.add(Capability.CONNECTION_GRAPH)
        static_capabilities = {Capability.CANONICAL_MODEL, Capability.CANONICAL_IDENTITY,
                               Capability.CONNECTION_GRAPH}
        analyzers = self.registry.static_analyzers()
        components = [SimpleNamespace(name=s.name, requires={
            component: needed for component, needed in getattr(s, 'requires', {}).items()
            if set(needed) <= static_capabilities}) for s in analyzers]
        outcome = RunOutcome(model_name=model.name)
        outcome.plan = CapabilityPlanner(frozenset(available)).plan(components, [])
        for sanitizer in analyzers:
            outcome.database.extend(attach(sanitizer.analyze(model, context)))
        return outcome

    def plan(self, model, context: AnalysisContext) -> Plan:
        requests = []
        for sanitizer in self.registry.instrumentation_requesters():
            requests.extend(sanitizer.requests(model, context))
        capabilities = set(getattr(self.backend, "capabilities", frozenset()))
        if model is not None:
            from .network import build
            capabilities.add(Capability.CANONICAL_MODEL)
            if not build(model).absent:
                capabilities.add(Capability.CONNECTION_GRAPH)
        return CapabilityPlanner(frozenset(capabilities)).plan(self.registry.active(), requests)

    def judge(self, result: ExecutionResult, model, context: AnalysisContext,
              testcase: TestCase) -> list[Finding]:
        """Run every runtime observer over one execution — success or failure.

        Sanitizers are not asked to check whether a trace exists; the planner
        already decided which of them can operate here. Ordering is preserved
        across sanitizers so a causal chain stays reconstructible.
        """
        findings: list[Finding] = []
        for sanitizer in self.registry.runtime_observers():
            findings.extend(
                sanitizer.observe(result.observations, model, context, testcase))
        findings.sort(key=lambda f: (f.time if f.time is not None else 0.0))
        return attach(findings)

    def compare(self, results: dict, model, context: AnalysisContext,
                testcase: TestCase) -> list[Finding]:
        """Run every differential oracle over a set of results for one case."""
        findings: list[Finding] = []
        for sanitizer in self.registry.differential_oracles():
            findings.extend(sanitizer.compare(results, model, testcase))
        return attach(findings)

    def run_comparative(self, model, model_path: str, model_name: str,
                        backends: dict, testcase: TestCase | None = None,
                        repeats: int = 1) -> list[Finding]:
        """Execute one case across several backends and/or repeats, then compare.

        Separate from `run` because it is a different shape of campaign: the
        unit of judgement is a *set* of results rather than one, and the extra
        executions cost real time. `repeats` covers DeterminismSan, where the
        several results come from one backend rather than several.
        """
        testcase = testcase or NOMINAL
        context = AnalysisContext(model)
        results: dict[str, ExecutionResult] = {}
        for name, backend in backends.items():
            failure = backend.prepare(model_path, model_name)
            if failure is not None:
                results[name] = failure
                continue
            for attempt in range(max(1, repeats)):
                label = name if repeats == 1 else f"{name}#{attempt + 1}"
                results[label] = backend.run(testcase)
        return self.compare(results, model, context, testcase)

    def run(self, model, model_path: str, model_name: str,
            testcases: list[TestCase] | None = None) -> RunOutcome:
        context = AnalysisContext(model)
        outcome = RunOutcome(model_name=model_name)
        outcome.plan = self.plan(model, context)
        outcome.hints = self.collect_hints(model, context)

        # Static evidence does not depend on whether the simulator can prepare
        # this artifact. Preserve it even when preparation fails.
        for sanitizer in self.registry.static_analyzers():
            outcome.database.extend(attach(sanitizer.analyze(model, context)))

        build_failure = self.backend.prepare(model_path, model_name)
        # Preparation discovers per-artifact observation/connector coverage.
        # Re-plan before executing; a saved program may have a different profile.
        outcome.plan = self.plan(model, context)
        if build_failure is not None:
            # A backend error is evidence about the tool, not the model. It is
            # still recorded and still judged — SolverSan reports it at INFO —
            # so coverage stays visible instead of the model looking clean.
            outcome.results.append(build_failure)
            outcome.database.extend(self.judge(build_failure, model, context, NOMINAL))
            outcome.note = ("compiler proved a violation in the declared model"
                            if build_failure.status is ExecutionStatus.FAILED else
                            "backend could not build the model")
            return outcome

        baseline = self.backend.run(NOMINAL, outcome.plan.satisfied)
        outcome.baseline = baseline
        outcome.results.append(baseline)
        outcome.database.extend(self.judge(baseline, model, context, NOMINAL))

        if not baseline.ok:
            # Without a clean baseline nothing can be attributed to a test case:
            # the first candidate tried would be blamed for a pre-existing
            # failure. The baseline failure is itself reported above.
            outcome.note = ("model does not run at its declared values: "
                            f"{baseline.failure}" if baseline.failure else
                            "model does not run at its declared values")
            return outcome

        for testcase in (testcases or []):
            result = self.backend.run(testcase, outcome.plan.satisfied)
            outcome.results.append(result)
            if result.status is ExecutionStatus.BACKEND_ERROR:
                continue  # says nothing about the model
            outcome.database.extend(self.judge(result, model, context, testcase))

        return outcome
