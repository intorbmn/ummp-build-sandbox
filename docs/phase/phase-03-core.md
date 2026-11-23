# Phase 3 — Core Engine

| Field | Value |
|---|---|
| Phase ID | P3 |
| Objective | Build the core that runs independently: project import, session, events, checkpoint, job persistence. |
| Scope | Project Manager, File System abstraction, Metadata, Task Management, Migration Session, Event Bus, Checkpoint, Rollback, Job Persistence. |
| Prerequisites | Phase 2. |
| Modules | CORE-01..07. |
| Architecture | Layered; Domain has no external deps. |
| Tasks | 3.1 Project import + path validation 3.2 File system abstraction trait 3.3 Project metadata model 3.4 Event bus 3.5 Migration session state machine 3.6 Checkpoint/rollback 3.7 Job persistence (SQLite) 3.8 Unit + integration tests |
| Dependencies | Phase 2. |
| Deliverables | Core engine runs standalone; API for other modules; tests. |
| Tests | Unit + integration for each sub-module. |
| Risks | Session state bugs; checkpoint corruption. |
| Acceptance Criteria | Import a fixture project; create a session; checkpoint; simulate failure; resume; rollback works. |
| Complexity | M |
| Status | COMPLETED |
| Notes | 2026-11-19: "rollback works" was not actually met (checkpoint stored metadata only). Closed with `CheckpointStore::snapshot`/`restore` (ADR 0006) and a rewritten integration test. |