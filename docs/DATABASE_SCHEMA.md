# DATABASE_SCHEMA — Data Model

Engine: SQLite (single file per project/session). No PostgreSQL/Graph/Vector DB at this stage — no proven need.

## Tables
- `mappings(version, loader, from_api, to_api, kind, source, confidence)` — API mappings between versions/loaders.
- `compatibility(minecraft_version, loader, loader_version, status, notes)` — version/loader compatibility matrix.
- `breaking_changes(version, change, severity, workaround, source)` — known breaking changes.
- `recipes(recipe_id, version, loader, description, steps_json, source, tested)` — reusable migration recipes.
- `dependencies(mod_id, name, version, loader, url, checksum, source)` — mod dependency metadata.
- `sessions(session_id, project_path, state, started_at, updated_at, toolchain_version, ai_model)` — migration session record.
- `checkpoints(session_id, step, snapshot_path, created_at)` — resumable state.
- `jobs(job_id, session_id, step, state, log, retries, timeout_at)` — pipeline job state.
- `ai_audit(id, session_id, agent, prompt_version, tool_calls, output, approved, cost)` — AI audit trail.
- `plugins(plugin_id, name, api_version, capabilities, enabled, config)` — plugin registry.

## Schema versioning
`schema_version` table bumps on migrations; old sessions remain readable via migration scripts.

## Data categories
- Structured data: the tables above.
- Reference docs: knowledge files (JSON/YAML) under `knowledge/`.
- Cache: build artifacts, downloaded deps — outside the DB.
- User project data: never written to the DB; referenced by path only.