# MCP server

UMMP exposes a local Model Context Protocol server over **stdio**. It is launched by the CLI and intended to be started as a child process by Claude Desktop, Claude Code or another MCP client.

## Start

Build UMMP, then configure your MCP client to launch the executable:

```json
{
  "mcpServers": {
    "ummp": {
      "command": "ummp",
      "args": ["mcp"]
    }
  }
}
```

If the executable is not on the client's PATH, use its absolute path. The server reads the same `ummp.toml` configuration as the CLI. Set `allowed_roots` to the project folders the assistant may access:

```toml
allowed_roots = ["D:/Minecraft/mods", "D:/Minecraft/work"]
```

When omitted, the allowed root defaults to the current user's home directory. Every supplied path is canonicalized and checked against these roots, including symlink and non-existing output path handling. Configure the narrowest roots practical.

## Tools

| Tool | Behavior |
|---|---|
| `ummp_analyze(path)` | Read-only project/JAR analysis |
| `ummp_scan(path)` | Read-only malware-risk scan |
| `ummp_plan(path, to, loader)` | Read-only migration plan with bounded diff previews |
| `ummp_port(path, to, loader, output?, zip?, approve)` | `approve=false` returns a plan only; `approve=true` starts a port job |
| `ummp_build(path, backend)` | Starts a build job; GitHub is default, local requires Low risk |
| `ummp_job(id)` | Reads job state, progress, logs and result |
| `ummp_cancel(id)` | Requests cancellation of a running job |
| `ummp_rollback(project, checkpoint?, confirm)` | Restores a checkpoint only with `confirm=true` |

Long-running operations return a job UUID. Poll with `ummp_job`; read the report resource for the Markdown summary.

## Resource

- URI template: `ummp://report/{job}`
- MIME type: `text/markdown`
- Replace `{job}` with the UUID returned by a job-starting tool.

## Safety and boundaries

- All operations call `AppServices`; MCP contains no migration or build engine logic.
- All paths are restricted to canonical `allowed_roots`.
- Porting is plan-only unless the caller explicitly sends `approve=true`. The MCP client should obtain human approval before doing so.
- Rollback is refused unless `confirm=true`.
- Local builds are allowed only when the application risk check reports Low. GitHub Actions is the preferred backend.
- MCP does not expose AI providers, credentials, arbitrary shell execution, REST authentication tokens, or HTTP transport.
- Stdout is reserved for MCP protocol messages. Diagnostics must go to stderr.
