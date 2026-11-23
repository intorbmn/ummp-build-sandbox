# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Phase 13.H: English and Vietnamese user guides covering installation, porting, packs, reports, build backends, AI, recovery and troubleshooting.

### Added
- Phase 13.G: read-only `ummp update --check` and opt-in once-daily background release checks; no auto-download or telemetry.

### Added
- Phase 13.F: cargo-dist 0.34.0 release configuration for Windows x64 (ZIP/MSI), Linux x64 and macOS ARM64 CLI archives/installers; 1.0.0-rc.1 crate versioning and release workflow.

## [1.0.0-rc.1] - 2026-11-22

### Added
- First production-readiness release candidate: cross-platform CLI distribution configuration, Windows MSI/ZIP and shell/PowerShell installers, and durable crash recovery.
- Nightly fuzz harnesses for nine untrusted-input surfaces; stable CI remains independent of scheduled fuzz campaigns.

### Changed
- CLI and workspace crates move to the 1.0.0-rc.1 release candidate version.

### Added
- Phase 13.E: durable checkpoint manifests are atomically persisted before migration writes; `ummp recover <project> --session <UUID>` restores an interrupted session to its pre-transform snapshot. Recovery is rollback-only until deterministic edit-plan replay is persisted.
- Phase 13.D: nine nightly cargo-fuzz targets for class files, descriptors/YAML, Modrinth indexes, ZIP imports, Gradle logs, crash reports, plugin JSONL, REST request DTOs and MCP tool arguments. Fixture-seeded fuzz campaigns run separately from stable CI; see [docs/FUZZING.md](docs/FUZZING.md).

### Fixed
- MCP server now reports its name as `ummp` (was the library default `rmcp`) in `initialize`.

### Changed
- Desktop AI assist tab: the AI provider (Anthropic / OpenAI-compatible) can be switched on the tab itself; the choice is saved to settings before the key form changes, and a failed save keeps the previous provider.

- Phase 12.E: initial out-of-process plugin runtime, API 1 manifest and JSON Lines handshake, plugin discovery, CLI validation commands, scrubbed environment, timeout, stdout cap and returned-path validation.
- Phase 12.D: root composite GitHub Action for analyze, scan, port and build, configurable risk gating, schema-1 JSON reports, GitHub Step Summary, report artifacts, and contract tests.
- Phase 12.B: local Axum REST API, `ummp serve`, localhost binding, bearer-token authentication, allowed-root path validation, job SSE, OpenAPI contract and request-size limits. See ADR 0016 and `docs/API.md`.
- Phase 12.C: official `rmcp` stdio MCP server, eight AppServices tools, `ummp://report/{job}` resource, canonical path and write-approval gates, protocol-only stdout, and a real-process JSON-RPC integration test. See ADR 0017 and `docs/MCP.md`.
- Phase 12.0: shared interim file-backed AI credentials for desktop and CLI, environment-first provider resolution, one-time migration of legacy AI keys from the OS keyring, masked credential management in the AI screen/Settings, and `ummp ai-key set|status|delete`.
- Phase 12.A: stable CLI exit codes, shell completions, strict `ummp.toml` configuration precedence, global schema-1 JSON envelopes, AppServices routing for migration/port/build/pack/rollback, and structured results for pack and AI commands. See ADR 0015, `docs/CLI.md`, and `docs/phase/phase-12-api.md`.

### Added
- Phase 11.B–11.E: desktop workflow screens for Target, Plan/Diff, Run, Report, Build, AI approval, modpack migration and Settings; OS keyring-backed credentials, output-folder opening, bilingual labels, bounded logs and paginated pack reports. See ADR 0014 and `docs/phase/phase-11-ui.md`. Automated CI is green; Windows runtime validation remains pending.

### Added
- Phase 11.C: desktop Target, Plan/Diff, Run, and Report screens; typed Tauri command adapters;
  blocking-pool dispatch; job event progress/cancel; reviewed in-place apply and rollback;
  source JAR metadata-only analysis; open-output-folder action; unified diff line numbers and
  collapsible unchanged runs.
- Phase 11.D: Settings screen for output/KB/sandbox/provider/language; keyring-rs OS credential
  storage with set/has/delete only; GitHub token injection through GH_TOKEN for remote build
  child processes; Build screen with core risk preview and explicit local High/Medium gates.
- Phase 11.E: AI assist screen with per-file diff approval/rejection and A/R keyboard shortcuts;
  Modrinth-backed modpack workflow and paginated 50-row report; shared 300-mod golden JSON
  fixture used by Rust and TypeScript tests.
- Phase 11.F: manually dispatched Windows MSI/NSIS packaging workflow; Linux CI installs
  Secret Service build headers; npm production audit is warning-only.
- Standalone JAR analysis now returns descriptor metadata for scan-only workflows; source
  ZIPs are selectable for porting.
- ADR 0014 records the desktop application boundary, OS keyring, and workflow decisions.

### Changed
- Phase 11.A: the CLI now shares one set of helpers with the desktop app
  (`ummp_core::app::helpers` and `::snapshots`: knowledge-base opening, snapshot store,
  mod identity, risk decision, `NoIndex`). The CLI's own copies are gone; behaviour, flags
  and exit codes are unchanged (checked with a real `migrate --apply` + `rollback` round trip
  and `cli/tests/pack_cli.rs`).

### Added
- Phase 11.A (application services, ADR 0004): `ummp_core::app` facade for the desktop UI.
  Sync services (analyze, scan, targets, plan, settings) and jobs on worker threads with
  events, cancel and stale-job repair: port, in-place migrate (review flag required, rollback
  on cancel), AI assist (per-file approval over events), pack and build (risk gate).
- Phase 10 (ADR 0013): `ummp pack inspect|migrate|export` for modpacks. Reads and
  writes Modrinth `.mrpack`, CurseForge zip, Prism/MultiMC instances and plain folders;
  analyses every JAR in parallel; dependency graph (missing, duplicate, cycle, side);
  per-mod plan (keep, update, port, replace, remove, manual) with a reason for each;
  optional read-only Modrinth lookups with `--online` (cached as JSON); config migration
  applied through `MigrationEngine` with a checkpoint; `UMMP_PACK_REPORT.md`. Exit codes:
  0 done, 2 refused for a High-risk JAR, 3 done with mods left to you. Archive reads are
  bounded and path-checked.
- `docs/guides/`: detailed step-by-step guides for Phases 10–14 (workflow, environment
  quirks, APIs to reuse, milestones with test names, definition of done). They supersede
  the rough drafts in `docs/drafts/`.
- Phase 9 (ADR 0012): technology adapter SDK (`core/src/adapter/`, `docs/ADAPTER_SDK.md`)
  with 15 data-driven detection rules, native-library report, Groovy/Scala renames, Mixin
  member renames, and loader conversion (Fabric/Quilt ↔ NeoForge, Forge ↔ NeoForge,
  Fabric → Forge) with 1.21.1 build templates. `ummp port` turns a mod's source (folder or
  zip) into a ported copy plus `UMMP_REPORT.md`; `ummp adapters`; `migrate --from-loader`.
  Analyzer reads descriptors of source projects. Rough designs for Phases 10–14 in
  `docs/drafts/`. Verified on GitHub Actions: the demo Fabric mod and its NeoForge port both
  build, test and start a server.

### Changed
- `scan`: the standard Gradle wrapper JAR is one Medium finding (verify checksum) instead of
  High content findings; GitHub builds run `gradle/actions/wrapper-validation`.
- `build` (GitHub): failed stage log tails go to the job log; the artifact-less fallback uses them.
- Phase 8 (ADR 0011): `ummp scan` static malware-risk scan (class constant pools, sources,
  build scripts, nested JARs, native libraries, obfuscation); `ummp build` runs build, tests
  and a dedicated-server start on GitHub Actions (default) or locally (`--local`, new folder
  per build, scrubbed env, timeouts, process-tree kill) and prints a scored report with
  classified errors and crash info. High-risk mods are not built locally without
  `--i-accept-risk`. `docs/BACKLOG.md` lists deferred work.
- Phase 7 AI agent system (`core/src/ai/`, crate `ummp-ai-providers`, ADR 0010): Analysis,
  Migration, Debug and Review agents with JSON-schema output; search/replace proposals validated
  and applied only through `MigrationEngine` after review, approval and a checkpoint; retry,
  fallback model, token budget, cost tracking, audit log. CLI `ai-assist` and `ai-debug`.
- `MigrationEngine::apply_edit_set` (public checkpointed apply).
- Phase 6.C–6.G: build-script migration (`build_script.rs`), resources and 1.21 data pack folder
  moves (`resources.rs`, KB `pack_formats` table), Mixin migration (`mixin.rs`), class-level
  bytecode remapping (`bytecode.rs`, `ummp remap-jar`, ADR 0009), and recipe execution.
- `FileSystem::remove`; `MigrationEngine::compute_edits` now returns an `EditSet` with moves.
- Phase 6.B descriptor migration (`core/src/descriptor.rs`): retargets Minecraft and loader
  version requirements in Fabric/Quilt/Forge/NeoForge descriptors, preserving formatting.
- ADR 0008: simple names after an all-lowercase (package) qualifier are renamed.
- Phase 6.A migration engine (`core/src/migration.rs`): planner with per-version stages,
  token-aware Java/Kotlin API renamer, unified diffs, Markdown/JSON report, checkpointed apply
  with automatic restore on failure.
- CLI: `ummp migrate` (dry run by default, `--apply`, `--report`), `ummp rollback`;
  `ummp analyze` now runs the analyzer engine (`--json` for machine-readable output).
- Real checkpoint snapshots and restore (`CheckpointStore::snapshot` / `restore`), and
  `filesystem::resolve_within` path guard.
- Analyzer: Quilt (`quilt.mod.json`), NeoForge `neoforge.mods.toml`, `[[dependencies.<modId>]]`
  tables and version ranges.
- ADRs 0001-0007 in `docs/decisions/`; `docs/KNOWN_LIMITATIONS.md`.

### Fixed
- Analyzer aborted on any JAR containing a binary entry (all real mods).
- `parse_version("1.21")` returned 1.21.1.
- All knowledge-base tests failed (temp directory deleted while the DB was open).
- CI: clippy errors and rustfmt drift on `master`.

### Added
- Phase 3 core engine: project import with path validation, filesystem abstraction trait, metadata model, event bus, migration session state machine, checkpoint/rollback, SQLite-backed job persistence (`JobDb`)
- Integration tests: full pipeline import/session/checkpoint/resume, job persistence round-trip, event bus, metadata roundtrip, session-job store consistency (27 tests total)
- Phase 4 analyzer engine: adapter-based detectors behind `IAnalyzer` (`JarInspector`, `SourceScanner`, `BuildSystemDetector`, `DependencyGraphBuilder`); JAR inspection covers Fabric `fabric.mod.json`, Forge/NeoForge `mods.toml` (flat and `[[mods]]` array formats), legacy `mcmod.info`, and `META-INF/MANIFEST.MF` fallback with `Mod-ID` header; source scan detects Java/Kotlin/Scala; build system detects Gradle/Maven; Windows zip path backslash normalization; 8 unit tests + integration test over 9-fixture mod corpus

## [0.1.0] - 2026-11-18

### Added
- Initial release placeholder
