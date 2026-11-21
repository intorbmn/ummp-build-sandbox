# CLAUDE.md — working on UMMP

Universal Minecraft Mod Porter: a Rust modular monolith (`core/` library + `cli/` binary)
that analyzes Minecraft mods and migrates them across versions and loaders. The product and
process requirements are in `universal mod porter.txt` (the master prompt). The architecture
is in `docs/SAD.md` and `docs/decisions/`.

## Before any change
1. Read `docs/PROJECT_STATUS.md`, the current phase file in `docs/phase/`, and
   `docs/KNOWN_LIMITATIONS.md`.
2. Load the skills listed for the phase you are working on (table below). Loading the
   skills is part of starting the milestone, not optional.
3. Work on a branch, never directly on `master`.
4. For Phases 10–14, follow the detailed guide in `docs/guides/` (start with
   `docs/guides/README.md`: workflow, environment quirks, entry points, definition of done).

## Commands (must all pass before a commit, same as CI)
```sh
cargo build --all
cargo test --all
cargo clippy --all --all-targets -- -D warnings
cargo fmt --all -- --check
```
The toolchain is pinned in `rust-toolchain.toml`. If rustup cannot download the pinned
version (for example in a sandbox without access to static.rust-lang.org), check that
`rustc -V` under `RUSTUP_TOOLCHAIN=stable` prints the pinned version, then run the commands
with that variable set. Never unpin the toolchain to get around this.

CI has two independent jobs: the root Rust workspace and the desktop workspace (`ui/src-tauri` plus npm/Vitest/Playwright). The desktop Linux job installs GTK/WebKit and `libdbus-1-dev` for Tauri and the Secret Service keyring backend. Windows MSI/NSIS packaging is a separate manually dispatched workflow (`.github/workflows/desktop-release.yml`).

## Rules that are easy to break
- **A status is COMPLETED only with test evidence** against the phase's acceptance criteria.
  Writing code without running the tests is not enough. Earlier phases were wrongly marked
  done; see the notes in `docs/phase/`.
- **Every test must be able to fail.** For key tests, break the code on purpose once and
  confirm the test goes red. Fixtures must look like real inputs: JARs contain binary
  `.class` files, and descriptors use real schemas.
- **Do not move on to the next phase on your own** (master prompt §14). Finish the current
  milestone, report it, and wait for the user to review.
- Every significant decision gets an ADR in `docs/decisions/` (next number: 0018).
- Update `PROJECT_STATUS.md`, the phase file, `CHANGELOG.md`, and `KNOWN_LIMITATIONS.md`
  in the same change as the code.
- Edits are applied only by `MigrationEngine` after a checkpoint (ADR 0005/0006). This
  includes AI output from Phase 7 onwards.
- Treat every input JAR, repository, and plugin as untrusted. Use `resolve_within` for any
  path that comes from input.
- Sample data uses clearly synthetic names (`DemoWidget -> DemoGadget`). Never use names
  that look like real Minecraft APIs.

## Skills per phase
"Always" skills apply in every phase. Where several skills cover the same job, use the one
listed here and not the duplicate (for example `ecc:tdd-workflow`, `ecc:rust-test` or
`rust-cli-skills:rust-build`).

| When | Skills |
|---|---|
| **Always: design a milestone** | `superpowers:brainstorming`, then `superpowers:writing-plans`; `andrej-karpathy-skills:karpathy-guidelines` (avoid overengineering, master prompt §12) |
| **Always: implement** | `superpowers:test-driven-development`; `rust-cli-skills:rust-errors`, `rust-cli-skills:rust-idiomatic`, `rust-cli-skills:rust-testing` |
| **Always: debug** | `superpowers:systematic-debugging`; `ecc:rust-build` for compiler or borrow-checker errors |
| **Always: finish** | `superpowers:verification-before-completion`; `ecc:rust-review`; `pr-review-toolkit:review-pr`; `commit-commands:commit-push-pr` |
| **Always: decisions** | `ecc:architecture-decision-records` |
| 6 Migration Engine | `rust-cli-skills:rust-filesystem`, `rust-cli-skills:rust-io` (descriptor, build-script, and resource transforms) |
| 7 AI Agent System | `claude-api:claude-api` (provider abstraction, tool calling, structured output, cost); `superpowers:brainstorming` first. AI must return `FileEdit`s only (ADR 0005) |
| 8 Build & Testing Engine | `rust-cli-skills:rust-processes`, `rust-cli-skills:rust-concurrency` (Gradle/Java runner, timeouts, cancellation); `claude-security:claude-security` before running untrusted builds |
| 9 Advanced Adapters | `rust-cli-skills:rust-design` (adapter traits); `superpowers:brainstorming` for each new adapter family |
| 10 Modpack Migration | `rust-cli-skills:rust-concurrency` (parallel analysis); `rust-cli-skills:rust-performance` |
| 11 User Interface (Tauri) | `frontend-design:frontend-design`, `ui-ux-pro-max:ui-ux-pro-max`. The UI calls application services only (ADR 0004) |
| 12 CLI, API & Integration | `rust-cli-skills:rust-cli-interface`; `example-skills:mcp-builder`, `ecc:mcp-server-patterns` (MCP server); `ecc:api-design` (REST) |
| 13 Production Readiness | `claude-security:claude-security` (full audit); `rust-cli-skills:rust-performance`, `rust-cli-skills:rust-memory`; `rust-cli-skills:rust-distribution` (installer, releases) |
| 14 Community & Ecosystem | `example-skills:doc-coauthoring` (contributor and plugin-SDK docs); `pr-review-toolkit:review-pr` for community recipes |

Skill names come from the plugins installed on the maintainer's account. If a skill is
not available in a session, say so and continue with the same practice by hand. Do not
skip the practice.

## Layout
| Path | Contents |
|---|---|
| `core/src/` | Domain and engine modules: project/filesystem/metadata/session/events/checkpoint/jobs/persistence, analyzer, knowledge, migration, descriptors, build scripts/resources/Mixins/bytecode, AI, build/scan, technology adapters, loader conversion, port and modpack engines |
| `core/src/app/` | Shared `AppServices` facade, command helpers, session/job views, port/migrate/build/pack/AI job runners, settings, risk, checkpoint orchestration and shared AI credential store |
| `ui/` | Tauri 2 + Svelte 5 + TypeScript desktop shell; `src/lib/api.ts` is the only frontend invoke boundary; screens, i18n, golden-backed unit/E2E tests; `src-tauri/` is a separate Cargo workspace with thin commands and OS keyring |
| `core/tests/` | Integration tests (`integration.rs` for Phase 3, `migration.rs` and `bytecode_jvm.rs` for Phase 6, `ai_eval.rs` for Phase 7, `scan.rs`, `build_local.rs`, `build_github.rs` for Phase 8, `adapters.rs`, `loader.rs`, `port.rs` for Phase 9, `modpack.rs` for Phase 10, `app.rs` for AppServices/golden views; `cli/tests/{pack_cli.rs,phase12_cli.rs,mcp_stdio.rs}` exercise the real CLI and MCP stdio process) |
| `cli/src/` | CLI entry point plus `args.rs`, `config.rs`, `exit.rs`, and `output.rs`; shared AI key commands and provider resolution |
| `providers/` | HTTP AI providers (Anthropic, OpenAI-compatible); the only crate that talks to model APIs |
| `server/` | Axum + Tokio REST adapter and official `rmcp` stdio MCP adapter; all domain work is delegated to `AppServices`, with shared canonical path policy |
| `tests/fixtures/` | Mod corpus (`generate.py`), the `forge-src-1.16` project, and `golden/` expected outputs |
| `docs/` | PRD, SAD, ROADMAP, phase files, ADRs, KNOWN_LIMITATIONS, PROJECT_STATUS |
