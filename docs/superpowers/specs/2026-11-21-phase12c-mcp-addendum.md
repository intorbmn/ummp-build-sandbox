# Phase 12.C — MCP implementation decisions

**Date:** 2026-11-21  
**Branch:** `phase-12c-mcp`  
**Authority:** `docs/guides/phase-12-api.md` §7 and §9a; approved Phase 12 design.

## Decisions

- **Placement:** implement MCP in `server/src/mcp.rs`. This shares the existing REST crate, `AppServices` boundary, allowed-roots policy and Tokio runtime without adding another workspace crate.
- **SDK:** use the official `rmcp = 3.10.0` SDK with stdio transport. Its published MSRV is Rust 1.88, compatible with this repository's pinned Rust 1.97.0. Enable only server/macros/stdio features needed by the implementation.
- **Transport:** stdio only. Stdout is reserved for MCP protocol frames; diagnostics go to stderr.
- **Tools:** expose analyze, scan, plan, port, build, job, cancel and rollback. Each description states its write/execution behavior.
- **Approval:** `ummp_port(approve=false)` is plan-only; `approve=true` starts the existing `AppServices::start_port` job. Rollback requires `confirm=true`.
- **Build:** GitHub backend is the default. Local execution is allowed only when `AppServices::check_build_risk` reports Low.
- **Resources:** expose `ummp://report/{job}`; resolve only job IDs and report data already held by `AppServices`.
- **Security:** no shell execution, AI provider calls, credential access, or MCP-over-HTTP. Every filesystem path uses the exact canonical allowed-roots policy shared with REST. All writes go through `AppServices`.

## Verification plan

- JSON-RPC pipe test: initialize, tools/list, plan, unapproved port (no writes), approved port (job created).
- Reject a path outside configured roots.
- Reject rollback without explicit confirmation.
- Save a synthetic credential and verify no tool/resource response contains it.
- Verify stdio stdout contains protocol messages only.
