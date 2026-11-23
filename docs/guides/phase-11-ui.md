# Phase 11 — User Interface (Tauri) — guide for Claude Code

This guide lets an agent that has never seen this repository build Phase 11 alone.
1. Read it once from top to bottom before doing anything.
2. Then read [README.md](README.md). It covers the general workflow, environment quirks,
   GitHub commands, and the definition of done for every phase.
3. Where this file and README disagree, this file wins for Phase 11.

- Handoff after 11.B (status, open items, remaining plan): [phase-11b-handoff.md](phase-11b-handoff.md).
- Earlier rough design: `docs/drafts/phase-11-ui.md` (history only).
- State when this guide was written: Phases 1–10 merged (master after PR #8), 213 tests,
  next ADR number **0014**.

---

## 0. Start-of-phase checklist (do in order; tick these in the phase file)

1. Create the branch:
   ```sh
   git checkout master && git pull && git checkout -b phase-11-ui
   ```
2. Set up the environment (README §2):
   ```sh
   export RUSTUP_TOOLCHAIN=stable   # only if the pinned 1.97.0 cannot be downloaded
   cargo build --all && cargo test --all     # expect 213+ passing
   node -v && npm -v                         # Node 22 / npm 10 are installed in the sandbox
   ```
3. Read these, in this order:
   - `docs/PROJECT_STATUS.md`, `docs/KNOWN_LIMITATIONS.md`, `docs/BACKLOG.md`
   - `docs/decisions/0004-tauri-desktop-ui.md`: Tauri is **already decided**; the
     front-end stack is not.
   - ADRs 0005 (AI writes go through a gate), 0006 (snapshot checkpoints) and 0011 (build
     isolation and the risk gate)
   - `cli/src/main.rs` completely (about 1100 lines): the UI must do what the CLI does,
     through the same code.
   - `core/src/lib.rs`
   - `core/src/migration.rs`: `MigrationEngine::{compute_edits, apply_edit_set, run}`
   - `core/src/port.rs`, `core/src/modpack/run.rs`, `core/src/build/mod.rs`
   - `core/src/ai/mod.rs`: `ApprovalGate`
   - `core/src/jobs.rs`, `core/src/persistence.rs`, `core/src/events.rs`,
     `core/src/session.rs`, `core/src/checkpoint.rs`
4. **Code reading:** use Graphify first (README §2b), then narrow Reads.
5. **Skill check, before any design work.** Follow §0.1. If a **required** skill is
   missing, stop and ask the user to install it, using the §0.2 message. Wait for the
   answer. Load every available skill with the Skill tool when its step comes. Record
   which ones you used in the phase file.
6. Ask the user the §3 questions (one message, with defaults). Then write the design doc
   (§10, step D1).

### 0.1 Skills this phase needs

How to check:
- The session lists the available skills, in the system message and in the Skill tool's
  list. Compare it with the table below, by the full `plugin:skill` name.
- Do not guess from memory, and do not count a skill as present because a similar one
  exists. For example, `ecc:tdd-workflow` does **not** replace
  `superpowers:test-driven-development`; CLAUDE.md names the one to use.

| Skill (`plugin:skill`) | Plugin to install | Used for | Need |
|---|---|---|---|
| `superpowers:brainstorming` | superpowers | design talk + design doc (D1, D2) | **required** |
| `superpowers:writing-plans` | superpowers | the implementation plan (P1) | **required** |
| `superpowers:executing-plans` | superpowers | Native execution with a ledger (B1–B6) | **required** |
| `superpowers:test-driven-development` | superpowers | every task, RED → GREEN | **required** |
| `superpowers:systematic-debugging` | superpowers | any failing test or build | **required** |
| `superpowers:verification-before-completion` | superpowers | before claiming done, before the PR | **required** |
| `andrej-karpathy-skills:karpathy-guidelines` | andrej-karpathy-skills | keeping the design small (master prompt §12) | **required** |
| `frontend-design:frontend-design` | frontend-design | the 3 visual directions, screen design | **required** |
| `ui-ux-pro-max:ui-ux-pro-max` | ui-ux-pro-max | styles, palettes, UX and accessibility rules, Svelte guidance | **required** |
| `rust-cli-skills:rust-errors` | rust-cli-skills | `AppError`, mapping to `UiError` | **required** |
| `rust-cli-skills:rust-idiomatic` | rust-cli-skills | facade code style | **required** |
| `rust-cli-skills:rust-testing` | rust-cli-skills | `core/tests/app.rs`, the desktop crate tests | **required** |
| `rust-cli-skills:rust-design` | rust-cli-skills | the `AppServices` API, traits for secrets and gates | **required** |
| `rust-cli-skills:rust-concurrency` | rust-cli-skills | worker threads, cancel tokens, approval channel | **required** |
| `example-skills:webapp-testing` | example-skills | Playwright E2E for the web view | **required** |
| `ecc:architecture-decision-records` | ecc | ADR 0014 | **required** |
| `ecc:rust-review` | ecc | the Rust review before the PR | **required** |
| `pr-review-toolkit:review-pr` | pr-review-toolkit | the whole-PR review before the report | recommended |
| `commit-commands:commit-push-pr` | commit-commands | commit/push flow (the PR itself is opened with REST, README §5) | recommended |
| `ecc:rust-build` | ecc | compiler and borrow-checker errors | recommended |
| `rust-cli-skills:rust-filesystem` | rust-cli-skills | moving the snapshot helpers, settings file writes | recommended |
| Playwright MCP tools (`browser_*`) | Playwright (Microsoft) MCP plugin | looking at the dev build by hand while designing | optional |

Rules:
- **Missing required skill:** stop and ask (§0.2). Do not start D1 until the user answers.
- **The user says "không cài, cứ làm"** (do not install, just continue): continue, apply
  that skill's practice by hand, and write in the phase file "skill X not available;
  practice applied by hand: …".
- **Missing recommended or optional skill:** do not stop. Mention it once in the first
  message and do the work by hand.
- **After the user installs something:** a running session may not see new skills. Ask
  the user to start a new session (or reload plugins) if the skill still does not appear.
  Then re-check.

### 0.2 Message to send when skills are missing (Vietnamese; fill in the lists)
```
Trước khi bắt đầu Phase 11, mình kiểm tra skill thì thấy thiếu:

Bắt buộc (cần cài trước khi làm):
- <plugin> — skill <plugin:skill>: dùng để <việc>
- ...

Nên có (không có vẫn làm được, mình sẽ làm tay theo cùng quy trình):
- <plugin> — skill <plugin:skill>: dùng để <việc>

Cách cài: bật plugin trên trang plugin của claude.ai, hoặc trong Claude Code gõ /plugin rồi
cài theo tên plugin ở trên. Cài xong bạn mở phiên mới (hoặc reload plugin) để mình thấy skill.

Bạn muốn: (1) cài rồi báo mình, hay (2) không cài, mình làm tay và ghi chú vào phase file?
```

---

## 1. Goal and scope

### 1.1 Goal (master prompt, Phase 11)
A desktop app for people who do not use a terminal. They must be able to:
1. open a mod source folder or zip, a pack (`.mrpack`, CurseForge zip, Prism instance,
   folder), or a JAR;
2. see what UMMP found: mods, loader, Minecraft version, technologies, malware risk;
3. pick a target Minecraft version and loader;
4. **review the diff before anything is written**;
5. run the port or migration with visible progress, and cancel it;
6. read the report, open the output folder, and roll back an in-place migration;
7. build and test the result (local or GitHub) and read the build report;
8. use the AI assistant, with an explicit approve or reject for every proposed file edit;
9. migrate a pack and read the per-mod table;
10. change settings: output folder, knowledge base, AI provider and key, GitHub sandbox
    repo, language.

### 1.2 Hard rules
- **ADR 0004:** the UI calls *application services* only.
  - No migration logic in the UI crate or in TypeScript.
  - Every UI action is a function in `core/src/app/` with a Rust test.
  - The CLI is moved onto the same functions.
- **ADR 0005/0006:** files change only in one of two ways: through `MigrationEngine` after
  a checkpoint, or by writing into a new output folder (`port`, `pack migrate`). The UI
  never writes mod files itself.
- **Diff review:**
  - The UI always shows the diff first.
  - An **in-place** apply needs an explicit "I reviewed the changes" confirmation.
  - Porting to a new output folder leaves the source untouched, so it needs no
    confirmation, but the diff is still shown first.
- **Untrusted input:** every path that comes from the user or the web view goes to the
  services. The services validate it: it exists, it is the right kind, and it goes
  through `resolve_within` wherever it is combined with input.
- **Secrets:** the AI key and the GitHub token are never written to plain-text settings,
  logs, events, or back to the web view after entry.

### 1.3 Non-goals for Phase 11 (requests for these go to BACKLOG)
- Auto-update, code signing, and installers for macOS/Linux (Phase 13).
- REST and MCP (Phase 12).
- Editing mod source inside the app.
- A web or cloud version.
- Accounts and telemetry.

---

## 2. What the code gives you today (verified on master after PR #8)

### 2.1 APIs

**Open and analyze**
- API: `project::DefaultProjectManager.open(path)`, then
  `analyzer::AnalyzerEngine::default().analyze(&root, &fs)` → `AnalysisReport { mods,
  build_system, languages, dependency_graph, compatibility, technologies }`.
- For the UI: already `Serialize`; build a smaller `AnalysisView` from it.

**Risk scan**
- API: `scan::scan_path(path)` → `ScanReport { findings }`, with `.risk()` and
  `.to_text()`.
- For the UI: a badge plus the findings list.

**Port (writes a copy)**
- API: `port::port(&PortOptions { input, output, to, loader, from, zip, min_confidence },
  &kb)` → `PortResult { output, zip, report }`; default root from
  `port::default_port_root()`.
- For the UI: it writes a **new** folder and refuses an output folder that is not empty.

**Preview edits (the diff preview)**
- API: `MigrationPlanner::plan(&kb, &MigrationRequest{..})`, then
  `MigrationEngine::compute_edits(..)` → `EditSet { edits: Vec<FileEdit>, moves, deletes,
  technologies, files_scanned, warnings }`; each edit has `FileEdit::unified_diff()`.
- For the UI: this writes nothing. Read the exact signature of `compute_edits` before
  using it.

**In-place migrate**
- API: `MigrationEngine::run(&plan, &project, &mut CheckpointStore, &RunOptions {
  dry_run, snapshot_root, session_id })` → `MigrationReport`.
- For the UI: the CLI keeps the checkpoint store as JSON in `<project>.ummp-snapshots/`
  (`default_snapshots`, `load_store`, `save_store` in `cli/src/main.rs`). Move these three
  helpers into `core`.

**Rollback**
- API: `CheckpointStore::restore(id, &root)` and `store.latest()`.
- For the UI: same snapshot folder convention as above.

**Pack**
- API: `modpack::run::migrate_pack(&MigrateOptions{..}, &dyn ModIndex, &kb)` →
  `MigrateOutcome { output, plan, report, written }`; `.exit_code()` returns 0, 2 or 3;
  default root from `modpack::run::default_pack_root()`.
- For the UI: `pack inspect` exists too (see `run_pack` in the CLI).

**Build**
- API: `build::{BuildRequest, BuildOutcome, StageResult}`,
  `build::local::LocalBackend`, `build::github::GithubBackend`,
  `build::report::{score, to_markdown}`; CLI helpers `run_build` and `check_risk`.
- For the UI: local builds run mod code, so the risk gate in `check_risk` applies in the
  UI too.

**AI**
- API: `ai::orchestrator::Orchestrator` and the trait `ai::ApprovalGate { fn approve(&mut
  self, path, diff, reviewer_approved, concerns) -> bool }`. Providers are in
  `ummp-ai-providers`; the key comes from `ANTHROPIC_API_KEY`.
- For the UI: `approve` is synchronous, so the UI gate blocks a worker thread on a
  channel (§4.4).

**Jobs**
- API: `jobs::{Job, JobState, JobStore}`. `Job` has `new(session_id, step)`,
  `log_line`, `mark_running`, `mark_succeeded(output)` and `mark_failed`.
  `persistence::JobDb` has `open`, `insert`, `by_session`, `delete_by_session`
  (SQLite, through rusqlite).
- For the UI: `JobState` has no `Cancelled` value yet; add it (§4.3).

**Sessions**
- API: `session::MigrationSession` with `new(project_path, SessionConfig)`,
  `start(step)`, `succeed`, `fail`.
- For the UI: lightly used so far; it becomes the "history" list.

**Events**
- API: `events::{EventBus, Event { id, kind, payload, timestamp }, EventHandler}`.
  `EventBus::new() -> Arc<Self>`, `subscribe(handler)`, `publish(kind, payload)`,
  `drain()`.
- For the UI: use it as-is; one handler forwards events to the Tauri window.

### 2.2 CLI helpers to move into core
`open_kb`, `check_risk`, `mod_identity`, `detected_loader`, `run_build`,
`default_snapshots`, `load_store`, `save_store`, `side`, `pack_format`.

Moving them is the proof that the CLI and the UI share one path.

### 2.3 Workspace facts
- `Cargo.toml` members: `core` (crate `ummp-core`), `cli` (crate `ummp`, binary `ummp`),
  `providers` (crate `ummp-ai-providers`). `default-members = ["cli"]`.
- Workspace lints: clippy pedantic + nursery as warnings, `-D warnings` in CI,
  `unsafe_code = "forbid"`.
- CI (`.github/workflows/ci.yml`) is one Ubuntu job: build, test, clippy, fmt.

---

## 3. Questions for the user (one message, in Vietnamese, with defaults)
1. **Front-end framework?**
   - Default: **Svelte 5 + Vite + TypeScript**. It has a small runtime, simple components
     and good Tauri templates.
   - Alternatives: React (larger, more libraries) or vanilla TS (no framework, more hand
     work).
2. **UI language?**
   - Default: **Vietnamese + English with a switch, English fallback**. Strings live in
     `ui/src/lib/i18n/{vi,en}.json`.
3. **Must-have screens for this phase?**
   - Default: all ten in §5.
   - If the phase gets too big, cut in this order: AI assistant, then Pack, then Build.
     Each cut screen goes to BACKLOG.
4. **Look and feel?**
   - Default: you show 3 text mock directions (§6.1) and the user picks one before any
     screen is built.
5. **Where is the installer built?**
   - Default: a `windows-latest` job on GitHub Actions.
   - Artifacts cannot be downloaded from this sandbox, so the user downloads the `.msi`
     from the run page and installs it on their PC.

If the user says "cứ làm" / "làm luôn", use the defaults and record them in the design doc
as your calls.

---

## 4. Architecture

### 4.1 Layers
```
ui/src (Svelte, TypeScript)      views + forms only; calls invoke("<cmd>"), listens to "ummp://event"
        │  Tauri IPC (JSON)
ui/src-tauri (crate ummp-desktop) one-line #[tauri::command]s, keyring, window/event wiring
        │  Rust calls
core/src/app/ (AppServices)      all use cases, validation, jobs, events, approvals
        │
core engines                     analyzer, migration, port, modpack, build, ai, scan, checkpoint
```
After milestone 11.A, `cli/src/main.rs` also calls `core/src/app/`.

### 4.2 `core/src/app/` — application services (milestone 11.A, no UI yet)

Keep the files small, one responsibility each:

| File | Contents |
|---|---|
| `mod.rs` | `AppServices`, `Settings`, `AppError`, re-exports, event kind constants |
| `views.rs` | `AnalysisView`, `PlanView`, `FileDiffView`, `JobView`, `SessionView`, `PackView`, `BuildView`, `RiskView`, `TargetsView`; all `Serialize + Deserialize + Clone + Debug + PartialEq` |
| `jobs.rs` | job runner: worker threads, cancellation tokens, event publishing |
| `approval.rs` | `ChannelGate` (implements `ai::ApprovalGate`) and `Decision` |
| `snapshots.rs` | `default_snapshots`, `load_store`, `save_store`, moved from the CLI |
| `helpers.rs` | `open_kb`, `risk_decision` (`check_risk` minus the interactive prompt), `mod_identity`, `detected_loader` |

Signatures to implement. Keep the names; the exact fields are yours to settle in the plan.

```rust
pub struct Settings {
    pub data_dir: PathBuf,            // holds jobs.db and settings.json (the UI passes the Tauri app-data dir)
    pub kb_path: Option<PathBuf>,     // None = built-in mappings only (in-memory KB)
    pub output_root: Option<PathBuf>, // None = port::default_port_root() / default_pack_root()
    pub sandbox_repo: Option<String>, // the UMMP_BUILD_REPO equivalent
    pub ai_provider: Option<String>,  // "anthropic" | "openai"
    pub language: String,             // "vi" | "en"
    // NO secrets. Keys are passed per call as `Secret` by the caller.
}

pub struct Secret(String);            // Debug/Display print "***"; not Serialize

#[derive(Debug, thiserror::Error)]
pub enum AppError { Input(String), Refused(String), Io(..), Port(..), Pack(..), Migration(..), Db(..), Cancelled, NotFound(String) }

impl AppServices {
    pub fn open(settings: Settings) -> Result<Self, AppError>;
    pub fn settings(&self) -> Settings;
    pub fn save_settings(&self, s: Settings) -> Result<(), AppError>;   // settings.json in data_dir

    // fast and synchronous (under about 1 s on the fixtures)
    pub fn analyze(&self, input: &Path) -> Result<AnalysisView, AppError>;
    pub fn scan(&self, input: &Path) -> Result<RiskView, AppError>;
    pub fn targets(&self) -> TargetsView;   // MC versions and loaders from loader_templates.toml / KB, never hard-coded in TS

    // preview: never writes
    pub fn plan(&self, req: &PlanRequest) -> Result<PlanView, AppError>;  // PlanView.files: Vec<FileDiffView { path, kind, diff, replacements, low_confidence }>

    // long work: returns at once, runs on a worker thread, reports through events
    pub fn start_port(&self, req: PortRequest) -> Result<Uuid, AppError>;
    pub fn start_migrate_in_place(&self, req: MigrateRequest, reviewed: bool) -> Result<Uuid, AppError>; // reviewed = false -> AppError::Refused
    pub fn start_pack(&self, req: PackRequest) -> Result<Uuid, AppError>;
    pub fn start_build(&self, req: BuildJobRequest, accept_risk: bool, token: Option<Secret>) -> Result<Uuid, AppError>;
    pub fn start_ai_assist(&self, req: AiRequest, key: Option<Secret>) -> Result<Uuid, AppError>;

    pub fn job(&self, id: Uuid) -> Result<JobView, AppError>;        // state, step, progress, last 200 log lines, result
    pub fn cancel(&self, id: Uuid) -> Result<(), AppError>;
    pub fn decide(&self, approval: Uuid, decision: Decision) -> Result<(), AppError>;  // AI approve / reject
    pub fn sessions(&self) -> Result<Vec<SessionView>, AppError>;   // history, newest first
    pub fn rollback(&self, project: &Path, checkpoint: Option<Uuid>) -> Result<(), AppError>;

    pub fn subscribe(&self, handler: Arc<dyn EventHandler>);         // the Tauri crate forwards events to the window
}
```

Event payloads are JSON in `events::Event.payload`. The `kind` is one of:

| kind | payload |
|---|---|
| `job.started` | `{job, session, step}` |
| `job.progress` | `{job, step, done, total}`. Coarse steps only, not one per file |
| `job.log` | `{job, line}`. At most 20 events per second per job; the full log is in `JobDb` |
| `job.approval` | `{job, approval, path, diff, reviewer_approved, concerns}` |
| `job.finished` | `{job, state: "Succeeded" \| "Failed" \| "Cancelled", result?, error?}` |

Keeping the Rust and TypeScript sides in step:
- Write the kinds as constants in `app/mod.rs`, and as a TypeScript union in
  `ui/src/lib/events.ts`.
- A Rust test serializes each kind and compares the result with golden JSON files in
  `tests/fixtures/golden/app-events/`.
- The TypeScript tests import the same golden files, so the two sides cannot drift.

### 4.3 Jobs, progress, cancellation
- **Threads:** one `std::thread::spawn` per job. No async runtime in `core`; Tauri's
  runtime stays in the UI crate.
- **Each job:**
  1. creates a `MigrationSession` and a `Job`;
  2. inserts the job into `JobDb`;
  3. publishes `job.started`;
  4. updates the job and publishes progress on each step;
  5. publishes `job.finished`.
- **Panics:** wrap the job body in `std::panic::catch_unwind`. If the worker panics,
  record the job as `Failed` with the message. A job must never stay "Running" forever.
- **Cancellation:**
  - Use `CancelToken(Arc<AtomicBool>)`, checked between steps.
  - Engines that cannot stop mid-step (`port`, `migrate_pack`) stop at the next step
    boundary. The UI says "stopping after the current step".
  - Builds stop at once: `build::process` kills the process tree (see
    `core/src/build/process.rs`). Wire the token to its watch hook.
  - A cancelled in-place migration rolls back to its checkpoint before it reports
    `Cancelled`.
- **New state:** add `JobState::Cancelled` to `core/src/jobs.rs`. Check how
  `persistence.rs` stores the state, and add a test for the new value.
- **On restart:** in `AppServices::open`, any job still `Running` in `JobDb` was cut off
  when the app closed. Mark it `Failed` with "interrupted (app closed)". Full crash
  recovery is Phase 13.

### 4.4 AI approval through the UI
```rust
pub struct ChannelGate { job: Uuid, bus: Arc<EventBus>, pending: Arc<Mutex<HashMap<Uuid, Sender<Decision>>>>, cancel: CancelToken }
impl ai::ApprovalGate for ChannelGate {
    fn approve(&mut self, path: &Path, diff: &str, reviewer_approved: bool, concerns: &[String]) -> bool {
        // 1. create an approval id and a (tx, rx) pair; store tx in `pending`
        // 2. publish job.approval
        // 3. loop on rx.recv_timeout(1 s), checking `cancel`
        // 4. cancelled or channel closed -> false (reject). Never default to approve.
    }
}
```
- `AppServices::decide(approval, decision)` sends the decision to the stored `Sender`.
- Test it with the Phase 7 fake provider; see `core/tests/ai_eval.rs` for how it is built.

### 4.5 The desktop crate `ui/`
```
ui/
  package.json, package-lock.json, vite.config.ts, tsconfig.json, svelte.config.js,
  playwright.config.ts, index.html
  src/
    lib/api.ts               typed wrappers around invoke(); the ONLY file that calls invoke
    lib/events.ts            AppEvent types + listen helper
    lib/i18n/{en,vi}.json, lib/i18n.ts
    lib/components/          DiffView, FileTree, JobLog, RiskBadge, Stepper, EmptyState, ...
    screens/                 Home, Analysis, Target, Plan, Run, Report, Build, Ai, Pack, Settings
  src-tauri/
    Cargo.toml               crate "ummp-desktop"; deps: tauri 2, tauri-plugin-dialog, tauri-plugin-opener,
                             keyring, ummp-core (path), ummp-ai-providers (path)
    tauri.conf.json, capabilities/default.json, icons/
    src/main.rs, src/commands.rs, src/secrets.rs
```

**Workspace placement.** This is the default; record it in the ADR.
- `ui/src-tauri` is **its own Cargo workspace**: add `exclude = ["ui/src-tauri"]` to the
  root `Cargo.toml`, and give it its own `Cargo.lock`.
- Why:
  - Tauri on Linux needs system libraries (webkit2gtk-4.1, libsoup-3). The sandbox and
    the current CI job do not have them, so the root `cargo build --all` would break.
  - The root CI stays as it is, and a second CI job builds the desktop crate.
  - The facade lives in `core`, so the root CI still tests all the logic.

**Commands** (`commands.rs`).
- Each command is one line and maps `AppError` to a serializable `{kind, message}`:
  ```rust
  #[tauri::command]
  fn analyze(state: State<'_, AppServices>, path: PathBuf) -> Result<AnalysisView, UiError> { Ok(state.analyze(&path)?) }
  ```
- Long commands return the job id at once.
- Run any sync service call that may take more than 50 ms with
  `tauri::async_runtime::spawn_blocking`.

**Events.** In the `setup` hook of `main.rs`, call
`services.subscribe(Arc::new(WindowForwarder(app_handle.clone())))`. The forwarder's
`handle()` does `app_handle.emit("ummp://event", &event)`.

**Secrets** (`secrets.rs`).
- Store keys with `keyring::Entry::new("ummp", "anthropic_api_key")` and
  `"github_token"`, behind a small trait so tests can use an in-memory store.
- Commands: `set_secret(name, value)`, `has_secret(name) -> bool`,
  `delete_secret(name)`.
- There is **no** `get_secret` command. The Rust side reads the key itself when it starts
  an AI or build job, and passes it to the services as a `Secret`.

**Capabilities** (`capabilities/default.json`).
- Allowed: the core defaults needed for the window and events, `dialog:allow-open`, and
  `opener` (only to open the output folder).
- Not allowed: no `shell`, `fs` or `http` plugin.

**CSP** (`tauri.conf.json`):
`default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src ipc: http://ipc.localhost`.

**Bundle.** Identifier `io.github.intorbmn.ummp`, product name `UMMP`, Windows targets
`msi` and `nsis`.

**Scaffolding.** Check the current Tauri 2 docs before running these: use WebFetch on
https://v2.tauri.app/start/create-project/; do not rely on memory.
```sh
cd ui
npm create vite@latest . -- --template svelte-ts
npm install
npm install -D @tauri-apps/cli@^2 vitest @testing-library/svelte jsdom @playwright/test
npm install @tauri-apps/api@^2 @tauri-apps/plugin-dialog@^2 @tauri-apps/plugin-opener@^2
npx tauri init   # app name UMMP, window title UMMP, dev url http://localhost:5173,
                 # dist ../dist, dev cmd "npm run dev", build cmd "npm run build"
```
In the final commit:
- pin exact versions in `package.json` (no `^`);
- commit `package-lock.json` and `ui/src-tauri/Cargo.lock`.

---

## 5. Screens (behavior contract; the look is chosen with the user in §6)

| # | Screen | Shows | Actions | Service calls |
|---|---|---|---|---|
| 1 | Home | drop zone, "Open folder / file", recent sessions | open, open a recent session | `sessions` |
| 2 | Analysis | detected kind (mod source, JAR, pack), mods table (id, version, loader, MC), technologies (adapter findings with handling), risk badge + findings | continue to Target, rescan | `analyze`, `scan` |
| 3 | Target | MC version list, loader list, output folder (default `D:\Claude\ummp-ports\…` or `ummp-packs`), mode: "port to new folder" (default) or "migrate in place" | preview | `targets` |
| 4 | Plan & Diff | stages, counts, warnings, file tree (edited / created / deleted / moved), diff per file, low-confidence marks | in place: Apply is enabled only after "I reviewed the changes" is ticked; port: Start | `plan` |
| 5 | Run | stepper, progress, live log (virtualized), cancel | cancel | `start_*`, events, `job`, `cancel` |
| 6 | Report | summary of `MigrationReport` / `PortResult`, warnings, breaking changes, technologies, files changed | open output folder, Build, Rollback (in place only) | `job`, `rollback` |
| 7 | Build | backend (local / GitHub), risk gate (High: explicit accept; Medium: confirm), stages, score, failed-log tail | start, cancel | `start_build` |
| 8 | AI assistant | prompt, proposed edits as diffs, reviewer verdict + concerns, cost so far | approve / reject each edit (keys A / R), stop | `start_ai_assist`, `decide` |
| 9 | Pack | per-mod table (id, version, action, reason, risk), conflicts, exit status | start migrate, open output | `start_pack` |
| 10 | Settings | output root, KB file, AI provider + key (set / replace / delete; shows only "saved"), GitHub sandbox repo + token, language | save | `save_settings`, `set_secret`, `has_secret`, `delete_secret` |

Behavior rules:
- **Errors:** every error shows the service message and a next step. Never show a raw
  Rust Debug string.
- **Long lists** (files, logs, mods) are virtualized or paginated. A 300-mod pack and a
  10 000-line log must stay smooth.
- **Empty states** always offer the next action.
- **High-risk input** blocks local build and pack migrate until the user ticks an
  explicit acknowledgment. This is the same rule as `--allow-risk` /
  `--i-accept-risk` in the CLI.

---

## 6. Design quality

### 6.1 Before building any screen
Using `frontend-design` and `ui-ux-pro-max`, write 3 short text directions in the design
doc:
- Each direction gives the palette, typography, density, and how the Plan & Diff screen
  looks, with an ASCII sketch of that screen.
- Examples: "calm workbench" (neutral greys, one accent, dense tables), "guided wizard"
  (large steps, more whitespace), "IDE-like" (split panes, mostly monospace).

The user picks one; record the pick in the design doc.

### 6.2 Rules
- Light and dark themes; follow the OS setting by default.
- Diff colors pass WCAG AA contrast. Lines also carry `+` / `−` markers, so meaning never
  depends on color alone. Risk works the same way: icon + text + color.
- Accessibility:
  - every control has an accessible name;
  - focus is always visible;
  - the main flows work with the keyboard. Check the Tab order in a Playwright test.
- Vietnamese text must fit: test the screens with `vi.json`, which has longer strings.
- No remote fonts or CDNs (the CSP blocks them anyway). Bundle the font, or use the
  system UI font stack.

---

## 7. Milestones
Every milestone is test-first. ★ marks a test you must mutation-check: break the code on
purpose, see the test go red, restore the code, and record how in the phase file.

### 11.A Application services facade (Rust only, in the root workspace)
**Tests** in `core/tests/app.rs`. They use fixtures that already exist:
`tests/fixtures/loader/fabric-src`, `tests/fixtures/demo-fabric-mod`, the Phase 10 pack
fixtures, and the High-risk fixture from `core/tests/scan.rs`.
- ★ `plan_returns_diffs_and_writes_nothing`: hash every file of a temp copy before and
  after `plan`; the hashes are equal and `PlanView.files` is not empty.
- ★ `in_place_migrate_requires_reviewed_flag`: with `reviewed = false` the result is
  `Refused` and the tree is unchanged.
- ★ `port_job_emits_events_in_order`: a collecting `EventHandler` sees `job.started`,
  then the `job.progress` events, then `job.finished(Succeeded)`; the output folder
  exists.
- `rollback_restores_tree_after_in_place_migrate`
- ★ `cancel_in_place_migrate_rolls_back_and_reports_cancelled`: a test-only step barrier
  pauses the job between steps (`#[cfg(test)]` or a test field in `Settings`). No
  sleeps in tests.
- `job_state_cancelled_round_trips_through_jobdb`
- `running_job_from_previous_run_is_marked_failed_on_open`
- `worker_panic_is_recorded_as_failed`
- ★ `ai_edit_waits_for_decision_and_reject_writes_nothing`: fake provider plus
  `decide(Reject)`.
- `ai_gate_cancel_means_reject`
- ★ `high_risk_input_refuses_pack_and_local_build_without_ack`
- `settings_json_never_contains_secret`: save the settings and read the file back; no
  key-like value is in it, and `Secret` prints `***` in Debug.
- `app_event_payloads_match_golden_json`

**Also in 11.A:** move the CLI helpers (§2.2) into `core/src/app/`, and make every CLI
command call the services.
- **Proof:** the whole existing test suite passes unchanged, including
  `cli/tests/pack_cli.rs`.
- Any change in CLI output must be intended and noted in CHANGELOG.

### 11.B Desktop shell, Home, Analysis
- Scaffold `ui/` (§4.5) with the commands `analyze`, `scan`, `sessions`, `targets`.
- **Rust tests:** unit tests in `ui/src-tauri` call the command functions directly, with a
  real `AppServices` on a temp data dir.
- **Front-end tests** (Vitest + Testing Library, with `mockIPC` from
  `@tauri-apps/api/mocks`):
  - `Analysis shows mod id and loader from analyze result`
  - `RiskBadge shows text and icon for High`
- **E2E:** Playwright against `npm run dev`, with `mockIPC` fed from golden JSON. Open →
  the analysis table is visible.
- **CI:** add a job `desktop` to `.github/workflows/ci.yml` on `ubuntu-latest`, and leave
  the existing job unchanged. Steps:
  1. `sudo apt-get install -y libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev librsvg2-dev libayatana-appindicator3-dev`
  2. `cd ui && npm ci && npm run check && npx vitest run && npx playwright install --with-deps chromium && npx playwright test`
  3. `cd ui/src-tauri && cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`

### 11.C Target, Plan & Diff, Run, Report (the core flow)
- **`DiffView`** renders the output of `FileEdit::unified_diff()`: line numbers, `+` /
  `−` markers, and unchanged hunks collapsed. Unit-test it with a golden diff made from
  the fixtures.
- **E2E** (mocked IPC, scripted event sequence):
  1. pick NeoForge 1.21.1 as the target;
  2. the plan shows N files;
  3. in in-place mode, Apply stays disabled until the box is ticked;
  4. Run shows progress;
  5. Report shows the counts;
  6. Rollback calls `rollback`.
- **Real-binary E2E** (once, recorded):
  - with a display: `tauri-driver` + WebdriverIO, or a manual run;
  - without a display: write "pending the user's Windows check" in the phase file, and
    cover it in 11.F.

### 11.D Build screen and Settings (keyring, i18n)
- **Rust tests:**
  - `set_then_has_secret`
  - `no_command_returns_secret_value`: no command returns a secret read from the store.
- **Front-end tests:**
  - `language switch changes labels`
  - `missing vi key falls back to en`
  - `build risk High requires acknowledgment checkbox`

### 11.E AI assistant and Pack screens
- **E2E** (mocked IPC):
  - a `job.approval` event shows the diff;
  - Reject calls `decide(…, Reject)` and Approve calls `decide(…, Approve)`;
  - the A and R keys work.
- **Pack table:** renders 300 rows from a golden `PackView` without jank. Measure the time
  to first paint in Playwright and record it.

### 11.F Packaging and the user's real-world check
1. **Workflow** `.github/workflows/desktop-release.yml`, started by `workflow_dispatch`
   only:
   - runs on `windows-latest`: `npm ci`, then `npx tauri build`;
   - uploads `ui/src-tauri/target/release/bundle/msi/*.msi` and `nsis/*.exe` as artifacts.
2. **Run it on the branch:**
   ```sh
   gh api -X POST repos/intorbmn/universal-minecraft-mod-porter/actions/workflows/desktop-release.yml/dispatches -f ref=phase-11-ui
   ```
   Then get the run id (README §5) and give the user the run URL. Artifact download is
   blocked in this sandbox, so the user downloads the installer themselves.
3. **The user installs it on Windows** and runs: open `demo-fabric-mod` → target NeoForge
   1.21.1 → port → open the output folder.
   - Record their confirmation and any problems.
   - Fix each problem with a test.
4. **Known limitation:** an unsigned installer shows a SmartScreen warning. Record it in
   KNOWN_LIMITATIONS; signing is Phase 13.

---

## 8. Docs to update (same PR as the code)
- **ADR 0014 "Desktop UI stack and application services":**
  - Content: front-end stack, the separate Cargo workspace for `ui/src-tauri`, the events
    contract, secrets in the OS keyring, capabilities and CSP.
  - Add it to `docs/decisions/README.md` and set the next ADR number in CLAUDE.md to 0015.
- **Phase file** `docs/phase/phase-11-ui.md`, in the same layout as
  `phase-10-modpack.md`: milestones with evidence, skills used, mutation checks, E2E
  results, the user's installer check, completion report.
- **Design and plan:** `docs/superpowers/specs/YYYY-MM-DD-phase11-ui-design.md` (with the
  3 directions and the user's pick) and `docs/superpowers/plans/YYYY-MM-DD-phase11-ui.md`.
- **`PROJECT_STATUS.md`:** Phase 11 row, test count; the next milestone is Phase 12 after
  review.
- **`CHANGELOG.md`:** Added and Changed sections; note the CLI refactor.
- **`KNOWN_LIMITATIONS.md`** (Phase 11 section): unsigned installer, cancellation only
  between steps, no macOS/Linux bundles, approval behavior on cancel.
- **`docs/BACKLOG.md`** (Phase 11 section).
- **`README.md`:** a "Desktop app" quick start with 2–3 screenshots in `docs/images/`,
  from the real app (the user can send them) or Playwright screenshots of the dev build.
- **`CLAUDE.md`:** layout rows for `core/src/app/`, `ui/` and `core/tests/app.rs`; note
  that CI now has two jobs.
- **`docs/SAD.md`:** make the application-services section match the code.

---

## 9. Risks
| Risk | Handling |
|---|---|
| Tauri system libraries missing in the sandbox | Separate workspace and a separate CI job (§4.5). The root `cargo build --all` must stay green locally |
| UI freezes during long work | Every possibly-long service returns a job id; commands use `spawn_blocking` |
| CLI and UI drift apart | Both call `core/src/app/`; the existing CLI tests are the regression guard |
| Event contract drift between Rust and TS | Golden JSON shared by both test suites (§4.2) |
| Secret leakage | No `get_secret` command; `Secret` is not `Serialize`; settings test; never log keys |
| GUI cannot be checked in a headless sandbox | Mocked-IPC Playwright in CI plus the user's real install in 11.F; the report says plainly what was not run |
| Scope creep | The §3 cut order: AI, then Pack, then Build screens go to BACKLOG first |
| npm supply chain | Exact versions, committed lockfile, `npm ci` in CI, `npm audit --omit=dev` in the CI job (warning only) |

## 10. Step-by-step order
1. **D1:** Ask the §3 questions. Write the design doc from §1–§7: copy what applies and
   record your calls. Do a self-read of the doc; use a fresh reviewer subagent only if the
   user asks for one.
2. **D2:** Show the 3 visual directions (§6.1). Get the user's pick, then their approval
   of the design doc.
3. **P1:** Write the plan (`superpowers:writing-plans`), one task per milestone. 11.A will
   need 3–4 tasks: facade + views, jobs + events, approvals, moving the CLI. Propose
   Native execution.
4. **B1:** Build 11.A completely, run the four commands, commit. Split it into its own PR
   if the user prefers smaller PRs.
5. **B2–B5:** Build 11.B to 11.E. After each one, run the root four commands and the `ui`
   checks.
6. **B6:** 11.F: the release workflow run, then the user's install check.
7. **F1:** Docs (§8), verification, self-review, then the PR through REST (README §5).
   CI must be green on both jobs. Send the short Vietnamese report (README §8). **Do not
   merge, and do not start Phase 12.**

## 11. Backlog candidates (record the ones you do not do)
- macOS and Linux bundles
- auto-update
- code signing
- search inside the diff
- side-by-side diff mode
- opening a file dropped on the app icon
- tray progress
- remembering the window size
- a theme editor

## 12. Definition of done (Phase 11)
README §7 applies, plus:
- [ ] `core/src/app/` exists. Every UI action is a tested service, the CLI uses it, and
      the old CLI tests pass unchanged.
- [ ] All ★ tests are mutation-checked and recorded.
- [ ] The `desktop` CI job is green: front-end unit tests, mocked-IPC Playwright E2E, and
      build/test/clippy/fmt of the desktop crate.
- [ ] The root CI is green and unchanged in scope.
- [ ] The Windows installer was built by the release workflow, and the user confirmed it
      installs and ports the demo mod. If not, the report says exactly what is waiting on
      the user.
- [ ] ADR 0014 is written and every doc in §8 is updated; the phase file has a completion
      report.
