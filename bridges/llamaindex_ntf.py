#!/usr/bin/env python3
"""LlamaIndex → NTF bridge adapter (frozen v1).

Converts LlamaIndex query, retrieval, and synthesis events into NTF trace records.
"""
import json
import os
import sys

def event_to_ntf(event_type, payload, seq=1, error=None):
    """Convert a LlamaIndex event into an NTF record."""
    event_str = str(event_type).lower()
    if "retrieve" in event_str or "retrieval" in event_str:
        kind = "tool.call"
        tool_name = "llamaindex:retriever"
    elif "query" in event_str:
        kind = "fn.return"
        tool_name = "llamaindex:query"
    else:
        kind = "llm.call"
        tool_name = "llamaindex:synthesize"

    record = {
        "v": 1,
        "seq": seq,
        "kind": kind,
        "provider": "llamaindex",
        "outcome": "error" if error else "ok",
    }
    if kind == "tool.call":
        record["tool"] = tool_name
        record["input"] = str(payload.get("query", ""))
        record["output"] = payload.get("nodes", payload.get("output", ""))
    elif kind == "fn.return":
        record["fn"] = tool_name
        record["output"] = payload.get("response", payload.get("output", ""))
    else:
        record["model"] = "llamaindex:llm"
        record["params"] = {"temperature": 0}
        record["input"] = str(payload.get("prompt", payload.get("query", "")))
        record["output"] = str(payload.get("response", payload.get("output", "")))
        record["tokens"] = {"in": 0, "out": 0}
        record["cost_usd"] = 0.0

    if error:
        record["error"] = str(error)
    return record


class LlamaIndexNTFHandler:
    """LlamaIndex callback handler that appends NTF records to a JSONL file."""

    def __init__(self, path):
        self.path = path
        self._seq = 0

    def _next_seq(self):
        self._seq += 1
        return self._seq

    def on_event_end(self, event_type, payload=None, **kwargs):
        payload = payload or {}
        seq = self._next_seq()
        rec = event_to_ntf(event_type, payload, seq=seq)
        with open(self.path, "a", encoding="utf-8") as f:
            f.write(json.dumps(rec, ensure_ascii=False) + "\n")
        return rec
