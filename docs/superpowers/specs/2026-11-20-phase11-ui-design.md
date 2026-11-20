# Phase 11 - User Interface (Tauri): design

Status: **approved by the user (2026-11-20); visual direction A, Calm workbench.**
Source of truth for scope: `docs/guides/phase-11-ui.md`. This file records the calls made on top of it.

## Intent
People who do not use a terminal need to port or migrate a mod or pack: open it, see what UMMP found,
pick a target, **review the diff first**, run with visible progress, read the report, build, and roll back.
Every action is an application service in `core/src/app/`; the UI contains no migration logic (ADR 0004).

## Goals
- Ten screens: Home, Analysis, Target, Plan & Diff, Run, Report, Build, AI assistant, Pack, Settings.
- `core/src/app/` facade (`AppServices`) with Rust tests; the CLI moves onto the same functions (existing CLI tests unchanged).
- Long work runs as jobs on worker threads; progress, log and approvals go through `EventBus` to the window.
- API keys and tokens live in the OS keyring only; no command ever returns a stored value.
- Windows installer (`.msi`/`.exe`) built by a `workflow_dispatch` job on `windows-latest`.

## Non-goals
Auto-update, code signing, macOS/Linux bundles (Phase 13); REST/MCP (Phase 12); editing mod source in the app;
web/cloud version; accounts and telemetry. Requests for these go to `docs/BACKLOG.md`.

## Anti-goals
- A UI-side file write (files change only through `MigrationEngine` after a checkpoint, or into a new output folder).
- An in-place apply without the explicit "I reviewed the changes" confirmation.
- A job stuck in "Running" forever; an AI edit approved by default; a key in settings, logs or events.
- Business logic or an event contract that exists only in TypeScript.

## Approaches
1. **`core/src/app/` facade + `ui/src-tauri` as a separate Cargo workspace + Svelte front-end (picked).** Root CI stays
   unchanged and green without webkit libraries; a second CI job builds the desktop crate.
2. `ui/src-tauri` as a member of the root workspace: `cargo build --all` would need webkit2gtk/libsoup everywhere. Rejected.
3. Use-case code in the Tauri crate: not testable without a display and the CLI could not share it. Rejected (ADR 0004).

## Calls made (guide defaults, approved by the user)
1. Front-end: Svelte 5 + Vite + TypeScript, Vitest + Testing Library, Playwright.
2. Language: Vietnamese + English with a switch, English fallback; strings in `ui/src/lib/i18n/{vi,en}.json`.
3. Screens: all ten. Cut order if the phase grows: AI assistant, then Pack, then Build (each cut goes to BACKLOG).
4. Workspace placement: `exclude = ["ui/src-tauri"]` in the root `Cargo.toml`, own `Cargo.lock`, CI job `desktop`.
5. Installer: `.github/workflows/desktop-release.yml` (`workflow_dispatch`, `windows-latest`); the user downloads the artifact.
6. Events: kinds `job.started|progress|log|approval|finished`, golden JSON in `tests/fixtures/golden/app-events/` shared by Rust and TypeScript tests.
7. Jobs: `std::thread` per job, `CancelToken`, `catch_unwind`, new `JobState::Cancelled`, stale `Running` jobs marked `Failed` on open.
8. Execution: native, no subagents.

## Design
Layers: `ui/src` (Svelte, `invoke` only in `lib/api.ts`) -> `ui/src-tauri` (one-line commands, keyring, event forwarder)
-> `core/src/app/` (`mod.rs`, `views.rs`, `jobs.rs`, `approval.rs`, `snapshots.rs`, `helpers.rs`) -> existing engines.
Signatures and event payloads follow guide sections 4.2-4.4. Capabilities: dialog open + opener only. CSP as in guide 4.5.

## Testing
Guide section 7 test names, test-first. Star tests are mutation-checked and recorded in the phase file.
11.A is Rust-only (`core/tests/app.rs`); 11.B-E add Vitest and mocked-IPC Playwright; 11.F is the real Windows install by the user.
Honest limit: this sandbox has no display and no Windows.

## Visual directions (user picked **A. Calm workbench**)
Common: light and dark theme following the OS, WCAG AA diff colors, `+`/`-` markers (never color alone),
system font stack, Vietnamese strings must fit.
- **A. Calm workbench** - neutral greys, one blue accent, dense tables, 13-14 px UI font, monospace only in diffs.
- **B. Guided wizard** - large single-purpose steps, generous whitespace, one primary button per page.
- **C. IDE-like** - split panes (tree | diff | inspector), mostly monospace, keyboard-first.
