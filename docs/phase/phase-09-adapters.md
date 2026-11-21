# Phase 9 — Advanced Technology Adapters

Design: `docs/superpowers/specs/2026-11-19-phase9-adapters-design.md` (approach C).
Plan: `docs/superpowers/plans/2026-11-19-phase9-adapters.md`. Decision: ADR 0012.
SDK: `docs/ADAPTER_SDK.md`. Skills: brainstorming, writing-plans, executing-plans, TDD,
systematic-debugging, karpathy-guidelines (`rust-cli-skills:rust-design` not loaded in this
session; trait design done by hand).

## Milestones
| ID | Milestone | Status | Evidence |
|---|---|---|---|
| 9.A | Adapter SDK, 15 rule adapters, Technologies in analyze/migrate reports, file create/delete | COMPLETED | `core/tests/adapters.rs` (each technology with file+line; create/delete + rollback, mutation-checked) |
| 9.B | Native report; Groovy/Scala renames | COMPLETED | missing platforms, loadLibrary without bundled lib; `'''`/`"""` strings (mutation-checked) |
| 9.C | Mixin member renames | COMPLETED | javac-compiled Mixin before/after; owner guard mutation-checked |
| 9.D | Loader conversion Fabric/Quilt ↔ NeoForge, Forge ↔ NeoForge, (Fabric → Forge) | COMPLETED | `core/tests/loader.rs` goldens, leftover API list, rollback identical; mutation-checked |
| 9.E | Build templates 1.21.1 (NeoForge MDG, Fabric Loom) + demo mod | COMPLETED | template/no-template tests; demo `tests/fixtures/demo-fabric-mod` |
| 9.F | `ummp port` (source in → ported source out), `ummp adapters`, `migrate --from-loader` | COMPLETED | `core/tests/port.rs` (folder, zip, zip-slip mutation-checked, zip out, non-empty output) |

## Real GitHub runs (9.E)
All on `intorbmn/ummp-build-sandbox` (private), 2026-11-19, Java 21 runner, Gradle wrapper
validated by `gradle/actions/wrapper-validation`:

| Run | What | Result |
|---|---|---|
| 44491374602 | `ummp build tests/fixtures/demo-fabric-mod` (original Fabric 1.21.1 mod) | build, test, server start: **3/3** |
| 44492376635 | `ummp port demo-fabric-mod --to 1.21.1 --loader neoforge --build` (converted mod) | build, test, server start: **3/3** |

Found and fixed on the way (each with a test):
- Newest Fabric Loom (1.19) needs a Java 25 JVM → template uses the official 1.21.1-era
  versions (fabric-loom 1.7-SNAPSHOT, loader 0.16.2, Gradle 8.10; example-mod commit fabfcbb).
- Stage logs live in the artifact, which a proxy can block → the workflow prints the tail of a
  failed stage's log into the job log, and the fallback saves and analyzes it.
- `scan` flagged every standard `gradle-wrapper.jar` as High → one Medium "verify checksum"
  finding instead; GitHub runs validate the wrapper.
- Demo Mixin on a mod-owned class needs `remap = false` (Mixin annotation processor).
- Two runs failed on HTTP 429 from Maven Central (external rate limit); a retry passed. The
  NeoForge template now also lists `mavenCentral()` for plugin dependencies.

## Phase 9 completion report
- **Implemented:** 9.A–9.F. Deferred work: `docs/BACKLOG.md` (Phase 9 section).
- **Entry condition for Phase 10:** user review of Phase 9.
