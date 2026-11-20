# ADAPTER_SDK — adding technologies and loader/version support

Phase 9, ADR 0012. Most additions are **data**, not code.

## 1. Detect a new technology (no code)
Add a `[[rule]]` to `core/src/adapter_rules.toml`:
```toml
[[rule]]
tech = "my-tech"            # adapter id
level = "detect"            # detect | report | migrate
handling = "manual"         # auto | assisted | manual
message = "What the user must know or do"
files = ["ext"]             # file extensions, and/or
names = ["exact-name.cfg"]  # exact file names
any = ["pattern"]           # optional: one of these must occur in the file
```
Add a fixture line to `rules_detect_each_technology` in `core/tests/adapters.rs`.

## 2. Support a new loader/Minecraft pair for conversion (no code)
Add a `[[template]]` to `core/src/loader_templates.toml` with `loader`, `minecraft`,
`gradle`, `settings_gradle`, `gradle_properties`, `build_gradle`. Take versions from the
official template for that version (NeoForge MDK, Fabric example mod) and say where in the
file header. Placeholders: `@MOD_ID@ @VERSION@ @GROUP@ @USER_DEPS@ @ACCESS@`.

## 3. Rename loader APIs (no code)
Built-in: `[[rename]]` rows in `core/src/loader_rules.toml` (`pair = "forge->neoforge"`).
Per project/KB: mappings with `loader = "<from>-><to>"` (planner adds them as a last stage).
Member renames for Mixins: mappings with `from_api = "pkg.Owner#name"`.

## 4. Write a code adapter
Implement `ummp_core::adapter::TechAdapter`:

| Method | Use |
|---|---|
| `id`, `level` | name and how far it goes |
| `findings(cx)` | what was found, with file and line |
| `owns(cx, rel)` | files you rewrite yourself (generic handlers skip them) |
| `rewrite(cx, rel, text)` | change source text after the generic renames |
| `edits(cx)` | new/changed files (`FileEdit`, empty `original` = create) and `deletes` |

Register it in `AdapterRegistry::default()`. Rules: read files only through
`AdapterContext::text` (path-checked, size-capped); never write — return edits; tests use
real file formats and synthetic names (CLAUDE.md).

## Current adapters
`ummp adapters` prints the live list. Levels today: 15 rule adapters (detect/report/migrate),
`native-libs` (report), `loader-conversion` (migrate).
