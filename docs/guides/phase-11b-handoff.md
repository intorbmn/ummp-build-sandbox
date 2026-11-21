# Phase 11 handoff (after 11.B) — for the next AI

Repo `intorbmn/universal-minecraft-mod-porter`, branch `phase-11-ui` (pushed). Draft PR #11 is open. Do NOT merge. Do NOT start Phase 12.
Reply to the user in Vietnamese; code, docs, commits in English; final report very short.

## Read first (in this order)
1. `CLAUDE.md`, `docs/guides/README.md`, `docs/guides/phase-11-ui.md`
2. Design: `docs/superpowers/specs/2026-11-20-phase11-ui-design.md` (approved). Plan: `docs/superpowers/plans/2026-11-20-phase11-ui.md`.
3. The ledger `.superpowers/sdd/phase11-ui/progress.md` is git-ignored and exists only in the first AI's sandbox. Its content is summarised below. Recreate it in your worktree (first line `# SDD ledger — plan: docs/superpowers/plans/2026-11-20-phase11-ui.md`, tasks A1-A8, B0, B1 marked complete) and keep adding `Ruling:` lines.

## Status
- 11.A (application services facade `core/src/app/`, jobs, events, approval gate, pack/build jobs, CLI on shared helpers) — DONE (A1-A8).
- B0: `AppServices::sessions(limit)` (+ `JobDb::recent`, `SessionView`) — DONE.
- 11.B (desktop shell + Home + Analysis) — DONE.
- Next: **11.C** (Target, Plan & Diff, Run, Report), then 11.D (Build, Settings), 11.E (AI assistant, Pack), 11.F (packaging), 11.G (docs, ADR 0014, PR). Cut order if too big: AI, then Pack, then Build (to BACKLOG).

## What exists (11.B)
- `ui/`: Svelte 5 + Vite 8 + TS, exact-pinned deps, `package-lock.json` committed. `src/lib/api.ts` is the ONLY file that calls `invoke`. `src/lib/events.ts` listens to `ummp://event`. `src/lib/i18n.svelte.ts` + `src/locales/{en,vi}.json` (fallback vi -> en -> key). `RiskBadge.svelte` shows text + icon, not colour only. Screens: `src/screens/Home.svelte`, `Analysis.svelte`. `src/App.svelte` has a simple nav (home, analysis); extend it for the new screens. Style in `src/app.css` ("Calm workbench": neutral greys, one blue accent, light+dark via `prefers-color-scheme`, system fonts, monospace only in diffs).
- `ui/src-tauri` is its own Cargo workspace (root `Cargo.toml` has `exclude = ["ui/src-tauri"]`; its `Cargo.lock` is committed through a `.gitignore` negation). Crate `ummp-desktop`: `src/commands.rs` holds one-line bodies calling `AppServices` plus `UiError{kind,message}`; `src/lib.rs` holds the `#[tauri::command]` wrappers (`spawn_blocking` for slow calls), `WindowForwarder` (core events -> window event `ummp://event`), and the managed state `Arc<AppServices>`. Commands so far: `analyze, scan, sessions, targets`. Register new ones in `generate_handler!`.
- Capabilities (`ui/src-tauri/capabilities/default.json`): `core:default`, `dialog:allow-open`, `opener:default` only. CSP and bundle id `io.github.intorbmn.ummp` are set in `tauri.conf.json`; bundle targets msi+nsis.
- Golden JSON shared by Rust and TS: `tests/fixtures/golden/app-views/*.json` (Rust test `view_shapes_match_golden_json` in `core/tests/app.rs`) and `tests/fixtures/golden/app-events/*.json`. Add a golden file for every new view or event the UI uses.
- CI: job `desktop` added to `.github/workflows/ci.yml`. It has never run on GitHub; check it after pushing.

## Commands (all must pass before each commit)
```sh
export RUSTUP_TOOLCHAIN=stable   # the pinned toolchain cannot be downloaded in the sandbox
cargo build --all && cargo test --all && cargo clippy --all --all-targets -- -D warnings && cargo fmt --all -- --check
cd ui && npm ci && npm run check && npx vitest run
PW_CHROMIUM=/opt/pw-browsers/chromium-1243/chrome-linux64/chrome npx playwright test   # sandbox only; CI runs `npx playwright install --with-deps chromium`
cd src-tauri && cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
```
`ui/src-tauri` needs `ui/dist/index.html` to exist for `tauri-build` (run `npm run build` in `ui/`). Linux Tauri system libraries (webkit2gtk-4.1, gtk3, ...) were installed in the first sandbox.

## Evidence at the end of 11.B
Root four commands pass. `ui/src-tauri`: build, 4 tests, clippy, fmt pass. `npm run check`: 0 errors. Vitest 3/3. Playwright 1/1 (with the sandbox Chromium). Mutations done (each went RED, then restored): RiskBadge high icon blanked; Analysis loader column shows name; serde rename on `build_system` (golden test); Home submit no longer calls `onAnalyze` (Playwright).

## Rules that bit before
- A status is COMPLETED only with test evidence. Every ★ test must be mutation-checked (break the code on purpose, see RED, restore) and recorded in the ledger. After `cargo fmt`, string-replace mutations may silently not apply: grep the marker before trusting a result.
- Clippy is pedantic+nursery as errors (for example `too_long_first_doc_paragraph`, `needless_pass_by_value`, `semicolon_if_nothing_returned`).
- Synthetic names only (`DemoWidget`); treat inputs as untrusted (`resolve_within`); edits only through `MigrationEngine` after a checkpoint; in-place migrate needs `reviewed=true`; no command may return a stored secret; next ADR number is 0014, then 0015.
- Do not work around safety hooks. A Bash `rm -rf` with a glob was blocked by a safety check, and a commit hook once matched on words inside a heredoc. Use the Write tool for text files. Skills such as `superpowers:*` may not be installed in your session: say so and apply the practice by hand.

## Open items and things NOT verified
- The Tauri window was never launched (no display in the first sandbox). `tauri dev` and the real window are unverified. The real Windows install check is the user's job (11.F).
- The Tauri docs fetch was not approved; the scaffold came from `tauri init --ci` (CLI 2.12.6).
- Leftover Vite demo files were NOT deleted because an `rm` was blocked: `ui/src/assets/`, `ui/src/lib/Counter.svelte` (unused), `ui/public/favicon.svg`, `ui/public/icons.svg`. `ui/index.html` still has the template title. Remove the unused ones with `git rm` and set the title to UMMP.
- Icons are Tauri defaults. The i18n test checks one fallback case only; add `missing vi key falls back to en` properly in 11.D.
- The Playwright test mocks `window.__TAURI_INTERNALS__` by hand; the plan said `mockIPC`, so switch to `@tauri-apps/api/mocks` if you prefer.
- CI job `desktop` and its apt package list are untested on GitHub.

## Remaining plan
- 11.C: Target, Plan & Diff (DiffView, FileTree, in-place Apply gated by an "I reviewed the changes" checkbox), Run (virtualized JobLog, Stepper, cancel), Report (open output, Build, Rollback); Playwright flow; tab-order test. New Tauri commands needed: plan, start_port, start_migrate_in_place (reviewed flag), cancel, job, rollback; plus handling of `job.*` events.
- 11.D: Build screen (risk-gate checkbox) and Settings (OS keyring behind a trait). Tests: `set_then_has_secret`, `no_command_returns_secret_value`, `language switch changes labels`, `missing vi key falls back to en`, `build risk High requires acknowledgment checkbox`.
- 11.E: AI assistant (approval diffs, A/R keys, `decide`) and Pack (300-row table, record first-paint time).
- 11.F: `.github/workflows/desktop-release.yml` (`workflow_dispatch`, `windows-latest`, `npm ci`, `npx tauri build`, upload msi+nsis). An unsigned installer goes into KNOWN_LIMITATIONS.
- 11.G: ADR 0014, `docs/phase/phase-11-ui.md`, PROJECT_STATUS, CHANGELOG, KNOWN_LIMITATIONS (including: build cancel only works between steps), BACKLOG, README "Desktop app", CLAUDE.md layout, SAD.md. Run verification-before-completion and the Rust review. Open the PR via REST (`gh api repos/intorbmn/universal-minecraft-mod-porter/...`). Both CI jobs must be green. Do not merge.
- Commit trailers: `Co-Authored-By: <your model> <noreply@anthropic.com>` and `Claude-Session: <your session url>`.

## Rulings so far (still binding)
- Decisions: Svelte 5 + Vite + TS; vi + en UI with en fallback; all 10 screens; visual direction A "Calm workbench"; Windows installer by `workflow_dispatch`.
- `JobDb::recent` reads all rows and truncates (fine while the job table is small; add a SQL LIMIT if Home gets slow).
- Core returns an empty in-memory knowledge base when the CLI's `open_kb` gets neither `--kb` nor `--mappings`.
- `#[allow(clippy::needless_pass_by_value)]` on the `targets` command, because Tauri passes `State` by value.
- Branch history: A1-A3 exist twice (squashed locally, three commits on the remote). A merge commit joined them with identical trees. Do not rewrite history or force-push.


## User acceptance update — 2026-11-20

- User reports that the desktop tool basically runs. Record this as a basic runtime smoke test only; no unreported sub-feature is assumed to have passed.
- AI has not been tested yet; keep AI acceptance open.
- Cancellation has not been exercised because jobs complete too quickly. Defer this test and use a deliberately long-running cancellable job before marking it passed.
- Keep the draft PR open. Installer artifacts/hashes, native screenshots, AI acceptance, and the remaining Windows integration checks still need evidence.
