# 33/60 - Streaming responses and token chunks

Prefix an LLM binding with `stream let` when downstream code should consume
the response incrementally. The compiler records that the call is streamed;
the runtime can lower it to token-chunk processing rather than waiting for a
complete response.

Streaming improves perceived latency for long text, but it does not remove
budgeting or output validation. Keep the same explicit prompt contract.

Try:
  nudgec check streaming.ndg
  nudgec build streaming.ndg
