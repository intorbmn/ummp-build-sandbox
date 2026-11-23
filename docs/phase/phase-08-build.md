# Phase 8 — Build & Testing Engine

Design: `docs/superpowers/specs/2026-11-19-phase8-build-test-design.md`. Plan:
`docs/superpowers/plans/2026-11-19-phase8-build-test.md`. Decision: ADR 0011.
Skills used: `superpowers:brainstorming`, `writing-plans`, `executing-plans`,
`test-driven-development`, `systematic-debugging`, `karpathy-guidelines`.
`rust-cli-skills:rust-processes`/`rust-concurrency` and `claude-security` were not loaded
in this session; their practices (timeouts, process-tree kill, env scrub, untrusted-input
handling) were applied by hand.

## Milestones
| ID | Milestone | Status | Evidence |
|---|---|---|---|
| 8.A | Process runner: timeout, tree kill, env scrub, watch-and-send | COMPLETED | `build/process.rs` 4 tests; mutation-checked (tree kill) |
| 8.B | Log and crash analyzers | COMPLETED | `build/logs.rs` 6 tests on real-format fixtures; mutation-checked |
| 8.C | Malware risk scan (`ummp scan`) | COMPLETED | `core/tests/scan.rs` 9 tests on `javac`-compiled classes, nested JARs, corrupt class, native lib, obfuscation; mutation-checked |
| 8.D | Local backend, JDK finder, report | COMPLETED | `core/tests/build_local.rs`: real Gradle build/test/server of `build-demo` (3/3), compile error skips later stages, missing Gradle; mutation-checked (eula) |
| 8.E | GitHub Actions backend | COMPLETED | `core/tests/build_github.rs` 6 tests (fake `gh`/`git`, golden workflow, job-log fallback). **Real run** 2026-11-19 on private `intorbmn/ummp-build-sandbox` (run 44480694285): build/test/server 3/3. Artifact download was blocked by the sandbox proxy; the stage results came from the job log |
| 8.F | CLI `scan`, `build` | COMPLETED | Smoke: scan exit 0 (clean) and 2 (High); High-risk `--local` build refused; `build --local` 3/3; JAR input refused |

## Phase 8 completion report
- **Implemented:** 8.A–8.F. 148 tests; clippy and fmt clean on the pinned toolchain.
- **Verified on real GitHub Actions** (8.E).
- **Deferred:** client launch, Maven, native builds, Docker isolation, JDK download, dynamic
  analysis → `docs/BACKLOG.md`.
- **Entry condition for Phase 9:** user review of Phase 8.
