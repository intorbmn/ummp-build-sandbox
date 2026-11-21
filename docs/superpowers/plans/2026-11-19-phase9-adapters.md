# Phase 9 Advanced Technology Adapters Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Adapter SDK + data-driven technology detection for every mod, plus real loader conversion (Fabric/Quilt ↔ NeoForge, Forge ↔ NeoForge), Mixin member renames, Groovy/Scala renames, native report, and `ummp port` (source in → ported source out).

**Architecture:** `core/src/adapter/` (trait, registry, rule adapter, native, loader converter) plugs into `MigrationEngine::compute_edits`; adapters may *own* files (the main loop skips them) and create/delete files. `core/src/port.rs` copies input → output and runs the pipeline there. CLI wires `port`, `adapters`, and the Technologies section.

**Tech Stack:** Rust 1.97, serde_json, toml 0.8, zip 0.6; javac; Gradle on GitHub Actions for the demo.

**Spec:** `docs/superpowers/specs/2026-11-19-phase9-adapters-design.md`

## Global Constraints
- All writes via `EditSet` + `MigrationEngine` (checkpoint); `port` never modifies its input.
- Paths from input (zip entries, descriptor values used as paths) go through `resolve_within`.
- Handling labels exactly: `auto`, `assisted`, `manual`.
- Default port output: `D:\Claude\ummp-ports\<mod>-<mc>-<loader>-<yyyymmdd-hhmmss>` (Windows), `~/ummp-ports/...` elsewhere.
- Synthetic sample names (`DemoWidget`, `demowidget`); real loader package/plugin names allowed in templates and rules.
- Unfinished items → `docs/BACKLOG.md`.
- Four CI commands green before each commit.

## Review Focus
1. Zip input with `../evil` entry → rejected, nothing written (Task 6 `port_rejects_zip_slip`).
2. Fabric mod whose main class has extra interfaces (`implements ModInitializer, DemoApi`) → only `ModInitializer` removed (Task 4 `strip_keeps_other_interfaces`).
3. Loader conversion with no template for the target version → descriptor/entry still converted, build untouched, `manual` finding (Task 5 `no_template_is_manual`).
4. Rollback after conversion restores deleted descriptor and removes created files (Task 4 `conversion_rollback_identical`).
5. Member mapping for a different owner must not touch a Mixin targeting another class (Task 3 `member_rename_owner_guard`).

---

### Task 1: Adapter core, rule adapter, deletes in EditSet
**Files:** create `core/src/adapter/{mod.rs,rules.rs}`, `core/src/adapter_rules.toml`; modify `core/src/migration.rs` (EditSet `deletes`, apply, report `technologies`), `core/src/lib.rs`, `core/src/analyzer.rs` (report `technologies`).
**Produces:**
```rust
pub enum Handling { Auto, Assisted, Manual }            // serde snake_case
pub struct TechFinding { pub tech: String, pub handling: Handling, pub file: String, pub line: Option<u32>, pub message: String }
pub struct AdapterContext<'a> { pub root: &'a Path, pub files: &'a [PathBuf], pub plan: Option<&'a MigrationPlan>, pub source_loader: Option<String> }
pub trait TechAdapter: Send + Sync { fn id(&self)->&'static str; fn level(&self)->Level; fn findings(&self, cx:&AdapterContext)->Vec<TechFinding>;
  fn owns(&self, _cx:&AdapterContext, _rel:&Path)->bool { false } fn edits(&self, _cx:&AdapterContext)->Result<EditSet, MigrationError> { Ok(EditSet::default()) } }
pub enum Level { Detect, Report, Migrate }
pub struct AdapterRegistry; // fn default_set()->Self, fn findings(&self,cx)->Vec<TechFinding>, fn list(&self)->Vec<(&str, Level)>
EditSet.deletes: Vec<PathBuf>; MigrationReport.technologies, deletes; AnalysisReport.technologies
```
- [ ] Tests: `rules_detect_each_technology` (fixture dir with one file per tech: `.kt`, `.scala`, `.groovy`, `CMakeLists.txt`, `Cargo.toml`, JNI `native` method, shader `.fsh`/`.vsh`/`.glsl`, networking payload class, classloader, `.accesswidener`, `accesstransformer.cfg`, coremod `coremods.json`, Architectury import) → one finding each with file and line; `delete_and_create_apply_and_rollback` (engine applies a create + delete, restore is identical).
- [ ] Commit `feat(adapter): adapter SDK, rule-based technology detection, file create/delete`.

### Task 2: Native report adapter, Groovy/Scala renames
**Files:** create `core/src/adapter/native.rs`; modify `TokenRenameTransformer` (`applies_to` groovy/scala, `"""` strings).
- [ ] Tests: `native_lists_missing_platforms` (libs `natives/windows-x86_64/demo.dll`, `natives/linux-x86_64/libdemo.so` → `manual` finding naming macOS); `native_methods_and_load_library_reported`; `groovy_and_scala_renamed_not_in_triple_strings`.
- [ ] Commit `feat(adapter): native report; Groovy/Scala renames`.

### Task 3: Mixin member renames
**Files:** modify `core/src/mixin.rs` (`migrate_mixin_members(content, rules) -> EditResult`), planner keeps `#` rules out of token stages (they go to members only).
- [ ] Tests: `member_rename_inject_accessor_invoker_at` (javac-compiled stub annotations source; `method = "tickDemo"`, `@Accessor("demoSize")`, `@Invoker("runDemo")`, `@At(target = "Lcom/example/demo/DemoWidget;tickDemo()V")` renamed); `member_rename_owner_guard`. Mutation-check the guard.
- [ ] Commit `feat(mixin): member renames for injector targets, accessors and invokers`.

### Task 4: Loader converter (descriptor, entrypoint, mixins, access, KB renames, leftovers)
**Files:** create `core/src/adapter/loader.rs`, fixtures `tests/fixtures/loader/{fabric-src,neoforge-src,forge-src}/`, goldens; modify `MigrationRequest` (`source_loader: Option<String>`, serde default), planner (`<from>-><to>` mapping stage), `compute_edits` (owned files skipped, adapter edits merged).
**Produces:** `pub struct ModInfo { id, version, name, description, authors: Vec<String>, license, entry: Option<String>, mixins: Vec<String>, deps: Vec<Dep>, access: Option<String> }`; `parse_fabric(&str)`, `parse_mods_toml(&str)`, `write_fabric(&ModInfo, &Target)`, `write_mods_toml(&ModInfo, &Target, neoforge: bool)`; `LoaderConverter` (TechAdapter, Level::Migrate).
- [ ] Tests: `fabric_to_neoforge_golden` (creates `META-INF/neoforge.mods.toml` + `UmmpNeoForgeEntry.java`, deletes `fabric.mod.json`, main class loses `implements ModInitializer`, its import and `@Override`); `strip_keeps_other_interfaces`; `neoforge_to_fabric_golden`; `forge_to_neoforge_golden`; `leftover_loader_apis_are_assisted` (a `net.fabricmc.fabric.api` import remains → assisted with line); `access_widener_to_transformer`; `conversion_rollback_identical`.
- [ ] Commit `feat(adapter): loader conversion`.

### Task 5: Build templates + demo mod on real GitHub
**Files:** create `core/src/loader_templates.toml` (Minecraft 1.21.1: Fabric Loom + mojmap, NeoForge ModDevGradle; versions verified from official sources), template application in `loader.rs`; `tests/fixtures/demo-fabric-mod/` (real Fabric mod: initializer + one Mixin, Gradle wrapper).
- [ ] Tests: `template_applied_for_known_version`; `no_template_is_manual`.
- [ ] Real runs: `ummp build demo-fabric-mod` (GitHub) → build passes; `ummp port demo-fabric-mod --to 1.21.1 --loader neoforge --build` → build + server pass. Record in phase file; failures → fix or BACKLOG with evidence.
- [ ] Commit `feat(adapter): loader build templates; demo mod`.

### Task 6: `ummp port`, CLI, docs
**Files:** create `core/src/port.rs`; modify `cli/src/main.rs` (`port`, `adapters`, analyze Technologies, migrate `--from-loader`); docs: ADR 0012, `docs/ADAPTER_SDK.md`, phase file, PROJECT_STATUS, KNOWN_LIMITATIONS, CHANGELOG, README, CLAUDE.md, BACKLOG.
**Produces:** `pub struct PortOptions { input, output: Option<PathBuf>, to: String, loader: Option<String>, from: Option<String>, kb: KnowledgeBase-like source, zip: bool }`, `pub fn port(opts) -> Result<PortResult, PortError>` (`PortResult { output, zip: Option<PathBuf>, report: MigrationReport }`).
- [ ] Tests: `port_folder_leaves_input_identical` (output has edits + `UMMP_REPORT.md`); `port_zip_input`; `port_rejects_zip_slip`; `port_writes_zip`.
- [ ] CLI smoke; full gate; push; PR; CI. Do not merge.
