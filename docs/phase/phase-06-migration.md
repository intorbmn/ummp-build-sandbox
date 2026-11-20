# Phase 6 — Migration Engine

| Field | Value |
|---|---|
| Phase ID | P6 |
| Objective | Execute the migration pipeline: plan, transform source/build/resources/mixins, produce diffs, rollback. |
| Scope | Version migration, loader migration, source transformation, build migration, metadata/resource/datapack/mixin/bytecode migration, diff viewer, migration report, rollback, regression tests. |
| Prerequisites | Phase 4 (analysis) + Phase 5 (recipes). |
| Modules | ENG-03. |
| Architecture | Pipeline steps behind interfaces; each step checkpointable and resumable. |
| Tasks | 6.1 Planner: build change graph from analysis + recipes 6.2 Source transformer 6.3 Build config transformer 6.4 Resource/datapack transformer 6.5 Mixin transformer 6.6 Bytecode transformer (adapter) 6.7 Diff viewer 6.8 Migration report 6.9 Rollback implementation 6.10 Regression tests on corpus |
| Dependencies | P4, P5. |
| Deliverables | Working migration pipeline, diff viewer, report, rollback, regression tests. |
| Tests | Integration: run full pipeline on corpus mods; assert transforms produce valid output. |
| Risks | Transform breaks compilation; recipe coverage gaps. |
| Acceptance Criteria | A corpus mod migrates through transform step and produces a valid diff; rollback restores original exactly. |
| Complexity | L |
| Status | COMPLETED |

## Milestones
| ID | Milestone | Status | Evidence |
|---|---|---|---|
| 6.A | Planner + Java/Kotlin API-rename transformer + diff + report + checkpointed apply + rollback + CLI | COMPLETED | `core/tests/migration.rs` (golden file, stage order, conflicts, downgrade rejection, rollback after simulated write failure); 8 tokenizer unit tests; CLI end-to-end run (dry run → apply → rollback, `diff -r` identical) |
| 6.B | Descriptor migration (`mods.toml`, `neoforge.mods.toml`, `fabric.mod.json`, `quilt.mod.json`) | COMPLETED | `core/src/descriptor.rs` (8 unit tests: style-preserving ranges, formatting kept, loader version from KB, mismatch skip, NeoForge rename warning); fixture `mods.toml` vs golden in `core/tests/migration.rs`; mutation-checked |
| 6.C | Build migration (Gradle plugin/loader/mappings versions, Java toolchain) | COMPLETED | `build_script.rs`: `gradle.properties` Minecraft/loader keys and ranges; Java level in `build.gradle(.kts)` raised to the target requirement (never lowered); mappings and Fabric API reported, not guessed. 6 unit tests + fixture |
| 6.D | Resource / datapack migration | COMPLETED | `resources.rs`: `pack.mcmeta` formats from the KB (`pack_formats` table); 1.21 singular data pack folders applied as file moves (13 renames). Integration test: moves, then rollback is byte-identical; failed-move rollback test; mutation-checked |
| 6.E | Mixin migration | COMPLETED | `mixin.rs`: `compatibilityLevel` raised; JVM descriptors, internal names and `targets` strings renamed in `@Mixin` classes; injector method targets counted and reported |
| 6.F | Bytecode migration (adapter over an external tool) | COMPLETED (class-level) | `bytecode.rs` + `ummp remap-jar` (ADR 0009): constant-pool class remapping, class files moved, signatures dropped, input untouched. `bytecode_jvm.rs` compiles with javac, remaps, then runs on a real JVM; mutation-checked |
| 6.G | Recipe execution (interpret `Recipe.steps`) | COMPLETED | `RecipeStep` (`rename`, `warn`); only `tested` recipes run, others are reported with a reason |

## Milestone 6.A report
- **Implemented:** `MigrationPlanner` (mappings in `(source, target]`, per-version stages,
  confidence threshold, conflict/no-op detection, breaking changes, recipes);
  `SourceTransformer` trait + `TokenRenameTransformer` (ADR 0007); `FileEdit` + unified diff;
  `MigrationReport` (Markdown/JSON); `MigrationEngine::run` with checkpoint-before-write and
  automatic restore on write failure (ADR 0005/0006); CLI `migrate` / `rollback`.
- **Architecture changes:** ADR 0006 (snapshot checkpoints), ADR 0007 (token-aware rewrite).
  Phase 3 checkpoint gained real `snapshot`/`restore`.
- **Known issues / technical debt:** see `docs/KNOWN_LIMITATIONS.md` → Migration engine.
- **Risks still open:** R1/R4 (no real mapping dataset yet), R5 (Mixins untouched).
- **Entry condition for 6.B:** user review of ADR 0007 rewrite policy (`should_rewrite`).
## Phase 6 completion report
- **Implemented:** 6.A–6.G (see the milestones table). CLI: `analyze`, `migrate`, `remap-jar`, `rollback`.
- **Evidence:** 92 tests (unit, integration, real-JVM), clippy `--all-targets` and fmt clean on the pinned toolchain; key tests mutation-checked.
- **Decisions:** ADR 0006–0009.
- **Open risks:** R1/R4 (no real mapping dataset in the KB yet), R5 (injector method targets need member mappings), method/field renames in bytecode (Phase 8, external remapper).
- **Known limitations:** `docs/KNOWN_LIMITATIONS.md` → Migration engine.
- **Entry condition for Phase 7:** user review of Phase 6.
