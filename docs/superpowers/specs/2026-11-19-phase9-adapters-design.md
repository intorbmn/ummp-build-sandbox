# Phase 9 design — Advanced Technology Adapters (approach C)

## Intent
UMMP is a tool that must port **any** mod across Minecraft versions **and loaders**. Today it
renames APIs in Java/Kotlin, remaps class names in bytecode, retargets descriptors and build
scripts within one loader, and fixes Mixin configs. Phase 9 makes every other technology
visible (detect + report) through one adapter system, and does real automatic work where it
unblocks the most ports: **loader-to-loader conversion** and **Mixin member renames**.
Anything not done goes to `docs/BACKLOG.md` (the user's "separate file, update later").

## Goals
1. **Adapter SDK:** a `TechAdapter` trait, a registry, and docs/template so adding a
   technology is small.
2. **Broad layer (data-driven):** detection/report rules in `core/src/adapter_rules.toml`
   for at least: Kotlin, Scala, Groovy, C/C++, Rust, JNI/JNA, rendering, shaders, networking,
   custom classloaders, advanced Mixin, access wideners/transformers, coremods, multi-loader
   libraries (e.g. Architectury). Output: a "Technologies" section in `ummp analyze` and in
   the migration report, each finding tagged `auto` (UMMP handles it), `assisted`
   (`ai-assist` can help) or `manual`.
3. **Deep layer:**
   - **Loader conversion** Fabric/Quilt ↔ Forge ↔ NeoForge (`ummp migrate --loader <other>`):
     descriptor conversion, entrypoint bridge, build-script template, Mixin registration,
     access widener ↔ access transformer syntax, cross-loader API renames from the KB, and a
     list of every remaining source-loader API use (file:line) as `assisted`.
   - **Mixin member renames:** method/field mappings applied to `@Inject(method=…)`,
     `@Redirect`/`@ModifyArg` targets, `@At(target=…)`, `@Accessor`/`@Invoker` values, only
     inside Mixins whose `@Mixin` target owns the member.
   - **Groovy and Scala source:** the token rewriter also handles `.groovy`/`.scala`
     (triple-quoted strings, Groovy `${}`), like Java/Kotlin today.
   - **Native report:** `native` methods, `System.loadLibrary`/JNA `Native.load` names, and
     bundled libraries per OS/arch, with the platforms that are missing.
4. **Demonstration:** a minimal real Fabric mod for one Minecraft version is converted to
   NeoForge (and Forge if templates allow) and built + server-started on GitHub Actions with
   `ummp build` (Phase 8). Both the original and the converted mod build.

## Non-goals (→ BACKLOG)
Automatic port of loader event/registry/networking **semantics** (only renames that exist in
the KB plus the `assisted` list); shader/rendering code rewriting; building native code;
templates for every Minecraft version (ship one, data-driven so others are rows); Bukkit/Paper.

## Anti-goals
- Pretending a conversion is complete: the report must list every remaining source-loader API.
- Edits outside `MigrationEngine` (all output is an `EditSet`, applied after a checkpoint).
- Names that look like real Minecraft/loader APIs in sample KB data (keep `DemoWidget` style);
  real loader **package/plugin names** in templates and detection rules are fine, since they
  are what the tool must recognize.

## Approaches (picked: C)
- A. Breadth only: every technology detected via data rules; little automatic work.
- B. Depth only: loader conversion + Mixin + languages; other technologies stay invisible.
- **C. Both:** data-driven broad layer + deep loader conversion and Mixin. Picked by the user.

## Design
### Core types (`core/src/adapter/`)
```rust
pub enum Handling { Auto, Assisted, Manual }
pub struct TechFinding { pub tech: String, pub handling: Handling, pub file: String,
                         pub line: Option<u32>, pub message: String }
pub struct AdapterContext<'a> { pub root: &'a Path, pub files: &'a [PathBuf],
                                pub plan: Option<&'a MigrationPlan>, pub source_loader: Option<&'a str> }
pub trait TechAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn findings(&self, cx: &AdapterContext) -> Vec<TechFinding>;
    fn edits(&self, cx: &AdapterContext) -> Result<EditSet, AdapterError> { Ok(EditSet::default()) }
}
pub struct AdapterRegistry { adapters: Vec<Box<dyn TechAdapter>> } // default(): rules + built-ins
```
- `RuleAdapter` loads `adapter_rules.toml` (same matching style as `scan_rules.toml`:
  file extensions/globs + any-of/all-of patterns, plus a `handling` and a message).
- `MigrationEngine::compute_edits` merges adapter `EditSet`s after the existing transforms
  (conflicts on the same file: adapters run on the already-transformed text, in registry
  order). `MigrationReport` and `AnalysisReport` gain `technologies: Vec<TechFinding>`.
- `EditSet` needs **file creation and deletion** (new descriptor, new entry class, removed
  old descriptor). `FileEdit` with empty `original` = create; a new `deletes: Vec<PathBuf>`.
  Rollback must remove created files and restore deleted ones — verify
  `CheckpointStore::restore`, extend it if it only copies files back.

### Loader conversion (`core/src/adapter/loader.rs`)
- Source loader from the analyzer; target from `--loader`. Today `migrate_descriptor` skips
  with "loader-to-loader migration is not supported" — that path now calls the converter.
- **Descriptor:** read id, version, name, description, authors, license, entrypoints,
  mixins and dependencies from the source descriptor; write the target descriptor
  (`fabric.mod.json` / `META-INF/mods.toml` / `META-INF/neoforge.mods.toml`); delete the old
  one. Dependency ids are mapped through KB rows (`kind = "dependency"`, e.g. the loader's
  API library); unknown ones are kept and reported `manual`.
- **Entrypoint bridge:** generate one small class in the mod's package for the target loader
  that calls the existing initializer(s) (e.g. NeoForge `@Mod(id)` class whose constructor
  calls the Fabric `onInitialize()` logic; Fabric `ModInitializer` that calls a Forge
  `@Mod` class's setup). Source-loader interfaces/annotations left in the old class are
  reported `assisted` with file:line.
- **Build script:** replace `build.gradle`/`settings.gradle`/`gradle.properties` loader
  parts from a template in `core/src/loader_templates.toml` keyed by `(loader, minecraft)`
  (plugin id + version, repositories, run configs, property names). Unknown pairs: no build
  edit, `manual` finding. User dependencies are carried over and listed for review.
- **Mixins:** keep the JSON configs; register them the target way (Fabric `mixins` array,
  (Neo)Forge `[[mixins]] config=`); refmap entries reported when namespaces differ.
- **Access:** `.accesswidener` ↔ `accesstransformer.cfg` line conversion when both use the
  same mapping namespace; otherwise `manual`.
- **APIs:** KB mappings with `loader = "<from>-><to>"` become rename rules for the token
  rewriter; afterwards, every remaining import/reference of the source loader's packages is
  an `assisted` finding.

### Mixin member renames (`core/src/mixin.rs`)
Mapping kind `method`/`field` with `from_api = "pkg.Owner#name"`, `to_api = "pkg.Owner#newName"`.
Applied only to string values of the listed annotation attributes in a Mixin class whose
`@Mixin` target is `Owner` (or whose `@At(target=…)` names `Owner`). Descriptors in strings
(`name(Lpkg/Type;)V`) keep their descriptor part, which class renames already handle.

### Source in, ported source out (user requirement)
A user hands UMMP their mod source and gets the ported source back, without touching the
original:
- `ummp port <input> --to <mc> [--loader <loader>] [--output <dir>] [--zip]`
- `<input>` is a source folder or a `.zip` of one (zip entries go through `resolve_within`).
- UMMP copies the input to a new output folder: by default
  `D:\Claude\ummp-ports\<mod>-<mc>-<loader>-<time>` on Windows and `~/ummp-ports/...`
  elsewhere. It then runs the whole pipeline (version migration, adapters, loader
  conversion) on the copy through `MigrationEngine`.
- The copy also receives `UMMP_REPORT.md` (diff summary, Technologies, every `assisted`/
  `manual` item with file:line). `--zip` additionally writes `<output>.zip`.
- The original is never modified. `migrate --apply` keeps its in-place behaviour with
  checkpoints.
- Optional `--build`: runs Phase 8 `ummp build` on the output and appends the result to the
  report.

### CLI
- `ummp analyze` prints the Technologies section.
- `ummp migrate --loader neoforge` on a Fabric mod performs the conversion (dry run by
  default, `--apply` with checkpoint, `rollback` works).
- `ummp adapters` lists adapters, their level (detect/report/migrate) and tested versions
  (this is the generated compatibility documentation).

## Testing
- Rule adapter: one fixture per technology (synthetic names, real file formats).
- Loader conversion: golden outputs for Fabric→NeoForge, NeoForge→Fabric, Forge→NeoForge
  on fixture projects; rollback restores byte-identical, including created/deleted files.
- Mixin members: compile a real Mixin-style source with annotations (javac with stub
  annotation types) and check only the right strings change; mutation-check the owner guard.
- Groovy/Scala: tokenizer tests for triple quotes and `${}`.
- `port`: input folder and input zip both produce an output folder (and zip) with the ported
  source and `UMMP_REPORT.md`; the input stays byte-identical; a zip entry with `../` is
  rejected.
- Demonstration: `tests/fixtures/demo-fabric-mod/` (minimal real mod, one Mixin, one
  initializer); manual evidence = `ummp build` on GitHub before and after conversion,
  recorded in the phase file.

## Calls made while writing (check these)
- Remaining-work list stays in `docs/BACKLOG.md` (already the separate file for this).
- First templates target one Minecraft version (1.21.1 proposed); Forge templates only if
  time allows, otherwise BACKLOG.
- Quilt is treated as Fabric (it reads `fabric.mod.json`); a `quilt.mod.json` target is BACKLOG.

## Decisions to record
ADR 0012: adapter architecture (rules data + trait) and the loader-conversion contract
(structural conversion + KB renames + assisted list; no semantic rewriting).

## Left to the builder
File layout inside `adapter/`, TOML schema details, exact report wording, whether the demo
mod uses a Mixin that both loaders accept without refmap changes.
