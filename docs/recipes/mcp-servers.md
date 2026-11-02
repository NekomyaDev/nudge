# Model Context Protocol (MCP) Server Configuration Recipes

Nudge has first-class integration with the Model Context Protocol (MCP). Agents can consume tools provided by local stdio processes or remote HTTP/SSE endpoints.

## Configuring MCP Servers in `nudge.toml`

Declare your MCP servers under `[mcp.servers.<name>]`:

```toml
# nudge.toml

# 1. Local Filesystem MCP Server (Node/npx)
[mcp.servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "./workspace"]

# 2. PostgreSQL Database MCP Server
[mcp.servers.postgres]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-postgres", "postgresql://user:pass@localhost:5432/mydb"]

# 3. Brave Search MCP Server (Environment Secret Passing)
[mcp.servers.brave_search]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-brave-search"]
[mcp.servers.brave_search.env]
BRAVE_API_KEY = "env:BRAVE_API_KEY"

# 4. Custom Python MCP Tool Server
[mcp.servers.custom_tools]
command = "python"
args = ["-m", "my_mcp_server"]
```

## Consuming MCP Tools in Nudge (`.ndg`)

Declare external tools using the `tool` keyword backed by `mcp("<server_name>")`:

```nudge
// Import tools from declared MCP server
tool read_file(path: string) -> string {
    impl: mcp("filesystem").read_file(path = path)
    side_effects: none
}

tool search_web(query: string) -> string {
    impl: mcp("brave_search").brave_web_search(query = query)
    side_effects: none
}

fn inspect_project(file_path: string) -> string uses Tool, IO {
    let content = read_file(file_path)
    println("Read content: " + content)
    content
}
```

## Running with Offline Replay

When running in offline mode (`--replay traces/run.jsonl`), all MCP tool calls return their recorded outputs without spawning child processes.
