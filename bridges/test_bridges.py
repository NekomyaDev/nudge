#!/usr/bin/env python3
"""Unit tests for NTF bridge converters."""

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from bridges.crewai_ntf import task_to_ntf, tool_use_to_ntf
from bridges.langchain_ntf import NTFTracer, llm_result_to_ntf
from bridges.langgraph_ntf import step_to_ntf


class TestBridges(unittest.TestCase):
    def test_crewai_task_and_tool(self):
        task_rec = task_to_ntf(1, "researcher", "find docs", {"found": 2})
        self.assertEqual(task_rec["output"], {"found": 2})
        self.assertEqual(task_rec["outcome"], "ok")

        tool_rec = tool_use_to_ntf(2, "web_search", "query", "results")
        self.assertEqual(tool_rec["tool"], "web_search")

        task_err = task_to_ntf(3, "writer", "draft", "", error="rate limit")
        self.assertEqual(task_err["outcome"], "error")
        self.assertEqual(task_err["error"], "rate limit")

    def test_langgraph_step(self):
        step_rec = step_to_ntf("plan", {"state": "ready"}, 1)
        self.assertEqual(step_rec["kind"], "fn.return")
        self.assertEqual(step_rec["output"], {"state": "ready"})

    def test_langchain_error(self):
        rec = llm_result_to_ntf("prompt", "", "gpt-4", outcome="error")
        self.assertEqual(rec["outcome"], "error")

    def test_tracer_error(self):
        with tempfile.NamedTemporaryFile("w+", suffix=".jsonl") as f:
            tracer = NTFTracer(f.name)
            tracer.on_llm_error(RuntimeError("connection dropped"))
            f.seek(0)
            line = f.readline()
            data = json.loads(line)
            self.assertEqual(data["outcome"], "error")
            self.assertEqual(data["error"], "connection dropped")


class ComputerBridgeContractTests(unittest.TestCase):
    """The bridge wire contract is a security boundary — these cover the
    fail-closed edge cases (state_id required, geometry required for
    coordinate targets, honest dispatch receipts after partial effects)."""

    @classmethod
    def setUpClass(cls):
        import importlib.util

        spec = importlib.util.spec_from_file_location(
            "cu_bridge",
            Path(__file__).resolve().parent.parent / "tools" / "cu_bridge.py",
        )
        cls.bridge = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.bridge)

    def test_action_without_state_id_is_refused(self):
        self.bridge._LAST_OBS["state_id"] = "b-7"
        req = {"op": "click", "target": {"index": 1}}  # no state_id
        reason = self.bridge._stale_reason(req)
        self.assertIn("invalid_request", reason)
        req = {"op": "click", "state_id": "", "target": {"index": 1}}
        reason = self.bridge._stale_reason(req)
        self.assertIn("invalid_request", reason)
        # a mismatched state_id stays stale_state
        req = {"op": "click", "state_id": "b-1", "target": {"index": 1}}
        reason = self.bridge._stale_reason(req)
        self.assertIn("stale_state", reason)

    def test_coordinate_target_without_geometry_is_refused(self):
        # inability to PROVE containment is never permission (fail closed)
        self.bridge._LAST_OBS["window"] = None
        with self.assertRaises(RuntimeError) as cm:
            self.bridge._center({"x": 100, "y": 100})
        self.assertIn("fail-closed", str(cm.exception))

    def test_partial_scroll_is_never_not_sent(self):
        calls = []
        def fake_sh(argv, stdin=None):
            calls.append(argv)
            if argv[1] == "click" and len(calls) > 2:
                raise RuntimeError("xdo died mid-scroll")
            return b""
        orig = self.bridge._sh
        self.bridge._sh = fake_sh
        try:
            out = self.bridge._real_act(
                "scroll", {"target": {}, "direction": "down", "pages": 1})
        finally:
            self.bridge._sh = orig
        self.assertNotEqual(out["dispatch"], "not_sent")
        self.assertEqual(out["dispatch"], "unknown")

    def test_drag_after_mousedown_is_unknown_not_sent(self):
        calls = []
        def fake_sh(argv, stdin=None):
            calls.append(argv)
            if argv[1] == "mousemove" and len(calls) > 1:
                raise RuntimeError("move failed")
            return b""
        orig = self.bridge._sh
        self.bridge._sh = fake_sh
        try:
            self.bridge._REAL_ELS.clear()
            self.bridge._REAL_ELS[1] = (object(), {"bounds": [0, 0, 10, 10]})
            out = self.bridge._real_act(
                "drag", {"target": {"index": 1}, "to": {"index": 1}})
        finally:
            self.bridge._sh = orig
        self.assertEqual(out["dispatch"], "unknown")
        # the drag must still release the button
        self.assertTrue(any(a[1] == "mouseup" for a in calls if len(a) > 1))

    def test_set_value_without_editable_is_not_actionable(self):
        class NoEditable:
            def queryEditableText(self):
                raise RuntimeError("no editable text interface")
        self.bridge._REAL_ELS.clear()
        self.bridge._REAL_ELS[3] = (NoEditable(), {})
        out = self.bridge._real_act("set_value", {"target": {"index": 3}, "value": "x"})
        self.assertFalse(out["ok"])
        self.assertEqual(out["outcome"], "not_actionable")
        self.assertEqual(out["dispatch"], "not_sent")


if __name__ == "__main__":
    unittest.main()
