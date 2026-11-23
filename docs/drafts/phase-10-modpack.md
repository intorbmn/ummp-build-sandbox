# Phase 10 — Modpack Migration (DRAFT)

## Intent
Move a whole modpack to a new Minecraft version/loader: know which mods already have a
target version, which can be ported by UMMP, which need a replacement, and produce a
buildable, exportable pack with a clear conflict report.

## Inputs
- Pack formats: CurseForge `manifest.json` (+ `overrides/`), Modrinth `.mrpack`
  (`modrinth.index.json`), plain `mods/` folder, Prism/MultiMC instance.
- Each mod: JAR (scan + analyze) and, if available, source repo.

## Pipeline
1. **Import:** parse the pack, collect mods + configs; every JAR goes through `ummp scan`
   (Phase 8) — High risk mods are flagged before anything else.
2. **Graph:** extend `DependencyGraphBuilder` across the whole pack; detect missing deps,
   cycles, version conflicts, client-only/server-only mods (from descriptors: `environment`,
   `side`).
3. **Resolve per mod:** (a) upstream already has a target build → use it (needs an online
   source: Modrinth/CurseForge API, opt-in, cached); (b) source available → UMMP migration
   (Phase 6/7) + build (Phase 8); (c) neither → replacement suggestion or "drop".
4. **Configs:** migrate config files that UMMP knows the schema for (TOML/JSON/properties
   key renames from KB); copy the rest and report.
5. **Split:** client pack and server pack.
6. **Build/verify:** start a dedicated server with the whole server pack (Phase 8 runner).
7. **Export:** `.mrpack` and CurseForge zip.

## Parallelism
Analysis and per-mod migration in parallel (rayon or a bounded worker pool) — bounded by
CPU and by GitHub/API rate limits; one checkpoint per mod plus one for the pack.

## Deliverables mapping
Migration pipeline (`ummp pack migrate`), conflict report, dependency resolution report
(Markdown + JSON).

## Open questions
- Which pack formats do you actually use?
- Is calling Modrinth/CurseForge APIs acceptable (network, API keys for CurseForge)?
- Replacement suggestions: curated list in the KB vs. search by category/tags?

## Risks
Licenses (redistributing JARs in exported packs), API terms of service, large packs
(200+ mods) → time and disk.
