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


if __name__ == "__main__":
    unittest.main()
