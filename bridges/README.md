# Community bridges

Bridges convert runs from other agent frameworks into frozen-v1 NTF traces, so `nudgec trace-check`, `trace-diff`, `policy-sweep` and `trace-view` work on them with no Nudge program involved.

| Bridge | Framework | Status |
|:---|:---|:---|
| [langchain_ntf.py](langchain_ntf.py) | LangChain | ✅ working (point conversion + callback tracer) |
| [langgraph_ntf.py](langgraph_ntf.py) | LangGraph | 🧪 pure helpers tested; callback tracer wanted — [#69](https://github.com/NekomyaDev/nudge/issues/69) |
| [crewai_ntf.py](crewai_ntf.py) | CrewAI | 🧪 pure helpers tested; hook integration wanted — [#69](https://github.com/NekomyaDev/nudge/issues/69) |

Building one of these is a great first contribution: the reference implementation is `langchain_ntf.py` (~150 lines, stdlib only), and the test suite runs the pure helpers without the framework installed.
