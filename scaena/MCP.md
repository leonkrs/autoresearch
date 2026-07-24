# Scaena MCP server

Pure-Rust stdio MCP server (`scaena-mcp`), no Node runtime. Exposes: `devices`, `capture`, `seed`,
`snapshot`, `restore`, `flow`. `capture` returns the screenshot inline as an `image/png` content block.

## Register in Claude Code
```
cargo build -p scaena-mcp
claude mcp add scaena "$(pwd)/scaena/target/debug/scaena-mcp"
```
Verify: `claude mcp list` shows `scaena … ✔ Connected`.

## Note on the SDK choice
Implemented as a hand-rolled newline-delimited JSON-RPC 2.0 server (deps: `serde_json` only) rather than
`rmcp`, for autonomous verifiability and a thin dependency tree. It still honours the decision's intent:
pure Rust, single binary, no Node. `rmcp` can be swapped in later if richer protocol features are needed.
