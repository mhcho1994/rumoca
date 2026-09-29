"""Rumoca behind the Backend contract.

This backend is what makes canonical identity available. It executes the same
artifact the analyses read, so every observation it produces can name the exact
DAE entity it came from — where OpenModelica can only report the name it happens
to use. That is the difference between a finding anchored to the model and one
anchored to a tool's vocabulary.

It is also the second implementation DifferentialSan needs. With one backend
there is nothing to differ from.
"""

from __future__ import annotations

import csv
import json
import math
import re
import subprocess
import tempfile
from pathlib import Path

from ..dae import load, save
from ..fuzz.testcase import TestCase
from ..instrumentation.capability import Capability
from ..runtime.anchors import CanonicalAnchor, EntityKind
from ..runtime.failures import ExecutionFailure, ExecutionPhase, FailureKind
from ..runtime.observations import (
    InitializationFailure,
    ObservationStream,
    SimulationAbort,
    SimulationEnd,
    SimulationStart,
    SolverFailure,
    VariableObservation,
)
from .base import ExecutionResult, ExecutionStatus, Trace
from . import rumoca_execution
from .process import execute
from ..campaign_provenance import file_digest
from .compile_diagnostics import proven_model_failure

# Ordered; first match wins.
CLASSIFIERS = (
    (re.compile(r"execution assertion:|assertion failed|assertion violated", re.I),
     FailureKind.ASSERTION_VIOLATED),
    (re.compile(r"division by zero|zero denominator", re.I), FailureKind.DIVISION_BY_ZERO),
    (re.compile(r"non-finite|\b(?:NaN|inf|infinity)\b", re.I), FailureKind.NON_FINITE_VALUE),
    (re.compile(r"structurally singular|singular", re.I), FailureKind.SINGULAR_SYSTEM),
    (re.compile(r"did not converge|projection", re.I),
     FailureKind.NONLINEAR_SOLVER_FAILURE),
    (re.compile(r"step size", re.I), FailureKind.STEP_SIZE_TOO_SMALL),
    (re.compile(r"division", re.I), FailureKind.DIVISION_BY_ZERO),
)


_HELD = re.compile(r"holding \d+ free input\(s\) constant at their start values: (.*)")


def held_inputs(stderr: str) -> dict[str, float]:
    """The inputs `--free-inputs` held constant, as Rumoca reported them."""
    match = _HELD.search(stderr or "")
    if not match:
        return {}
    held = {}
    for pair in match.group(1).split(", "):
        name, _, value = pair.rpartition("=")
        try:
            held[name] = float(value)
        except ValueError:
            continue
    return held


def classify(text: str) -> FailureKind:
    for pattern, kind in CLASSIFIERS:
        if pattern.search(text):
            return kind
    return FailureKind.UNKNOWN


class RumocaBackend:
    """Simulates a canonical DAE artifact directly."""

    name = "rumoca"

    #: The only backend that can supply CANONICAL_IDENTITY, because it runs the
    #: same artifact the analyses read.
    capabilities = frozenset({
        Capability.OBSERVE_VARIABLE,
        Capability.OBSERVE_FAILURE,
        Capability.CANONICAL_IDENTITY,
        Capability.OBSERVE_DOMAIN_FAILURE,
        Capability.OBSERVE_EVENTS,
        Capability.OBSERVE_SOLVER_STEPS,
    })

    def __init__(self, executable: str = "./target/debug/rumoca",
                 t_end: float = 0.5, timeout: float = 90.0, *, replay=None,
                 source_roots=None, cache_dir=None, dt=None, freeze_parameters=False,
                 free_inputs=None) -> None:
        if dt is not None and (not math.isfinite(dt) or dt <= 0):
            raise ValueError("output interval must be finite and positive")
        self.executable = executable
        self.t_end = t_end
        self.timeout = timeout
        roots = source_roots if source_roots is not None else (
            "target/msl/ModelicaStandardLibrary-4.1.0",
            "target/corpus/ModelicaStandardLibrary-4.1.0")
        self.source_roots = tuple(str(Path(root).resolve()) for root in roots)
        self.cache_dir = str(Path(cache_dir).resolve()) if cache_dir is not None else None
        self.dt = dt
        self.freeze_parameters = bool(freeze_parameters)
        # `"start"` holds every unbound top-level input at its start value
        # (else 0) instead of refusing the model; None keeps the refusal. A
        # run under held inputs records which were held, because a finding
        # it produces is about that input history, not every history.
        if free_inputs not in (None, "start"):
            raise ValueError("free_inputs must be None or 'start'")
        self.free_inputs = free_inputs
        # Opt-in authoring permission; None never re-lowers a saved program.
        self.replay = replay
        self._work: tempfile.TemporaryDirectory | None = None
        self._artifact: Path | None = None
        self._ids: dict[str, int] = {}
        self._observed_ids: set[int] = set()
        self._execution_targets = None
        self._run_index = 0
        self.capabilities = type(self).capabilities

    def prepare_from_artifact(self, artifact: Path) -> ExecutionResult | None:
        """Instrument an existing artifact so its variables are observable.

        `--simulate --trace-out` reports only declared trace points, and a
        freshly compiled artifact declares none. Adding one per variable is a
        pure observation change: no equation, variable or parameter is touched,
        so what runs is what was analysed.
        """
        self.close()
        self._work = tempfile.TemporaryDirectory()
        work = Path(self._work.name)
        try:
            model = load(artifact)
        except Exception as error:
            return ExecutionResult.backend_error(self.name, f"cannot load: {error}")

        self._ids = {v.name: v.id for v in model.variables}
        if model.has_execution:
            destination = work / "observed-execution.rbc"
            try:
                targets = rumoca_execution.prepare(
                    artifact, destination, executable=self.executable, timeout=self.timeout,
                    replay=self.replay)
            except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired) as error:
                return ExecutionResult.backend_error(self.name, f"cannot prepare executable: {error}")
            self._execution_targets = targets
            self._observed_ids = {v.id for v in targets}
            self._artifact = destination
            self._set_capabilities(model)
            return None
        added = 0
        for variable in model.variables:
            if variable.is_parameter:
                continue  # constant over the run; a column per sample says nothing
            try:
                model.add_trace_point(variable, label=variable.name,
                                      added_by="modelsan.RumocaBackend")
                self._observed_ids.add(variable.id)
                added += 1
            except Exception as error:
                return ExecutionResult.backend_error(
                    self.name, f"cannot observe {variable.name}: {error}")
        if not added:
            return ExecutionResult.backend_error(self.name, "no traceable variables")

        self._artifact = work / "traced.rbc"
        try:
            save(model, self._artifact)
            from rumoca_bitcode.compiler import invoke
            invoke("bitcode", "check", self._artifact, "--strict",
                   executable=self.executable, timeout=self.timeout)
            self._set_capabilities(model)
        except Exception as error:
            self._artifact = None
            return ExecutionResult.backend_error(self.name, f"cannot save: {error}")
        return None

    def _set_capabilities(self, model):
        from ..network import build
        available = set(type(self).capabilities) | {Capability.CANONICAL_MODEL}
        network = build(model)
        if not network.absent:
            available.add(Capability.CONNECTION_GRAPH)
            members = {v.id for p in network.ports for v in p.potentials}
            members |= {v.id for p in network.ports for v, _ in p.flows}
            if members <= self._observed_ids:
                available.add(Capability.OBSERVE_CONNECTOR)
        self.capabilities = frozenset(available)

    def prepare(self, model_path: str, model_name: str) -> ExecutionResult | None:
        """Compile Modelica source, then instrument the result."""
        if Path(model_path).suffix.lower() in {".rbc", ".json"}:
            return self.prepare_from_artifact(Path(model_path))
        self.close()
        # Keep the source artifact alive while prepare_from_artifact owns a
        # separate output directory. Resetting self._work used to remove raw.
        source_work = tempfile.TemporaryDirectory()
        work = Path(source_work.name)
        raw = work / "m.rbc"
        diagnostics = work / "compile-diagnostics.json"
        command = [self.executable, "compile", model_path, "--model", model_name,
                   "--emit-bitcode", str(raw), "--diagnostics-json", str(diagnostics)]
        if self.freeze_parameters:
            command += ["--freeze-parameters"]
        for root in self.source_roots:
            command += ["--source-root", root]
        if self.cache_dir is not None:
            command += ["--cache-dir", self.cache_dir]
        try:
            done = execute(command, Path.cwd(), self.timeout)
        except (subprocess.TimeoutExpired, OSError) as error:
            return ExecutionResult.backend_error(self.name, f"cannot compile: {error}")
        if done.returncode != 0 or not raw.exists():
            try:
                proved = proven_model_failure(diagnostics, self.name) if done.returncode > 0 else None
            except (OSError, ValueError) as error:
                return ExecutionResult.backend_error(self.name, f"invalid compiler diagnostics: {error}")
            if proved is not None:
                proved.backend_metadata.update(command=command, returncode=done.returncode,
                    parameter_policy="frozen" if self.freeze_parameters else "declared")
                return proved
            return ExecutionResult.backend_error(
                self.name, (done.stdout + done.stderr).strip()[-300:])
        return self.prepare_from_artifact(raw)

    def run(self, testcase: TestCase, instrumentation: list | None = None) -> ExecutionResult:
        if self._artifact is None or self._work is None:
            return ExecutionResult.backend_error(self.name, "model was not prepared")
        if self.freeze_parameters and testcase.parameters:
            return ExecutionResult.backend_error(self.name,
                "frozen-parameter profile requires recompilation for parameter overrides")
        if testcase.input_trajectory is not None or testcase.solver_options:
            return ExecutionResult.backend_error(self.name, "input trajectories/solver overrides unsupported")
        for request in instrumentation or ():
            if request.capability not in self.capabilities:
                return ExecutionResult.backend_error(self.name, f"unsupported request: {request.capability}")
            if request.anchor is not None and (request.anchor.kind is not EntityKind.VARIABLE
                    or request.anchor.dae_id not in self._observed_ids):
                return ExecutionResult.backend_error(self.name, "requested observation unavailable")
        if self._execution_targets is not None:
            return self._run_execution(testcase, instrumentation)
        if testcase.initial_values:
            return ExecutionResult.backend_error(
                self.name, "initial-value overrides unsupported; --param only configures tunable parameters")
        work = Path(self._work.name)
        self._run_index += 1
        diagnostics_path = work / f"domain-{self._run_index}.json"
        trace_csv = work / "trace.csv"
        if trace_csv.exists():
            trace_csv.unlink()

        # `--check` and `--trace-out` are mutually exclusive in Rumoca, and the
        # trace is the right choice: `--check` reports non-finite values and
        # bound breaches, which is exactly what NumericSan and RangeSan do from
        # observations. Letting the backend do it would move a sanitizer's
        # judgement inside the execution boundary and hide it from the planner.
        command = [self.executable, "compile-bitcode", str(self._artifact),
                   "--simulate", "--t-end", str(self.t_end),
                   "--trace-out", str(trace_csv), "--domain-diagnostics", str(diagnostics_path)]
        if self.dt is not None:
            command += ["--dt", str(self.dt)]
        for name, value in testcase.parameters.items():
            command += ["--param", f"{name}={value!r}"]
        if self.free_inputs is not None:
            command += ["--free-inputs", self.free_inputs]

        try:
            done = execute(command, Path.cwd(), self.timeout)
        except OSError as error:
            return ExecutionResult.backend_error(self.name, f"cannot execute: {error}")
        except subprocess.TimeoutExpired:
            stream = ObservationStream()
            stream.add(SimulationAbort(kind=FailureKind.TIMEOUT,
                                       reason=f"exceeded {self.timeout:g}s"))
            return ExecutionResult(backend=self.name, status=ExecutionStatus.TIMEOUT,
                                   observations=stream,
                                   failure=ExecutionFailure(
                                       kind=FailureKind.TIMEOUT,
                                       phase=ExecutionPhase.SIMULATION,
                                       message=f"exceeded {self.timeout:g}s"))

        try:
            times, columns = self._read_trace(trace_csv)
            expected = {name for name, identifier in self._ids.items()
                        if identifier in self._observed_ids}
            if done.returncode == 0 and not expected <= columns.keys():
                raise ValueError(f"missing requested columns: {sorted(expected - columns.keys())}")
        except (OSError, ValueError, csv.Error) as error:
            return ExecutionResult.backend_error(self.name, f"invalid observations: {error}")
        trace = Trace(times=times, columns=columns) if times else None
        stream = self._stream(times, columns)
        try:
            diagnostics = rumoca_execution.read_domain_diagnostics(diagnostics_path, stream)
            if done.returncode == 0 and diagnostics.get("available") is False:
                raise ValueError("missing requested domain diagnostics")
        except (OSError, ValueError, KeyError, TypeError) as error:
            return ExecutionResult.backend_error(self.name, f"invalid domain diagnostics: {error}")
        metadata = {"domain_diagnostics": diagnostics, "execution": "saved-equations",
                    "artifact_sha256": file_digest(self._artifact),
                    "output_interval": self.dt, "command": command,
                    "returncode": done.returncode, "stdout": done.stdout, "stderr": done.stderr,
                    "held_inputs": held_inputs(done.stderr)}
        text = " ".join((done.stdout + done.stderr).split())

        if (done.returncode != 0 and "simulation failed:" not in text.lower()
                and re.search(r"not supported by bitcode|unsupported|cannot rebuild a checked DAE", text, re.I)):
            return ExecutionResult.backend_error(self.name, text[-2000:])

        # `--param` naming a parameter the artifact does not expose is a harness
        # error, not a model failure: reporting it as one blames the model for a
        # badly formed question.
        if "not a tunable parameter" in text or "structural or constant" in text:
            return ExecutionResult.backend_error(
                self.name, "parameter not tunable in this artifact",
                phase=ExecutionPhase.INITIALIZATION)

        if done.returncode == 0 and trace is None:
            return ExecutionResult.backend_error(self.name, "successful command produced no observations")
        if done.returncode == 0:
            stream.add(SimulationEnd(completed=True))
            return ExecutionResult(backend=self.name, status=ExecutionStatus.SUCCESS,
                                   phase=ExecutionPhase.FINALIZATION,
                                   observations=stream, trace=trace, backend_metadata=metadata)

        violations = self._violations(done.stdout)
        detail = violations[0] if violations else text[-200:]
        at_init = "structural" in detail.lower() or "initial" in detail.lower()
        phase = ExecutionPhase.INITIALIZATION if at_init else ExecutionPhase.SIMULATION
        # Classify from the whole output, not the truncated summary: the
        # phrase that identifies the failure is often earlier than the last
        # 200 characters.
        failure = ExecutionFailure(kind=classify(text) if classify(text)
                                   is not FailureKind.UNKNOWN else classify(detail),
                                   phase=phase,
                                   message=detail[:200], raw=text[-2000:])
        cls = InitializationFailure if at_init else SolverFailure
        stream.add(cls(kind=failure.kind, reason=failure.message, raw=failure.raw))
        stream.add(SimulationEnd(completed=False, message=failure.message))
        return ExecutionResult(backend=self.name, status=ExecutionStatus.FAILED,
                               phase=phase, observations=stream, trace=trace,
                               failure=failure, backend_metadata=metadata)

    @staticmethod
    def _violations(stdout: str) -> list[str]:
        """`--check` emits machine-readable JSON; parse that, not the prose."""
        start = stdout.find("[")
        if start < 0:
            return []
        try:
            entries = json.loads(stdout[start:stdout.rfind("]") + 1])
        except (ValueError, TypeError):
            return []
        return [str(e.get("detail", e.get("kind", ""))) for e in entries
                if isinstance(e, dict)]

    def _stream(self, times: list[float], columns: dict) -> ObservationStream:
        stream = ObservationStream()
        stream.add(SimulationStart())
        for index, time in enumerate(times):
            for name, values in columns.items():
                dae_id = self._ids.get(name)
                anchor = (CanonicalAnchor(EntityKind.VARIABLE, dae_id, name)
                          if dae_id is not None else None)
                stream.add(VariableObservation(time=time, canonical=anchor, value=values[index]))
        return stream

    @staticmethod
    def _read_trace(path: Path) -> tuple[list[float], dict[str, list[float]]]:
        """Rumoca writes long format: time, trace_id, ..., variable, ..., value."""
        if not path.exists():
            return [], {}
        series: dict[str, list[tuple[float, float]]] = {}
        names: dict[str, str] = {}
        previous = -math.inf
        with path.open(newline="", encoding="utf-8") as handle:
            rows = csv.DictReader(handle, strict=True)
            if rows.fieldnames != ["time", "trace_id", "connection", "variable", "quantity", "unit", "value"]:
                raise ValueError("malformed equation trace header")
            for row in rows:
                if None in row or any(value is None for value in row.values()):
                    raise ValueError("malformed equation trace row")
                when, value = float(row["time"]), float(row["value"])
                if not math.isfinite(when) or when < previous or not row["variable"]:
                    raise ValueError("invalid equation publication coordinate")
                previous = when
                identity = row["trace_id"]
                if not identity or (identity in names and names[identity] != row["variable"]):
                    raise ValueError("invalid equation trace identity")
                names[identity] = row["variable"]
                series.setdefault(identity, []).append((when, value))
        if not series:
            return [], {}
        times = [time for time, _ in next(iter(series.values()))]
        columns = {}
        for identity, points in series.items():
            if [time for time, _ in points] != times:
                raise ValueError("unsynchronized equation observations")
            values = [value for _, value in points]
            name = names[identity]
            if name in columns and any(a != b and not (math.isnan(a) and math.isnan(b))
                                       for a, b in zip(values, columns[name])):
                raise ValueError("conflicting duplicate equation observation")
            columns[name] = values
        return times, columns

    def close(self) -> None:
        self._artifact = None
        self._execution_targets = None
        self._ids = {}
        self._observed_ids = set()
        self._run_index = 0
        self.capabilities = type(self).capabilities
        if self._work is not None:
            self._work.cleanup()
            self._work = None

    def _run_execution(self, testcase, instrumentation):
        self._run_index += 1
        root = Path(self._work.name) / f"execution-run-{self._run_index}"
        command = [self.executable, "bitcode", "run", str(self._artifact),
                   "--execution", "require", "--stop", str(self.t_end),
                   "--trace-root", str(root)]
        command.append("--domain-diagnostics")
        if self.dt is not None:
            command += ["--publish-interval", str(self.dt)]
        for name, value in testcase.parameters.items():
            command += ["--param", f"{name}={value!r}"]
        for name, value in testcase.initial_values.items():
            command += ["--initial", f"{name}={value!r}"]
        try:
            done = execute(command, Path.cwd(), self.timeout)
        except OSError as error:
            return ExecutionResult.backend_error(self.name, f"cannot execute: {error}")
        except subprocess.TimeoutExpired:
            stream = ObservationStream()
            stream.add(SimulationAbort(kind=FailureKind.TIMEOUT, reason=f"exceeded {self.timeout:g}s"))
            return ExecutionResult(
                backend=self.name, status=ExecutionStatus.TIMEOUT,
                observations=stream,
                failure=ExecutionFailure(kind=FailureKind.TIMEOUT, phase=ExecutionPhase.SIMULATION,
                                         message=f"exceeded {self.timeout:g}s"))
        text = " ".join((done.stdout + done.stderr).split())
        try:
            times, columns = rumoca_execution.read_trace(root / rumoca_execution.FILENAME,
                                                       self._execution_targets)
        except (ValueError, OSError) as error:
            return ExecutionResult.backend_error(self.name, f"invalid observations: {error}")
        stream = self._stream(times, columns)
        metadata = {"execution": "saved", "trace_root": str(root)}
        try:
            diagnostics = rumoca_execution.read_domain_diagnostics(root / "domain-diagnostics.json", stream)
            if done.returncode == 0 and diagnostics.get("available") is False:
                raise ValueError("missing requested domain diagnostics")
        except (OSError, ValueError, KeyError, TypeError) as error:
            return ExecutionResult.backend_error(self.name, f"invalid domain diagnostics: {error}")
        metadata["domain_diagnostics"] = diagnostics
        trace = Trace(times=times, columns=columns) if times else None
        if done.returncode == 0:
            if trace is None:
                return ExecutionResult.backend_error(self.name, "successful command produced no observations")
            stream.add(SimulationEnd(completed=True))
            return ExecutionResult(backend=self.name, status=ExecutionStatus.SUCCESS,
                                   phase=ExecutionPhase.FINALIZATION, observations=stream,
                                   trace=trace, backend_metadata=metadata)
        # Only a native execution-failure envelope proves execution began.
        # Staleness/unsupported/CLI refusals are tool coverage, not model bugs.
        if '"kind":"execution-failure"' not in text:
            return ExecutionResult.backend_error(self.name, text)
        failure = ExecutionFailure(kind=classify(text), phase=ExecutionPhase.SIMULATION,
                                   message=text[-200:], raw=text)
        stream.add(SolverFailure(kind=failure.kind, reason=failure.message, raw=text))
        stream.add(SimulationEnd(completed=False, message=failure.message))
        return ExecutionResult(backend=self.name, status=ExecutionStatus.FAILED,
                               observations=stream, failure=failure, trace=trace,
                               backend_metadata=metadata)
