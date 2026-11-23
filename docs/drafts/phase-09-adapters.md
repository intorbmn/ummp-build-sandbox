# Phase 9 — Advanced Technology Adapters (DRAFT)

> Superseded by the approved spec once accepted: `docs/superpowers/specs/2026-11-19-phase9-adapters-design.md`.

## Intent
Mods are not only Java. Today UMMP renames APIs in Java/Kotlin source (token rewriter,
ADR 0007), class names in bytecode (ADR 0009), and Mixin config/targets. Everything else is
invisible. Phase 9 makes each technology a pluggable **adapter** that can at least detect and
report, and migrate only where that is safe and testable.

## Adapter levels (each adapter declares one)
1. **Detect:** knows the technology is present (files, build plugins, JAR entries).
2. **Report:** lists what will break on the target version (APIs used, native ABI, shaders).
3. **Migrate:** produces `FileEdit`s through `MigrationEngine` (checkpoint, ADR 0005/0006).

## Proposed trait (core, `core/src/adapter/`)
```rust
pub trait TechAdapter: Send + Sync {
    fn id(&self) -> &'static str;                 // "kotlin", "jni", "shader"...
    fn level(&self) -> AdapterLevel;              // Detect | Report | Migrate
    fn detect(&self, project: &ProjectView) -> Option<Detection>;
    fn report(&self, project: &ProjectView, plan: &MigrationPlan) -> Vec<Finding>;
    fn edits(&self, project: &ProjectView, plan: &MigrationPlan) -> Result<EditSet, AdapterError>;
}
```
`SourceTransformer` (existing) becomes an implementation detail of language adapters.
A registry lists adapters; the analyzer and migration report gain an "Adapters" section.

## Candidate adapters and realistic first level
| Adapter | First level | Notes |
|---|---|---|
| Kotlin | Migrate | Already renamed by the token rewriter; add `kotlinc`-based compile check and Kotlin-specific syntax (string templates, backticks) |
| Groovy / Scala | Report → Migrate | Same token approach; Scala needs care with `implicit`/operators |
| JNI/JNA | Report | Find `native` methods, `System.loadLibrary`, bundled libs per OS/arch; report missing platforms. Never edit binaries |
| C/C++/Rust native | Detect/Report | Detect CMake/Cargo; build via Phase 8 runner later (BACKLOG) |
| Rendering | Report | Detect use of rendering APIs that changed (needs KB data: mapping kind `rendering`) |
| Shader | Report | GLSL files under `assets/*/shaders`; check `#version`, removed uniforms (KB) |
| Networking | Report | Packet/payload registration patterns that change between versions (KB) |
| Custom classloader | Report | Already a risk signal in `scan`; report as compatibility risk too |
| Advanced Mixin | Migrate | Member mappings for injector targets (`method = "..."`), `@Accessor`/`@Invoker` names; needs member-level mappings (BACKLOG item) |

## Deliverables mapping
- Adapter SDK: the trait + registry + `docs/ADAPTER_SDK.md` + a template adapter.
- Adapter tests: each adapter with real-format fixtures (synthetic names) and one mutation check.
- Compatibility documentation: generated table of adapters × level × tested versions.
- Demonstration projects: `tests/fixtures/demo-<tech>/` built by Phase 8 `build --local`.

## Open questions (for brainstorming)
- Which adapters matter most for your real mods (e.g. the Fabric 26.3 port)?
- Do plugins (external crates/processes) need to provide adapters already in Phase 9, or is
  an in-tree registry enough until Phase 12/14?
- Where does rendering/shader/network knowledge come from (hand-curated KB rows vs. diffing
  versions)?

## Risks
Scope is huge (11 technologies). Without real mapping data most adapters can only report.
Cut to 3–4 adapters at Migrate/Report level plus the SDK, and leave the rest at Detect.
