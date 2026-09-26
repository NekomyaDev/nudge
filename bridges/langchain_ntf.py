#!/usr/bin/env python3
"""LangChain → NTF bridge (frozen v1).

Converts LangChain LLM results into NTF `llm.call` records so runs from
other frameworks can replay, diff and audit through the nudge tooling
(`nudgec trace-check`, `trace-diff`, `trace-view`).

Two ways to use it:

1. Point conversion — you already have the pieces (prompt, completion,
   model, token usage)::

       from langchain_ntf import llm_result_to_ntf
       record = llm_result_to_ntf(prompt="hi", output="hello",
                                  model="gpt-4.5-mini", provider="openai")
       print(json.dumps(record))

2. Callback tracing — records stream into a file as your chain runs::

       from langchain_ntf import NTFTracer
       tracer = NTFTracer("run.jsonl")
       result = chain.invoke(inputs, config={"callbacks": [tracer]})

Only the stdlib is required here; langchain is imported lazily and only
by `NTFTracer`. Field names follow docs/ntf-spec.md (frozen v1); unknown
LangChain token fields are skipped rather than guessed.
"""
import json
import os
import time


def llm_result_to_ntf(prompt, output, model, provider="langchain",
                      tokens=None, cost_usd=0.0, repair_round=0,
                      outcome="ok", params=None):
    """Build one NTF `llm.call` record from a completed LLM call."""
    return {
        "v": 1,
        "kind": "llm.call",
        "model": str(model),
        "params": params or {"temperature": 0},
        "input": str(prompt),
        "output": output if isinstance(output, (str, int, float, list, dict))
        else str(output),
        "tokens": tokens or {"in": len(str(prompt).split()),
                             "out": len(str(output).split())},
        "cost_usd": cost_usd,
        "repair_round": repair_round,
        "outcome": outcome,
        "provider": provider,
    }


class NTFTracer:
    """LangChain callback handler that appends NTF records to a JSONL file.

    Requires `langchain-core` (only inside on_llm_end); install it in the
    environment that runs the chain, not here.
    """

    def __init__(self, path, provider="langchain"):
        self.path = path
        self.provider = provider
        self._seq = 0
        self._pending = {}

    def _next_seq(self):
        self._seq += 1
        return self._seq

    # -- langchain-core callback protocol (methods duck-typed) ------------
    def on_llm_start(self, serialized, prompts, **kwargs):
        run_id = kwargs.get("run_id") or id(prompts)
        self._pending[run_id] = {"input": "\n".join(prompts)}

    def on_llm_end(self, response, **kwargs):
        from langchain_core.messages import AIMessage  # lazy import

        run_id = kwargs.get("run_id") or 0
        pending = self._pending.pop(run_id, {})
        output = getattr(response, "generations", None) or []
        text = ""
        if output and output[0]:
            first = output[0][0]
            text = first.text if hasattr(first, "text") else str(first)
        usage = getattr(response, "llm_output", None) or {}
        token_usage = usage.get("token_usage") or {}
        tokens = None
        if token_usage:
            tokens = {"in": token_usage.get("prompt_tokens", 0),
                      "out": token_usage.get("completion_tokens", 0)}
        model = (usage.get("model_name")
                 or (serialized or {}).get("id", ["?"])[-1])
        record = llm_result_to_ntf(
            prompt=pending.get("input", ""),
            output=text if isinstance(text, str) else _decode(text),
            model=model,
            provider=self.provider,
            tokens=tokens,
        )
        if isinstance(output and output[0] and output[0][0], AIMessage):
            record["output"] = text
        record["seq"] = self._next_seq()
        with open(self.path, "a", encoding="utf-8") as f:
            f.write(json.dumps(record) + "\n")

    def on_llm_error(self, error, **kwargs):
        run_id = kwargs.get("run_id") or 0
        pending = self._pending.pop(run_id, {})
        record = llm_result_to_ntf(prompt=pending.get("input", ""),
                                   output="", model="unknown",
                                   provider=self.provider, outcome="error")
        record["seq"] = self._next_seq()
        with open(self.path, "a", encoding="utf-8") as f:
            f.write(json.dumps(record) + "\n")


def _decode(value):
    """Best-effort JSON decode of a model output, per the spec's
    string-or-object `output` rule."""
    try:
        return json.loads(value)
    except (TypeError, ValueError):
        return str(value)


if __name__ == "__main__":
    import argparse

    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out", default=f"ntf-{int(time.time())}.jsonl")
    ap.add_argument("--demo", action="store_true",
                    help="write one synthetic record and validate the file shape")
    args = ap.parse_args()
    if args.demo:
        rec = llm_result_to_ntf("Say hi", {"answer": "hi"},
                                model="demo", provider="fake")
        rec["seq"] = 1
        with open(args.out, "w") as f:
            f.write(json.dumps(rec) + "\n")
        print(f"wrote {args.out} — check with: nudgec trace-check {args.out}")
