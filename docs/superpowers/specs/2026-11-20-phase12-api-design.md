# Phase 12 — CLI, API & Integration: design

**Status:** approved by the user (2026-11-20)  
**Date:** 2026-11-20  
**Branch:** `phase-12-api`  
**Scope authority:** `docs/guides/phase-12-api.md` (Phase 12 guide overrides this design if they disagree).

## 1. Intent

Make UMMP a safe, reusable local service for humans, scripts, AI assistants and CI while keeping one business-logic boundary: `core/src/app/AppServices`.

The phase has six milestones, in this order:

1. **12.0 — AI credentials:** let a user enter an AI API key directly in the desktop AI screen or CLI and share it between desktop and CLI through an interim `AiKeyStore` abstraction backed by `credentials.json`.
2. **12.A — CLI contract:** stable exit codes, global versioned JSON output, shell completions and a validated `ummp.toml` configuration file.
3. **12.B — REST:** a localhost-only, token-authenticated HTTP API that exposes safe application services and job events.
4. **12.C — MCP:** a stdio MCP server for assistant-driven analysis, planning and explicitly approved actions.
5. **12.D — GitHub Action:** a composite action for analyze, scan, port and build in another repository's CI.
6. **12.E — Plugins:** out-of-process adapters/rules that return findings and proposed edits; UMMP alone applies edits.

## 2. Goals and non-goals

### Goals

- CLI, desktop, REST and MCP are thin adapters over `AppServices`; shared behavior stays in `core/src/app/`.
- The CLI and desktop use the same AI credential store and data directory.
- Environment AI keys take precedence over saved credentials for automation.
- Every write remains behind an existing application-service approval/checkpoint boundary or writes to a new output directory.
- Every untrusted path is checked against allowed roots and resolved with `resolve_within` before use.
- Secrets never appear in settings, logs, events, command output, JSON, REST/MCP responses or plugin environments.
- Async runtime dependencies are confined to the new `server/` crate. No Tokio or async code in `core`.
- Each milestone has focused tests, security review where required, documentation and CI evidence.

### Non-goals

Hosted or multi-user service; remote build execution through REST; MCP over HTTP; AI routes in REST/MCP; WASM plugins; signed releases; Phase 13 work.

## 3. User-approved decisions

The user approved all eight defaults in guide §3 on 2026-11-20:

1. AI credentials: `credentials.json` in the shared UMMP application-data directory. This is explicitly interim storage behind `AiKeyStore`; do not promise that file storage is permanent.
2. GitHub token: remains in the OS keyring. Only AI provider keys move to the interim file store.
3. REST: Axum + Tokio in a new `server/` workspace crate.
4. REST access: bind to `127.0.0.1` by default; non-loopback binding requires both `--listen 0.0.0.0` and `--i-understand-remote`. A random bearer token is stored in `<data_dir>/server-token`, created with restrictive permissions on Unix and printed once.
5. MCP: official Rust SDK `rmcp` over stdio if its current API and MSRV fit the pinned toolchain; otherwise use a small, tested JSON-RPC stdio implementation. Confirm the crate version before implementation.
6. MCP tools do not call UMMP's AI providers. The external assistant is already the AI.
7. Plugins: out-of-process executable, manifest `ummp-plugin.toml`, newline-delimited JSON request/response protocol.
8. Delivery: three separately reviewed PRs, each merged before starting the next:
   - PR 1: 12.0 + 12.A.
   - PR 2: 12.B + 12.C.
   - PR 3: 12.D + 12.E.
   Never merge a PR or start Phase 13 without the user's review.

## 4. Architecture

```text
Desktop UI ─┐
CLI ────────┼──> core/src/app::AppServices ──> existing engines
REST ───────┤          (synchronous; no Tokio)
MCP ────────┘
                  ▲
       server/ (Axum + Tokio; spawn_blocking)
```

- `core/src/app/` owns use-case orchestration, validation, jobs, views, approval and rollback.
- `cli/`, `ui/src-tauri/`, `server/` and MCP handlers perform argument/transport conversion only.
- Long-running service calls from REST/MCP run through `tokio::task::spawn_blocking`; do not hold async runtime workers while synchronous engines run.
- Reuse existing `Secret`, `AppError`, `JobView`, event payloads and service methods where they fit. Add the smallest service method required when an adapter otherwise would duplicate business logic.
- Keep the desktop Tauri workspace separate from the root Rust workspace, as established in Phase 11.

## 5. Milestone design

### 5.1 12.0 — shared AI credentials

Introduce `core/src/app/credentials.rs`:

- `AiProviderKind::{Anthropic, Openai}`, serializable in lowercase.
- `AiKeyStore` trait with `get`, `set`, `delete` and `masked`.
- `FileKeyStore` is the only implementation in this phase. Callers depend on the trait, not JSON or file paths.
- `CredentialsFile` stores schema version 1, Anthropic key, OpenAI key and optional OpenAI-compatible base URL.
- `resolve_ai_key` checks environment first, then the store.
- `default_data_dir()` follows the desktop app identifier `io.github.intorbmn.ummp` on each platform; `UMMP_DATA_DIR` overrides it for tests and portable use.

Storage and validation:

- Read-modify-write to preserve the other provider's key.
- Atomic replacement using a temporary file in the same directory; flush before rename.
- Unix file mode `0600` is set before writing secret bytes. Windows relies on the private per-user app-data directory; document that same-user processes can read the file.
- Trim surrounding whitespace; reject empty values, values over 512 characters and control characters.
- Warn, but do not reject, an Anthropic key without the `sk-ant-` prefix.
- Mask with first seven characters + ellipsis + last four; for short keys show only ellipsis + last two.
- Corrupt JSON returns a clear typed application error, never a panic.
- Delete is idempotent. Never serialize `Secret` or include the raw value in Debug/Display.

Desktop:

- Remove AI key names from the OS keyring allow-list; retain `github_token`.
- Add status/save/delete commands. Status returns only saved/masked/path; save returns masked/warning; no command returns a full key.
- AI screen has a password input and show toggle when no key exists. Start saves the typed key and then starts the AI job. After save, clear component state and show only the masked value with Change/Delete.
- Settings offers the same masked status and Change/Delete controls.
- Keep UI wording neutral: say “saved”, not “saved in a file”.
- Migrate old AI keys from keyring once, through `SecretStore`; copy to file then delete the old keyring entry. Failure to read/delete the keyring is logged without the secret and does not block app startup.
- Add English and Vietnamese strings.

CLI:

- Add `ummp ai-key set [--provider anthropic|openai]`, `status` and `delete`.
- Never accept a key as a command-line argument. Read hidden terminal input or one line from stdin.
- `status` alone shows the credentials path and masked key.
- AI commands resolve environment first, then saved credentials. OpenAI base URL comes from the credentials file unless explicitly overridden.
- Missing-key guidance names `ummp ai-key set` and the desktop AI screen.

ADR 0015 is **AI API keys in a user credentials file**, status **Accepted (interim)**. ADR 0014 becomes “partly superseded by 0015”. Keep final storage choice in BACKLOG until the user chooses it.

### 5.2 12.A — CLI contract

The branch already contains partial 12.A work. Retain and review it rather than recreate it:

- `cli/src/exit.rs` defines 0–5.
- `clap_complete` shell completions exist.
- `cli/src/config.rs` has an initial TOML/environment/CLI precedence implementation.
- `analyze --json` and `scan --json` have initial schema-1 envelopes.
- Initial integration tests and a CI workflow trigger exist.

Complete the guide contract:

- Move every command that has an application service onto `AppServices`; CLI-specific code is limited to clap, terminal prompts and output. Split `main.rs` into `args.rs`, `exit.rs`, `output.rs` and a short entry point as justified by the final design.
- Make `--json` global and support every command using the same `*View` structs as the desktop, with top-level `schema: 1`. JSON goes to stdout; human logs go to stderr. Errors use `{"schema":1,"error":{"kind":"…","message":"…"}}`.
- Exit codes: 0 success; 1 unexpected error; 2 refused (risk/review/missing key); 3 completed with user work remaining; 4 build failed; 5 invalid input. Preserve `pack migrate` 0/2/3.
- Config lookup: explicit `--config PATH`, then `./ummp.toml`, then `<data_dir>/ummp.toml`. Fields: `output_root`, `kb`, `sandbox_repo`, `ai_provider`, `openai_base_url`. Reject key-like fields; no credentials in TOML. Precedence: CLI flag > `UMMP_*` environment > file > built-in.
- Completions for bash, zsh, fish and PowerShell must work independently of user config.
- Pin JSON shapes with golden fixtures and retain all existing CLI behavior unless an intentional change is documented.

The current branch's CLI ADR 0015 conflicts with the guide's required ADR numbering. Before PR 1, remove that premature ADR entry/file and keep the CLI contract documented in `docs/CLI.md`; ADR 0015 must describe AI credential storage. Do not renumber or rewrite Git history.

### 5.3 12.B — local REST API

Create `server/` as crate `ummp-server`, the only crate in this phase that depends on Tokio.

- `ummp serve` starts Axum; use `spawn_blocking` around synchronous `AppServices` calls.
- Version routes under `/v1`: health, analyze, scan, plan, port, reviewed migrate, pack, build, job status, job SSE events, job cancel, session rollback and OpenAPI.
- No AI routes.
- Limit bodies to 1 MiB; CORS off.
- Default bind `127.0.0.1`. Remote bind requires explicit opt-in flag.
- Bearer token is generated once, stored at `<data_dir>/server-token`, mode 0600 on Unix and compared in constant time. Never log or return the token. Print it once only when starting the server.
- Every path is canonicalized and checked against canonical allowed roots from `ummp.toml`; default allowed root is the user's home. Use `resolve_within` for joined paths.
- OpenAPI is generated with `utoipa` if compatible; otherwise commit `docs/openapi.json` and test its route set against the router. Make the choice after dependency/MSRV check.

### 5.4 12.C — MCP stdio server

- Expose `ummp mcp` on stdio; choose `rmcp` only after checking current crate version, transport support and pinned Rust toolchain compatibility. Otherwise implement only the required JSON-RPC stdio subset.
- Tools: analyze, scan, plan, port, build, job, cancel and rollback. Resource: `ummp://report/{job}`.
- Every tool description states whether it writes.
- `ummp_port(approve:false)` returns a plan and writes nothing; `approve:true` executes through safe services. Rollback requires `confirm:true`.
- Local build is refused unless risk is Low; GitHub backend is preferred.
- No shell execution, no key access, no AI provider calls, no MCP-over-HTTP.
- Reuse REST allowed-root validation policy; do not make a second weaker path validator.

### 5.5 12.D — GitHub Action

Add root `action.yml` as a composite action with inputs `command` (analyze|scan|port|build), `path`, `to`, `loader`, `fail-on-risk` (default high), and outputs `exit-code`, `report-path`.

Steps: install Rust; install UMMP from `cli/` with `--locked` and cache; run command with `--json`; append summary to `$GITHUB_STEP_SUMMARY`; upload report artifact. Test the action manifest and run it in `intorbmn/ummp-build-sandbox` from a pushed test branch. Record the real workflow run id.

### 5.6 12.E — plugin protocol

- Discover plugins only under `<data_dir>/plugins/`.
- Manifest: `name`, `version`, `kind = "adapter" | "rules"`, relative `command` array and `ummp_api = 1`.
- JSON-lines protocol: `hello`, `findings`, `edits`. Plugin output is data, never instructions.
- Plugins never write project files. Convert returned edits into an `EditSet` and apply only through the engine after its checkpoint.
- Use existing `build::process::run` with 30-second timeout per call, process-tree kill and scrubbed environment; cap stdout at 8 MiB.
- Validate every returned path through `resolve_within`; reject `.jar` and `.class` targets.
- Add `ummp plugins list` and `ummp plugins check <dir>`, plus `docs/PLUGIN_PROTOCOL.md`.

## 6. Security and reliability invariants

- Do not expose secrets in serialized data, Debug/Display, tracing, terminal errors, process arguments, child environment (except the existing narrowly scoped GitHub build token mechanism), REST/MCP payloads or plugin input.
- Do not trust a path because it is absolute or canonical. Check it against allowed roots and use `resolve_within` for path joins.
- Keep REST localhost-only unless the caller explicitly acknowledges remote exposure.
- Compare bearer tokens in constant time.
- Bound request bodies, plugin output and plugin execution time.
- Keep `core` synchronous.
- Preserve approval gates and checkpoint behavior; an adapter must not make a write path easier to invoke accidentally.
- Treat malformed credentials, config, MCP messages and plugin responses as recoverable errors, not panics.

## 7. Verification strategy

All tests use temporary directories and fake credentials. Never touch the maintainer's real home directory or use a real API key in automated tests.

### 12.0 required tests

- credentials round-trip and preserve the other provider; environment overrides file; fake key absent from settings, jobs DB and captured events; mask behavior; invalid key rejection; prefix warning; Unix mode 0600; atomic-write failure leaves prior file intact; corrupt file gives clear error; idempotent delete; data-dir override.
- Desktop keyring migration once, no command returns full key, missing-key start refusal gives entry hint.
- Frontend shows input when missing; Start saves before job start; saved key is masked and input cleared.
- CLI hidden/stdin set, status mask, raw key argument rejected, AI command resolves file key.
- Mutation-check the three starred credential/security tests and record how.

### 12.A required tests

- Golden key sets for global JSON output; error JSON shape; one test per exit code; flag/env/file precedence; key-like config field rejected; completions for all four shells; all existing CLI tests pass.
- Mutation-check JSON shape and exit-code tests.

### 12.B required tests

- Real server on ephemeral port with ureq and temporary data dir: token required; outside-root path rejected; plan is read-only; migrate without review refused; port job completes and SSE events are ordered; cancel; 413 for oversized body; OpenAPI contains every route; token absent from tracing.
- Focused security review of `server/` and record findings.

### 12.C required tests

- JSON-RPC pipe exchange: initialize, tools/list, plan, unapproved port no write, approved port writes through safe service.
- Outside-root rejection; rollback confirmation; key absent from every tool output.

### 12.D–12.E required tests

- Golden action manifest plus one real sandbox workflow run.
- Fake plugin renames `DemoWidget` to `DemoGadget`, applies via engine and rolls back; traversal path rejected; hanging plugin killed at timeout; plugin environment contains no key names or values; unsupported API version rejected.

### Full verification

Before each PR: root `cargo build --all`, `cargo test --all`, `cargo clippy --all --all-targets -- -D warnings`, `cargo fmt --all -- --check`; desktop `npm ci`, `npm run check`, `npm run test`, and `cargo test` in `ui/src-tauri` where system libraries are available. CI must be green for both root and desktop jobs. Do not claim a local test passed unless it was actually run.

## 8. Existing branch audit and reconciliation

The existing `phase-12-api` branch is already 28 commits ahead of current `master` and has no commits behind it. It contains partial 12.A work (exit codes, completions, config loader, analyze/scan JSON envelopes and tests). Keep this work and its history; do not recreate or reset the branch.

The current 12.A implementation is not complete against the guide:
- JSON is per-command rather than global and only analyze/scan are covered.
- Config fields and lookup behavior do not yet match the guide's full contract.
- `main.rs` still calls many engines directly instead of routing through `AppServices`.
- 12.0 has not been implemented.
- The branch's existing ADR 0015 is for CLI output/config, but the guide now reserves ADR 0015 for interim AI credential storage. Remove the conflicting CLI ADR file/index entry in a normal forward commit before adding the correct ADR 0015. Keep the CLI contract in `docs/CLI.md`. Do not rewrite history.
- Complete 12.0 before returning to finish 12.A, even though partial 12.A commits already exist.

The branch HEAD is `9e5dcb18050a727ec5cddd7e0e388ab6f9ace1e4`; current master is `4344bef7705e340f0d86bf74eee729cbdd1ad945`. The merge base is current master.

## 9. Delivery and acceptance

- PR 1 contains 12.0 + completed 12.A, including ADR 0016 and docs.
- PR 2 contains 12.B + 12.C, including ADR 0017 (REST) and ADR 0018 (MCP).
- PR 3 contains 12.D + 12.E, including ADR 0019 (plugin protocol).
- Each PR is opened only after tests, docs, review and CI are ready. Wait for the user to review and merge before beginning the next PR.
- The phase is complete only after every guide acceptance item and starred mutation check has recorded evidence, the Action has a real run, the user confirms the shared AI key on their machine, and the user agrees to the MCP real-world check.
- Do not start Phase 13.


## 5.7 12.B design addendum (2026-11-21)

The user resumed Phase 12.B after PR #14 was merged. The following 12.B choices resolve guide §9a:

- **CLI placement:** keep one installed binary. The existing `ummp` CLI depends on the `ummp-server` library and exposes `ummp serve`; Tokio remains a dependency of `server/` only.
- **Token rotation:** no `--rotate-token` option in this milestone. Stop the server, delete `server-token`, and restart to create a new token.
- **Allowed roots:** `allowed_roots` in `ummp.toml`; default to the current user's home directory. Canonicalize roots and candidates, resolve symlinks, compare Windows paths case-insensitively, and reject paths outside the configured roots.
- **Job retention:** finished jobs remain queryable for the lifetime already provided by `jobs.db`; no second retention store.
- **OpenAPI:** committed `docs/openapi.json` plus an integration test asserting that every router path is documented.
- **Security:** localhost by default, remote bind requires explicit acknowledgement, bearer token file mode 0600 on Unix, constant-time comparison of token digests, 1 MiB body cap, CORS off, and no AI routes.

These decisions are recorded in ADR 0016. MCP and later Phase 12 decisions remain out of scope for this 12.B implementation.


### 5.5.1 12.D implementation decisions

- **Toolchain:** pin the Action to the same Rust version as `rust-toolchain.toml` (currently 1.97.0), with Clippy and rustfmt components. Keep this value synchronized when the repository pin changes.
- **Cargo cache:** cache the Cargo registry, Git index and target directory. Use an OS + Rust-version key with a restore prefix; install the `ummp` executable into a run-specific temporary root so a cached older binary cannot shadow the current source.
- **Lockfile constraint:** the tracked root `Cargo.lock` predates the `server/` workspace dependencies. Do not regenerate or commit it as part of 12.D. The Action regenerates a lockfile only inside its isolated checkout, then installs with `cargo install --path cli --locked`. This locks each individual run but dependency resolution can change between runs; revisit when the user decides whether the repository lockfile should be updated or untracked.
- **Risk policy:** `scan` performs one scan; `analyze`, `port` and `build` first run a scan and stop when the selected `fail-on-risk` threshold is reached. Thresholds are `high` (default), `medium`, and `none`.
- **Report contract:** `report-path` identifies a directory containing `result.json` plus any generated Markdown report. The directory is uploaded as one artifact.


### 5.6 12.E plugin protocol decisions

- **Discovery:** installed plugins are immediate child directories of `<data_dir>/plugins/`, each containing `ummp-plugin.toml`. `ummp plugins list` lists manifests; `ummp plugins check <dir>` validates the manifest, executable, API version and `hello` response.
- **Protocol:** each invocation sends one JSON object terminated by LF and expects exactly one JSON response line. Requests are `hello`, `findings` and `edits`; project file contents are sent over stdin. Secret-like files are excluded.
- **Execution:** child process, 30-second timeout, process-tree kill, allow-listed environment, 8 MiB stdout cap. Stderr is drained separately and is not protocol data.
- **Edit validation:** each returned path must name an existing file supplied in the request, resolve within the canonical project root, and not end in `.jar` or `.class`. The returned `original` must exactly match current disk content. UMMP applies the resulting EditSet through MigrationEngine's checkpointed apply path.
- **Trust limitation:** no portable OS sandbox is added. A malicious executable still has the current user's filesystem/process permissions; install only trusted executables. This is recorded as a limitation, not described as sandboxing.
- **ADR numbering:** ADR 0018 is already accepted for the project license; plugin protocol uses ADR 0019.
