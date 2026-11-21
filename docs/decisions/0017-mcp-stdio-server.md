# ADR 0017: MCP server over stdio

- **Status:** Accepted
- **Date:** 2026-11-21
- **Decision owners:** Project maintainer

## Context

Phase 12.C adds assistant access to UMMP's existing safe application workflows. MCP must not introduce a second business-logic layer, weaken path checks, bypass approval gates, expose credentials, or run arbitrary shell commands.

## Decision

Implement the MCP adapter in `server/src/mcp.rs`, reusing the existing `ummp-server` crate and its Tokio runtime. Use the official Rust MCP SDK `rmcp 3.10.0` over stdio. The SDK declares Rust 1.88 as its MSRV, which is compatible with the project's pinned Rust 1.97.0.

Expose only these tools: `ummp_analyze`, `ummp_scan`, `ummp_plan`, `ummp_port`, `ummp_build`, `ummp_job`, `ummp_cancel`, and `ummp_rollback`. Also expose the `ummp://report/{job}` resource.

Every handler delegates to `AppServices`. A port call with `approve=false` is read-only; `approve=true` starts the existing port job. Rollback requires `confirm=true`. GitHub build is preferred; local build is permitted only after the service risk check returns Low. Paths use the same canonical allowed-roots validation as REST.

MCP does not expose AI calls, credentials, arbitrary shell execution, or HTTP transport. Stdout is protocol-only; logs go to stderr.

## Consequences

- MCP shares the REST crate's path policy and dependency boundary.
- The official SDK adds dependencies, but avoids a hand-maintained JSON-RPC implementation and is compatible with the pinned compiler.
- All existing application approval, checkpoint, rollback, and build-risk rules remain authoritative.
- MCP is a local child process intended to be launched by an MCP client.
