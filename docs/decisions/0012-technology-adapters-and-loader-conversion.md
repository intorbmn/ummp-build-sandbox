# ADR 0012 — Technology adapters (data rules + trait) and structural loader conversion

**Status:** Accepted (Phase 9)

## Context
UMMP must port any mod across Minecraft versions and loaders. Before Phase 9 it handled
Java/Kotlin renames, class-level bytecode, descriptors and build scripts within one loader,
and Mixin configs. Other technologies were invisible, and changing loader was explicitly
unsupported.

## Options
1. Breadth only: detect every technology from data rules
2. Depth only: real loader conversion and Mixin member renames
3. Both (picked by the user)

## Decision
- `TechAdapter` trait (`core/src/adapter/`): `findings` (always), `owns` (files the adapter
  rewrites itself), `rewrite` (source text on top of the generic transforms) and `edits`
  (new/changed/deleted files). The registry runs inside `MigrationEngine::compute_edits`, so
  every write still goes through a checkpoint (ADR 0005/0006). `EditSet` gained `deletes`;
  a `FileEdit` with empty `original` creates a file.
- Broad layer: `adapter_rules.toml`, one adapter per rule (15 technologies). Each finding is
  `auto`, `assisted` or `manual`.
- Loader conversion is **structural**: descriptor, entry-point bridge class, build files from
  `loader_templates.toml` (per loader × Minecraft version), Mixin and AT/AW registration,
  package renames from `loader_rules.toml` and from KB rows with loader `"<from>-><to>"`.
  Loader API semantics are not rewritten; every remaining use of the source loader's API is
  an `assisted` finding with file:line.
- Mixin member renames use KB mappings `pkg.Owner#name`; they apply only inside Mixins whose
  target owns the member.
- `ummp port` copies the input (folder or zip, zip entries checked with `resolve_within`) to a
  new folder and runs the pipeline there; the input is never modified.

## Consequences
- New technologies and new loader/version templates are data changes.
- Conversions compile only when the mod uses no loader API beyond the entry point (or the
  user ports the listed `assisted` items, possibly with `ai-assist`).
- One template version per loader today (1.21.1); others report `manual` for the build.
