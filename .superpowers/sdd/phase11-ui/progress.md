# SDD ledger — plan: docs/superpowers/plans/2026-11-20-phase11-ui.md

- 2026-11-20: Ruling: the handed-over patch (`phase_11_ui_patch.md`) is unusable as a git patch (markdown rendering stripped generics and mangled indentation); 11.A1-A3 rebuilt by hand from its text and its test names, tests first - cost if wrong: small naming drift from the previous author's code
- 2026-11-20: Ruling: Rust 1.97 via `RUSTUP_TOOLCHAIN=stable`, not a rustup download - static.rust-lang.org blocked in sandbox - cost if wrong: clippy/fmt output may differ from pinned 1.97.0 by a patch version
- 2026-11-20: Ruling: `PlanView.files[].needs_attention` (assisted/manual adapter finding in that file) instead of per-file `low_confidence` - Replacement carries no confidence; low-confidence mappings are reported in `PlanView.skipped` - cost if wrong: UI label wording only
- 2026-11-20: Ruling: `AppServices::analyze` accepts folder or .zip only (JAR -> Input error pointing to scan) - CLI `analyze` is folder-only too - cost if wrong: add JAR analysis later in the service
- 2026-11-20: Ruling: `targets()` returns the four loader families the converter knows; `versions` come from `loader_templates.toml` only (ports to other versions stay possible via `plan`/`port`) - cost if wrong: UI offers fewer versions than the engine supports
- 2026-11-20: Mutation check (11.A3) `plan_returns_diffs_and_writes_nothing`: (1) plan writes MUTANT.txt into the input -> red at "plan must not write"; (2) plan drops all file views -> red at "files is not empty"; restored -> green.

Task A4 (jobs + events): complete. `core/src/app/jobs.rs` (CancelToken, JobCtx step/log/progress with <=20 log events/s, worker thread + catch_unwind, stale Running -> Failed on open), `start_port`, `job`, `cancel`, `subscribe`, `set_step_hook`; event payload golden JSON in tests/fixtures/golden/app-events/.
- Ruling: finish order = save job -> remove from running -> publish job.finished -> drain bus (a listener that sees finished can already read the stored job; the queue cannot grow).
- Ruling: failure text is stored in Job.output as {"error": msg}; JobView.error is derived from it.
- Mutation check (★ port_job_emits_events_in_order): (1) job.started published under another name -> RED (left "job.skipped", right "job.started"); (2) success finished as Failed -> RED at the state assertion. Restored -> 22/22 green, stable over 6 runs.
- Four commands pass; cargo test --all green (app: 22 tests).

Task A5 (in-place migrate, rollback, cancel): complete. `start_migrate_in_place(req, reviewed)` (Refused before any write when !reviewed; folder only), `rollback(project, Option<Uuid>)`, cancel restores the checkpoint at the post-apply "verify" step. `plan()` now shares `resolve_plan` with the job.
- Ruling: cancel is checked at step boundaries only (engine apply is atomic and already rolls itself back on failure); cancelling after apply restores the checkpoint and ends Cancelled.
- Mutation check: (1) ignore `reviewed` -> in_place_migrate_requires_reviewed_flag RED; (2) skip restore after cancel -> cancel_in_place_migrate_rolls_back_and_reports_cancelled RED. Restored -> 25/25, stable over 5 runs. Four commands pass.

Task A6 (AI approvals): complete. `core/src/app/approval.rs`: `ChannelGate` (publishes `job.approval`, waits on a one-shot channel, polls cancel every 100 ms, closed channel/cancel/timeout-without-yes = reject), `decide(approval, bool)` (NotFound once answered), `start_ai_assist(req, Arc<dyn AiProvider>, AiConfig)`. Provider is injected so core never depends on `providers/`. `AppError::Ai` wraps `OrchestratorError`.
- Ruling: AI assist is folder-only and takes a plain `PlanRequest`; the per-file approval IS the review, so no `reviewed` flag.
- Ruling: after `assist` returns, a cancel that arrived mid-session restores the checkpoint and ends Cancelled (covers files approved before the cancel).
- Mutation check: (1) gate ignores the decision (always yes) -> ai_edit_waits_for_decision_and_reject_writes_nothing RED; (3) gate never waits -> same test RED; (2) cancel treated as approve -> first version of ai_gate_cancel_means_reject stayed GREEN (cancel-restore fallback hid it; my strengthening edit had silently not applied after cargo fmt reflowed the assert). Fixed by asserting no checkpoint exists (a rejecting gate never starts an apply); re-run -> RED, restored -> GREEN. 27/27, stable over 4 runs. Four commands pass.

Task A7 (risk gate for pack and local build): complete. `core/src/app/runs.rs`: `start_pack(PackRequest, Arc<dyn ModIndex+Send+Sync>)`, `start_build(BuildJobRequest)`, `check_build_risk(path, local, accept_high, confirm_medium)`; `helpers::NoIndex` moved into core (CLI copy goes in A8). Local High risk -> `AppError::Refused` before any job exists; Medium needs `confirm_medium`; GitHub builds only warn. A pack with a High-risk mod fails with a "risk" error and exports nothing (report only) unless `allow_risk`.
- Ruling: pack refusal is surfaced as a Failed job with a Refused error text rather than a synchronous check, because only the pack pipeline knows the per-mod risk after import+analysis (reuses its own gate, no second implementation).
- Ruling: cancelling a running build is honoured between steps only; an already running Gradle/GitHub build is not interrupted (backends have no cancel hook). Goes to KNOWN_LIMITATIONS in 11.G.
- Mutation check (★ high_risk_input_refuses_pack_and_local_build_without_ack): (1) gate allows High local -> RED; (2) pack allow_risk forced true -> RED; (3) start_build skips the gate -> RED (first attempt did not apply because cargo fmt had reflowed the code; redone against the real text). Restored -> 28/28, stable over 3 runs. The M3 mutant started a real local build and left two scratch folders under ~/ummp-builds in the sandbox; removed them.
- Four commands pass.

Task A8 (CLI onto services): complete. `cli/src/main.rs` lost its copies of pack_format, NoIndex, default_snapshots/load_store/save_store, open_kb, mod_identity, detected_loader, side (1132 -> 1001 lines) and imports them from `ummp_core::app::{helpers,snapshots}`; `check_risk` is a thin wrapper over `risk_decision` (prompt stays in the CLI).
- Ruling: CLI `open_kb` no longer panics on the impossible (None, None) case; core returns an empty in-memory KB (clap still requires --kb or --mappings for `migrate`). Error text for a corrupt checkpoint store is now AppError::Input's wording.
- Evidence: four commands pass; cargo test --all green; manual real-binary round trip `migrate --apply` (tree changed, checkpoints.json written) -> `rollback` (tree identical to fixture); `cli/tests/pack_cli.rs` 5/5 unchanged.
- 11.A COMPLETE (A1-A8). Next: 11.B (Tauri 2 scaffold). Before it: read Tauri 2 docs, load frontend-design + ui-ux-pro-max skills.
Task B0 (sessions for Home): complete. `JobDb::recent`, `SessionView`, `AppServices::sessions(limit)` newest first; test `sessions_lists_recent_jobs_newest_first_with_output` RED (no method) -> GREEN x3; four commands pass.
- Ruling: `recent` reads all rows and truncates (job table is small, one row per user action) — avoids a new SQL path — cost if wrong: slow Home load after thousands of jobs; add LIMIT then.
Task B1 (desktop shell, Home, Analysis): complete. `ui/` (Svelte 5 + Vite 8 + TS, exact-pinned deps, package-lock.json committed), `ui/src-tauri` crate `ummp-desktop` (commands analyze|scan|sessions|targets, `UiError`, `WindowForwarder` -> `ummp://event`, capabilities core:default + dialog:allow-open + opener:default, CSP and bundle id as designed, msi+nsis targets), `lib/api.ts` (only `invoke` caller), `lib/events.ts`, i18n vi/en with en fallback, `RiskBadge`, Home, Analysis; root `Cargo.toml` `exclude`; CI job `desktop`; golden JSON `tests/fixtures/golden/app-views/` shared by Rust (`view_shapes_match_golden_json`) and Vitest.
- Evidence: root four commands pass; ui/src-tauri cargo build/test(4)/clippy/fmt pass; `npm run check` 0 errors; Vitest 3/3; Playwright e2e 1/1 (run with the sandbox's preinstalled Chromium via `PW_CHROMIUM`, because cdn.playwright.dev is blocked here; CI installs its own).
- Mutations (all RED then restored): RiskBadge high icon blanked -> `RiskBadge shows text and icon for High` fails; Analysis loader column shows name -> `Analysis shows mod id and loader` fails; serde rename on `build_system` -> `view_shapes_match_golden_json` fails; Home submit no longer calls onAnalyze -> Playwright flow fails.
- Ruling: Tauri docs WebFetch got no approval — used the installed Tauri CLI 2.12.6 (`tauri init --ci`) and its generated templates instead — cost if wrong: a config key differs from current docs; `cargo build` + CI would show it.
- Ruling: `#[allow(clippy::needless_pass_by_value)]` on the `targets` command — Tauri passes `State` by value — cost: none.
- Ruling: the Vite template's demo files (`ui/src/assets`, `ui/src/lib/Counter.svelte`, `ui/public/*`, `ui/index.html` title) were not deleted because the sandbox's rm safety check refused the cleanup — cost: dead files; delete by hand.
- NOT done in B1: Tauri window never launched (no display in sandbox); `tauri dev` is unverified; icons are the Tauri defaults; CI job `desktop` not yet run on GitHub.
- 11.B COMPLETE. PAUSED by the user's request. Next: 11.C.


## Phase 11 continuation — manual execution

- 2026-11-20: Ruling: required Claude Code skills are unavailable in this session; the user explicitly approved continuing without installation. Apply the practices manually and record verification honestly — cost if wrong: missed specialist guidance, mitigated by the approved design/plan, task ledger, tests and review gates.
- 2026-11-20: Ruling: keep implementation minimal; add no new abstraction or dependency unless an existing contract cannot be met — reduces maintenance surface — cost if wrong: a future feature may need a deliberate extension.
- 2026-11-20: Ruling: enable push CI for `phase-11-ui` because the existing workflow only ran on `main`/ `master`; a branch push must produce evidence before PR — cost if wrong: additional CI minutes on this feature branch.
- 2026-11-20: Ruling: `core/src/build/github.rs` needs an explicit `crate::app::Secret` import for the new optional credential field — the field otherwise does not resolve — cost if wrong: none; fixed in commit `eed44999`.
- Task 11.C: Target, Plan/Diff, Run and Report screens are present. Playwright coverage includes target → preview → in-place review gate → run → report → rollback and keyboard focus; DiffView has a collapse/expand unit test. Fresh execution is pending.
- Task 11.D: Build and Settings screens are present. Tests cover High-risk local-build acknowledgement, immediate language switching, English fallback, and the in-memory secret store. Fresh execution is pending.
- Task 11.E: AI approval supports explicit A/R decisions; PackReport paginates 300 rows at 50 per page and Playwright records first-page render time. Fresh execution is pending.
- Task 11.F: `.github/workflows/desktop-release.yml` exists for manual Windows MSI/NSIS packaging. It has not yet been dispatched; unsigned installer and real Windows smoke test remain open.
- Task 11.G: ADR 0014 exists. Phase record, project status, known limitations, backlog, SAD, CLAUDE layout and changelog are being aligned. Draft PR #11 is open; do not merge.
- Verification gate: GitHub reported zero workflow runs for `phase-11-ui` before the branch push trigger was added. No tests are claimed as run by this continuation until the new workflow reports results.


## Phase 11 verification and final review

- 2026-11-20: Ruling: use the GitHub Actions run as the acceptance source because no local repository checkout/runtime is available in this session — avoids claiming unexecuted local tests — cost if wrong: CI environment may differ from a user's Windows desktop.
- 2026-11-20: Ruling: serialize fake GitHub CLI integration tests with a test-only mutex after repeated Linux `ETXTBSY` failures; keep production process spawning direct — isolates the runner race without adding production retry behavior.
- 2026-11-20: Ruling: compile `MemorySecretStore` only under `cfg(test)`; it is a test fixture, not a production secret backend — removes dead-code lint without suppressing Clippy.
- 2026-11-20: Ruling: add `uuid` as a direct Tauri dependency and synchronize the desktop lockfile — `ui/src-tauri/src/lib.rs` uses UUID types directly.
- 2026-11-20: Ruling: ZIP port requests derive the archive flag from the input extension; AI assist remains disabled for ZIP inputs because the AI service accepts a source directory — prevents treating a ZIP as a folder and avoids unsupported AI requests.
- 2026-11-20: Ruling: the AI screen starts the job once and passes the returned job ID to the parent; the parent only navigates to Run — prevents duplicate AI jobs.
- 2026-11-20: Ruling: align exact frontend dependency pins with the committed lockfile so `npm ci` is deterministic — cost if wrong: updates frontend package versions; verified by green npm install/check/test/build.
- 2026-11-20: Ruling: keep the Phase 11 PR in draft until a maintainer completes the Windows installer and real runtime smoke test — automated Linux/browser CI cannot prove Windows Credential Manager, WebView2, installer or OS opener behavior.
- CI #44709516766 on `7607ab2c`: root Rust 248 tests passed; Tauri Rust 12 tests passed; Vitest 4 files / 11 tests passed; Playwright 6 tests passed; npm production audit found 0 vulnerabilities; root and desktop build, clippy and format checks passed.
- Latest PR-head verification: push run #44710620730 and pull-request run #44710673870 both passed on `0eeafe63a2fdd04d9f25d410c3b20f8d4760d265`; both jobs (`build-test-lint`, `desktop`) succeeded and both expected CI artifacts were uploaded.
- CI artifacts: `ummp-phase11-playwright` (Home, Plan/Diff, PackReport screenshots) and `ummp-desktop-cargo-lockfile` were uploaded successfully.
- 11.C–11.E: automated implementation and test gates complete. 11.F: manual Windows release dispatch/install/smoke test remains. 11.G: documentation aligned; draft PR is the review handoff. Do not merge.

- 2026-11-20: User acceptance feedback: the desktop tool basically runs. Treat this as a basic runtime smoke test only; do not infer that all Windows integration points passed.
- 2026-11-20: User has not tested AI yet. AI desktop acceptance remains OPEN until provider setup, request execution, proposal review, explicit approve/reject and resulting job/report are exercised.
- 2026-11-20: Cancellation test is DEFERRED, not passed/failed: current sample jobs finish too quickly to observe cancellation. A deliberately long-running cancellable test case is needed before closing this item.
