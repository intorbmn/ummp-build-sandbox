# ADR 0016: Local REST API and async boundary

**Status:** Accepted  
**Date:** 2026-11-21  
**Scope:** Phase 12.B

## Context

UMMP needs a local HTTP interface for scripts and external tools. The existing \`core/src/app/AppServices\` facade is synchronous and is already used by the desktop and CLI. A network adapter must not introduce a second business-logic layer or make core asynchronous.

## Decision

- Add \`server/\` as the \`ummp-server\` workspace crate. It is the only Phase 12 crate that depends on Tokio and Axum.
- Keep one installed executable: \`ummp serve\` is a CLI subcommand that calls the server library. The CLI does not depend directly on Tokio.
- Run every synchronous \`AppServices\` operation through \`tokio::task::spawn_blocking\`.
- Bind to \`127.0.0.1\` by default. A non-loopback address requires \`--i-understand-remote\` in addition to an explicit \`--listen\` value.
- Generate a bearer token once and store it in \`<data_dir>/server-token\`. Create it with Unix mode \`0600\`; compare SHA-256 digests using constant-time equality. Print the token once at startup, never log it or return it from an API route.
- Do not add a token-rotation command in 12.B. To rotate, stop the server, remove \`server-token\`, and restart.
- Read \`allowed_roots\` from \`ummp.toml\`; default to the current user's home directory. Canonicalize both roots and requested paths, resolve symlinks, compare Windows paths case-insensitively, and reject paths outside the roots. Continue using \`resolve_within\` for safe relative joins.
- Retain finished-job queryability for as long as the existing \`jobs.db\` retains the job. Do not add a second retention store.
- Keep request bodies at or below 1 MiB and CORS disabled.
- Commit \`docs/openapi.json\` as the source of truth and test that it describes every registered route. This avoids a runtime OpenAPI dependency.
- Keep the API surface free of AI credential routes.

## Consequences

The desktop, CLI and REST adapter share one application-service contract. The server can stream the existing job event payloads without changing core. Localhost binding, token authentication, path allow-listing and bounded request bodies reduce exposure, but this is not a hosted or multi-user service. Remote binding is an explicit user risk decision.

## Alternatives rejected

- Tokio in \`core\`: rejected because it would contaminate the synchronous engine and every consumer.
- A separate \`ummp-server\` executable: rejected because it would require users to install and maintain a second binary.
- Dynamic OpenAPI generation: rejected for this milestone because a checked-in document plus route-set test is smaller and has no runtime macro dependency.
- Token rotation CLI: deferred; deleting the token file is sufficient for the local single-user use case.
