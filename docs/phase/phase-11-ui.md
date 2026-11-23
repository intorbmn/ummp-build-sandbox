# Phase 11 — Desktop UI (Tauri)

**Status:** IN PROGRESS  
**Branch:** `phase-11-ui`  
**Design:** approved; [phase 11 UI design](../superpowers/specs/2026-11-20-phase11-ui-design.md)  
**Plan:** [phase 11 UI plan](../superpowers/plans/2026-11-20-phase11-ui.md)  
**Guide:** [phase 11 guide](../guides/phase-11-ui.md)  
**Handoff:** [11.B handoff](../guides/phase-11b-handoff.md)  
**ADR:** [0014 — Desktop UI boundary and keyring](../decisions/0014-desktop-ui-boundary-and-keyring.md)

## Scope and architecture

Deliver the ten-screen desktop workflow using Svelte 5, TypeScript, Vite and Tauri 2. The UI is a presentation layer only:

`ui/src` → `ui/src-tauri` → `core/src/app/` → existing engines.

Only `ui/src/lib/api.ts` calls Tauri `invoke`. Core owns migration, planning, risk checks, jobs, cancellation, approvals, rollback, build and pack behavior. Secrets stay in the OS keyring and are never returned to the web view. In-place migration requires an explicit review acknowledgement.

## Skill availability and manual practice

The required Claude Code skills are not available in this ChatGPT session. The handoff records the user's approval to continue without installing them. Their practices are applied manually:

- Brainstorming and writing-plans: use the approved design and implementation plan as the scope boundary.
- Executing-plans and TDD: work from the task ledger; add or inspect tests before changing behavior.
- Systematic debugging and verification: trace failures to their source and do not mark work complete without recorded evidence.
- Karpathy guidelines: prefer the smallest implementation that satisfies the contract; avoid speculative abstractions and dependencies.
- Frontend design and UI/UX: retain the approved Calm workbench, keyboard accessibility, readable diffs, responsive layouts and Vietnamese/English fallback.
- Rust errors, idioms, testing, design and concurrency: keep typed errors, thin command adapters, trait-based boundaries and explicit cancellation/approval behavior.
- Web app testing: use Vitest/Testing Library and Playwright for user-visible flows.
- ADR and Rust review: record architectural choices in ADR 0014 and review security, ownership and error paths manually.
- PR/release workflow: inspect the final diff and CI evidence before opening a PR; do not merge.

## Milestone status

| Milestone | Implementation | Verification |
|---|---|---|
| 11.A Application services A1–A8 | Complete before this branch | Historical Rust build/test/clippy/fmt and mutation evidence recorded in the SDD ledger |
| B0 Sessions | Complete before this branch | Historical Rust test evidence recorded in the SDD ledger |
| 11.B Desktop shell, Home, Analysis | Complete before this branch | Verified in the green Phase 11 CI run below |
| 11.C Target, Plan/Diff, Run, Report | Implemented | Playwright flow, keyboard focus, ZIP archive mode, in-place review and rollback passed |
| 11.D Build and Settings | Implemented | Keyring boundary, GitHub token injection, risk gate, Tauri build/test/clippy/fmt passed |
| 11.E AI assistant and Pack | Implemented | AI approval, single-start behavior, 300-row golden/pagination and first-page timing passed |
| 11.F Windows packaging | Workflow added | Windows workflow has not yet been dispatched; real Windows install/smoke test remains outstanding |
| 11.G Documentation and PR | In progress | ADR 0014 and phase/status/limitations/backlog/SAD/README/changelog records aligned; draft PR follows this record |

## Desktop workflow

1. Home selects a project folder, source ZIP, or standalone JAR and lists recent session summaries.
2. Analysis shows mod metadata, technology adapter findings, and static risk findings. Standalone JARs produce a metadata-only view and can be scanned; port/build require a source folder or source ZIP.
3. Target selects Minecraft version, loader, output path, and port vs in-place mode.
4. Plan shows file tree, unified diffs with line numbers, moves, deletes, technology handling, skipped mappings, warnings, and an explicit review gate for in-place apply.
5. Run observes persisted job state and `ummp://event`; cancellation is available while a job is active. AI file proposals require explicit Approve/Reject decisions (A/R keyboard shortcuts).
6. Report shows migration/build/AI outcome, checkpoint rollback where available, and opens only the output path attached to the completed job.
7. Settings stores non-secret preferences in settings JSON and credentials in the OS keyring.
8. Build previews the core risk result; local High/Medium builds require explicit acknowledgement. GitHub builds can use a keyring token passed through `GH_TOKEN`, or existing `gh auth login`.
9. Pack migration reports per-mod decisions; 300-row reports render 50 rows per page.

## Files added or changed

- `ui/src-tauri/src/commands.rs`, `secrets.rs`, `lib.rs`: thin Tauri adapters and keyring boundary.
- `ui/src/screens/`: Target, Plan, Run, Report, Build, AI, Pack, PackReport, Settings.
- `ui/src/lib/api.ts`, `types.ts`, `components/`, `locales/`: typed invoke boundary, views, reusable UI, English/Vietnamese.
- `core/src/app/mod.rs`, `runs.rs`, `views.rs`: standalone JAR analysis and pack report item payload; optional GitHub credential support is isolated in the build backend.
- `core/src/build/github.rs`: token injected only through child-process environment; redacted by `Secret` Debug.
- `providers/src/lib.rs`: explicit OpenAI-compatible API-key constructor.
- `tests/fixtures/golden/app-views/pack-report-300.json`: shared Rust/TypeScript golden report.
- `.github/workflows/desktop-release.yml`: manual Windows MSI/NSIS packaging.
- `.github/workflows/ci.yml`: keyring system headers, warning-only npm audit, and a resolved Tauri Cargo.lock artifact.

## CI verification — passed

- `ui/src-tauri/src/commands.rs`: analysis, scan error mapping, sessions/targets, read-only plan, in-place review refusal, keyring set/has/delete with no returned secret, build risk preview, and output-path validation.
- `ui/src-tauri/src/secrets.rs`: memory-store lifecycle and credential-name allow-list.
- `core/tests/app.rs`: standalone JAR metadata analysis, pack result item serialization, and shared 300-row golden JSON.
- `core/tests/build_github.rs`: `GH_TOKEN` environment injection and Debug redaction.
- `ui/tests/phase11c.test.ts`: plan/diff/stepper and diff context collapsing.
- `ui/tests/phase11d.test.ts`: local High-risk build gate, immediate language switch, English fallback.
- `ui/tests/phase11e.test.ts`: AI approval keyboard decision and 300-row pagination.
- `ui/e2e/home-analysis.spec.ts`: Home→Analysis, reviewed in-place Run→Report→Rollback, ZIP archive mode and AI restriction, 300-row Pack report first-page timing/output-folder action, keyboard focus, and AI A/R approval events.

Latest green run on the current PR head: [GitHub Actions #44710673870](https://github.com/intorbmn/Universal-Minecraft-Mod-Porter/actions/runs/44710673870), commit `0eeafe63a2fdd04d9f25d410c3b20f8d4760d265`. Both the `push` run #44710620730 and `pull_request` run #44710673870 completed successfully; both jobs (`build-test-lint`, `desktop`) passed.

- Root Rust: 248 tests passed; build, Clippy (`-D warnings`) and rustfmt passed.
- Tauri desktop Rust: 12 tests passed; build, Clippy (`-D warnings`) and rustfmt passed.
- Vitest: 4 test files, 11 tests passed.
- Playwright: 6 tests passed, including ZIP archive mode and one-job AI start.
- Production dependency audit: 0 vulnerabilities.
- CI artifacts: `ummp-phase11-playwright` (3 screenshots) and `ummp-desktop-cargo-lockfile` (resolved desktop lockfile).

## Test and acceptance commands

Run from the repository root unless noted:

```sh
cargo fmt --all -- --check
cargo build --all
cargo test --all
cargo clippy --all --all-targets -- -D warnings

cd ui
npm ci
npm run check
npm run test
npm run e2e
npm audit --omit=dev # warning-only in CI

cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
```

The root Rust toolchain must not be changed to bypass the pin. The desktop Linux job installs GTK/WebKit and `libdbus-1-dev`. These commands were executed by the green GitHub CI run above; this session did not claim a local checkout execution.

## Outstanding acceptance gates

1. Dispatch `.github/workflows/desktop-release.yml` from branch `phase-11-ui`; verify MSI and NSIS artifacts are produced.
2. Basic desktop runtime: user reports that the application now runs at a basic level. This is a smoke-test signal only; it does not certify every Windows integration or workflow.
3. AI: NOT TESTED by the user yet. Verify provider/key configuration, request execution, proposal rendering, explicit approve/reject, and resulting job/report before marking accepted.
4. Cancellation: DEFERRED. The sample job completes too quickly to exercise the cancel path; do not count this as passed or failed. Add a deliberately long-running/cancellable test case before closing this gate.
5. Commit 2–3 screenshots from the real Windows app under `docs/screenshots/phase-11/`. The three Playwright screenshots are already captured in the CI artifact; they are browser evidence, not a replacement for Windows runtime screenshots.
6. Record installer hashes and the remaining Windows smoke-test results in this file. PR #11 was merged on 2026-11-20 so the `AppServices` facade is available on `master`; this does not waive the remaining Phase 11 acceptance gates.

## Known limitations

- Build cancellation is honored between steps; an active Gradle or GitHub Actions process is not interrupted.
- The Windows installer is unsigned; macOS/Linux packaging and auto-update are not implemented.
- User-reported basic desktop runtime works. Full Windows acceptance is still incomplete: AI has not been tested, cancellation is deferred because jobs finish too quickly, and the remaining OS integrations/installer checks need explicit results.
- Three real Playwright browser screenshots are available in CI artifact `ummp-phase11-playwright`; native Windows desktop screenshots are still outstanding.
- AI desktop providers are Anthropic and OpenAI only; local Ollama, custom base URLs, model selection, streaming, and prompt customization are deferred.
- Recent sessions are listed but cannot yet be reopened into a full event timeline.
- Job log rendering is bounded to the last 200 lines, not a full virtual scroller.
- Screen-reader and high-contrast accessibility audits remain pending.
