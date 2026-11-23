# Phase 11 — User Interface (DRAFT)

## Intent
A desktop app for people who do not want the CLI: import a mod or pack, see the analysis,
preview and apply migrations, run builds, read reports, and roll back — all with the same
safety rules as the CLI.

## Architecture (ADR 0004)
Tauri 2: Rust backend commands call the **same application services** the CLI uses (no
business logic in the UI). Frontend: a small TypeScript SPA (framework chosen at design
time). Long jobs (build, AI, pack migration) run as background jobs (`jobs`/`persistence`
modules) with progress events streamed to the UI and cancellation.

## Screens (master prompt list)
| Screen | Built on |
|---|---|
| Dashboard | recent projects, last builds, warnings |
| Project Manager / Import Wizard | `ProjectManager`, `scan` first |
| Analyzer UI | `AnalysisReport` + dependency graph view |
| Migration UI | dry-run diff → apply → rollback (checkpoints) |
| AI Assistant UI | `ai-assist`/`ai-debug` with per-edit approval (the `ApprovalGate` becomes a UI dialog) |
| Build & Test UI | Phase 8 outcome, live log tail, score |
| Diff Viewer / Report Viewer | unified diff + Markdown report |
| Plugin Manager | Phase 12/14 plugin registry |
| Settings | AI provider/keys (OS keychain), build repo, workdir (default `D:\Claude\ummp-builds`) |

## Deliverables mapping
Desktop app (Windows first, since your workdir is on `D:`), user workflow tests (Tauri +
WebDriver or Playwright on the web build), UI docs with screenshots.

## Open questions
- Windows only first, or Windows + Linux + macOS?
- Language of the UI: Vietnamese, English, or both (i18n from the start)?
- Visual direction: mockups before code (Design artifact) — recommended.

## Risks
Largest UX surface; keep every write behind the same checkpoint/approval paths as the CLI.
API keys must live in the OS keychain, never in plain config.
