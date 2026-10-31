"""Offline checks for the Phase 0 spike harness (no model calls).

  python3 -m unittest prototypes/phase0-spike/test_spike.py
"""
import json
import os
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from aiq_spike import generate, group, heldout  # noqa: E402
from aiq_spike.judge import signals  # noqa: E402
from aiq_spike.normalize import load_turns  # noqa: E402
import run_spike  # noqa: E402


def _corpus(d, scenarios=None, plan=None):
    generate.main(d, scenarios=scenarios, plan=plan)
    turns = load_turns(generate.REPO, os.path.join(d, "claude_home"), os.path.join(d, "copilot_home"))
    labels = json.load(open(os.path.join(d, "labels.json"), encoding="utf-8"))
    return turns, labels


class SpikeHarness(unittest.TestCase):
    def test_reader_recovers_every_labelled_turn_on_both_surfaces(self):
        with tempfile.TemporaryDirectory() as d:
            turns, labels = _corpus(d)
            self.assertEqual({t["key"] for t in turns}, set(labels["turns"]))
            self.assertEqual({t["surface"] for t in turns}, {"claude", "copilot"})

    def test_heldout_set_renders_and_reads(self):
        lib = heldout.to_library(os.path.join(HERE, "data", "heldout_scenarios.json"))
        with tempfile.TemporaryDirectory() as d:
            turns, labels = _corpus(d, scenarios=lib, plan=heldout.plan_for(lib))
            self.assertEqual(len(turns), len(labels["turns"]))
        self.assertIn("c01", generate.SCENARIOS)  # the dev library is restored after a held-out render

    def test_grouping_metrics_perfect_and_merged(self):
        gold = {"a": "t1", "b": "t1", "c": "t2"}
        self.assertEqual(run_spike.grouping_metrics(gold, gold)["bcubed_f1"], 1.0)
        merged = run_spike.grouping_metrics({"a": "x", "b": "x", "c": "x"}, gold)
        self.assertEqual(merged["gold_tasks_merged"], ["t1", "t2"])

    def test_signals_mode_carries_no_prompt_text(self):
        with tempfile.TemporaryDirectory() as d:
            turns, _ = _corpus(d)
            blob = json.dumps(signals(turns[:3]))
            self.assertNotIn(turns[0]["prompt"][:30], blob)

    def test_heuristic_beats_gap_baseline_on_dev(self):
        with tempfile.TemporaryDirectory() as d:
            turns, labels = _corpus(d)
            gold = labels["turns"]
            self.assertGreater(run_spike.grouping_metrics(group.heuristic(turns), gold)["bcubed_f1"], 0.9)


if __name__ == "__main__":
    unittest.main()
