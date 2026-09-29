"""Rumoca backend options: holding unconnected inputs, and the simulation window."""
import pytest

from modelsan.backends.rumoca import RumocaBackend, held_inputs


def test_held_inputs_are_read_back_from_what_rumoca_reported():
    stderr = ("reading x.rbc\n"
              "holding 2 free input(s) constant at their start values: a.b[1]=0, c=2.5\n")
    assert held_inputs(stderr) == {"a.b[1]": 0.0, "c": 2.5}


def test_a_run_that_held_nothing_reports_nothing_held():
    assert held_inputs("Simulation complete: 501 time points\n") == {}
    assert held_inputs(None) == {}


def test_free_inputs_accepts_only_what_rumoca_implements():
    assert RumocaBackend("rumoca", free_inputs="start").free_inputs == "start"
    assert RumocaBackend("rumoca").free_inputs is None
    with pytest.raises(ValueError):
        RumocaBackend("rumoca", free_inputs="zero")


def test_simulation_window_starts_where_the_model_says():
    backend = RumocaBackend("rumoca", t_start=0.01, t_end=3.0)
    assert (backend.t_start, backend.t_end) == (0.01, 3.0)
    assert RumocaBackend("rumoca").t_start == 0.0
    for start, end in ((1.0, 1.0), (2.0, 1.0), (float("nan"), 1.0)):
        with pytest.raises(ValueError):
            RumocaBackend("rumoca", t_start=start, t_end=end)
