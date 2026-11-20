# Phase 12 — CLI, API & External Integration (DRAFT)

## Intent
Make UMMP usable from other tools: a stable CLI contract, a local REST API, an MCP server so
AI assistants (Claude Code, Claude Desktop) can drive it, and a GitHub Action for CI ports.

## Parts
- **CLI:** freeze command names/flags, `--json` everywhere, documented exit codes
  (0 ok, 1 failure, 2 high risk, 3 usage). Shell completions. Man page/markdown docs
  generated from clap.
- **REST API (`ummp serve`):** localhost-only by default, token auth, OpenAPI spec. Endpoints
  mirror application services: projects, analyze, scan, migrate (dry-run/apply), rollback,
  build, jobs (status, logs, cancel), reports. Long work = job resources.
- **MCP server (`ummp mcp`):** stdio transport; tools `analyze`, `scan`, `plan_migration`,
  `apply_migration` (requires explicit approval argument), `build`, `rollback`; resources for
  reports. Writes still go through checkpoints; the MCP client never gets raw file writes.
- **GitHub Action:** `intorbmn/ummp-action` — runs `ummp migrate` + `ummp build` in a mod's
  CI and opens a PR with the diff and report.
- **Remote execution:** reuse Phase 8's GitHub backend; anything beyond that only if needed.
- **External plugins:** versioned plugin interface (process-based, JSON over stdio first —
  safer than dynamic libraries), consumed by Phase 14.

## Deliverables mapping
CLI docs, OpenAPI spec, MCP tool list + examples, integration examples (Action workflow,
MCP config for Claude Desktop).

## Open questions
- Is the REST API needed if the MCP server and CLI exist? (Maybe only for the UI.)
- Plugin transport: stdio JSON-RPC vs. WASM components.

## Risks
API stability promises; the MCP server must not become a way around the approval gate.
