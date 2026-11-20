# TECH_STACK — Technology Decisions

## Primary language
**Rust** — chosen for safety, deterministic builds, strong interface contracts, and good process isolation. Alternatives rejected: Java (too heavy for a desktop tool, runtime coupling), Node (weak for sandboxing native processes), Python (runtime coupling, GIL).

## Toolchain
Pinned in `rust-toolchain.toml` (currently 1.97.0) so local runs and CI use the same rustc and
clippy. CI broke once on floating `stable` when Rust 1.100 added new lints; the code is already
clean under those lints, so bumping is a one-line change plus a full CI run.

## UI
- **Tauri** (Rust backend + web frontend) for desktop. Rejected: Electron (huge bundle), native Rust windows (more code, no web reuse).
- CLI: Rust `clap`.

## Build orchestration
- Wrap existing Gradle/Maven/ native toolchains; do not reimplement them. Use `java`/`gradle`/`mvn` as external processes in a sandbox.

## Data store
- **SQLite** for structured knowledge (mappings, recipes, compatibility, job state). Rejected: PostgreSQL (deploy burden), Graph/Vector DB (no proven need — no embeddings use case yet).
- JSON/YAML for project config and export artifacts.

## AI
- Provider-agnostic interface. Initial providers: OpenAI-compatible endpoints. Model router picks model per task. No vendor lock-in.

## Sandbox
- OS process isolation per build/test run. Docker only if a specific untrusted-build need emerges.

## Testing
- Rust `cargo test` for unit/integration. JVM mods tested by actually launching a client/server in sandbox.

## Decision records
See `docs/decisions/`.