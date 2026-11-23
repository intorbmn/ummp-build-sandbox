# Phase 8 Build & Testing Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `ummp build` (GitHub or local) with build/test/server stages and a scored report, plus `ummp scan` static malware-risk reporting run before every build.

**Architecture:** `core/src/build/` holds a timeout-and-kill process runner, log/crash analyzers, a `BuildBackend` trait with `LocalBackend` and `GithubBackend` (drives `git` + `gh` as subprocesses), and a report. `core/src/scan.rs` applies TOML rules to source files, build scripts and class-file constant-pool strings. The CLI wires both.

**Tech Stack:** Rust 1.97 (pinned), std::process, zip 0.6, toml 0.8, serde; Gradle and javac for tests.

**Spec:** `docs/superpowers/specs/2026-11-19-phase8-build-test-design.md`

## Global Constraints
- No new crates unless unavoidable. Process-tree kill uses `kill -- -PGID` on Unix (via `process_group(0)`) and `taskkill /T /F /PID` on Windows.
- Build env keeps only: `PATH`, `JAVA_HOME`, `HOME`, `USERPROFILE`, `TEMP`, `TMP`, `SystemRoot`, `GRADLE_USER_HOME`.
- Default local workdir: `D:\Claude\ummp-builds` on Windows, `~/ummp-builds` elsewhere; each build in `<workdir>/<mod>-<yyyymmdd-hhmmss>/`.
- Server success line: `Done (`; then send `stop\n`; kill if not exited within 30 s.
- Local policy: High risk is refused unless `--i-accept-risk`; Medium needs `--yes` or an interactive y/N. GitHub: warn only.
- Workflow: `permissions: contents: read`, no secrets, branch `ummp/<mod>-<timestamp>`.
- Scan output never says "safe"; the clean case prints "No risk indicators found (this is not proof the mod is safe)".
- Synthetic names in fixtures (`DemoWidget`, `com.example.demo`).
- All four CI commands green before each commit.

## Review Focus
1. A child that ignores SIGTERM and spawns grandchildren: the whole tree is gone after the timeout (Task 1 `timeout_kills_grandchild`).
2. A project with no `gradlew` and no `gradle` on PATH: the stage fails with "no Gradle found", no panic (Task 4 `missing_gradle_is_reported`).
3. A JAR with a nested JAR that hides the risky class: the finding is still reported, with the nested path (Task 3 `nested_jar_is_scanned`).
4. A corrupt or truncated class file in a JAR: the scan reports a Low "unreadable class" finding and continues (Task 3 `corrupt_class_does_not_abort`).
5. `gh` not installed or not logged in: a clear error that suggests `--local` (Task 5 `gh_missing_suggests_local`).

---

### Task 1: Process runner

**Files:** Create `core/src/build/mod.rs` (module decls) and `core/src/build/process.rs`; modify `core/src/lib.rs` (`pub mod build;`).

**Interfaces — Produces:**
```rust
pub struct ProcSpec { pub program: PathBuf, pub args: Vec<String>, pub cwd: PathBuf, pub env: Vec<(String,String)>, pub timeout: Duration }
pub struct Watch { pub pattern: String, pub send: String, pub grace: Duration }
pub enum ProcStatus { Exited(i32), TimedOut, WatchMatched }
pub fn run(spec: &ProcSpec, log: &Path, watch: Option<&Watch>) -> io::Result<ProcStatus>
pub fn scrubbed_env() -> Vec<(String,String)>   // from std::env, allow-list above
```
`run` clears the env, sets `spec.env`, and merges stdout and stderr into `log` line by line. On a `watch` match it writes `send` to stdin, waits `grace`, then kills the tree and returns `WatchMatched`.

- [ ] Tests (Unix, `sh` child): `exit_code_is_returned` (`exit 3` → `Exited(3)`); `env_is_scrubbed` (parent sets `ANTHROPIC_API_KEY`; the child's `env` output in the log lacks it and has `PATH`); `timeout_kills_grandchild` (`sh -c 'sleep 100 & echo $! > pid; trap "" TERM; sleep 100'`, timeout 1 s → `TimedOut`, and the pid from the file is no longer alive via `kill -0`); `watch_sends_stop` (child prints `Done (1s)`, reads a line, and exits 0 if it is `stop`).
- [ ] Run `cargo test -p ummp-core build::process` → fail; implement; pass.
- [ ] Commit `feat(build): process runner with timeout, tree kill, env scrub`.

### Task 2: Log and crash analyzers

**Files:** Create `core/src/build/logs.rs` and fixtures `tests/fixtures/logs/{gradle-compile.log,gradle-deps.log,gradle-java.log,mixin-fail.log,crash-report.txt}` (real formats, synthetic names).

**Interfaces — Produces:**
```rust
pub enum ErrorKind { Compile, MissingSymbol, Dependency, JavaVersion, Mixin, Crash, Timeout, Other }
pub struct LogError { pub kind: ErrorKind, pub file: Option<String>, pub line: Option<u32>, pub message: String }
pub fn analyze_log(text: &str) -> Vec<LogError>
pub struct CrashInfo { pub description: String, pub exception: String, pub suspected_mods: Vec<String> }
pub fn parse_crash_report(text: &str) -> Option<CrashInfo>
```
- [ ] Tests: one per fixture, asserting kind, file and line (e.g. the compile log → `Compile`, `src/main/java/com/example/demo/DemoWidget.java`, line `12`). The crash fixture → description, first exception line, suspected mods `["demowidget"]`. `analyze_log("")` → empty. Mutation-check the compile rule.
- [ ] Implement with line scans: `error:` with a `path:line:` prefix, `cannot find symbol`, `Could not resolve`, `class file has wrong version` / `Unsupported class file major version`, `Mixin apply failed` / `MixinApplyError`, `---- Minecraft Crash Report ----` / `Description:` / `Suspected Mod`.
- [ ] Commit `feat(build): log and crash analyzers`.

### Task 3: Malware risk scan

**Files:** Create `core/src/scan.rs`, `core/src/scan_rules.toml` (via `include_str!`), and test `core/tests/scan.rs`. The test compiles Java fixtures with `javac` at test time, like `bytecode_jvm.rs`, and skips with a message if `javac` is absent.

**Interfaces — Produces:**
```rust
#[derive(PartialOrd, Ord)] pub enum Severity { Low, Medium, High }
pub struct Finding { pub rule: String, pub severity: Severity, pub file: String, pub evidence: String }
pub struct ScanReport { pub findings: Vec<Finding> } // fn risk(&self)->Option<Severity>, to_text(), to_json()
pub fn scan_path(path: &Path) -> Result<ScanReport, ScanError>   // dir or .jar
```
Rule TOML: `[[rule]] id, severity, description, files = ["class","java","kt","gradle","kts","properties"], all = [[..any-of..], [..any-of..]]`. A rule matches when every group has a hit in the same file's text; for class files the text is the joined Utf8 constants.

Built-in checks in code:
- native libraries not referenced by the descriptor: High
- nested JARs: recurse, depth 3
- obfuscation: Medium, when more than 60 % of at least 10 classes have 1–2-char simple names
- a base64 run of 2000 chars or more: Medium
- an unreadable class: Low

Rules to ship (all from the spec):
- exec + network
- `defineClass` / `URLClassLoader`
- session token: `getAccessToken`, `accessToken`, or `session` + network
- credential paths: `Local Storage/leveldb`, `Login Data`, `discord`
- webhook or paste URLs: `discord.com/api/webhooks`, `pastebin.com/raw`
- a build script that downloads and executes: `exec {`/`Exec` + `http`
- writes to startup folders
- writes of `.jar` files into the `mods` folder
- reflection on string-built names: Medium
- plain network use: Low
- file deletion: Low

- [ ] Tests: `clean_jar_has_no_findings` (the text contains "not proof"); `exec_with_network_is_high`; `defineclass_is_high`; `webhook_url_is_high`; `build_script_download_exec_is_high` (a dir with `build.gradle`); `nested_jar_is_scanned`; `corrupt_class_does_not_abort`; `native_lib_is_high`; `obfuscated_jar_is_medium`. Mutation-check the exec rule.
- [ ] Implement, reusing the Utf8-constant reading idea from `bytecode.rs` (a read-only parser here).
- [ ] Commit `feat(scan): static malware risk scan`.

### Task 4: Build core, Java toolchain, local backend, report

**Files:**
- Create `core/src/build/{local.rs,java.rs,report.rs}`.
- Create fixture `tests/fixtures/build-demo/`:
  - `settings.gradle`
  - `build.gradle` with the `java` plugin and a `runServer` `JavaExec` task that runs `com.example.demo.FakeServer`
  - sources `DemoWidget.java`, and `FakeServer.java`, which prints `Done (0.1s)!` and exits on `stop`
- Create test `core/tests/build_local.rs` (skips if `gradle` or `javac` is missing).

**Interfaces — Produces:**
```rust
pub enum Stage { Build, Test, Server }          // build/mod.rs
pub enum StageStatus { Passed, Failed, Skipped, TimedOut }
pub struct StageResult { pub stage: Stage, pub status: StageStatus, pub log: PathBuf, pub errors: Vec<LogError> }
pub struct BuildRequest { pub project: PathBuf, pub mod_name: String, pub stages: Vec<Stage>, pub java: Option<u32>, pub timeout: Duration }
pub struct BuildOutcome { pub stages: Vec<StageResult>, pub tests: Option<(u32,u32)>, pub workdir: PathBuf, pub crash: Option<CrashInfo> }
pub trait BuildBackend { fn run(&self, req: &BuildRequest) -> Result<BuildOutcome, BuildError>; }
pub struct LocalBackend { pub workdir: PathBuf, pub gradle: Option<PathBuf> }
pub fn default_workdir() -> PathBuf
pub fn find_jdk(required: u32) -> Option<PathBuf>       // java.rs: JAVA_HOME, then PATH `java -version` parse
pub fn score(o: &BuildOutcome) -> (u32, u32)            // report.rs: passed, run (Skipped excluded)
pub fn to_markdown(o: &BuildOutcome, scan: &ScanReport) -> String
```
- Stages stop after the first failure; later stages are `Skipped`.
- Gradle args: `build -x test`, `test`, `runServer`, each with `--no-daemon --console=plain`.
- The server stage writes `run/eula.txt` and uses `Watch{pattern:"Done (",send:"stop\n",grace:30s}`.
- JUnit counts come from `build/test-results/**/*.xml` (`tests=`, and `failures=` + `errors=`).
- Crash: the newest `run/crash-reports/*.txt`.

- [ ] Tests:
  - `local_build_demo_all_pass`: 3 Passed, score (3,3), the workdir is under the given tempdir, and the source is untouched.
  - `compile_error_skips_later_stages`: copy the fixture and break `DemoWidget.java`; Build is Failed with a `Compile` error at that file, and the others are Skipped.
  - `missing_gradle_is_reported`.
  - `markdown_contains_score_and_ai_debug_hint`.
  - `find_jdk_reads_java_home`: a fake `JAVA_HOME/bin/java` script prints `openjdk version "21.0.2"`.
- [ ] Implement; commit `feat(build): local backend, java toolchain, report`.

### Task 5: GitHub backend

**Files:** Create `core/src/build/github.rs`, golden `tests/fixtures/golden/ummp-build.yml`, and test `core/tests/build_github.rs`, which prepends a dir of fake `gh`/`git` shell scripts to PATH.

**Interfaces — Produces:**
```rust
pub struct GithubBackend { pub repo: String, pub gh: PathBuf, pub git: PathBuf, pub poll: Duration, pub workdir: PathBuf }
pub fn workflow_yaml(java: u32, stages: &[Stage]) -> String
```
Flow:
1. Copy the project to `<workdir>/<mod>-<ts>/` and write `.github/workflows/ummp-build.yml`.
2. `git init` and commit, then `git push https://github.com/<repo>.git HEAD:refs/heads/ummp/<mod>-<ts>` with `-c credential.helper= -c credential.helper=!gh auth git-credential`.
3. Poll `gh run list -R <repo> --branch <b> --json databaseId,status,conclusion` until `completed`.
4. `gh run download <id> -R <repo> -D <workdir>/artifact`.
5. Parse the artifact's `stages.json` (written by the workflow, one `{stage,status}` per stage) and the logs into a `BuildOutcome`.

Workflow contents:
- `on: push: branches: ['ummp/**']` and `permissions: contents: read`.
- `actions/checkout@v4`, then `actions/setup-java@v4` (temurin, the given version).
- One step per stage with `continue-on-error`, then a final step that writes `stages.json`.
- `actions/upload-artifact@v4` with `if: always()`.
- The server step runs under `timeout 600` and watches for `Done (` in bash.

- [ ] Tests: `workflow_matches_golden`; `fake_gh_round_trip` (fake scripts record argv to a file and return canned JSON plus a zip; assert the branch name pattern, the push URL, and the outcome stages); `gh_missing_suggests_local`; `failed_run_reports_failed_stage`.
- [ ] Commit `feat(build): GitHub Actions backend`.

### Task 6: CLI, docs, real runs

**Files:**
- Modify `cli/src/main.rs`: commands `scan {path, --json}` and `build {path, --local, --repo, --workdir, --stages, --timeout, --i-accept-risk, --yes, --report}`.
- Create `docs/BACKLOG.md`, `docs/phase/phase-08-build.md`, and `docs/decisions/0011-build-isolation-and-risk-gate.md`.
- Update `PROJECT_STATUS.md`, `KNOWN_LIMITATIONS.md`, `CHANGELOG.md`, `README.md`, `CLAUDE.md` (layout, next ADR 0012), and `docs/decisions/README.md`.

`build` behavior:
- Scan first, print the scan summary, then apply the policy in Global Constraints.
- Java version: from the analyzer, falling back to `required_java(target mc)`.
- JAR input: refuse ("scan only").
- `--repo` defaults to `intorbmn/ummp-build-sandbox`, overridable by config or env `UMMP_BUILD_REPO`.

- [ ] CLI smoke:
  - `ummp scan` on the clean and risky fixtures: exit code 0, and 2 when High.
  - `ummp build --local` on `build-demo` → 3/3.
  - The risky fixture with `--local` is refused without the flag.
- [ ] Real GitHub run: create private repo `intorbmn/ummp-build-sandbox` (`gh api user/repos -f name=ummp-build-sandbox -F private=true`), then `ummp build tests/fixtures/build-demo` → 3/3 from Actions. Record the result in the phase file.
- [ ] Verification gate, commit, push `phase-8-build`, open a PR, wait for CI. Do not merge.
