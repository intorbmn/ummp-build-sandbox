# Phase guides for Claude Code (Phases 10–14 and the backlog)

These guides let a Claude Code session finish UMMP without the history of earlier sessions.
Read this file completely first, then the guide for the phase you are working on.
They replace the rough drafts in `docs/drafts/` (kept for history).

| Phase | Guide | Depends on |
|---|---|---|
| 10 Modpack Migration | [phase-10-modpack.md](phase-10-modpack.md) | 4, 6, 8, 9 |
| 11 User Interface | [phase-11-ui.md](phase-11-ui.md) | application services (built in 11.A), all engines |
| 12 CLI, API & Integration | [phase-12-api.md](phase-12-api.md) | application services, 8 |
| 13 Production Readiness | [phase-13-production.md](phase-13-production.md) | everything |
| 14 Community & Ecosystem | [phase-14-community.md](phase-14-community.md) | 12 (plugin interface), 13 (releases) |
| Backlog | [`../BACKLOG.md`](../BACKLOG.md) | the phase each item came from |

**Order.** Do the phases in order. One phase per branch and per PR. Never start the next
phase until the user has reviewed and merged the current one (master prompt §14,
CLAUDE.md). Backlog items are done only when the user asks, each as its own small PR.

---

## 1. The user and how to talk to them

- The user writes **Vietnamese**. Reply in Vietnamese. Code, comments, commit messages,
  docs and PR texts are in **English** (as in the whole repository).
- Standing preference: after finishing a task, give **only a very short report** (a few
  bullets: what was done, evidence, what is open, what you need from them). No long recaps.
- The user decides scope; you decide the how and state each call in one line. Before
  designing, ask the "Questions for the user" listed in the phase guide (one message, at most
  4 questions, each with a recommended default). If they say "cứ làm" / "làm luôn", use the
  defaults and record them as your calls in the design doc.
- Default output folder for anything produced for the user on their machine:
  `D:\Claude\...` (Windows). UMMP code already uses `D:\Claude\ummp-builds` and
  `D:\Claude\ummp-ports`. New user-facing outputs follow the same pattern
  (`D:\Claude\ummp-<thing>`), with `~/ummp-<thing>` on other OSes.
- Unfinished items always go to `docs/BACKLOG.md` (the user asked for one separate file).

## 2. Environment setup (do this first in every session)

```sh
cd <repo>                       # e.g. /home/claude/universal-minecraft-mod-porter
git checkout master && git pull
rustc -V                        # if rustup tries to download 1.97.0 and fails:
export RUSTUP_TOOLCHAIN=stable  # only if `RUSTUP_TOOLCHAIN=stable rustc -V` prints 1.97.0
cargo build --all && cargo test --all
which java javac gradle gh      # Java 21, Gradle 8.x, GitHub CLI are used by tests/builds
```
- Every shell command that runs cargo needs `RUSTUP_TOOLCHAIN=stable` exported in the same
  command when the pinned toolchain cannot be downloaded (scripts started by skills too).
  Never edit `rust-toolchain.toml`.
- Tests that need `javac`/`gradle` skip themselves with a message when the tool is missing;
  CI has Java. Do not mark a phase COMPLETED on skipped tests: check the output.

## 2b. Read code with Graphify first (saves tokens)

The `mcp__Graphify__*` tools answer code-structure questions from a prebuilt code graph.
Use them before reading whole files.
1. **Check the index.** Call `list_repositories` and look for
   `intorbmn/Universal-Minecraft-Mod-Porter` (case as shown by Graphify; pass it as `repository_id`) with `queryable: true`. The build sandbox `intorbmn/ummp-build-sandbox` is indexed too.
   - If it is not listed, ask the user once to connect the repo in Graphify. Until then,
     use grep and partial Reads.
2. **Check freshness.** Compare the result's `commitSha` with `git rev-parse HEAD`.
   - Equal: trust the answer without grepping again.
   - Different: Read the current version of any file changed since that commit.
3. **Which tool for which question:**
   | Question | Tool |
   |---|---|
   | Where is X defined? | `graphify_find` (e.g. `compute_edits`, `ApprovalGate`, `migrate_pack`) |
   | Who calls X? Before moving or changing it | `graphify_callers`, `graphify_impact` (`include_risk=true`) |
   | Which tests cover X? | `graphify_tests_for` |
   | What does a file depend on? | `graphify_file_neighbors` |
   | How does a behavior work? | `query_graph` (a question in prose) |
4. **Then read narrowly.** Use Read with `offset`/`limit` on the lines Graphify points to.
   Do not read a whole 1000-line file when one function is enough.
5. **Literal text stays with grep.** Config values, TOML data, docs and strings in tests
   are found with `grep`.

Examples:
- Phase 11, moving the CLI helpers: run `graphify_callers` on `check_risk`, `run_build`
  and `load_store` before moving them.
- Phase 11, adding `JobState::Cancelled`: run `graphify_impact` on `JobState` first.

## 2a. Skill check (every phase, before design)

The user installs plugins on their account. A new agent may run in a session that lacks
some of them. At the start of every phase:
1. Compare the session's available skills (the system message list and the Skill tool
   list) with the table below. Use full `plugin:skill` names, and do not count look-alikes.
2. If a **required** skill is missing, **stop and ask the user to install it** before
   designing. Use the message template in `phase-11-ui.md` §0.2: it lists each plugin,
   its skill, and what it is used for. Offer two options: install, or continue by hand.
3. A skill installed during a session may only appear in a new session. Ask the user to
   restart or reload plugins, then check again.
4. Record the skills used, and any applied by hand, in the phase file.

Plugins and the skills each phase needs (R = required, r = recommended):

| Plugin | Skills | 10 | 11 | 12 | 13 | 14 |
|---|---|---|---|---|---|---|
| superpowers | brainstorming, writing-plans, executing-plans, test-driven-development, systematic-debugging, verification-before-completion | R | R | R | R | R |
| andrej-karpathy-skills | karpathy-guidelines | R | R | R | R | R |
| rust-cli-skills | rust-errors, rust-idiomatic, rust-testing | R | R | R | R | R |
| rust-cli-skills | rust-concurrency | R | R | R | r | – |
| rust-cli-skills | rust-performance | R | – | – | R | – |
| rust-cli-skills | rust-filesystem | R | r | r | R | – |
| rust-cli-skills | rust-design | – | R | R | – | r |
| rust-cli-skills | rust-cli-interface | – | – | R | – | – |
| rust-cli-skills | rust-memory, rust-distribution | – | – | – | R | – |
| ecc | architecture-decision-records, rust-review | R | R | R | R | R |
| ecc | rust-build | r | r | r | r | r |
| ecc | mcp-server-patterns, api-design | – | – | R | – | r |
| frontend-design | frontend-design | – | R | – | – | – |
| ui-ux-pro-max | ui-ux-pro-max | – | R | – | – | – |
| example-skills | webapp-testing | – | R | – | – | – |
| example-skills | mcp-builder | – | – | R | – | – |
| example-skills | doc-coauthoring | – | – | – | r | R |
| claude-security | claude-security (full scan agent) | – | – | R | R | – |
| pr-review-toolkit | review-pr | r | r | r | r | R |
| commit-commands | commit-push-pr | r | r | r | r | r |

Phase-specific details, such as what each skill is used for and which tools are optional,
are in each phase guide. `phase-11-ui.md` §0.1 is the most complete example.

## 3. The workflow for one phase (follow it exactly)

1. **Start**
   - `git checkout -b phase-NN-<topic>` from an up-to-date `master`.
   - Read `docs/PROJECT_STATUS.md`, `docs/KNOWN_LIMITATIONS.md`, `docs/BACKLOG.md`, the
     phase guide, and the ADRs it names.
   - Run the skill check (§2a). Missing required skills: ask the user to install them,
     and wait for the answer. Then load the skills for the phase as each step needs them,
     and write in the phase file which ones you used.
2. **Design** (`superpowers:brainstorming`)
   - Ask the guide's questions. Lay out 2–3 approaches with your pick.
   - Write `docs/superpowers/specs/YYYY-MM-DD-phaseNN-<topic>-design.md`, using the guide as
     the starting point: intent, goals, non-goals, anti-goals, approaches, design, testing,
     your calls.
   - Get explicit approval ("duyệt"/"có"/"chấp nhận").
3. **Plan** (`superpowers:writing-plans`)
   - Write `docs/superpowers/plans/YYYY-MM-DD-phaseNN-<topic>.md`: one task per milestone in
     the guide, with exact files, signatures and test names.
   - Ask the user to choose Native or Subagent execution. The user chose **Native** for
     Phases 8–9, for token economy; propose it again.
4. **Implement** (`superpowers:executing-plans` + `superpowers:test-driven-development`)
   - For every task: write the test → run it, see it FAIL for the right reason → implement →
     run it, see it PASS → run the full suite.
   - Mutation-check the key test of each milestone: break the code on purpose once, see the
     test go red, restore. Record that in the phase file ("mutation-checked").
   - Commit per task. The four CI commands must pass before every commit (CLAUDE.md).
   - Keep the ledger in `.superpowers/sdd/<plan>/progress.md`. Write every deviation as
     `Ruling: <decision> — <why> — <cost if wrong>`.
5. **Real-world check.** Where the phase produces something runnable (a pack, an app, a
   server), run it for real at least once, e.g. a GitHub Actions build with `ummp build`.
   Record run ids and results in the phase file. Fix what the real run finds, each fix with
   a test.
6. **Docs, same PR as the code**
   - Phase file `docs/phase/phase-NN-<topic>.md`: milestones table with evidence, real
     runs, completion report.
   - `PROJECT_STATUS.md`: row status, test count, next milestone.
   - `KNOWN_LIMITATIONS.md` (new section for the phase); `CHANGELOG.md` (Unreleased/Added,
     Changed).
   - `README.md` (quick start for new commands); `CLAUDE.md` (layout table, next ADR
     number); `docs/decisions/README.md`.
   - `docs/BACKLOG.md`: new section for the phase.
   - ADR for each significant decision (next number in CLAUDE.md).
7. **Finish**
   - `superpowers:verification-before-completion`: run the four commands, read the output,
     and count the tests.
   - Self-review of the branch, or a reviewer subagent only if the user asks; say which.
   - Push, open a PR (section 5), wait for CI to go green.
   - Send the short Vietnamese report: what was done, evidence, the calls you made,
     deferred minors, what you need from the user.
   - **Do not merge** until the user says so.

## 4. Repository rules that are easy to break (all learned the hard way)

- **Writes:** every change to a mod project is an `EditSet` applied by
  `MigrationEngine::apply_edit_set` / `run` after a checkpoint.
  - New files are `FileEdit { original: "" }`; deletions go in `EditSet.deletes`.
  - `CheckpointStore::restore` removes created files and restores deleted ones.
  - AI output (Phase 7) follows the same path.
- **Untrusted input:** every path that comes from input (zip entries, descriptor values,
  pack manifests, plugin manifests) goes through `filesystem::resolve_within`.
  - Zip extraction: `entry.enclosed_name()` + `resolve_within`; refuse the whole archive on
    an unsafe entry (see `core/src/port.rs`).
  - Cap sizes when reading: e.g. `take(MAX)` on zip entries.
- **Synthetic names** (`DemoWidget`, `demowidget`, `com.example.demo`) in sample data,
  fixtures and KB rows. Real loader/plugin package names are allowed only where the tool
  must recognize them (detection rules, templates, loader renames).
- **Fixtures look real:** JARs contain real `.class` files, built at test time with
  `javac`. Descriptors and manifests use the real schemas. Logs use real tool output
  formats. Golden files live in `tests/fixtures/golden/`.
- **Data over code:** rules, templates and renames are TOML embedded with `include_str!`
  (`scan_rules.toml`, `adapter_rules.toml`, `loader_rules.toml`, `loader_templates.toml`).
  New knowledge goes there or into the KB, not into `match` statements.
- **No HTTP in `core`.** Network clients live in `providers/` (ureq) behind a trait defined
  in core (see `AiProvider`). Tests use a local `TcpListener` fake server or fake CLI
  scripts (see `core/tests/build_github.rs`, `providers/src/lib.rs` tests).
- **Dependencies:** prefer std and crates already in `Cargo.lock`. A new crate is a design
  decision: name it in the design doc and in an ADR when it is significant
  (async runtime, web framework, UI toolchain).
- **Clippy is strict** (pedantic + nursery, `-D warnings`). Common hits:
  - `format_push_string`: use `write!`/`writeln!` with `use std::fmt::Write as _`.
  - `map_or_else`, `option_option`, `needless_pass_by_value`, `missing_const_for_fn`,
    `doc_markdown`, `too_long_first_doc_paragraph`, `literal_string_with_formatting_args`
    (`"${version}"` → `concat!("$", "{version}")`), `many_single_char_names` in tests,
    `from_secs(300)` → `from_mins(5)`.
  - `unsafe` is forbidden: tests cannot `set_var`, so make functions take their inputs
    (e.g. `find_jdk_in(candidates)`).
- **Edit tooling:** `cargo fmt` reflows code. A later `sed`/Python replace that assumes the
  old layout silently does nothing, so always grep after editing (`grep -c`). Prefer the
  Edit tool for code. Large multi-replace shell scripts may be blocked by a hook; split them
  up.
- **Hooks in this environment:**
  - A "Fact-Forcing Gate" denies the first Write/Edit of each new file and "destructive"
    commands (`rm -rf`, `git checkout --`, `git branch -D`). Answer with the facts it asks
    for (callers, no duplicate, schemas, the user's verbatim instruction; or files deleted,
    rollback, instruction), then retry.
  - Avoid needless deletions.

## 5. GitHub in this environment

- The repo is `intorbmn/universal-minecraft-mod-porter`.
- **GraphQL is blocked**: `gh pr create` and `gh repo view` fail. Use REST only:
  ```sh
  gh api repos/intorbmn/universal-minecraft-mod-porter/pulls -f title="..." -f head=<branch> -f base=master -F body=@/tmp/pr.md --jq .number
  gh api -X PUT repos/intorbmn/universal-minecraft-mod-porter/pulls/<N>/merge -f merge_method=merge   # only when the user says merge
  gh api repos/intorbmn/universal-minecraft-mod-porter/commits/<sha>/check-runs --jq '[.check_runs[]|.status+":"+(.conclusion//"")]|join(" ")'
  ```
- Push: `git -c push.negotiate=true push -u origin <branch>`. Check with `git ls-remote`.
- CI runs only on PRs to master (and pushes to master). To retrigger, close and reopen.
- The build sandbox repo is `intorbmn/ummp-build-sandbox` (private).
  - Attach it with `add_repo` (access: push) if a session does not have it.
  - `ummp build` / `ummp port --build` use it by default.
- Artifact downloads from Actions go to Azure blob storage, which **the sandbox proxy
  blocks**. `ummp build` then falls back to the job log, which carries the `UMMP-STAGES`
  JSON and the tail of failed stage logs. Read logs with:
  ```sh
  gh api repos/intorbmn/ummp-build-sandbox/actions/runs/<id>/jobs --jq '.jobs[0].id'
  gh api repos/intorbmn/ummp-build-sandbox/actions/jobs/<job>/logs
  ```
- Maven Central sometimes answers **HTTP 429** to Actions runners. That is not a UMMP bug:
  wait about 2 minutes and retry once.
- Gradle/Loom versions: Minecraft 1.21.1 needs Java 21. Newest Fabric Loom needs Java 25 to
  run Gradle, so templates pin the official versions of the Minecraft version's era
  (look them up in the official template repo's git history, never guess).
- Commit/PR attribution lines: use the ones given in the system reminder of your session.

## 6. Architecture you build on (read the code, these are the entry points)

| Need | Use |
|---|---|
| Open a project | `project::DefaultProjectManager.open(path)` → `Project { root, fs }` |
| Analyze | `analyzer::AnalyzerEngine::default().analyze(root, &fs)` → `AnalysisReport { mods, build_system, languages, dependency_graph, compatibility, technologies }`. Reads JARs and source descriptors (`src/main/resources`) |
| Knowledge base | `knowledge::KnowledgeBase::open(path or ":memory:")`, `insert_mapping`, `mappings_by_loader`, `import_mappings_from_file`, `compatibilities_by_loader` |
| Plan + migrate | `migration::MigrationPlanner::plan(&kb, &MigrationRequest { source_minecraft, target_minecraft, target_loader, min_confidence, source_loader })` → `MigrationEngine::default().run(&plan, &project, &mut CheckpointStore::new(), &RunOptions { dry_run, snapshot_root, session_id })` → `MigrationReport` (`to_markdown`) |
| Port a copy | `port::port(&PortOptions { input, output, to, loader, from, zip, min_confidence }, &kb)` |
| Adapters | `adapter::AdapterRegistry::default()`, `scan_dir(root)`, `TechFinding { tech, handling, file, line, message }` |
| Malware risk | `scan::scan_path(path)` → `ScanReport { findings }`, `risk()`, `to_text()` |
| Build/test/server | `build::{BuildRequest, BuildBackend}`, `build::local::LocalBackend`, `build::github::GithubBackend`, `build::report::{score, to_markdown}` |
| Logs/crashes | `build::logs::{analyze_log, parse_crash_report}` |
| Processes | `build::process::{run, ProcSpec, Watch, scrubbed_env}` (timeout + process-tree kill) |
| AI | `ai::orchestrator::Orchestrator` + `ai::ApprovalGate`; providers in `providers/` |
| Jobs/sessions/events | `jobs::{Job, JobStore}`, `persistence::JobDb`, `session::MigrationSession`, `events::EventBus` (exist since Phase 3, lightly used: the UI/API phases should make them real) |
| CLI | `cli/src/main.rs` (clap derive). `run_build`, `check_risk`, `mod_identity`, `detected_loader`, `open_kb` are reusable helpers |

## 7. Definition of done (every phase)

- [ ] Every milestone in the guide: COMPLETED with named tests (or explicitly moved to the
      BACKLOG with the user's agreement).
- [ ] Key tests mutation-checked; fixtures are real formats.
- [ ] A real-world run where applicable, recorded.
- [ ] The four commands pass locally; CI is green on the PR.
- [ ] Docs from step 6 updated in the same PR, ADRs written.
- [ ] Short Vietnamese report sent; waiting for the user's review before merging.

## 8. Report template (Vietnamese, short)

```
Phase NN xong, PR #N (CI xanh, chưa merge):
- <2–4 bullets: what was built>
- Kiểm chứng: <tests count>, <real run result + run id>
- Quyết định mình tự đưa ra: <1–4 bullets, each with its cost if wrong>
- Chưa làm (BACKLOG): <short>
Bạn review PR #N rồi báo merge và sang Phase NN+1.
```
