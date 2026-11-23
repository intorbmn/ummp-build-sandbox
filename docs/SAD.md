# SAD — Software Architecture Document

## Architecture style
Modular monolith. Layers: Domain → Application → Infrastructure/Presentation. Core engine has no hard dependency on GUI, AI provider, Minecraft loader, or build tool.

## Layer rules
- **Domain**: pure business logic (project, migration session, compatibility). No GUI, DB, AI, or loader imports.
- **Application**: use cases, orchestration, pipeline. Depends on Domain + interfaces only.
- **Infrastructure**: adapters (loaders, mappings, build, AI providers, DB, sandbox). Implement interfaces.
- **Presentation**: desktop GUI, CLI, REST API. Call application services only.

## Core modules
| Module | Responsibility |
|---|---|
| Project Manager | open/import project, metadata, file system abstraction |
| Migration Session | checkpoint, rollback, job persistence, state machine |
| Event Bus | decoupled communication between pipeline steps |
| Analyzer Engine | detect loader/version/Java/deps/build system |
| Compatibility Engine | mapping DB, breaking changes, recipes, knowledge search |
| Migration Engine | plan + transform source/build/resources/mixins |
| Build Engine | Java/Gradle/Maven runner, launcher, log/crash analysis |
| AI Agent System | provider abstraction, agents, context, cost tracking, approval gate |
| Plugin System | registry, loader, lifecycle, capability discovery, sandbox |

## Desktop presentation (Phase 11)
The Tauri 2 desktop shell in `ui/` is a presentation adapter, not a second application layer. Svelte screens call typed commands through `ui/src/lib/api.ts`; Tauri commands forward to `core/src/app/` and return `UiError` values. Core job events are forwarded to the window as `ummp://event`. GitHub credentials are accessed through the OS `SecretStore`; AI provider keys use the shared interim `AiKeyStore` file implementation (ADR 0015). Neither is serialized into settings or returned raw to the web view. Rust and TypeScript view/event contracts share JSON golden fixtures.

## Plugin boundary (Phase 12.E, ADR 0019)

Plugins are out-of-process executables described by `ummp-plugin.toml`. They receive bounded JSON Lines requests containing project file contents and return findings or proposed edits. The core validates every returned path and verifies original content still matches; edits are applied only by `MigrationEngine` after its checkpoint. Plugin child processes receive the build runner's scrubbed environment, a 30-second timeout and an 8 MiB stdout cap. This is process isolation, not an OS sandbox: a malicious executable still runs with the user's permissions.

## Local MCP presentation (Phase 12.C, ADR 0017)

The `server/src/mcp.rs` adapter serves the official `rmcp` SDK over stdio and is launched as `ummp mcp`. It reuses the REST server's canonical `allowed_roots` path policy and delegates every operation to `AppServices`. The eight tools expose analyze, scan, plan, port, build, job, cancel and rollback, plus the `ummp://report/{job}` Markdown resource. Port writes require `approve=true`; rollback requires `confirm=true`. No AI credentials, shell execution, or HTTP transport are exposed. Stdout carries protocol frames only; tracing and diagnostics go to stderr.

## Local REST presentation (Phase 12.B, ADR 0016)

The `server/` crate is a thin Axum adapter over `core/src/app::AppServices`. It owns the Tokio runtime and uses `spawn_blocking` for synchronous use cases; `core` and the CLI do not depend on Tokio. The API binds to loopback by default, requires a per-user bearer token, enforces configured canonical path roots and a 1 MiB request limit, and streams the existing job event contract through SSE. CORS is disabled and no AI credentials are exposed.

## Key patterns
- **Adapter**: wraps a specific loader/build/ai technology behind an interface.
- **Strategy**: interchangeable implementations (e.g. Gradle vs Maven runner).
- **Repository**: abstract storage of mappings/recipes/compatibility data.
- **Plugin Registry**: dynamic capability discovery at runtime.
- **Pipeline**: each step has input/output/state/log/timeout/retry/checkpoint/resume.

## Data flow (migration)
Import → Detect → Analyze → Plan → Transform → Build → Test → Review → Export

## Desktop presentation (Phase 11, ADR 0014)
- Tauri 2 hosts a Svelte 5 + TypeScript + Vite web view. The frontend calls only
  `ui/src/lib/api.ts`; Tauri command adapters in `ui/src-tauri/src/commands.rs` call
  `ummp_core::app::AppServices`. The web view does not contain migration, scan, build, or
  modpack business logic.
- Long-running work uses Tauri's blocking worker pool and the core job/event model. Events
  are forwarded to the window as `ummp://event`; persisted job/session state remains in core.
- GitHub credentials live in keyring-rs (Windows Credential Manager, macOS Keychain, Linux
  Secret Service). AI provider keys use the interim shared `FileKeyStore` behind
  `AiKeyStore`, shared with the CLI until the user selects a final backend (ADR 0015).
  Settings contain only non-secret preferences. No UI command returns a raw credential.
- AI proposals and in-place migration remain behind explicit user review; local builds repeat
  the core risk gate before execution. Output-folder opening is limited to paths stored in a
  completed job result.

## Cross-cutting
- Security: untrusted JARs and repositories are path-validated; GitHub builds use a disposable
  runner by default. Local builds are process-limited and risk-gated, not a true sandbox.
  Plugin isolation and stronger VM/container isolation remain future work.
- Observability: structured logs, metrics, tracing, diagnostic reports, job state visible to user.
- Versioning: project, session, schema, plugin API, mapping, recipe, artifact all versioned. Every migration is reproducible.

## Diagrams
See `docs/architecture/` for Mermaid diagrams.