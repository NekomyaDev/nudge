#!/usr/bin/env python3
"""Regression tests for the Python runtime's hardening contracts:

- NUDGE_TOOL_GRANTS fail-closed semantics + least-privilege precedence
- screenshot sidecar path-security (traversal on write and replay read)
- replay request_hash identity (llm / tool / computer) and identity-first
  parallel consumption
- hard-budget fail-closed on unknown-priced real models
- atomic checkpoint writes

Run: python3 -m unittest runtime.test_nudge_runtime -v (from the repo root,
after `cd runtime` for the import to resolve) or directly as a script.
"""

import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import nudge_runtime as rt  # noqa: E402


def env(**kwargs):
    """Set/clear env vars for the duration of the test."""
    old = {k: os.environ.get(k) for k in kwargs}
    for k, v in kwargs.items():
        if v is None:
            os.environ.pop(k, None)
        else:
            os.environ[k] = v
    import contextlib

    return contextlib.ExitStack()


class ToolGrantsTests(unittest.TestCase):
    def setUp(self):
        rt._TOOL_GRANTS_CACHE = rt._TOOL_GRANTS_UNSET

    def tearDown(self):
        os.environ.pop("NUDGE_TOOL_GRANTS", None)
        rt._TOOL_GRANTS_CACHE = rt._TOOL_GRANTS_UNSET

    def test_absent_env_is_unrestricted(self):
        self.assertTrue(rt._tool_allowed("web_search", "untrusted"))

    def test_empty_object_denies_everything(self):
        os.environ["NUDGE_TOOL_GRANTS"] = "{}"
        self.assertFalse(rt._tool_allowed("web_search", None))

    def test_malformed_json_fails_closed(self):
        os.environ["NUDGE_TOOL_GRANTS"] = "not-json{"
        with self.assertRaises(RuntimeError):
            rt._tool_allowed("web_search", None)

    def test_non_object_fails_closed(self):
        os.environ["NUDGE_TOOL_GRANTS"] = '["web_search"]'
        with self.assertRaises(RuntimeError):
            rt._tool_allowed("web_search", None)

    def test_invalid_rule_values_fail_closed(self):
        os.environ["NUDGE_TOOL_GRANTS"] = '{"web_search": "*"}'
        with self.assertRaises(RuntimeError):
            rt._tool_allowed("web_search", None)

    def test_server_restriction_beats_global_rule(self):
        os.environ["NUDGE_TOOL_GRANTS"] = json.dumps(
            {"web_search": ["*"], "untrusted/*": []}
        )
        self.assertFalse(rt._tool_allowed("web_search", "untrusted"))
        self.assertTrue(rt._tool_allowed("web_search", "trusted"))

    def test_server_tool_beats_server_wide(self):
        os.environ["NUDGE_TOOL_GRANTS"] = json.dumps(
            {"kb/*": ["*"], "kb/secret": []}
        )
        self.assertFalse(rt._tool_allowed("secret", "kb"))
        self.assertTrue(rt._tool_allowed("retrieve", "kb"))

    def test_global_star_allows_unknown_tools(self):
        os.environ["NUDGE_TOOL_GRANTS"] = json.dumps({"*": ["*"]})
        self.assertTrue(rt._tool_allowed("anything", None))

    def test_no_matching_key_denies(self):
        os.environ["NUDGE_TOOL_GRANTS"] = json.dumps({"web_search": ["*"]})
        self.assertFalse(rt._tool_allowed("other_tool", None))


class SidecarSecurityTests(unittest.TestCase):
    def _observe_record(self, trace):
        os.environ["NUDGE_TRACE"] = str(trace)
        Path(trace).write_text("", encoding="utf-8")
        obs = rt._fake_observe("FakeApp", True)
        digest = rt._local_screenshot_digest(obs["screenshot"])
        name = rt._write_trace_asset(digest, obs["screenshot"])
        return obs, digest, name

    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.trace = Path(self.dir) / "trace.jsonl"

    def tearDown(self):
        os.environ.pop("NUDGE_TRACE", None)
        os.environ.pop("NUDGE_REPLAY", None)

    def test_asset_name_is_the_local_digest_only(self):
        obs, digest, name = self._observe_record(self.trace)
        self.assertRegex(name, r"^[0-9a-f]{64}\.txt$")
        self.assertEqual(name, digest + ".txt")
        self.assertEqual(
            (Path(self.dir) / "trace.jsonl.assets" / name).read_text(),
            obs["screenshot"],
        )

    def test_provider_traversal_hash_cannot_escape(self):
        # a hostile provider-supplied hash is never the filename authority
        name = rt._write_trace_asset("../../evil", "data:image/png;base64,AAAA")
        self.assertIsNone(name)
        name = rt._write_trace_asset("sha256:" + "a" * 64 + "/../../evil", "x")
        self.assertIsNone(name)
        name = rt._write_trace_asset("C:\\evil", "x")
        self.assertIsNone(name)
        # nothing was written outside the assets dir
        self.assertFalse((Path(self.dir) / "evil.txt").exists())

    def test_crafted_asset_name_cannot_traverse_on_read(self):
        os.environ["NUDGE_REPLAY"] = str(self.trace)
        self.assertEqual(rt._read_trace_asset("../../secret.txt"), "")
        self.assertEqual(rt._read_trace_asset("../../../etc/passwd"), "")
        self.assertEqual(rt._read_trace_asset("C:\\win\\evil.txt"), "")
        self.assertEqual(rt._read_trace_asset("../" + "a" * 64 + ".txt"), "")

    def test_replay_rejects_content_that_fails_the_digest(self):
        obs, digest, name = self._observe_record(self.trace)
        asset = Path(self.dir) / "trace.jsonl.assets" / name
        asset.write_text("data:image/png;base64,TAMPERED", encoding="utf-8")
        os.environ["NUDGE_REPLAY"] = str(self.trace)
        # pixels disagreeing with the recorded digest are not served
        self.assertEqual(rt._read_trace_asset(name, digest), "")

    def test_valid_asset_replay(self):
        obs, digest, name = self._observe_record(self.trace)
        os.environ["NUDGE_REPLAY"] = str(self.trace)
        self.assertEqual(rt._read_trace_asset(name, digest), obs["screenshot"])
        # also valid without the digest check (legacy callers)


class ReplayIdentityTests(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.trace = Path(self.dir) / "trace.jsonl"

    def tearDown(self):
        os.environ.pop("NUDGE_REPLAY", None)
        os.environ.pop("NUDGE_TRACE", None)
        rt._REPLAY_STATE.update(records=None, outputs=None, idx=0, consumed=None)
        rt._DECISION_REPLAY_STATE.update(records=None, answers=None, idx=0, consumed=None)
        rt._REPLAY_TOOL_STATE.update(records=None, idx={}, consumed=None)
        rt._COMPUTER_REPLAY_STATE.update(path=None, obs=[], acts=[], obs_idx=0, act_idx=0)

    def _record(self, record):
        with open(self.trace, "a", encoding="utf-8") as f:
            f.write(json.dumps(record) + "\n")

    def _replay(self):
        os.environ["NUDGE_REPLAY"] = str(self.trace)
        rt._REPLAY_STATE.update(records=None, outputs=None, idx=0, consumed=None)
        rt._DECISION_REPLAY_STATE.update(records=None, answers=None, idx=0, consumed=None)
        rt._REPLAY_TOOL_STATE.update(records=None, idx={}, consumed=None)
        rt._COMPUTER_REPLAY_STATE.update(path=None, obs=[], acts=[], obs_idx=0, act_idx=0)

    def test_llm_changed_prompt_raises(self):
        self._record({
            "kind": "llm.call", "v": 1, "model": "fake",
            "request_hash": rt._llm_request_hash("fake", "prompt one", None, []),
            "output": "answer one", "input": "prompt one",
        })
        self._record({
            "kind": "llm.call", "v": 1, "model": "fake",
            "request_hash": rt._llm_request_hash("fake", "prompt two", None, []),
            "output": "answer two", "input": "prompt two",
        })
        self._replay()
        # matching prompt is served regardless of order
        self.assertEqual(
            rt._replay_take_llm(rt._llm_request_hash("fake", "prompt two", None, [])),
            "answer two",
        )
        # a changed prompt cannot consume the remaining record
        with self.assertRaises(rt.ReplayMismatch):
            rt._replay_take_llm(rt._llm_request_hash("fake", "prompt CHANGED", None, []))

    def test_llm_parallel_out_of_order_identity(self):
        # live: branch 1 finished first (B), branch 0 second (A) — replay
        # must serve each call ITS result, not the next record in order
        hashes = ["prompt A", "prompt B"]
        for prompt in reversed(hashes):
            self._record({
                "kind": "llm.call", "v": 1, "model": "fake",
                "request_hash": rt._llm_request_hash("fake", prompt, None, []),
                "output": f"answer for {prompt[-1]}", "input": prompt,
            })
        self._replay()
        self.assertEqual(
            rt._replay_take_llm(rt._llm_request_hash("fake", "prompt A", None, [])),
            "answer for A",
        )
        self.assertEqual(
            rt._replay_take_llm(rt._llm_request_hash("fake", "prompt B", None, [])),
            "answer for B",
        )

    def test_tool_changed_arguments_raise(self):
        self._record({
            "kind": "tool.call", "v": 1, "tool": "web_search", "server": "kb",
            "request_hash": rt._tool_request_hash("web_search", "kb", {"q": "old"}),
            "input": {"q": "old"}, "output": [{"old": True}],
        })
        self._replay()
        with self.assertRaises(rt.ReplayMismatch):
            rt._replay_tool_take(
                "web_search", "kb", rt._tool_request_hash("web_search", "kb", {"q": "NEW"}))
        # same server + arguments is the same call
        matched, out = rt._replay_tool_take(
            "web_search", "kb", rt._tool_request_hash("web_search", "kb", {"q": "old"}))
        self.assertTrue(matched)
        self.assertEqual(out, [{"old": True}])

    def test_computer_type_payload_mismatch_raises(self):
        rec = {
            "kind": "computer.act", "v": 1, "action": "type", "app": "Notes",
            "target": {"index": -1}, "value": "A",
            "request_hash": rt._computer_request_hash(
                "type", "Notes", {"target": {"index": -1}, "text": "A"}),
            "ok": True, "outcome": "ok", "latency_ms": 1, "action_sent": True,
        }
        self._record(rec)
        self._replay()
        # matching payload replays dry-run (consumes the record)
        result = rt._computer_act(
            "type", "Notes", {"target": {"index": -1}, "text": "A"})
        self.assertTrue(result["ok"])
        self.assertEqual(result["outcome"], "ok")
        # fresh cursor: different text, same app/target — must raise (P0-4)
        self._replay()
        with self.assertRaises(rt.ReplayMismatch) as cm:
            rt._computer_act("type", "Notes", {"target": {"index": -1}, "text": "B"})
        self.assertIn("request", str(cm.exception))


class BudgetWallTests(unittest.TestCase):
    def tearDown(self):
        for k in ("NUDGE_BUDGET", "NUDGE_BUDGET_ALLOW_UNKNOWN", "NUDGE_PROVIDER"):
            os.environ.pop(k, None)

    def test_unknown_price_fails_closed_under_hard_budget(self):
        os.environ["NUDGE_BUDGET"] = "1.00"
        with self.assertRaises(rt.BudgetExceeded):
            rt._call_cost("openai", "no-such-model", 100, 100)

    def test_unknown_price_warns_only_without_hard_budget(self):
        self.assertEqual(rt._call_cost("openai", "no-such-model", 100, 100), 0.0)

    def test_explicit_opt_out(self):
        os.environ["NUDGE_BUDGET"] = "1.00"
        os.environ["NUDGE_BUDGET_ALLOW_UNKNOWN"] = "1"
        self.assertEqual(rt._call_cost("openai", "no-such-model", 100, 100), 0.0)


class CheckpointAtomicTests(unittest.TestCase):
    def tearDown(self):
        os.environ.pop("NUDGE_RUN_ID", None)
        os.environ.pop("NUDGE_RESUME", None)

    def test_checkpoint_write_is_atomic_and_complete(self):
        import shutil

        run = f"test-ckpt-{os.getpid()}"
        os.environ["NUDGE_RUN_ID"] = run
        state = rt.AgentState("t", {"x": 0})
        state.x = 5
        ckpt = Path(".nudge") / "runs" / run / "checkpoint.json"
        data = json.loads(ckpt.read_text(encoding="utf-8"))
        self.assertEqual(data["values"]["x"], 5)
        # the temp file never survives the rename
        self.assertFalse((Path(".nudge") / "runs" / run / ".checkpoint.json.tmp").exists())
        shutil.rmtree(".nudge", ignore_errors=True)


if __name__ == "__main__":
    unittest.main()
