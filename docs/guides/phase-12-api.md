# Phase 12 — CLI, API & Integration (guide for Claude Code)

Read [README.md](README.md) first. Rough history: `docs/drafts/phase-12-api.md`.

## 0. Skills
- **Always:** the README §3 set.
- **Phase-specific:**
  - `rust-cli-skills:rust-cli-interface`: exit codes, `--json`, completions.
  - `example-skills:mcp-builder` and `ecc:mcp-server-patterns`: the MCP server.
  - `ecc:api-design`: REST.
  - `rust-cli-skills:rust-concurrency`: the server.
  - `claude-security:claude-security`: a focused review of the network surface before the
    merge.
  - `ecc:architecture-decision-records`.

## 1. Goal (master prompt, Phase 12)
Make UMMP usable from scripts, CI and other tools:
- **CLI polish:** stable exit codes, machine-readable output, shell completions, a config
  file.
- **Local REST API** (`ummp serve`).
- **MCP server** (`ummp mcp`), so AI assistants (Claude Code, Claude Desktop) can analyze,
  plan and port through UMMP's safe paths.
- **GitHub Action**, to analyze or port in CI.
- **Plugin interface**, for third-party adapters and rules that run out of process.

Everything goes through `AppServices` from Phase 11 (ADR 0004). If Phase 11 is not
merged, first build the services facade (guide 11, milestone 11.A) as its own PR.

**Non-goals:** a hosted service, multi-user auth, remote execution of untrusted builds
through the API.

## 2. What exists
- The clap derive CLI with these commands: `analyze`, `migrate`, `remap-jar`, `rollback`,
  `ai-assist`, `ai-debug`, `scan`, `build`, `port`, `adapters`, plus `pack` after
  Phase 10.
- Reports have `to_markdown()`. Check each report struct for `Serialize`, and add the
  derive where it is missing.
- `AppServices` (Phase 11.A): jobs, events, approval.

## 3. Questions for the user (defaults in brackets)
1. REST framework? [`axum` + `tokio`, the most common choice. It needs an ADR, because it
   is the first async runtime in the workspace. The alternative is `tiny_http`: sync and
   small, but no streaming.]
2. Who may call the API? [Localhost only. A random bearer token is written to a file with
   0600 permissions and printed once.]
3. MCP transport? [stdio first, which is the standard for Claude Desktop/Code; HTTP later]
4. Plugin model? [Out of process: an executable that speaks JSON over stdin/stdout, with a
   manifest `ummp-plugin.toml`. No dynamic libraries, because `unsafe` is forbidden; WASM
   goes to the backlog.]

## 4. Milestones

### 12.A CLI polish
- **Exit codes** in `cli/src/exit.rs`, documented in `docs/CLI.md` and `--help`:
  - 0: ok
  - 1: error
  - 2: refused (risk or approval)
  - 3: done, with items left to the user
  - 4: build failed
  - 5: invalid input
- **`--json` global flag:** print the view or report struct as JSON on stdout. Logs go to
  stderr. Every JSON object has `"schema": 1`.
- **`ummp completions <shell>`** with `clap_complete` (a new crate; name it in the design
  doc).
- **`ummp.toml` config file:** looked up in the current dir, then the user config dir. It
  holds defaults (output dir, sandbox repo, provider) and is read with `config::load_any`.
  Precedence: CLI flag > env `UMMP_*` > config file > built-in default.
- **Tests** in `cli/tests/`:
  - ★ `json_output_is_valid_and_stable`: parse the output and compare its keys with a
    golden JSON.
  - `exit_code_on_risk_refusal_is_2`
  - `config_file_default_is_overridden_by_flag`
  - `completions_generate_for_bash_zsh_powershell`

### 12.B REST API (`ummp serve`)
- **Where:** a new workspace member `server/`, so `core` stays free of async. It calls the
  sync services through `spawn_blocking`.
- **Endpoints:** versioned under `/v1`. The OpenAPI document is served at
  `/v1/openapi.json`; generate it with `utoipa`, or check in a hand-written file that a
  test compares with the routes.
  ```
  GET  /v1/health
  POST /v1/analyze      {path}                    -> AnalysisView
  POST /v1/plan         {path, to, loader}        -> PlanView (dry run)
  POST /v1/port         {path, to, loader, ...}   -> {job}
  POST /v1/build        {path, backend}           -> {job}
  GET  /v1/jobs/{id}                              -> JobView
  GET  /v1/jobs/{id}/events   (SSE)               -> AppEvent stream
  POST /v1/jobs/{id}/cancel
  POST /v1/jobs/{id}/approve  {edits:[ids], decision}
  POST /v1/sessions/{id}/rollback
  ```
- **Security:**
  - Bind `127.0.0.1` by default. `--listen 0.0.0.0` requires `--i-understand-remote`.
  - Compare the bearer token in constant time.
  - Limit the request body size.
  - Every `path` must be inside the `allowed_roots` from the config. Reject others with
    403, using a canonical-path check plus `resolve_within`.
  - CORS is off.
  - Never log the token; a test checks that it is absent from the logs.
- **Tests** in `server/tests/api.rs` (a real server on port 0, with a ureq client):
  - ★ `requires_token`
  - ★ `rejects_path_outside_allowed_roots`
  - `plan_does_not_write`
  - `port_job_completes_and_events_stream_in_order`
  - `cancel_works`
  - `openapi_matches_routes`
  - `token_never_logged`

### 12.C MCP server (`ummp mcp`)
- Follow `mcp-builder`. Use the official Rust SDK (`rmcp`) if it fits, otherwise a small
  hand-rolled JSON-RPC over stdio. Record the choice in the ADR.
- **Tools:**
  - `ummp_analyze(path)`
  - `ummp_plan(path, to, loader)`: returns a diff summary.
  - `ummp_port(path, to, loader, output?, approve: bool)`:
    - with `approve: false` it only plans and writes nothing;
    - an agent must call it again with `approve: true` to write.
  - `ummp_scan(path)`
  - `ummp_build(path, backend)`
  - `ummp_job(id)`
  - `ummp_rollback(session)`
- **Resource:** `ummp://report/{session}`, the markdown report.
- **Safety:**
  - The same allowed-roots check as REST.
  - Tool descriptions say which tools write.
  - No tool takes a shell command.
- **Tests:**
  - ★ A JSON-RPC conversation over pipes:
    1. `initialize`
    2. `tools/list`
    3. `ummp_plan`
    4. `ummp_port` with `approve: false`: nothing is written.
    5. `ummp_port` with `approve: true`: the files are written.
  - `rejects_path_outside_roots`
- **Real-world check:** with the user's consent, add the server on their machine
  (`claude mcp add ummp -- ummp mcp`) and port the demo mod through it. Record the result.

### 12.D GitHub Action
- `action.yml` at the repo root, a composite action. Steps:
  1. Download the release binary for the runner OS. Releases come in Phase 13; until then,
     build with cargo, with a cache.
  2. Run `ummp <command> ... --json`.
  3. Write a summary to `$GITHUB_STEP_SUMMARY` and upload the report as an artifact.
- **Inputs:** `command` (analyze|scan|port|build), `path`, `to`, `loader`,
  `fail-on-risk`.
- **Tests:**
  - A golden test of `action.yml`.
  - One real run in the sandbox repo that uses the action from the branch
    (`uses: intorbmn/universal-minecraft-mod-porter@<branch>`) on the demo mod. Record the
    run id.

### 12.E Plugin interface (out of process)
- **Manifest** `ummp-plugin.toml`:
  `name`, `version`, `kind = "adapter" | "rules"`, `command = ["./bin/plugin"]`,
  `ummp_api = 1`.
- **Protocol:**
  - One JSON request per line on stdin, one JSON response per line on stdout.
  - The methods mirror the `Adapter` trait: `id`, `findings{files}`,
    `edits{files, target}`.
  - Responses are data only. UMMP turns them into an `EditSet` and applies it through the
    engine; plugins never write files (ADR 0005).
- **Isolation:**
  - Run the plugin with `build::process::run` (timeout, process-tree kill,
    `scrubbed_env`).
  - Cap the output size.
  - Validate every path it returns with `resolve_within`.
- **Discovery:** a `plugins/` dir in the user config dir;
  `ummp plugins list|check <dir>`.
- **Docs:** `docs/PLUGIN_PROTOCOL.md` with examples. The SDK docs come in Phase 14.
- **Tests:**
  - ★ A fake plugin (a test binary or a script) returns an edit renaming `DemoWidget` →
    `DemoGadget`. The edit is applied through the engine, then rolled back.
  - ★ A plugin that returns `../../x` is rejected.
  - A hanging plugin is killed at the timeout.
  - A plugin with `ummp_api = 2` is refused with a clear message.

## 5. Docs (same PR; ask the user whether they prefer one PR per milestone)
- **ADRs:** the async runtime and REST framework, the MCP approach, the plugin protocol.
- **New docs:** `docs/CLI.md`, `docs/API.md` (+ the OpenAPI file), `docs/MCP.md`,
  `docs/PLUGIN_PROTOCOL.md`.
- **Updated:** the phase file, STATUS, CHANGELOG, README, the CLAUDE.md layout,
  KNOWN_LIMITATIONS, BACKLOG.

## 6. Risks
- **Async creeping into `core`:** only `server/` may use tokio.
- **Breaking the JSON output:** the golden tests and the `schema` field protect it.
- **Token leaks:** covered by the logging test.

## 7. Backlog candidates
- MCP over HTTP
- WASM plugins
- a webhook when a job completes
- a Gradle plugin wrapper
- a VS Code extension

## 8. Definition of done
README §7, plus:
- The starred tests of each surface are mutation-checked: CLI JSON, REST, MCP, Action,
  plugins.
- The Action has one real run.
- The MCP server has had one real use, if the user agreed.
