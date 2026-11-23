# KNOWN_LIMITATIONS

Limitations of the current code, so nobody mistakes "it ran" for "it is correct".
Each entry names the module and the phase expected to address it.

## Analyzer (Phase 4)
- **Minecraft version fallback.** When a descriptor has no readable Minecraft version, the
  analyzer reports `1.20.1` instead of "unknown". `ModMetadata.minecraft` is not optional yet,
  so a missing version cannot be told apart from a real 1.20.1.
- **Source descriptor coverage.** Source projects are analyzed from the supported descriptor
  locations under `src/main/resources`; custom layouts and descriptors outside those paths
  can still be missed. Standalone JARs now produce a metadata-only analysis view and can be
  scanned, but cannot be ported or built without source.
- **Only the first mod** in a multi-mod `mods.toml` is reported.
- `tests/fixtures/generate.ps1` does not yet generate the three fixtures added in Phase 4
  gap closure (`fabric-classes`, `neoforge-b`, `quilt-a`); `generate.py` does.

## Knowledge base (Phase 5)
- **Seed data is illustrative, not authoritative.** `seed_knowledge()` contains sample rows
  (for example the synthetic `DemoWidget -> DemoGadget` rename and a "Fabric 1.12.2" row, which is really the
  separate Legacy Fabric project). It exists for tests. The CLI never seeds a user's KB.
- **Rows that fail to decode are skipped silently.** Query methods use
  `.filter_map(Result::ok)`; a corrupt row disappears instead of producing an error.
- No real mapping dataset (MCP, Yarn, Mojang, Parchment) is imported yet.

## Migration engine (Phase 6)
- **Covered:** source API renames, descriptors, `gradle.properties`/Java level, `pack.mcmeta`
  and 1.21 data pack folder renames, Mixin configs and descriptor strings, class-level
  bytecode remapping, tested KB recipes. **Not covered:** loader-to-loader migration;
  method/field renames (source and bytecode; needs member mappings); Gradle plugin and
  mappings versions (reported only); data pack *content* changes (e.g. recipe JSON schema
  changes); Kotlin string templates.
- **Built-in tables** (not in the KB): the Minecraft → Java table (`required_java`) and the
  1.21 data pack folder renames. Both are cited in code; update them there.
- **Pack formats** come only from the KB `pack_formats` table, which ships empty. Without
  rows, `pack.mcmeta` is left unchanged with a warning.
- **`remap-jar`** drops JAR signatures and does not touch reflection strings. Mixin
  injector method targets are reported, not verified.
- **Descriptors:** the loader requirement is updated only when the KB has a compatibility row
  for the exact target version; otherwise it is left as is with a warning. `ummp migrate
  --mappings` uses a KB with no compatibility rows, so it never updates loader versions.
  `loaderVersion` (the javafml language provider) and `mods.toml` → `neoforge.mods.toml`
  renames (NeoForge 1.20.5+) are reported, not changed. Escaped JSON strings are skipped.
- **No type resolution** (ADR 0007). A simple-name rule such as `DemoWidget -> DemoGadget` renames every
  standalone `DemoWidget` token, whichever class it refers to. After a dot it is renamed only
  when the qualifier is all-lowercase (ADR 0008), so `x.DemoWidget` on a lowercase variable
  is renamed too. Prefer qualified mappings.
- **Not rewritten:** string literals, comments, Kotlin string templates (`"${DemoWidget.ID}"`),
  and wildcard imports (`import com.example.*`).
- **Versions must be numeric** (`1.20.1`). Snapshots and pre-releases (`24w14a`,
  `1.21-pre1`) are rejected by the planner.
- **Recipes are listed, not executed.** The report names applicable recipes, but recipe
  steps are not interpreted yet.
- Files over 2 MiB or not valid UTF-8 are skipped with a warning.

## Checkpoints (Phase 3)
- Snapshots are full copies of the project (minus `.git`, `.gradle`, `build`, `target`,
  `out`, `run`), so disk use grows with each `migrate --apply`. Old snapshots are never
  pruned automatically.
- Symlinks inside the project are neither snapshotted nor restored.
- `restore` does not touch the skipped directories. Build output produced after a
  checkpoint survives a rollback.

## AI agents (Phase 7)
- No streaming; requests use a 10-minute HTTP timeout.
- No live API test in CI; the providers are tested against a local fake server only.
- Edits are search/replace; the model must quote text exactly once per file.
- Context is the files the rule-based migration touched, up to `context_chars`; larger
  files are skipped, not summarized.
- `ai-debug` needs a build or crash log supplied by the user (no build runner until Phase 8).
- ureq does not read `HTTPS_PROXY`.
- OpenAI-compatible servers differ in `json_schema` support; some ignore it.
- Agent memory across sessions is not implemented.

## Build & malware scan (Phase 8)
- If the network blocks GitHub artifact storage, `build` falls back to the job log: stage results only, no per-stage logs or test counts. Each build leaves a `ummp/<mod>-<time>` branch in the build repo (not deleted automatically).
- Local builds are isolated at process level only (copy, env allow-list, timeout, tree kill);
  a malicious build script still runs with your user's rights. Prefer the GitHub backend.
- The env allow-list drops proxy variables, so local builds behind a proxy may fail.
- Server check = a `Done (` line appears; client launch is not checked.
- Gradle only; JAR-only mods are scanned, not built.
- The scan is static: obfuscated or novel malware can pass, and native libraries and
  reflection give false positives. "No risk indicators" is not proof of safety.
- Full list of deferred items: `docs/BACKLOG.md`.

## Adapters and loader conversion (Phase 9)
- Loader conversion is structural: descriptor, entry bridge, build template, Mixin/AT/AW
  registration, a few package renames. Loader APIs (events, registries, networking, config)
  are listed as `assisted` with file:line, not rewritten.
- Build templates exist only for Minecraft 1.21.1 (NeoForge ModDevGradle, Fabric Loom);
  other versions and Forge targets get a `manual` finding for the build files.
- Descriptor dependency ranges without KB data are permissive (`[0,)`, `*`).
- Technology detection is pattern-based (first match per rule); rendering, shader and
  networking are detected, not migrated.
- The analyzer reads source descriptors only from `src/main/resources`.
- `scan` does not inspect `gradle/wrapper/gradle-wrapper.jar` (one Medium finding instead);
  GitHub builds validate it, local builds need `--yes`.
- Full list: `docs/BACKLOG.md`.

## Modpacks (Phase 10)
- "Supports the target" means exact Minecraft version equality plus the same loader; version
  ranges are not widened. A JAR whose descriptor cannot be read (the analyzer falls back to
  1.20.1) must not be trusted to fit a target.
- Closed JARs are never ported; only mods whose source you supply with `--sources` are.
  A ported mod is left out of the exported pack (you build it and add it yourself).
- UMMP never downloads JARs. An updated or replacement mod is written as URL + hash; folder,
  Prism and CurseForge outputs cannot hold URL-only entries and say so in the report.
- CurseForge: no API key, so entries stay unresolved and CurseForge packs only list entries
  that already had project/file ids. A Modrinth-only update cannot be written there.
- The mrpack hash check covers files bundled in the zip; listed downloads are verified by the
  launcher. Modrinth `env` is defaulted to required/required when the index omits it.
- `client-overrides/` and `server-overrides/` are merged into one overrides tree (a later
  file wins and a warning is added).
- Update versions' own required dependencies are not added; range upper bounds are
  under-reported.
- A server start is not a gameplay test, and no exported pack has been launched yet.
- Config migration knows only the rules in `pack_config_rules.toml` (synthetic sample data).

## Desktop application (Phase 11)
- The Windows installer is unsigned. Do not use it on a production machine; code signing is outside Phase 11.
- The Windows release workflow is manual (`workflow_dispatch`). It has not yet produced a verified installer in this phase.
- A real Windows install, launch, and sample-mod smoke test still requires a maintainer machine; automated UI tests do not replace this check.
- Build cancellation is cooperative between pipeline steps. A currently running Gradle or GitHub Actions build is not interrupted.
- The first implementation uses the default Tauri icons. Branding and screenshots remain follow-up work.

## Phase 12 (in progress)
- **Interim AI credential storage is plaintext.** `credentials.json` is shared by desktop and
  CLI. Unix mode 0600 and the per-user application-data directory reduce exposure, but any
  process running as the same user can read the key. This is temporary and must not be
  described as encrypted or equivalent to an OS keyring. The final storage backend remains
  a user decision (ADR 0015).
- Phase 12.D GitHub Action is implemented and smoke-tested. Phase 12.E plugin support is verified by CI #697. Plugins still run as the current OS user and are not OS-sandboxed. Plugins run as the current OS user: scrubbed environment and process-tree timeout do not provide an OS sandbox, so a malicious executable can still access files and processes permitted to that user.

- **REST API is local and single-user only.** Remote binding is an explicit opt-in and is not protected by TLS; do not expose it to untrusted networks. There are no AI routes, token-rotation command, or multi-user authorization model. Phase 12.B.
- **REST job events are bounded in-memory replay.** Events older than the server's 4096-event buffer are not replayed to late subscribers; persisted job state remains available through `GET /v1/jobs/{id}`. Phase 12.B.
- **REST path checks have a time-of-check/time-of-use gap.** A path is canonicalised and checked against `allowed_roots` before use; a local process that swaps a directory for a symlink in between could escape. The bearer token is printed once to the terminal when `ummp serve` starts.

## Not started
Phase 13 is in progress; Phase 14 (community) has not started.


## MCP server (Phase 12.C)
- MCP is a local stdio child process, not a remote or multi-user service. A compatible client must launch it.
- The caller is responsible for obtaining human approval before invoking `ummp_port` with `approve=true` or `ummp_rollback` with `confirm=true`; stdio itself cannot prove that a human reviewed the action.
- Job reports expose the persisted job result and recent log tail only; they are not a full archival report store.
- Local builds are permitted only when the application risk check allows Low risk, but build scripts still run with the user's OS permissions. Prefer GitHub Actions.
- MCP uses the same canonical path validation as REST and therefore shares its time-of-check/time-of-use gap. No real Claude Code client smoke test has been recorded yet; see `docs/MCP.md`.

## Phase 13 production readiness (in progress)

- **13A benchmarks:** Criterion workloads exist, but no baseline timings or performance targets have been recorded. The guide requires measurements on a named machine and targets agreed before optimization claims.
- **13C security:** RustSec and cargo-deny checks pass in the integrated CI, but the full repository security review is reserved for the RC commit and has not been completed.
- **13D fuzzing:** Nine fuzz targets cover class files, descriptors/YAML, mrpack, ZIP import, Gradle logs, crash reports, plugin protocol, REST DTOs and MCP arguments. The workflow runs 10-minute campaigns; a Phase 13H push touching `fuzz/**` or `.github/workflows/fuzz.yml` triggers all nine; documentation-only pushes do not. No campaign may be called clean until its workflow run completes and artifacts are triaged. The `plugin.yml` arm tests YAML decoding only; UMMP does not migrate Spigot/Paper descriptors.
- **13E recovery:** Recovery restores the latest pre-transform checkpoint for a supplied session UUID. It is rollback-only; it does not continue/replay migration because the exact edit plan is not persisted. `AppServices::open` marks persisted `Running` jobs as interrupted. Snapshots exclude `.git`, build output and other `SKIP_DIRS`. A real process-kill smoke test remains pending.
- **13F distribution:** cargo-dist configuration and PR plan are validated. No RC tag has been published and no release artifacts have been installed by a user; Windows binaries are unsigned and may trigger SmartScreen.
- **13G update checks:** explicit and opt-in checks are implemented and stable CI passes. A live check against an actual published release and manual opt-in/throttle verification remain pending until an RC/stable release exists.
- **13H documentation:** English and Vietnamese user guides are present; local Markdown links in README and both guides resolve, and examples were cross-checked against Clap argument definitions. Running a built RC binary's `--help` and comparing installer instructions with actual platform artifacts remain pending.
- Incremental modpack migration remains unimplemented; the Phase 13B cache reuses deterministic per-JAR analysis and scan results only.
