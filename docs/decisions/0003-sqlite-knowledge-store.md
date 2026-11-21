# ADR 0003 — SQLite for structured knowledge and job state

**Status:** Accepted (Phase 1, implemented Phases 3 and 5)

## Context
The knowledge base holds mappings, compatibility rows, breaking changes, recipes and
dependency metadata. Job state must survive crashes.

## Problem
Which store fits a desktop tool that must work offline?

## Options
1. SQLite (bundled via `rusqlite`)
2. PostgreSQL
3. Plain JSON/YAML files only
4. Graph or vector database

## Decision
SQLite for structured data; JSON for import/export and human-editable seed data.

## Consequences
- No server to install; the DB file can be copied, backed up and versioned.
- Schema changes go through the `schema_version` table and `apply_migrations`.
- Concurrent multi-user writes are not supported (not a requirement).

## Alternatives rejected
- **PostgreSQL:** deployment burden for a desktop app.
- **Files only:** no indexed search, no transactional updates.
- **Graph/vector DB:** no proven use case yet; revisit if semantic search over docs is needed.
