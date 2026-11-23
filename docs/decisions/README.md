# Architecture Decision Records

One file per significant decision, numbered in order. A decision is never edited
after it is `Accepted`; a later ADR supersedes it instead.

Each ADR has: **Status**, **Context**, **Problem**, **Options**, **Decision**,
**Consequences**, **Alternatives rejected**.

| ADR | Title | Status |
|---|---|---|
| [0001](0001-rust-primary-language.md) | Rust as the primary language | Accepted |
| [0002](0002-modular-monolith.md) | Modular monolith with layered dependencies | Accepted |
| [0003](0003-sqlite-knowledge-store.md) | SQLite for structured knowledge and job state | Accepted |
| [0004](0004-tauri-desktop-ui.md) | Tauri for the desktop UI | Accepted (deferred to Phase 11) |
| [0005](0005-ai-write-gate.md) | AI never writes to a project directly | Accepted |
| [0006](0006-filesystem-snapshot-checkpoints.md) | File-copy snapshots for checkpoint/rollback | Accepted |
| [0007](0007-token-level-source-transform.md) | Token-aware source transformer | Accepted (after-dot rule superseded by 0008) |
| [0008](0008-rename-after-package-path.md) | Rename simple names after a package path | Accepted |
| [0009](0009-in-process-class-remapping.md) | In-process class remapping | Accepted |
| [0010](0010-ai-structured-search-replace.md) | AI structured search/replace proposals | Accepted |
| [0011](0011-build-isolation-and-risk-gate.md) | Build isolation and risk gate | Accepted |
| [0012](0012-technology-adapters-and-loader-conversion.md) | Technology adapters and loader conversion | Accepted |
| [0013](0013-modpack-model-online-index-hashing.md) | Modpack model and online index | Accepted |
| [0014](0014-desktop-ui-boundary-and-keyring.md) | Desktop UI boundary and keyring | Accepted (partly superseded by 0015) |
| [0015](0015-ai-key-file-storage.md) | Interim file-backed AI API key storage | Accepted (interim) |
| [0016](0016-local-rest-api.md) | Local REST API and async boundary | Accepted |
| [0017](0017-mcp-stdio-server.md) | MCP server over stdio | Accepted |
| [0018](0018-project-license.md) | Project license: Apache-2.0 core, separate proprietary Pro | Accepted |
| [0019](0019-plugin-protocol.md) | Out-of-process plugin protocol and trust boundary | Accepted |
| [0020](0020-sqlite-analysis-cache.md) | SQLite cache for deterministic per-JAR analysis | Accepted |
| [0021](0021-crash-recovery-protocol.md) | Durable pre-transform checkpoint recovery | Accepted |
