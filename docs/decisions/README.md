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
| [0007](0007-token-level-source-transform.md) | Token-aware text rewriting for the first source transformer | Accepted (after-dot rule superseded by 0008) |
| [0008](0008-rename-after-package-path.md) | Rename simple names after a package path | Accepted |
| [0009](0009-in-process-class-remapping.md) | Remap class names in-process instead of running an external remapper | Accepted |
| [0010](0010-ai-structured-search-replace.md) | AI agents return validated search/replace proposals through structured output | Accepted |
| [0011](0011-build-isolation-and-risk-gate.md) | Build on disposable GitHub runners by default; gate local builds on a static risk scan | Accepted |
| [0012](0012-technology-adapters-and-loader-conversion.md) | Technology adapters (data rules + trait) and structural loader conversion | Accepted |
| [0013](0013-modpack-model-online-index-hashing.md) | Modpack model, online index, and content hashing | Accepted |
| [0014](0014-desktop-ui-boundary-and-keyring.md) | Desktop UI boundary, credential storage, and workflow UI | Accepted (partly superseded by 0015) |
| [0015](0015-ai-key-file-storage.md) | Interim file-backed AI API key storage | Accepted (interim) |
| [0016](0016-local-rest-api.md) | Local REST API and async boundary | Accepted |

| [0017](0017-mcp-stdio-server.md) | MCP server over stdio | Accepted |
