# Phase 10 — Modpack Migration (guide for Claude Code)

Read [README.md](README.md) first: it covers the workflow, the rules and the environment.
This file covers what is specific to Phase 10: what to build, in which order, and how to
prove it. The earlier rough design is in `docs/drafts/phase-10-modpack.md` (history only).

## 0. Skills
- **Always:**
  - `superpowers:brainstorming` → `superpowers:writing-plans` →
    `superpowers:executing-plans` + `superpowers:test-driven-development`
  - `andrej-karpathy-skills:karpathy-guidelines`
  - `rust-cli-skills:rust-errors`, `rust-idiomatic`, `rust-testing`
- **Phase-specific:**
  - `rust-cli-skills:rust-concurrency` (parallel per-mod analysis)
  - `rust-cli-skills:rust-performance`
  - `rust-cli-skills:rust-filesystem` (archive extraction, output layout)
  - `ecc:architecture-decision-records`
- **Finish:** `superpowers:verification-before-completion`, `ecc:rust-review`.

## 1. Goal (master prompt, Phase 10)
Migrate a whole modpack, not only one mod:
- Import the common pack formats.
- Build the dependency graph between mods.
- Find conflicts and missing or incompatible mods.
- Decide what to do with each mod:
  - keep it;
  - port it (`port::port`, when the user has its source);
  - update it to a version that exists for the target;
  - replace it;
  - remove it;
  - or leave it to the user.
- Migrate the config files.
- Export a new pack.

The user's product intent: the tool ports any kind of mod across versions and loaders.
Packs are the most common real request.

**Non-goals:**
- Porting closed-source JARs of a whole pack to a new Minecraft version: list them as manual.
- Launcher integration.
- Downloading JARs without the user's consent.

## 2. What exists that you reuse (read these files before designing)
- **`core/src/analyzer.rs`, `core/src/metadata.rs`**:
  - `AnalyzerEngine::analyze(root, &fs)` reads every JAR descriptor (fabric.mod.json,
    mods.toml, neoforge.mods.toml, quilt.mod.json, plugin.yml, …) into `ModInfo`.
  - It already fills `dependency_graph` and `compatibility` for a folder of JARs.
  - Phase 10 builds on that; it does not write a second parser.
- **`core/src/scan.rs`**: `scan_path` gives malware risk per JAR.
  - Every imported pack is scanned **first**.
  - A High finding stops the pack unless `--allow-risk`, same as `check_risk` in the CLI
    for `ummp port`.
- **`core/src/knowledge.rs`**: compatibility rows (`compatibilities_by_loader`).
- **`core/src/port.rs`**: `port::port(&PortOptions, &kb)` for mods the user has source
  for. It also has the zip-slip pattern (`enclosed_name` + `resolve_within`); copy it.
- **`core/src/adapter/`**: `AdapterRegistry::scan_dir` gives technology findings
  (mixins, access wideners, Kotlin) for the per-mod risk column.
- **`core/src/build/`**: the GitHub backend and the workflow template, for an optional
  server launch test (§7, 10.G).

## 3. Questions for the user (one message; recommended default in brackets)
1. Which formats first? [Modrinth `.mrpack` and plain folder now; CurseForge zip and
   Prism/MultiMC instance read-only]
2. May UMMP look up mods online (Modrinth API) to find versions for the target?
   [Yes, read-only, only with `--online`; offline by default]
3. May it download replacement JARs? [No. The exported `.mrpack` index carries the URLs and
   hashes, and the launcher downloads them; that is how the format works]
4. Output folder? [`D:\Claude\ummp-packs\<pack>-<mc>-<loader>-<time>\`, `~/ummp-packs/…`
   elsewhere]

## 4. Formats (fixtures must use these real schemas)

### 4.1 Modrinth `.mrpack` (a zip)
Fetch https://support.modrinth.com/en/articles/8802351-modrinth-modpack-format-mrpack and
check the details before coding. Shape of `modrinth.index.json`:
```json
{
  "formatVersion": 1, "game": "minecraft", "versionId": "1.0.0", "name": "Demo Pack",
  "summary": "optional",
  "files": [
    { "path": "mods/demowidget-1.0.0.jar",
      "hashes": { "sha1": "<40 hex>", "sha512": "<128 hex>" },
      "env": { "client": "required", "server": "required" },
      "downloads": ["https://cdn.modrinth.com/data/<project>/versions/<version>/demowidget-1.0.0.jar"],
      "fileSize": 12345 }
  ],
  "dependencies": { "minecraft": "1.20.1", "fabric-loader": "0.15.11" }
}
```
- `dependencies` keys: `minecraft`, `forge`, `neoforge`, `fabric-loader`, `quilt-loader`.
- `env` values: `required`, `optional`, `unsupported`.
- `overrides/`, `client-overrides/` and `server-overrides/` are copied into the instance
  root: configs, scripts, extra JARs in `overrides/mods/`.
- **Security:**
  - `files[].path` is untrusted. Use `resolve_within(instance_root, path)` and reject
    absolute paths, drive letters and `..`.
  - The spec allows only some download hosts (cdn.modrinth.com, github.com,
    raw.githubusercontent.com, gitlab.com). Keep that list as data and warn on any other
    host.

### 4.2 CurseForge zip
`manifest.json`:
```json
{
  "minecraft": { "version": "1.20.1", "modLoaders": [{ "id": "forge-47.2.0", "primary": true }] },
  "manifestType": "minecraftModpack", "manifestVersion": 1, "name": "Demo Pack",
  "version": "1.0.0", "author": "demo",
  "files": [{ "projectID": 123, "fileID": 456, "required": true }],
  "overrides": "overrides"
}
```
- Files are ids only: no JAR, no hash.
- Offline, UMMP can only list them as "unresolved". Resolving them needs the CurseForge API
  key, so it goes to the BACKLOG unless the user provides a key.

### 4.3 Plain folder
- An instance folder (`mods/`, `config/`, `kubejs/`, `resourcepacks/`, …) or only `mods/`.
- The source loader and version come from the JARs (majority of the descriptors), or from
  `--from-loader` / `--from-mc`.

### 4.4 Prism Launcher / MultiMC instance
- `instance.cfg` (INI) plus `mmc-pack.json`:
  ```json
  { "formatVersion": 1, "components": [
      { "uid": "net.minecraft", "version": "1.20.1" },
      { "uid": "net.fabricmc.fabric-loader", "version": "0.15.11" } ] }
  ```
- Loader uids: `net.minecraftforge`, `net.neoforged`, `net.fabricmc.fabric-loader`,
  `org.quiltmc.quilt-loader`.
- Mods live in `.minecraft/mods/` or `minecraft/mods/`.

## 5. Architecture

### 5.1 New module `core/src/modpack/` (one responsibility per file)
| File | Contents |
|---|---|
| `mod.rs` | `Pack { name, version, source: PackSource, entries: Vec<PackEntry>, overrides: Vec<PathBuf> }`; `PackEntry { path, sha1, sha512, size, env, downloads, mod_info: Option<ModInfo> }`; `pub fn import(path: &Path, work: &Path) -> Result<Pack>`, which picks the format reader |
| `formats.rs` | `read_mrpack`, `read_curseforge`, `read_instance`, `read_folder`, `write_mrpack`, `write_folder` |
| `graph.rs` | `PackGraph::build(&[PackEntry])`, giving missing required deps, duplicate mod ids, cycles, side mismatches (`env`) |
| `plan.rs` | `PackAction { Keep, Update(RemoteVersion), Port, Replace { with }, Remove { reason }, Manual { reason } }`; `plan(&Pack, &PlanInput, &dyn ModIndex) -> PackPlan` |
| `config.rs` | config migration with rules from `core/src/pack_config_rules.toml` |
| `report.rs` | `PackReport::to_markdown()`: one row per mod (id, version, action, reason, risk, technologies), then conflicts, then a summary |

### 5.2 Online lookups (a trait in core, the HTTP client in `providers/`)
```rust
pub trait ModIndex {
    /// Versions of `project` that support `mc` + `loader`, newest first.
    fn versions(&self, project: &str, mc: &str, loader: Loader) -> Result<Vec<RemoteVersion>>;
    /// The remote version for a JAR, by sha1.
    fn by_hash(&self, sha1: &str) -> Result<Option<RemoteVersion>>;
}
pub struct RemoteVersion { pub project: String, pub version: String, pub file_name: String,
    pub url: String, pub sha1: String, pub sha512: String, pub size: u64,
    pub required_deps: Vec<String>, pub env: Env }
```
- `ModrinthIndex` lives in `providers/` and uses ureq, like the AI providers. Endpoints:
  - `GET https://api.modrinth.com/v2/version_file/{sha1}?algorithm=sha1`
  - `GET https://api.modrinth.com/v2/project/{id}/version?loaders=["fabric"]&game_versions=["1.21.1"]`
- Send `User-Agent: intorbmn/universal-minecraft-mod-porter/<crate version>`; Modrinth
  requires it.
- Core tests use `FakeIndex`. Provider tests use a local `TcpListener` fake server.
- **Hashing:**
  - sha1 and sha512 are needed. Check `Cargo.lock` first.
  - If they are missing, adding RustCrypto `sha1` + `sha2` is a dependency decision:
    record it in ADR 0013.

### 5.3 Parallelism
- `std::thread::scope` over chunks of entries, with N = `available_parallelism()`; no
  new crate.
- Collect results and sort them by path, so reports are deterministic.

### 5.4 CLI (`cli/src/main.rs`, a `pack` subcommand group)
```
ummp pack inspect <pack>                      # import + analyze + graph + scan, writes nothing
ummp pack migrate <pack> --to 1.21.1 --loader fabric [--from-loader ..] [--online]
                  [--sources DIR] [--side client|server|both] [--output DIR] [--allow-risk]
                                               # writes <out>/<name>.mrpack, <out>/instance/, <out>/UMMP_PACK_REPORT.md
ummp pack export <instance-dir> --to-format mrpack
```
Exit codes:
- 0: done;
- 2: refused because of risk;
- 3: done, but some mods are left to the user.

Document them in `--help` and the README.

## 6. Decision rules (`plan.rs`)
Rules are evaluated in this order and the first match wins. Every decision carries a
reason string for the report. Replacement rules are data (`[[replacement]]` rows in
`pack_config_rules.toml`) with synthetic sample names.
1. Scan risk High and no `--allow-risk` → `Remove { "malware risk" }` and abort the
   migrate with exit code 2.
2. A replacement rule matches the mod id for the target loader → `Replace`.
3. The mod already supports the target mc (descriptor range) and the target loader →
   `Keep`.
4. `--online` is set and the index has a version for the target → `Update`.
5. `--sources DIR` has a folder or zip for this mod id → `Port` (runs `port::port`).
6. The mod's side does not match `--side` → `Remove { "wrong side" }`.
7. Otherwise → `Manual { "no version for <mc>/<loader>" }`.

Then re-check the graph of the planned pack. A kept, updated or ported mod whose required
dependency was removed or left to the user becomes `Manual { "missing <ids>" }`. Repeat
until nothing changes (the number of mods bounds the loop).

## 7. Milestones
Each milestone is test-first. ★ marks a test you must mutation-check.

### 10.A Pack import
- **Fixtures** in `tests/fixtures/modpack/`, built at test time so the hashes are real:
  - `demo.mrpack`, with `overrides/config/demowidget.toml` and `overrides/mods/extra.jar`
  - `demo-curseforge.zip`
  - `demo-prism/`, with `mmc-pack.json` and `minecraft/mods/*.jar`
  - a plain `mods/` folder
  - JARs come from the generated corpus (`tests/fixtures/generate.py`) or are built with
    `javac`.
- **Tests** in `core/tests/modpack.rs`:
  - ★ `imports_mrpack_files_and_overrides`: entries, env, mc "1.20.1", loader Fabric.
  - ★ `rejects_mrpack_with_path_traversal`: `../evil.jar`, `/abs.jar` and `C:\x.jar`
    each give an Err, and nothing is written outside the work dir.
  - `rejects_zip_slip_in_overrides`
  - `rejects_mrpack_hash_mismatch`
  - `caps_archive_entry_size` (zip-bomb guard)
  - `imports_curseforge_manifest_as_unresolved`
  - `imports_prism_instance_components`
  - `imports_plain_folder_majority_loader`

### 10.B Analysis and graph
- Materialize `mods/` in a temp dir, then analyze and scan in parallel.
- **Tests:**
  - ★ `graph_reports_missing_required_dependency`
  - `graph_reports_duplicate_mod_id`
  - `graph_reports_cycle_without_hanging`
  - `graph_reports_side_mismatch`
  - ★ `pack_with_high_risk_jar_is_refused` (reuse the High-risk fixture from
    `core/tests/scan.rs`)
  - `parallel_analysis_matches_sequential` (N=1 against N=8)
  - `analysis_is_deterministic`

### 10.C Plan with `FakeIndex`
- **Tests:**
  - ★ `plan_keeps_mod_that_supports_target`
  - ★ `plan_updates_when_index_has_target_version`
  - `plan_ports_when_source_supplied` (uses `tests/fixtures/loader/fabric-src`)
  - `plan_replaces_by_rule`
  - `plan_marks_manual_with_reason`
  - `plan_rechecks_dependencies_after_decisions`
  - `offline_mode_never_calls_index`: a `FakeIndex` that panics when called.

### 10.D Modrinth client (`providers/`)
- Record one real response by hand, rename its contents to synthetic names, and serve it
  from the fake server.
- **Tests:**
  - `by_hash_parses_version_file`
  - `versions_request_has_loader_and_game_version_params`: assert on the request line.
  - `sends_user_agent`
  - `http_429_retried_once_then_error`
  - `invalid_json_is_error_not_panic`

### 10.E Config migration
- Default: copy overrides unchanged.
- Rules can rename a file or a key (TOML, JSON, `.properties`, `.cfg`) for one mod id
  between versions.
- Rules are applied as an `EditSet` through `MigrationEngine` after a checkpoint.
- **Tests:**
  - ★ `config_rule_renames_key`
  - `config_without_rule_is_copied_unchanged`
  - `config_rollback_restores_original`

### 10.F Export, report, CLI
- `write_mrpack`:
  - kept and updated entries carry their URL and hashes;
  - ported mods and the user's local JARs go to `overrides/mods/`;
  - `dependencies` holds the target mc and loader version. The loader version comes from
    `loader_templates.toml` (data) or from `--loader-version`.
- **Tests:**
  - ★ `export_round_trips`: import(export(plan)) gives the planned entries.
  - `exported_index_has_required_fields`
  - `report_lists_every_mod_once_with_action_and_reason`
  - CLI tests for `pack inspect` and `pack migrate` offline, using the built binary like
    the existing CLI tests, including the exit codes.

### 10.G Real-world check
- Synthetic pack: 3–5 demo mods, one of them with source, Fabric 1.20.1 → Fabric 1.21.1
  offline. Expect one `Port` and the rest `Manual` or `Keep`. Record the report.
- If the user agrees to `--online`, run `pack inspect --online` on one small public pack
  the user chooses (read-only).
- **Optional:** a server launch test in the sandbox repo, with a new template
  `pack-server.yml` that has a golden test like `ummp-build.yml`. Steps:
  - install the Fabric server for the target;
  - copy `mods/`;
  - write `eula.txt`;
  - run `nogui`;
  - wait for `Done (`;
  - send `stop`.

  If it does not fit in the phase, put it in the BACKLOG.

## 8. Docs to update (same PR)
- **ADR 0013:** pack model, opt-in online index as a provider, hashing crates.
- `docs/phase/phase-10-modpack.md`, `PROJECT_STATUS.md`, `CHANGELOG.md`,
  `README.md` (`ummp pack` quick start).
- `CLAUDE.md`: layout rows for `modpack/` and `core/tests/modpack.rs`; next ADR is 0014.
- **`KNOWN_LIMITATIONS.md`:**
  - CurseForge needs a key;
  - closed JARs are not ported;
  - a server boot is not a gameplay test.
- **BACKLOG** items.

## 9. Risks
- **Big packs (300+ mods):** time `pack inspect` on 300 copies of the corpus JARs and
  record it. Profile before optimizing if it takes more than about 60 s.
- **API rate limits:** cache `by_hash` results, either as a KB table `remote_versions` or
  as a JSON cache in the output dir. Choose one and record it in the ADR.
- **Licensing:** never put downloaded third-party JARs into an exported pack; only URLs and
  hashes go there.

## 10. Backlog candidates
- CurseForge API resolution
- Prism/MultiMC export
- client launch test
- KubeJS/CraftTweaker script migration
- datapack/resource-pack `pack_format` bumps
- pack diff between two versions

## 11. Definition of done
README §7, plus:
- every ★ test is mutation-checked;
- `ummp pack inspect` and `ummp pack migrate` work offline on the fixtures;
- the real-world check is recorded in the phase file.
