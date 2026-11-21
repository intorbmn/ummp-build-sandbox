# Phase 5 — Compatibility & Knowledge Base

| Field | Value |
|---|---|
| Phase ID | P5 |
| Objective | Build the knowledge base: mappings, version/loader compatibility, breaking changes, migration recipes, knowledge search. |
| Scope | Mapping DB, version compatibility, loader compatibility, breaking change DB, recipes, dependency compatibility, knowledge search, import/export, versioned schema. |
| Prerequisites | Phase 4 (needs detected versions/loaders). |
| Modules | ENG-02, knowledge/. |
| Architecture | Repository pattern over SQLite + JSON/YAML knowledge files. |
| Tasks | 5.1 SQLite schema + migrations 5.2 Mapping import/export 5.3 Version/loader compatibility matrix 5.4 Breaking change DB 5.5 Migration recipes (CRUD + versioning) 5.6 Knowledge search 5.7 Schema versioning + seed data |
| Dependencies | Phase 4. |
| Deliverables | Working DB, import/export, versioned schema, test coverage for mappings/recipes. |
| Tests | Unit on recipe/mapping logic; integration with seeded data. |
| Risks | Low-quality community data; schema drift. |
| Acceptance Criteria | Search returns correct mappings/recipes; schema migrations apply cleanly; seed data loads. |
| Complexity | L |
| Status | COMPLETED |
| Notes | 2026-11-19: all 10 KB tests failed on `master` (TempDir dropped before use → SQLITE_READONLY_DBMOVED); fixed. Seed data is illustrative only (see KNOWN_LIMITATIONS). |