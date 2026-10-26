# 52/60 - Publish functions through MCP

`nudgec mcp program.ndg` exposes selected Nudge functions as Model Context
Protocol tools over stdio. Define a `tool` declaration for external MCP calls,
or expose ordinary typed functions with `--fns name1,name2`. Types become the
tool input schema and function results become structured tool output.

Try:
  nudgec check mcp_tools.ndg
  nudgec mcp mcp_tools.ndg --fns add
