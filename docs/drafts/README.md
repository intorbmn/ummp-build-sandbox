# Rough designs for Phases 9–14 (DRAFT)

> **Superseded** by the detailed guides in [`docs/guides/`](../guides/README.md). Kept for
> history; when the two disagree, the guides win.

First drafts written on 2026-11-19 so the whole road ahead is visible. They are **not
approved designs**. Each phase still goes through brainstorming → design → plan → approval
before code (CLAUDE.md), and its draft is replaced by a spec in `docs/superpowers/specs/`.

| Phase | Draft | One-line intent | Size |
|---|---|---|---|
| 9 Advanced Adapters | [phase-09-adapters.md](phase-09-adapters.md) | One adapter trait per language/technology; detect, report, migrate what is safe | XL |
| 10 Modpack Migration | [phase-10-modpack.md](phase-10-modpack.md) | Import a pack, resolve the graph, migrate mods in parallel, build and export | L |
| 11 User Interface | [phase-11-ui.md](phase-11-ui.md) | Tauri desktop app over the existing application services | XL |
| 12 CLI, API & Integration | [phase-12-api.md](phase-12-api.md) | Stable CLI, local REST API, MCP server, GitHub Action | L |
| 13 Production Readiness | [phase-13-production.md](phase-13-production.md) | Performance, caching, security audit, installer, updates, RC | L |
| 14 Community & Ecosystem | [phase-14-community.md](phase-14-community.md) | Plugin/recipe/mapping contributions with review and signing | L |

Cross-phase dependencies: 10 uses 9's adapters and 8's builder; 11 and 12 are both thin
layers over the same application services (ADR 0004), so 12's API shape should be fixed
before 11's screens; 13 audits everything; 14 needs 12's plugin interface to be stable.
Deferred work from finished phases stays in `docs/BACKLOG.md`.
