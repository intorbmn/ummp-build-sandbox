# Phase 12 — CLI, API & Integration

**Status:** IN PROGRESS  
**Branch:** `phase-12d-action` (PR #21 draft; based on current `master`)  
**Guide:** [Phase 12 API guide](../guides/phase-12-api.md)  
**Design history:** [Phase 12 draft](../drafts/phase-12-api.md)  
**ADRs:** [0015 — interim AI key storage](../decisions/0015-ai-key-file-storage.md); [0016 — local REST API](../decisions/0016-local-rest-api.md); [0017 — MCP stdio server](../decisions/0017-mcp-stdio-server.md); [0019 — out-of-process plugin protocol](../decisions/0019-plugin-protocol.md)  
**Design:** [approved specification](../superpowers/specs/2026-11-20-phase12-api-design.md)  
**Plan:** [implementation plan](../superpowers/plans/2026-11-20-phase12-api.md)

## Working method

The phase guide names Claude Code skills that are not installed in this session. The user
approved continuing manually. Apply their practices by hand: design before implementation,
small TDD changes, explicit security review, and verification before marking a milestone done.
Prefer the smallest correct implementation and do not add dependencies when the workspace
already provides the needed capability.

## Dependency boundary

Phase 11 was merged into `master` by PR #11 on 2026-11-20. Phase 12 therefore starts from
the merged `AppServices` facade. The async runtime, HTTP server and transport adapters must
remain outside `core`.

## Milestone status

| Milestone | Status | Notes |
|---|---|---|
| 12.0 AI credentials | IMPLEMENTATION COMPLETE; ACCEPTANCE PENDING | File store, desktop UI, CLI commands, legacy keyring migration and automated leak/write-failure tests. CI #288 green; real-machine sharing check remains. |
| 12.A CLI polish | IMPLEMENTATION COMPLETE; ACCEPTANCE PENDING | Exit codes, global JSON envelopes, strict config precedence, completions and AppServices routing for core workflows. CI #288 green; golden/mutation and real-shell checks remain. |
| 12.B REST API | IMPLEMENTATION COMPLETE; ACCEPTANCE IN SECTION 15 | `server/` crate, Axum + Tokio, localhost + bearer token, path allow-list and SSE. PR #16 merged; CI #553 green. |
| 12.C MCP server | IMPLEMENTATION COMPLETE; CI GREEN; MUTATION CHECKED; REAL-CLIENT CHECK PENDING CONSENT | Official rmcp stdio server, eight AppServices tools, report resource, shared canonical path policy, port/rollback approval gates, strict stdout isolation, and `ummp mcp`. CI #593 green on `80bf512c`; final branch/docs CI #594 green on `b2db8fd5`. |
| 12.D GitHub Action | VERIFIED | Composite Action, contract tests, CI #626 and sandbox smoke #7 passed. PR #21 remains Draft. |
| 12.E Plugin interface | VERIFIED; CI #697 GREEN; MUTATION CHECKED | API 1 manifest, JSON Lines runtime, adapter integration, CLI checks, path validation, scrubbed environment, timeout/output limits and security tests. |

## D2 / P1 design record

- User approved the Phase 12 design specification on 2026-11-20.
- Added ADR 0015, status Accepted (interim), for file-backed AI credentials.
- Reconciled ADR numbering to the Phase 12 guide: ADR 0015 is interim AI credential storage; the CLI contract remains documented in `docs/CLI.md`; later ADRs are REST 0016, MCP 0017, project license 0018 and plugins 0019.
- Added the 12.0-first implementation plan. Execution is manual without skill installation, as the user explicitly approved.
- At the design approval checkpoint, implementation code had not yet been changed.

## 12.A implementation record

- Added `cli/src/exit.rs` with the six documented numeric exit statuses.
- Added `clap_complete` and `ummp completions bash|zsh|fish|powershell`.
- Added `cli/src/config.rs` with strict TOML fields: `output_root`, `kb`,
  `sandbox_repo`, `ai_provider`, and `openai_base_url`. It supports explicit
  `--config PATH`, current-directory config, then the shared UMMP data directory.
- Environment values override TOML; CLI values override both. Existing
  `UMMP_OUTPUT_DIR` and `UMMP_PROVIDER` remain accepted for compatibility.
- Rejects unknown TOML fields, including key-like fields, and maps missing/invalid
  explicit config files to exit code 5 without echoing secret values.
- Applies configured output root, KB path, sandbox repository, AI provider and OpenAI
  base URL to the existing command paths.
- Extended `core::config::load_any` with TOML support (the `toml` dependency already existed).
- Added a versioned `schema: 1` JSON envelope for `analyze --json` and `scan --json`.
- Added typed failure mapping for safety refusal (2), invalid command input (5), and build
  backend/stage failure (4); generic failures remain 1.
- Added config unit tests for field loading, explicit/local/shared lookup precedence,
  environment overrides, blank environment values, and secret-field rejection.
- Added CLI integration tests for explicit config loading, missing config, and secret-field
  rejection without value leakage, while retaining existing exit/completion/JSON tests.
- Updated `docs/CLI.md` to match the shared data directory, supported fields, environment
  variables and `--config` behavior.
- CI run [#44852188873](https://github.com/intorbmn/Universal-Minecraft-Mod-Porter/actions/runs/44852188873) on `b5e66e8`: both `build-test-lint` and `desktop` passed, including Rust build/tests/Clippy/rustfmt and desktop checks/Vitest/Playwright. This verifies the branch state through that commit; it does not by itself close the remaining manual acceptance items.

## Outstanding 12.A acceptance

1. Finish golden fixtures for every command-specific JSON result shape; current tests pin the envelope and representative command results.
2. Verify completion installation/behavior in real bash, zsh, fish and PowerShell sessions.
3. Have the user confirm on their machine that a key saved in desktop appears as masked status in CLI, and vice versa.
4. Run and record mutation checks for the starred credential, leak, JSON and exit-code tests.
5. PR #14 was reviewed and merged; 12.B implementation is now in progress.

## 12.A JSON coverage follow-up

- Added structured JSON results for `pack inspect`, `pack export`, `ai-assist` and `ai-debug`; all commands except shell completion generation now produce a versioned result or error envelope.
- Added integration coverage for `pack inspect --json` and the missing-AI-key refusal envelope.
- Added the six stable exit-code meanings to top-level `ummp --help`.
- Corrected ADR 0015's stale historical note: ADR 0015 is the interim AI credential decision; the CLI contract remains in `docs/CLI.md`.
- CI verification is pending for this follow-up.

## Decisions and open work

- REST framework default: `axum + tokio`, recorded in its own ADR before implementation.
- REST default bind: `127.0.0.1`; non-loopback requires an explicit acknowledgement flag.
- MCP default transport: stdio.
- Plugin execution: child process with bounded JSON-lines input/output, timeout and path validation.
- Do not merge this phase or begin Phase 13 without the user's review.

## 12.A follow-up — AppServices routing

- CI run [#44852188873](https://github.com/intorbmn/Universal-Minecraft-Mod-Porter/actions/runs/44852188873) on `b5e66e8`: both jobs completed successfully. Root and desktop build, tests, Clippy and formatting checks passed; desktop web checks, Vitest and Playwright passed.
- Fixed the desktop OpenAI provider call to borrow the configured base URL; commit `e8a83ee`.
- Routed both human-readable and JSON `analyze` and `scan` commands through `AppServices`, removing their direct CLI calls to the analyzer/scanner. Human output now renders the corresponding serializable app views.
- Commit: `c394737f` (`refactor(cli): route analyze and scan through app services`).
- CI run [#44849791146](https://github.com/intorbmn/Universal-Minecraft-Mod-Porter/actions/runs/44849791146) was triggered for that commit and is pending. No test success is claimed for this change until that run completes.
- Remaining 12.A work is still substantial: route plan/migrate, port, build, pack and rollback through application services; add domain result views for every supported JSON command; complete error/exit mapping and golden tests; verify all four shell completions and documentation.


## 12.A JSON envelope tests and CI correction

- Added pure success/failure JSON envelope builders and four golden-shape unit tests in `cli/src/output.rs`; commit `a935c36`.
- CI run [#44852734836](https://github.com/intorbmn/Universal-Minecraft-Mod-Porter/actions/runs/44852734836) failed only in Clippy: `success_envelope` accepted `serde_json::Value` by value without consuming it.
- Fixed the Clippy finding by borrowing the result value; commit `7cbdd8e` (`fix(cli): borrow JSON envelope result`). The new commit's CI must pass before this change is considered verified.


## 12.A utility JSON contract

- Added an integration test for `adapters --json`: validates the versioned envelope, stable result keys, and typed adapter entries.
- CI run for the test commit is required before treating it as verified.
- `remap-jar`, `adapters`, and `rollback` now expose structured JSON results; updated `docs/CLI.md` to state exactly which commands have command-specific JSON results.


## 12.A AppServices command routing

- Confirmed CI run #252 on commit `0ed95d1`: both `build-test-lint` and `desktop` passed.
- Routed CLI `port` and `build` through `AppServices` jobs. The CLI polls the job view,
  forwards log lines, and returns the job's structured result in JSON mode.
- Preserved local build risk confirmation (Medium requires confirmation; High requires
  `--i-accept-risk`) and the existing custom build report path.
- Extended `PortRequest` with optional KB/mapping overrides so CLI flags are not dropped
  by the application boundary.
- Routed `rollback` through `AppServices`, including the CLI's custom snapshot directory.
- Added CLI JSON failure-contract tests for invalid `port`, `build`, and `rollback` inputs.
- Verification pending: the CI run for this routing change must pass before these changes
  are treated as complete.


### 12.A AppServices routing follow-up

- Routed `migrate` planning through `AppServices::plan`; `--apply` now requires the
  explicit flag and runs `start_migrate_in_place` as a tracked job after producing the
  review plan.
- Extended `PlanRequest` with backward-compatible optional source-loader, KB, mappings
  and snapshot overrides. Desktop callers remain compatible through serde defaults.
- Routed `pack migrate` through `AppServices::start_pack`, retaining online/offline
  index selection, output options, KB/mappings overrides and pack exit codes.
- Added regression coverage for request-level mapping overrides and the CLI invalid-input
  JSON contract. CI verification is pending for this commit.


## PR 1 verification follow-up

- Added a core integration test that resolves a synthetic AI key from `credentials.json`
  and asserts it never appears in settings, the jobs database, job views, job logs, or events.
- Strengthened the AI screen test to verify the masked key appears after save and the raw value
  never enters the rendered document.
- Added a dedicated Tauri Cargo cache to CI; the previous desktop job had no Rust cache and
  rebuilt the Tauri dependency graph on every run.
- Corrected the implementation plan and project docs to use ADR 0015 for interim AI storage;
  ADR 0016 remains reserved for REST.
- CI verification for this follow-up is pending. The only remaining PR 1 acceptance outside
  automation is the user's real-machine check that a key saved in desktop is visible as masked
  status to the CLI (and vice versa).


## 12.A exit/config integration tests — pending CI

- Added an integration test proving CLI provider flags override `UMMP_AI_PROVIDER` and
  TOML, while the environment overrides TOML when no flag is supplied.
- Added a local Gradle failure test for exit code 4 and the versioned JSON result envelope.


## PR 1 verification update — 2026-11-21

- The shared credentials implementation includes atomic file replacement, private Unix
  permissions, key masking, environment-first resolution, schema validation, idempotent
  deletion, OpenAI base-URL validation and legacy keyring migration.
- Added regressions proving failed writes preserve the previous credential file and
  unsupported schema versions fail without exposing the version value.
- CLI integration coverage now verifies stdin key entry, rejection of command-line key
  arguments, provider-specific deletion, and file-backed key resolution by `ai-debug`
  through a local HTTP mock. The mock is isolated from external networking and writes its
  snapshots only into the test's temporary directory.
- CI run [#288](https://github.com/intorbmn/Universal-Minecraft-Mod-Porter/actions/runs/44867212630)
  on commit `9660301`: `build-test-lint` and `desktop` both passed, including Rust
  build/tests/Clippy/rustfmt and desktop npm checks, Vitest, Playwright, Tauri build/tests,
  Clippy and formatting.
- PR 1 is the 12.0 + 12.A review batch. PR 2 (12.B + 12.C) and PR 3 (12.D + 12.E) remain
  gated on the preceding PR being reviewed and merged by the user.
- Outstanding manual acceptance: real desktop↔CLI credential sharing, actual shell
  completion behavior, and mutation-check evidence for starred tests. Do not mark Phase 12
  complete until these and the later PR batches satisfy the guide's definition of done.

- Opened [PR #14](https://github.com/intorbmn/Universal-Minecraft-Mod-Porter/pull/14) for review on 2026-11-21; it was merged at `e86a914e2926060aecc8c65dcfba652f0d11b3a1`.

## Latest PR 1 checkpoint — 2026-11-21

- Verified implementation head: `d369f09dc37515e498f22c03962a74d28d45f406`.
- CI [#522](https://github.com/intorbmn/Universal-Minecraft-Mod-Porter/actions/runs/44892515155)
  passed both `build-test-lint` and `desktop`, including Rust tests, Clippy, formatting,
  Tauri build/tests, npm checks, Vitest and Playwright. CI #496 and #510 also passed on
  their respective restored implementation checkpoints.
- CI correction history:
  - #482 exposed a status-fixture mismatch: the status test included both providers while
    the golden assertion described one. The test now scopes status to Anthropic.
  - #488 exposed the missing successful `ai-assist` result coverage.
  - #490 confirmed the AI-assist analysis result is distinct from AI-debug. Added a dedicated
    `ai-assist-result.json` fixture and a local mock-provider test covering both AI-assist
    agent requests. #492 then caught only rustfmt differences; those were fixed before #496.
- Golden JSON coverage now includes:
  - shared success/error envelopes, scan, analyze, migrate plan, port, build success/failure,
    rollback, adapters and remap-jar;
  - pack inspect, pack migrate and pack export;
  - AI key set/status/delete, successful ai-debug and successful ai-assist.
  Object key sets are checked recursively, including nested arrays and maps.
- Desktop setup and CLI use core `default_data_dir()`; the resolver honors
  `UMMP_DATA_DIR` so both surfaces target the same credentials directory.
- Completion tests generate Bash, Zsh, Fish and PowerShell scripts and check their registration
  markers. Bash also sources its script and checks command registration. Interactive completion
  in real Zsh/Fish/PowerShell sessions remains a manual acceptance item.
- Mutation evidence:
  - CI #345: mutating the JSON command field and InvalidInput exit code caused their golden
    and exit-table tests to fail; restored CI #348 passed.
  - CI #380: exposing the raw credential instead of its mask caused the CLI masking test to fail.
  - CI #382: removing environment precedence and skipping legacy keyring deletion caused their
    respective regression tests to fail; restored CI #386 passed.
  - CI #502: replacing credential read-modify-write with a fresh empty store caused
    `credentials_round_trip_and_keep_other_provider` and the concurrent-provider preservation
    test to fail; restored CI #506 passed.
  - CI #508: exposing `Secret` through `Debug` caused
    `secret_prints_stars_in_debug_and_display` to fail; restored CI #510 passed.
  - CI #520: fault-injecting the file-resolved key into the provider prompt caused
    `file_resolved_ai_key_never_enters_settings_jobs_logs_or_events` to fail, including the
    successful-job assertion. The mutation was restored; CI #522 passed both jobs. All starred
    12.0 mutation checks and the 12.A JSON/exit-code mutation checks now have red/restore-green
    evidence.
- Remaining manual acceptance before PR 1 can be closed:
  1. In real Zsh, Fish and PowerShell sessions, verify completion suggestions, not only script
     generation/registration.
  2. Save a key in Desktop, confirm `ummp ai-key status` shows the same masked key; then
     save from CLI and confirm Desktop shows it masked. Do not send a real AI request.
- PR [#14](https://github.com/intorbmn/Universal-Minecraft-Mod-Porter/pull/14) was merged at
  `e86a914e2926060aecc8c65dcfba652f0d11b3a1`; it covers 12.0 + 12.A only.
- **Gate cleared:** 12.B may proceed. Never merge automatically, rebase, force-push or rewrite existing history.

## 12.A latest verification — 2026-11-21

- Current branch head: `98bb45065a91af8de3c3f8040884d6f8b51334bb`.
- CI run [#524](https://github.com/intorbmn/Universal-Minecraft-Mod-Porter/actions/runs/44892712607)
  passed on this exact head: both `build-test-lint` and `desktop` completed successfully.
- The final credential-prompt mutation was restored in commit `d369f09`; the restored
  implementation passed CI #522. The follow-up documentation commit records the complete
  starred mutation evidence above and does not change runtime code.
- Automated 12.A implementation and verification are complete: command-specific JSON golden
  fixtures, exit-code mapping, config precedence, shell completion generation, and
  AppServices routing are covered by tests.
- Remaining acceptance is manual and requires the user's machine:
  1. Verify completion suggestions in interactive Zsh, Fish and PowerShell sessions.
  2. Save a key in Desktop and confirm CLI status shows the same mask; then save through CLI
     and confirm Desktop shows the same mask. Use synthetic keys if preferred; do not send
     an AI request.
- PR #14 merged at `e86a914e2926060aecc8c65dcfba652f0d11b3a1`; Phase 12.B gate is cleared.


## 12.B implementation record — 2026-11-21

- Confirmed PR #14 merge commit: `e86a914e2926060aecc8c65dcfba652f0d11b3a1`.
- Created implementation branch `phase-12b-rest` from that merge commit.
- Added `server/` as the `ummp-server` workspace crate. Axum and Tokio remain outside `core`; the CLI exposes `ummp serve` through the server library.
- Added REST routes for health, analyze, scan, plan, port, reviewed migrate, pack, build, job state, SSE job events, cancellation, rollback and OpenAPI.
- Added bearer-token persistence, Unix mode 0600, constant-time digest comparison, localhost default, explicit remote-bind acknowledgement, 1 MiB body limit, disabled CORS, canonical allowed-root checks and symlink resolution.
- SSE uses a bounded event replay buffer so a client that connects immediately after job creation does not miss initial job events. Events retain the desktop event payload format and per-job order.
- Added `allowed_roots` to the strict `ummp.toml` config contract; absent configuration defaults to the user's home directory.
- Added ADR 0016, `docs/API.md`, `docs/openapi.json`, and real-TCP integration tests on ephemeral ports for auth, path containment, read-only planning, review refusal, job/SSE ordering, cancellation, request size and OpenAPI route coverage; added tracing-capture coverage for token non-disclosure.
- Manual security review: no route exposes credentials; all request paths are checked before reaching `AppServices`; writes use existing service methods and the reviewed migration gate; synchronous work is isolated with `spawn_blocking`. Full automated verification is pending CI.

### 12.B verification and mutation evidence — 2026-11-21

- Re-run locally on `480c356`: `cargo test --all` 329 passed / 0 failed, `cargo clippy --all --all-targets -- -D warnings` clean, `cargo fmt --all -- --check` clean. CI: both jobs green.
- Mutation checks of the starred REST tests (each applied, marker confirmed with grep, test went RED, change restored, `ummp-server` tests 10/10 green again):

| Test | Mutation | Result |
|---|---|---|
| `requires_token` | `authorize` accepts every request (`if true \|\| ...`) | RED |
| `rejects_path_outside_allowed_roots` | `path_is_within` always returns true | RED |
| `plan_does_not_write` | the plan handler writes `injected.txt` into the source folder | RED |

- Security review of `server/` was done by hand (no security-review skill in this session): token is 244 random bits, file mode 0600, constant-time digest compare; loopback by default, remote bind needs `--i-understand-remote`; every request path is canonicalised and checked against `allowed_roots` (symlink-escape test present); 1 MiB body limit; CORS off; writes only through `AppServices`; the token is printed once to stdout and never logged (`token_never_logged`). Residual risks: a time-of-check/time-of-use gap between path check and use, and the token printed to the terminal. Both go to KNOWN_LIMITATIONS.
- Existing tests also cover `port_job_completes_and_sse_events_are_ordered`, `cancel_works`, `rejects_symlink_escape`, `core_remains_free_of_tokio` and `token_file_is_private`.


## 12.C design checkpoint — 2026-11-21

- PR #16 (12.B REST) merged at `c1d08b44aa8d21363a4dfd3e76c30633e5528f12`; CI for the final 12.B implementation passed (run #553).
- Created `phase-12c-mcp` from the merged `master`.
- Chose `server/src/mcp.rs` to share the REST crate, runtime and path policy.
- Checked official `rmcp 3.10.0`: its declared MSRV is Rust 1.88, compatible with the repository's pinned Rust 1.97.0. Stdio/server transport is supported.
- Added ADR 0017 and the 12.C design addendum. Implementation and verification are next; 12.C is not marked complete.


## 12.C implementation checkpoint — 2026-11-21

- Added `server/src/mcp.rs` using official `rmcp = 3.10.0` stdio transport. Exposes `ummp_analyze`, `ummp_scan`, `ummp_plan`, `ummp_port`, `ummp_build`, `ummp_job`, `ummp_cancel`, and `ummp_rollback`.
- Added the `ummp://report/{job}` Markdown resource template. Job state and reports are sourced from `AppServices` only.
- Reused the REST crate's canonical candidate resolution, root canonicalization and path containment helpers for MCP path checks.
- Port is read-only unless `approve=true`; rollback requires `confirm=true`; local build requests are gated by `AppServices::check_build_risk` and require Low risk. GitHub is the default build backend.
- Added `ummp mcp`, MCP client/setup documentation, README and CLI references, and CI push trigger for `phase-12c-mcp`.
- Added `cli/tests/mcp_stdio.rs`: launches the real `ummp mcp` binary and verifies initialize, tools/list, all eight tools, report resource discovery/read, plan-only behavior, approved port output, and that a synthetic API key never appears in tool/resource responses.
- Added regression coverage for an unapproved port whose optional output path escapes `allowed_roots`; MCP now validates the output path even when it is only previewing.
- Explicitly routes CLI tracing to stderr; stdout remains JSON-RPC only.
- CI run #593 on `80bf512c`: both `build-test-lint` and `desktop` passed, including tests, Clippy and formatting.
- Mutation check: renamed the `ummp_analyze` handler to `ummp_analyze_mutated`; the stdio integration test went RED with “tools/list omitted ummp_analyze” (CI #592). Restored the handler; CI #593 is green.
- The real Claude Code client smoke test is NOT RUN: it requires the user's consent and an available local Claude Code installation.


## 12.D GitHub Action — verified

- Added root `action.yml` as a composite Action supporting `analyze`, `scan`, `port` and `build`, with configurable risk gating, schema-1 JSON output, GitHub Step Summary and report artifact.
- The action installs the pinned Rust 1.97.0 toolchain, caches Cargo data, installs the CLI, and captures command exit status without inherited Bash `-e` interrupting report publication.
- Added `cli/tests/action_contract.rs` and Action usage documentation.
- CI #626: `build-test-lint` and `desktop` both passed.
- Sandbox smoke run #7 (run ID `44917327945`) passed in `intorbmn/ummp-build-sandbox`; verified outputs and JSON report. Artifact `ummp-report-44917327945-1` (ID `14712426834`) uploaded successfully.
- PR #21 remains Draft and is not merged.

## 12.E Out-of-process plugin system — verified

- User explicitly approved manual implementation without skills on 2026-11-21.
- Manifest: `ummp-plugin.toml`, fields `name`, `version`, `kind`, `command`, and `ummp_api = 1`.
- Discovery default: `<data_dir>/plugins/`. CLI: `ummp plugins list` and `ummp plugins check <dir>`; check validates manifest, executable, API version and the `hello` handshake.
- JSON Lines methods: `hello`, `findings`, and `edits`. Each invocation launches an out-of-process executable.
- Runtime uses a scrubbed environment, 30-second timeout with process-tree termination, and an 8 MiB stdout cap. Secret/config files, SSH/cloud/container/Kubernetes credential directories and private-key files are excluded from plugin input; source files such as `TokenManager.java` remain eligible.
- Returned paths must match supplied project files, canonicalize within the project root, and cannot target `.jar` or `.class`. Returned original content must match the current file. Accepted edits flow through MigrationEngine checkpoints and rollback.
- Added fake-plugin, CLI, API compatibility, path traversal, timeout, stdout-limit and environment-security tests.
- ADR 0019 records the plugin protocol; ADR 0018 remains the project-license decision.
- CI #697: both `build-test-lint` and `desktop` passed, including tests, Clippy and formatting.
- Mutation checks:
  | Test | Mutation | Result |
  |---|---|---|
  | `rejects_plugin_returned_path_traversal` | Bypassed path validation; CI #668 | RED as expected; restored in commit `b43fbe8`, CI #670 green |
  | `plugin_edit_is_checkpointed_and_rollback_restores_source` | Changed fake plugin output from `DemoGadget` to `DemoMutated`; CI #695 | RED as expected; fixture restored in commit `5d61759`, final CI #697 green |
- Security boundary: the protocol exposes no file-write operation, but this is not an OS sandbox. Plugins run with the current user's OS permissions. Install only plugins you trust.
- Protocol and usage details: [`docs/PLUGIN_PROTOCOL.md`](../PLUGIN_PROTOCOL.md).

