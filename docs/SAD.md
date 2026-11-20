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
The Tauri 2 desktop shell in `ui/` is a presentation adapter, not a second application layer. Svelte screens call typed commands through `ui/src/lib/api.ts`; Tauri commands forward to `core/src/app/` and return `UiError` values. Core job events are forwarded to the window as `ummp://event`. OS credentials are accessed through the `SecretStore` trait and are never serialized into settings or returned to the web view. Rust and TypeScript view/event contracts share JSON golden fixtures.

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
- OS credentials live in keyring-rs (Windows Credential Manager, macOS Keychain, Linux
  Secret Service). Settings contain only non-secret preferences. The UI has no command that
  returns a credential.
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