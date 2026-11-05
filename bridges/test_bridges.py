#!/usr/bin/env python3
"""Unit tests for NTF bridge converters."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from bridges.crewai_ntf import CrewAINTFTracer, task_to_ntf, tool_use_to_ntf
from bridges.langchain_ntf import NTFTracer, llm_result_to_ntf
from bridges.langgraph_ntf import LangGraphNTFTracer, step_to_ntf


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

    def test_langgraph_tracer(self):
        with tempfile.NamedTemporaryFile("w+", suffix=".jsonl") as f:
            tracer = LangGraphNTFTracer(f.name)
            tracer.on_chain_start({"name": "planner"}, {"goal": "analyze"})
            rec1 = tracer.on_chain_end({"steps": ["step1", "step2"]})
            self.assertEqual(rec1["kind"], "fn.return")
            self.assertEqual(rec1["fn"], "planner")
            self.assertEqual(rec1["outcome"], "ok")

            rec2 = tracer.on_tool_end("docs found", name="search_tool", input="search query")
            self.assertEqual(rec2["kind"], "tool.call")
            self.assertEqual(rec2["tool"], "search_tool")

            tracer.on_chain_start({"name": "executor"}, {})
            rec3 = tracer.on_chain_error(RuntimeError("step timeout"))
            self.assertEqual(rec3["outcome"], "error")
            self.assertEqual(rec3["error"], "step timeout")

            f.seek(0)
            lines = [json.loads(line) for line in f.readlines()]
            self.assertEqual(len(lines), 3)

    def test_crewai_tracer(self):
        with tempfile.NamedTemporaryFile("w+", suffix=".jsonl") as f:
            tracer = CrewAINTFTracer(f.name)
            rec1 = tracer.on_task_completed("researcher", "find articles", {"count": 3})
            self.assertEqual(rec1["kind"], "llm.call")
            self.assertEqual(rec1["agent"], "researcher")
            self.assertEqual(rec1["outcome"], "ok")

            rec2 = tracer.on_tool_use("fetch_page", "https://example.com", "page content")
            self.assertEqual(rec2["kind"], "tool.call")
            self.assertEqual(rec2["tool"], "fetch_page")

            rec3 = tracer.on_task_error("writer", "draft summary", RuntimeError("quota exceeded"))
            self.assertEqual(rec3["outcome"], "error")
            self.assertIn("quota exceeded", rec3["error"])

            f.seek(0)
            lines = [json.loads(line) for line in f.readlines()]
            self.assertEqual(len(lines), 3)


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

    def test_cu_bridge_scenario_traversal_rejected(self):
        import os
        old = os.environ.get("CU_BRIDGE_SCENARIO")
        os.environ["CU_BRIDGE_SCENARIO"] = "../../../etc/passwd"
        try:
            with self.assertRaises(ValueError):
                self.bridge._load_scenes()
        finally:
            if old is None:
                os.environ.pop("CU_BRIDGE_SCENARIO", None)
            else:
                os.environ["CU_BRIDGE_SCENARIO"] = old

    def test_cu_bridge_lock_traversal_rejected(self):
        import os
        old = os.environ.get("CU_BRIDGE_LOCK")
        os.environ["CU_BRIDGE_LOCK"] = "../../../etc/passwd"
        try:
            with self.assertRaises(ValueError):
                self.bridge.Lease()
        finally:
            if old is None:
                os.environ.pop("CU_BRIDGE_LOCK", None)
            else:
                os.environ["CU_BRIDGE_LOCK"] = old


    def test_llamaindex_ntf_event_adapter(self):
        from bridges.llamaindex_ntf import event_to_ntf, LlamaIndexNTFHandler
        rec1 = event_to_ntf("retrieve", {"query": "how to build agent", "nodes": ["node1", "node2"]}, seq=1)
        self.assertEqual(rec1["kind"], "tool.call")
        self.assertEqual(rec1["tool"], "llamaindex:retriever")
        self.assertEqual(rec1["input"], "how to build agent")

        rec2 = event_to_ntf("query", {"response": "agent answer"}, seq=2)
        self.assertEqual(rec2["kind"], "fn.return")
        self.assertEqual(rec2["fn"], "llamaindex:query")

        with tempfile.NamedTemporaryFile(suffix=".jsonl", delete=False) as tf:
            tf_path = tf.name
        try:
            handler = LlamaIndexNTFHandler(tf_path)
            handler.on_event_end("synthesize", {"response": "final synthesis"})
            with open(tf_path, "r", encoding="utf-8") as f:
                lines = [json.loads(line) for line in f if line.strip()]
            self.assertEqual(len(lines), 1)
            self.assertEqual(lines[0]["kind"], "llm.call")
        finally:
            if os.path.exists(tf_path):
                os.unlink(tf_path)

    def test_langchain_streaming_tokens(self):
        from bridges.langchain_ntf import NTFTracer
        with tempfile.NamedTemporaryFile(suffix=".jsonl", delete=False) as tf:
            tf_path = tf.name
        try:
            tracer = NTFTracer(tf_path)
            tracer.on_llm_start({}, ["What is 2+2?"], run_id="stream_run_1")
            tracer.on_llm_new_token("4", run_id="stream_run_1")
            tracer.on_llm_new_token(" is the answer", run_id="stream_run_1")
            tracer.on_llm_end(None, run_id="stream_run_1")

            with open(tf_path, "r", encoding="utf-8") as f:
                lines = [json.loads(line) for line in f if line.strip()]
            self.assertEqual(len(lines), 1)
            self.assertEqual(lines[0]["output"], "4 is the answer")
            self.assertTrue(lines[0].get("streamed"))
        finally:
            if os.path.exists(tf_path):
                os.unlink(tf_path)

    def test_crewai_tool_metadata(self):
        from bridges.crewai_ntf import tool_use_to_ntf, CrewAINTFTracer
        rec = tool_use_to_ntf(
            1, "search_web", "python 3.12", "results",
            agent_role="researcher", task_description="investigate features"
        )
        self.assertEqual(rec["metadata"]["agent_role"], "researcher")
        self.assertEqual(rec["metadata"]["task_description"], "investigate features")

        with tempfile.NamedTemporaryFile(suffix=".jsonl", delete=False) as tf:
            tf_path = tf.name
        try:
            tracer = CrewAINTFTracer(tf_path)
            tracer.on_tool_use("calc", "1+1", "2", agent_role="analyst", task_description="do math")
            with open(tf_path, "r", encoding="utf-8") as f:
                lines = [json.loads(line) for line in f if line.strip()]
            self.assertEqual(lines[0]["metadata"]["agent_role"], "analyst")
        finally:
            if os.path.exists(tf_path):
                os.unlink(tf_path)

    def test_langgraph_bidirectional_conversion(self):
        from bridges.langgraph_ntf import ntf_to_langgraph_events
        records = [
            {"kind": "fn.return", "seq": 1, "fn": "plan", "output": {"steps": [1, 2]}},
            {"kind": "tool.call", "seq": 2, "tool": "search", "input": "query", "output": "answer"},
            {"kind": "llm.call", "seq": 3, "model": "gpt-4o", "input": "prompt", "output": "reply"},
        ]
        events = ntf_to_langgraph_events(records)
        self.assertEqual(len(events), 3)
        self.assertEqual(events[0]["event"], "on_chain_end")
        self.assertEqual(events[0]["name"], "plan")
        self.assertEqual(events[1]["event"], "on_tool_end")
        self.assertEqual(events[2]["event"], "on_llm_end")

    def test_stream_events_async_generator(self):
        import asyncio
        from nudge_runtime import stream_events
        async def run_test():
            items = ['{"kind": "llm.call", "seq": 1}', '{"kind": "tool.call", "seq": 2}']
            results = []
            async for ev in stream_events(items):
                results.append(ev)
            return results
        res = asyncio.run(run_test())
        self.assertEqual(len(res), 2)
        self.assertEqual(res[0]["kind"], "llm.call")
        self.assertEqual(res[1]["kind"], "tool.call")


if __name__ == "__main__":
    unittest.main()
