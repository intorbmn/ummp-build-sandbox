# ROADMAP — UMMP

Status legend: NOT_STARTED | PLANNED | IN_PROGRESS | BLOCKED | NEEDS_REVIEW | TESTING | COMPLETED | DEPRECATED

| Phase | Name | Status | Complexity |
|---|---|---|---|
| 0 | Product Discovery & Feasibility | COMPLETED | S |
| 1 | System Architecture | COMPLETED | S |
| 2 | Development Foundation | COMPLETED | M |
| 3 | Core Engine | COMPLETED | M |
| 4 | Analyzer Engine | COMPLETED | M |
| 5 | Compatibility & Knowledge Base | COMPLETED | L |
| 6 | Migration Engine | COMPLETED | L |
| 7 | AI Agent System | PLANNED | XL |
| 8 | Build & Testing Engine | PLANNED | L |
| 9 | Advanced Technology Adapters | PLANNED | XL |
| 10 | Modpack Migration | PLANNED | L |
| 11 | User Interface | PLANNED | L |
| 12 | CLI, API & External Integration | PLANNED | M |
| 13 | Optimization & Production Readiness | PLANNED | L |
| 14 | Community & Ecosystem | PLANNED | XL |

## MVP scope (Phase 2-4 + partial 5/6)
Import a Forge/Fabric mod project, detect version+loader+deps, produce an analysis report and dependency graph, and suggest a migration plan. No AI, no build, no GUI — CLI only.

**MVP status (2026-11-19):** reached for Java/Kotlin API renames: `ummp analyze`, then
`ummp migrate` (dry run with diff + report), `--apply` (checkpointed), `ummp rollback`.
Missing for a useful real-world port: a real mapping dataset in the KB, and descriptor/build
migration (Phase 6.B/6.C).

**Phase 7 (2026-11-19):** AI agents done (`ummp ai-assist`, `ummp ai-debug`); see
`docs/phase/phase-07-ai.md`.

**Phases 9–14:** rough drafts in [docs/drafts/](drafts/README.md) (not approved designs).

## Detailed plans
See `docs/phase/` per phase (created on execution).