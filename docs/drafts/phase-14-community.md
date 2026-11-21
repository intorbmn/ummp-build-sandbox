# Phase 14 — Community & Ecosystem (DRAFT)

## Intent
Let other people add what UMMP lacks most — mappings, recipes, adapters — without
weakening safety.

## Parts
- **Community mappings & recipes:** a public data repo (`ummp-knowledge`) with JSON in the
  KB export format; CI validates schema, runs the recipe tests (only `tested` recipes run,
  as in Phase 6.G), and records provenance (`source`, `confidence`). UMMP downloads signed
  snapshots.
- **Plugin marketplace:** index of plugins (Phase 12 interface) with version compatibility,
  required permissions, and a review status. Install = download + signature check + permission
  prompt; plugins run as separate processes (PLUGIN_SDK sandboxing).
- **Plugin review:** checklist + automated `ummp scan` of plugin artifacts + manual review
  for permissions beyond read-only.
- **Documentation portal:** mdBook site from `docs/` (user guide, adapter/plugin SDK, recipes).
- **Issue reporting:** `ummp report-issue` bundles versions, analysis summary and logs
  (secrets scrubbed) into a GitHub issue template — user confirms before anything is sent.
- **Community testing:** a public corpus of open-source mods; nightly runs of
  migrate + build on GitHub Actions with a results dashboard.

## Deliverables mapping
`CONTRIBUTING.md`, plugin SDK docs, release & maintenance policy (`docs/RELEASE_POLICY.md`:
SemVer, support window, security disclosure process).

## Open questions
- Will the project be public/open-source, and under which license?
- Who reviews contributions (just you at first)?

## Risks
Malicious contributions (mappings that rename into harmful code, plugins) — signing,
review and the existing scan are the defenses; keep plugins out-of-process.
