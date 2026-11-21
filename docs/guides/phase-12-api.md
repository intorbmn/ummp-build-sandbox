# Phase 12 — CLI, API & Integration — guide for Claude Code

This guide lets an agent that has never seen this repository build Phase 12 alone. Read it
once, top to bottom. Then read [README.md](README.md), which covers:
- the workflow and environment quirks;
- GitHub commands (REST only);
- Graphify (§2b) and the skill check (§2a);
- the definition of done.

Where this file and README disagree, this file wins for Phase 12.

Status when this guide was written:
- Phases 1–11 are merged (master after PRs #11 and #10). The next ADR number is **0015**.
- CI has two jobs: the root Rust workspace, and the desktop app (`ui/` +
  `ui/src-tauri`).
- Rough history of this phase: `docs/drafts/phase-12-api.md`.

### Status update (after PR 1 = 12.0 + 12.A, PR #14)
Read this block first. Where it differs from the text below, this block wins.
- **Done, CI green on both jobs at `d1e21fd`:** 12.0 shared AI key store
  (`core/src/app/credentials.rs`, `AiKeyStore` trait, `FileKeyStore`, `default_data_dir()`,
  `UMMP_DATA_DIR`) and 12.A (`cli/src/{args,config,exit,output}.rs`, `docs/CLI.md`,
  golden JSON in `tests/fixtures/golden/cli/`, 315 root tests). PR 1 is not merged yet.
  **Do not start 12.B until the user has merged PR 1**; then branch from `master`.
- **Contracts that 12.B–12.E must reuse, not reinvent:**
  - Exit codes: 0 ok, 1 general, 2 safety refusal, 3 done with manual items, 4 build
    failed, 5 invalid input (`cli/src/exit.rs`). REST maps them to HTTP statuses; write
    the table in the REST ADR (suggested: 2 → 403, 5 → 400, 4 → 200 with a failed job,
    1 → 500).
  - JSON envelope `schema: 1` with a result or an error (`cli/src/output.rs`). REST and MCP
    tool results reuse the same `result` shapes, so one golden file serves all three.
  - Config precedence: CLI flag > environment > `ummp.toml` > default. `allowed_roots`
    (new, for 12.B/12.C) becomes a strict TOML field in `cli/src/config.rs`. Unknown
    fields stay rejected, and key-like fields stay rejected.
  - All writes go through `AppServices` jobs (`start_port`, `start_migrate_in_place` with
    `reviewed`, `start_build`, `start_pack`, `rollback`). The server and MCP never call the
    engine directly, and nothing exposes a stored key.
- **Sandbox quirks seen in PR 1:**
  - tokio and axum are not in the workspace yet. Add them only in `server/`.
  - Some sessions have hooks that block shell commands containing certain words. Write
    text files with the Write tool, not shell heredocs.
  - `cdn.playwright.dev` is blocked, so Playwright in the sandbox needs a preinstalled
    Chromium path; the pinned toolchain cannot be downloaded either
    (`RUSTUP_TOOLCHAIN=stable`).
- **Acceptance items from PR 1 still open** (on the user's machine): the key is shared
  between desktop and CLI, and shell completions work in bash, zsh, fish and PowerShell.
- **New in this update:** §6–§9 list which design choices are fixed and which the design
  doc must decide, and §15 is the end-to-end test to run once **after all of Phase 12**.

---

## 0. Start-of-phase checklist (tick these in the phase file)

1. **Create the branch:**
   ```sh
   git checkout master && git pull && git checkout -b phase-12-api
   ```
2. **Check the environment** (README §2). Everything should be green before you start:
   ```sh
   export RUSTUP_TOOLCHAIN=stable    # only if the pinned toolchain cannot be downloaded
   cargo build --all && cargo test --all
   (cd ui && npm ci && npm run check && npm run test)
   (cd ui/src-tauri && cargo test)   # needs the GTK/WebKit/dbus libs; if missing locally, rely on the CI desktop job
   ```
3. **Read code with Graphify first** (README §2b).
   - Repository id: `intorbmn/Universal-Minecraft-Mod-Porter`.
   - If a result's `commitSha` is not `git rev-parse HEAD`, Read the files changed since
     that commit.
   - After Phase 11 the index may lag behind. If so, read `core/src/app/` and `ui/`
     directly.
4. **Skill check** (README §2a). If a required skill is missing, stop and ask the user,
   using the Vietnamese template in `phase-11-ui.md` §0.2 with the list adapted. Phase 11
   ran without skills, with the user's approval. Do not assume that approval still holds:
   ask again.
   - **Required:**
     - superpowers: brainstorming, writing-plans, executing-plans,
       test-driven-development, systematic-debugging, verification-before-completion
     - andrej-karpathy-skills:karpathy-guidelines
     - rust-cli-skills: rust-errors, rust-idiomatic, rust-testing, rust-design,
       rust-concurrency, rust-cli-interface
     - ecc: architecture-decision-records, rust-review, mcp-server-patterns, api-design
     - example-skills:mcp-builder
     - claude-security:claude-security (the agent), for reviewing the network surface
   - **Recommended:**
     - rust-cli-skills:rust-filesystem (for the credentials file)
     - ecc:rust-build
     - pr-review-toolkit:review-pr
     - commit-commands:commit-push-pr
5. **Read, in this order:**
   1. `docs/PROJECT_STATUS.md`, `docs/KNOWN_LIMITATIONS.md`, and the Phase 11 section of
      `docs/BACKLOG.md`.
   2. `docs/phase/phase-11-ui.md`, especially the open acceptance items.
   3. ADR 0004 (app services boundary), 0005 (AI write gate), 0011 (build risk gate),
      0014 (desktop boundary + keyring).
   4. Code:
      - `core/src/app/{mod.rs, runs.rs, approval.rs, jobs.rs, views.rs, helpers.rs}`
      - `ui/src-tauri/src/{commands.rs, secrets.rs, lib.rs}`
      - `cli/src/main.rs`
      - `providers/src/lib.rs`
6. **Ask the §3 questions** (one message in Vietnamese, with defaults), then write the
   design doc (§10, step D1).

---

## 1. Goal and scope

### 1.1 Goal (master prompt Phase 12, plus the user's request of 2026-11-20)
1. **AI key: typed in directly and saved to a file automatically.** This is milestone
   12.0 and comes first. The user's words: "phần AI cần thay đổi thành nhập trực tiếp API
   key và tự lưu vào file".
2. **CLI polish:** stable exit codes, `--json` output, shell completions, a config file.
3. **Local REST API** (`ummp serve`) for scripts and other tools.
4. **MCP server** (`ummp mcp`), so AI assistants (Claude Code, Claude Desktop) can
   analyze, plan and port through UMMP's safe paths.
5. **GitHub Action** that analyzes, scans or ports a mod in another repository's CI.
6. **Plugin interface** that runs third-party adapters and rules out of process.

### 1.2 Hard rules
- **All surfaces call `AppServices`** (`core/src/app/`, ADR 0004): CLI, desktop, REST,
  MCP, Action. No new business logic goes in `cli/`, `server/`, `ui/` or the MCP code.
- **File writes** go through `MigrationEngine` after a checkpoint, or into a new output
  folder (ADR 0005/0006).
- **Untrusted input:**
  - Every path from a request goes through the allowed-roots check and `resolve_within`.
  - Plugins are untrusted executables.
- **Secrets** (AI keys, GitHub token, REST token):
  - Never put them in logs, events, JSON output, REST/MCP responses, `settings.json` or
    the web view.
  - Show them masked, like `sk-ant-…abcd`.
- **No async in `core`.** Only the new `server/` crate may use tokio.

### 1.3 Non-goals (requests for these go to BACKLOG)
- A hosted or multi-user service, or remote build execution through the API.
- MCP over HTTP.
- WASM plugins.
- Signed releases (Phase 13).

---

## 2. What the code gives you today (checked on master after PR #11)

| Area | Current state | File |
|---|---|---|
| App services | `AppServices::{open, settings, save_settings, analyze, scan, targets, plan, start_port, start_migrate_in_place, rollback, check_build_risk, start_build, start_build_with_token, start_pack, start_ai_assist(req, provider, config), decide(approval, bool), job, sessions(limit), cancel, subscribe}`; `Secret::{new, expose}` (Debug output is masked); `Settings::new(data_dir)` | `core/src/app/*.rs` |
| Desktop secrets | Trait `SecretStore { set, get, delete }`. `OsKeyring` uses keyring-rs with service `ummp`. Allowed names: `anthropic_api_key`, `openai_api_key`, `github_token`. Commands: `set_secret`, `has_secret`, `delete_secret` (no get) | `ui/src-tauri/src/secrets.rs`, `commands.rs` |
| Desktop AI start | `commands::start_ai_assist` reads `anthropic_api_key` / `openai_api_key` from the `SecretStore`, builds `AnthropicProvider::with_api_key(key, url)` or the OpenAI-compatible provider, then calls `AppServices::start_ai_assist`. With no key, it replies "Save an Anthropic API key in Settings…" | `ui/src-tauri/src/commands.rs` |
| Desktop data dir | `app.path().app_data_dir()` (Tauri; identifier `io.github.intorbmn.ummp`) | `ui/src-tauri/src/lib.rs` |
| CLI AI | `provider(args)` uses `AnthropicProvider::from_env()` (reads `ANTHROPIC_API_KEY`), or `OpenAiCompatibleProvider::new(url)` with `--base-url` | `cli/src/main.rs` |
| CLI | About 1000 lines of clap derive. It already imports `ummp_core::app::helpers` and `app::snapshots`, but most commands still call the engines directly | `cli/src/main.rs` |
| Exit codes | Only `pack migrate` defines them: 0, 2 (refused for risk), 3 (manual items) | `core/src/modpack/run.rs` |

---

## 3. Questions for the user (one message in Vietnamese, with defaults)

1. **Where does the AI key file live, and in what format?**
   - Default: `credentials.json` in the UMMP data folder. On Windows that is
     `%APPDATA%\io.github.intorbmn.ummp\`, the folder the desktop app already uses.
   - The CLI and the desktop app share the file, so a key typed once works in both.
2. **Should the GitHub token move into the file too?**
   - Default: **no**. It stays in the OS keyring, because the user's request only covers
     the AI part. If they say yes, use the same mechanism for `github_token`.
3. **REST framework?**
   - Default: **axum + tokio** in a new `server/` crate. It is the first async runtime in
     the project, so it gets an ADR.
   - Alternative: `tiny_http`, which is sync and small but has no SSE streaming.
4. **Who may call the REST API?**
   - Default: localhost only. A random bearer token is written to `<data_dir>/server-token`
     and printed once.
5. **MCP SDK?**
   - Default: the official Rust SDK `rmcp` over stdio, if its current version fits (check
     docs.rs).
   - Otherwise: a small hand-written JSON-RPC loop over stdio.
6. **Should MCP get AI tools?**
   - Default: **no**. MCP gets analyze, plan, port, scan, build, job and rollback only.
   - Reason: the calling assistant is already an AI. Running UMMP's AI on top would make
     the user pay twice.
7. **Plugin model?**
   - Default: an out-of-process executable that exchanges JSON lines over stdin/stdout,
     described by a manifest `ummp-plugin.toml`.
8. **One PR or several?**
   - Default: three PRs.
     - PR 1: 12.0 + 12.A (key file + CLI polish)
     - PR 2: 12.B + 12.C (REST + MCP)
     - PR 3: 12.D + 12.E (Action + plugins)
   - Each one is reviewed and merged before the next starts.

If the user says "cứ làm" ("just do it"), use the defaults and record them in the design
doc as your calls.

---

## 4. Milestone 12.0 — type the AI API key in directly and save it to a file (do this first)

> **Interim storage.** The user said on 2026-11-20: "lưu vào file chỉ là tạm thời" (saving
> the key to a file is only temporary). Build 12.0 so that the storage backend can be
> replaced later without touching callers:
> - Put the file behind a small trait in `core/src/app/credentials.rs`:
>   `trait AiKeyStore { get, set, delete, masked }`, with `FileKeyStore` as the only
>   implementation for now.
> - Every caller (CLI, desktop, `resolve_ai_key`) uses the trait, never the file path or
>   the JSON directly.
> - The UI and CLI wording says "saved" and must not promise "saved in a file". Only
>   `ai-key status` shows the file location.
> - ADR 0015's status is **"Accepted (interim)"**. Record the trigger to revisit it: the
>   user picks the final storage. The final choice is a BACKLOG item ("final AI key
>   storage"). Do not choose it yourself.

### 4.1 What the user sees

**Desktop, AI screen**
- No key saved for the chosen provider: the screen shows an **API key field right
  there**. There is no need to go to Settings first.
  - The field is `type=password`, with a "show" toggle.
  - The label names the provider:
    - Anthropic: "Anthropic API key (sk-ant-…)"
    - OpenAI-compatible: "API key + base URL"
- Pressing **Start** **saves the key to the file automatically**, then starts the job.
  There is no separate Save step.
- Once a key is saved, the field becomes a masked line, `sk-ant-…abcd`, with **Change**
  and **Delete** buttons.

**Desktop, Settings**
- The same masked line, with Change and Delete, for each provider.
- The full key is never shown again.

**CLI**
- `ummp ai-key set [--provider anthropic|openai]` saves the key to the same file:
  - In a terminal it asks for the key with the input hidden.
  - When stdin is not a terminal, it reads one line from stdin.
  - The key is **never accepted as a command-line argument**: shell history and process
    lists would leak it.
- `ummp ai-key status` shows the provider, the masked key and the file path.
- `ummp ai-key delete [--provider …]` removes it.
- Once the file exists, `ummp ai-assist` and `ai-debug` need no environment variable.
- When no key is found, the error says what to do: run `ummp ai-key set`, or type the key
  in the app.

**Key precedence (CLI and desktop)**
1. The environment variable (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`), so CI and power
   users keep working.
2. `credentials.json`.
3. Nothing: show the error above.

**One-time move from the keyring**
- On the first start after the upgrade, if the OS keyring still holds
  `anthropic_api_key` or `openai_api_key` and the file has no key for that provider:
  1. copy the key into the file;
  2. delete it from the keyring;
  3. log one line without the key, for example "moved Anthropic key from keyring to
     credentials file".
- If keyring access fails, log it and continue. Never block the app.

### 4.2 Design

**New module `core/src/app/credentials.rs`.** It sits in `core` so the CLI, the desktop
app and, later, `server/` all share it.

```rust
pub const CREDENTIALS_FILE: &str = "credentials.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiProviderKind { Anthropic, Openai }

pub struct CredentialsFile { path: PathBuf }   // <data_dir>/credentials.json

impl CredentialsFile {
    pub fn in_dir(data_dir: &Path) -> Self;
    pub fn path(&self) -> &Path;
    pub fn get(&self, p: AiProviderKind) -> Result<Option<Secret>, AppError>;
    pub fn set(&self, p: AiProviderKind, key: &Secret) -> Result<(), AppError>;   // atomic write, validated
    pub fn delete(&self, p: AiProviderKind) -> Result<(), AppError>;              // idempotent
    pub fn masked(&self, p: AiProviderKind) -> Result<Option<String>, AppError>;  // "sk-ant-…abcd"
}

/// Environment variable first, then the file. Used by the CLI and the desktop app.
pub fn resolve_ai_key(data_dir: &Path, p: AiProviderKind) -> Result<Option<Secret>, AppError>;

/// The folder the desktop app uses, computed without Tauri (for the CLI):
/// Windows %APPDATA%\io.github.intorbmn.ummp; macOS ~/Library/Application Support/io.github.intorbmn.ummp;
/// Linux $XDG_DATA_HOME/io.github.intorbmn.ummp or ~/.local/share/io.github.intorbmn.ummp.
/// UMMP_DATA_DIR overrides it (tests and portable use).
pub fn default_data_dir() -> Result<PathBuf, AppError>;
```

**File format.** The `schema` field allows later changes:
```json
{ "schema": 1, "anthropic_api_key": "sk-ant-...", "openai_api_key": "...", "openai_base_url": "http://localhost:11434/v1" }
```

**Writing the file** (follow `rust-cli-skills:rust-filesystem`):
- Write to a temp file in the same folder, flush it, then rename it over the old file.
- On Unix, create the temp file with mode `0600` **before** writing the key into it.
- On Windows, the file lives under the user's `%APPDATA%`, which is private to the user by
  default. Do not widen its permissions. Write in KNOWN_LIMITATIONS that any process
  running as the same user can read it.
- Keep the keys the user did not change: read the file, change one entry, write it back.

**Validation in `set`:**
- Trim surrounding whitespace.
- Reject a key that is empty, longer than 512 characters, or contains control characters.
- An Anthropic key that does not start with `sk-ant-` gets a **warning** that the UI
  shows. Do not reject it: proxies and other key formats exist.

**Masking:** the first 7 characters + `…` + the last 4. For keys shorter than 16
characters, show only `…` + the last 2.

**Desktop changes (`ui/src-tauri`):**
- AI keys stop using `OsKeyring`. Remove `anthropic_api_key` and `openai_api_key` from
  `SECRET_NAMES`. Keep `github_token` there unless the user answered otherwise in §3.
- New commands:
  - `ai_key_status(provider) -> { saved: bool, masked: Option<String>, path: String }`
  - `ai_key_save(provider, key, base_url?) -> { masked, warning? }`
  - `ai_key_delete(provider)`
- No command returns the full key.
- `start_ai_assist` calls `resolve_ai_key(data_dir, provider)`.
- The one-time keyring → file move runs in the setup of `lib.rs`. It goes through the
  `SecretStore` trait, so tests can drive it with `MemorySecretStore`.

**UI changes (`ui/src`):**
- The AI screen and Settings call the new commands through `lib/api.ts`.
- Add the i18n strings to both `vi.json` and `en.json`.
- The key input is cleared from component state as soon as the save call returns.

**CLI changes:**
- Add the `ai-key` subcommand.
- `provider(args)` uses `resolve_ai_key(default_data_dir()?, …)`.
- When `--base-url` is not given, use `openai_base_url` from the file.

**ADR 0015 "AI API keys in a user credentials file" — status: Accepted (interim):**
- **Context:** the user asked to type the key in directly and to keep it in a file, as a
  temporary solution.
- **Decision:** the design in this section (§4).
- **Consequences:**
  - Weaker than the OS keyring: any process running as the user can read the file.
  - Mitigations: file permissions, never in the repo or logs, masked display, the env var
    override.
  - One key now works for both the CLI and the desktop app.
- **Supersedes** the AI-key part of ADR 0014. The GitHub token is unchanged unless §3 said
  otherwise. Mark ADR 0014 "partly superseded by 0015".

### 4.3 Tests

★ = mutation-check it (break the code on purpose, see the test go red, restore) and record
how in the phase file.

All tests use a temp data dir through `UMMP_DATA_DIR`. Never touch the real home folder.
Use clearly fake keys such as `sk-ant-test-0000000000000000`.

**Core** (`core/tests/app.rs`, or a new `core/tests/credentials.rs`):
- ★ `credentials_round_trip_and_keep_other_provider`
- ★ `env_var_overrides_file`. Do not use `set_var` where it needs `unsafe`. Give
  `resolve_ai_key` an inner function that takes the env value as a parameter, and test
  that.
- ★ `key_never_in_settings_json_events_or_job_log`. Run a fake AI job with a file key. Then
  search `settings.json`, `jobs.db` (read as text) and every captured event for the key;
  none may contain it.
- `masked_shows_prefix_and_last_four_only`
- `set_rejects_empty_control_chars_and_too_long`
- `anthropic_prefix_warning_does_not_block`
- `unix_file_mode_is_0600` (`#[cfg(unix)]`)
- `write_is_atomic_old_file_survives_failed_write`: make the rename fail by pointing at a
  read-only folder (`#[cfg(unix)]`).
- `corrupt_file_gives_clear_error_not_panic`
- `delete_is_idempotent`
- `default_data_dir_honours_UMMP_DATA_DIR`

**Desktop crate** (`ui/src-tauri`, with `MemorySecretStore`):
- ★ `keyring_ai_key_is_moved_to_file_once`
- `no_command_returns_full_key`
- `start_ai_assist_without_key_returns_refused_with_entry_hint`

**Front end** (Vitest):
- `AI screen shows key field when no key saved`
- `Start saves the key then starts the job` (check the call order)
- `masked key shown after save; input cleared`

**CLI** (`cli/tests/ai_key_cli.rs`, runs the built binary with `UMMP_DATA_DIR` set to a
temp dir):
- `set_reads_stdin_and_status_shows_masked`
- `key_argument_is_not_accepted`: clap rejects `ummp ai-key set sk-…`.
- `ai_assist_uses_file_key`. Use the existing fake-provider path, or point the base URL at
  an unreachable address and check that the error changes from "no key" to a
  provider/network error.

### 4.4 Real-world check
1. The user types their real key once in the desktop app.
2. The user runs `ummp ai-key status` in a terminal. It must show the same masked key.
   This proves the file is shared without making any AI request.
3. Only if the user agrees: run one small `ai-assist` on `demo-fabric-mod`. This also
   closes the Phase 11 open item "AI not tested by hand".

---

## 5. Milestone 12.A — CLI polish

**Move the CLI onto `AppServices`:**
- Every command that has a service calls it: analyze, scan, plan/migrate, port, pack,
  build, rollback.
- Only CLI-specific work stays in `cli/`: argument parsing, terminal prompts, printing.
- Split `cli/src/main.rs` into `cli/src/{main.rs, args.rs, exit.rs, output.rs}`; it should
  get shorter.

**Exit codes** (`cli/src/exit.rs`), documented in `--help` and in `docs/CLI.md`:

| Code | Meaning |
|---|---|
| 0 | ok |
| 1 | unexpected error |
| 2 | refused (risk gate, missing review, missing key) |
| 3 | completed, with items left to the user |
| 4 | build failed |
| 5 | invalid input (path, version, loader) |

`pack migrate` keeps its existing 0/2/3.

**`--json` (global flag):**
- Print the result as JSON on stdout: the same `*View` structs as the desktop app, with
  `"schema": 1` at the top level.
- Human-readable logs go to stderr.
- Errors print as `{"schema":1,"error":{"kind":"…","message":"…"}}`.

**`ummp completions <bash|zsh|fish|powershell>`:** uses `clap_complete`, a new crate. Name
it in the design doc.

**`ummp.toml` config file:**
- Search order: `--config PATH`, then `./ummp.toml`, then `<data_dir>/ummp.toml`.
- Fields: `output_root`, `kb`, `sandbox_repo`, `ai_provider`, `openai_base_url`.
- **No keys.** If the file contains a field that looks like a key, refuse it and point to
  `ummp ai-key set`.
- Precedence: flag > `UMMP_*` env > file > built-in default.

**Tests** (`cli/tests/`):
- ★ `json_output_is_valid_and_keys_stable`: compare the key sets with golden files in
  `tests/fixtures/golden/cli-json/`.
- ★ `exit_codes_match_table`: one case for each code.
- `json_error_shape`
- `config_file_is_overridden_by_flag_and_env`
- `config_with_api_key_field_is_refused`
- `completions_generate_for_each_shell`
- All existing CLI tests still pass. Any output change must be intended and listed in
  CHANGELOG.

---

## 6. Milestone 12.B — local REST API (`ummp serve`)

**Crate:** a new workspace member `server/` (crate `ummp-server`), using axum + tokio. It
calls `AppServices` through `tokio::task::spawn_blocking`. The CLI gets a `serve`
subcommand that starts it.

**Routes**, versioned under `/v1`. Bodies are the `*View` structs:
```
GET  /v1/health                                   (no auth)
POST /v1/analyze      {path}                      -> AnalysisView
POST /v1/scan         {path}                      -> RiskView
POST /v1/plan         {path, to, loader, ...}     -> PlanView (never writes)
POST /v1/port         {...PortRequest}            -> {job}
POST /v1/migrate      {..., reviewed: true}       -> {job}   (refused without reviewed)
POST /v1/pack         {...PackRequest}            -> {job}
POST /v1/build        {...BuildJobRequest, accept_risk} -> {job}
GET  /v1/jobs/{id}                                -> JobView
GET  /v1/jobs/{id}/events          (SSE)          -> same event JSON as the desktop
POST /v1/jobs/{id}/cancel
POST /v1/sessions/{project}/rollback {checkpoint?}
GET  /v1/openapi.json
```
No AI routes in this phase; record them in BACKLOG.

**Security:**
- **Binding:** listen on `127.0.0.1:<port>`. `--listen 0.0.0.0` also requires
  `--i-understand-remote`.
- **Auth:** bearer token stored in `<data_dir>/server-token`, created with mode 0600 on
  Unix and compared in constant time. The token never appears in logs.
- **Body size:** limit request bodies to 1 MiB.
- **Paths:** every path must be inside `allowed_roots` from `ummp.toml`; the default is the
  user's home folder only.
  - Canonicalize the path, then check that it starts with one of the canonical roots.
  - Use `resolve_within` wherever paths are joined.
- **CORS:** off.

**OpenAPI:** either generate it with `utoipa`, or commit `docs/openapi.json` plus a test
that compares its paths with the router. Pick one in the design doc.

**Tests** (`server/tests/api.rs`: real server on port 0, `ureq` client, temp data dir):
- ★ `requires_token`
- ★ `rejects_path_outside_allowed_roots`
- ★ `plan_does_not_write`
- `migrate_without_reviewed_is_refused`
- `port_job_completes_and_sse_events_in_order`
- `cancel_works`
- `body_too_large_is_413`
- `openapi_lists_every_route`
- `token_never_logged` (capture the tracing output)

**Before the PR:** run a focused `claude-security:claude-security` review of `server/` and
record its findings in the phase file.

---

## 7. Milestone 12.C — MCP server (`ummp mcp`)

**Transport and placement:**
- stdio, started by `ummp mcp` from the CLI.
- The code goes either in the `server/` crate (module `mcp`) or in a new `mcp/` crate;
  decide in the design doc.
- Follow `example-skills:mcp-builder` and `ecc:mcp-server-patterns`.

**Tools.** Each tool maps to `AppServices`. Paths go through the same allowed-roots check
as REST.

| Tool | Writes? | Notes |
|---|---|---|
| `ummp_analyze(path)` | no | |
| `ummp_scan(path)` | no | |
| `ummp_plan(path, to, loader)` | no | returns a diff summary plus the first N diff lines per file |
| `ummp_port(path, to, loader, output?, approve)` | only with `approve: true` | `approve:false` returns the plan only; the assistant must ask the human and then call again |
| `ummp_build(path, backend)` | runs code | the local backend is refused unless the risk is Low; GitHub is preferred |
| `ummp_job(id)` / `ummp_cancel(id)` | no | |
| `ummp_rollback(project, checkpoint?)` | restores | requires `confirm: true` |

**Resource:** `ummp://report/{job}` returns the markdown report.

**Safety rules:**
- Each tool description says plainly whether it writes.
- No tool runs shell commands.
- No tool reads or returns keys.

**Tests:**
- ★ A JSON-RPC exchange over pipes, in this order:
  1. `initialize`
  2. `tools/list` shows every tool with its schema
  3. `ummp_plan`
  4. `ummp_port` with `approve:false` writes nothing
  5. `ummp_port` with `approve:true` writes the output
- `rejects_path_outside_roots`
- `rollback_requires_confirm`
- `no_tool_output_contains_key`: save a key in the file, call every tool, and search the
  outputs for the key.

**Real-world check** (only with the user's consent):
1. On the user's machine, run `claude mcp add ummp -- ummp mcp`.
2. Ask Claude Code to port `demo-fabric-mod` through it.
3. Record a summary of the transcript.

---

## 8. Milestone 12.D — GitHub Action

**`action.yml`** at the repo root, as a composite action:
- Inputs: `command` (analyze|scan|port|build), `path`, `to`, `loader`, `fail-on-risk`
  (default `high`).
- Outputs: `exit-code`, `report-path`.
- Steps:
  1. install the Rust toolchain;
  2. `cargo install --path cli --locked`, with a cache (release binaries replace this in
     Phase 13);
  3. `ummp <command> … --json`;
  4. write the summary to `$GITHUB_STEP_SUMMARY`;
  5. upload the report as an artifact.

**Tests:**
- A golden test checking that `action.yml` declares the inputs and outputs above.
- **A real run in the sandbox repo.** GitHub can only dispatch workflows that exist on the
  default branch (README §5), so do this instead:
  1. Push a test branch to `intorbmn/ummp-build-sandbox` with a workflow that runs on
     `push`.
  2. That workflow uses `intorbmn/Universal-Minecraft-Mod-Porter@phase-12-api`.
  3. Record the run id.

  This is the same approach used for the Phase 11 installer build (run 44718403971).

---

## 9. Milestone 12.E — plugin interface (out of process)

**Manifest `ummp-plugin.toml`:**
- `name`, `version`
- `kind = "adapter" | "rules"`
- `command = ["bin/plugin"]` (relative to the plugin folder)
- `ummp_api = 1`

**Protocol:** one JSON request per line on stdin, one JSON response per line on stdout.

| Method | Returns |
|---|---|
| `hello` | `{id, ummp_api}` |
| `findings {files}` | `[TechFinding]` |
| `edits {files, target}` | `[{path, original, updated}]` |

**Plugins never write files.** UMMP turns their edits into an `EditSet` and applies it
through the engine (ADR 0005).

**Isolation:**
- Run the plugin with `build::process::run`: 30 s timeout per call, process-tree kill,
  and `scrubbed_env` (no keys, no tokens).
- Cap the plugin's stdout at 8 MiB.
- Pass every path the plugin returns through `resolve_within`.
- Refuse any `.jar` or `.class` path.

**Discovery:** plugins live in `<data_dir>/plugins/`. Commands: `ummp plugins list`,
`ummp plugins check <dir>`.

**Docs:** `docs/PLUGIN_PROTOCOL.md`, with a complete example exchange.

**Tests:**
- ★ A fake plugin (a small Rust binary in `cli/tests/fixtures/`, or one built by the test)
  renames `DemoWidget` → `DemoGadget`. The edit is applied through the engine, then rolled
  back.
- ★ A plugin that returns `../../x` is rejected.
- `hanging_plugin_killed_at_timeout`
- `plugin_env_has_no_keys`: the plugin echoes its environment; check that it holds no key
  names and no key values.
- `ummp_api_2_refused_with_message`

---

## 9a. Decisions for the 12.B–12.E design doc

Write these in a short addendum to `docs/superpowers/specs/2026-11-20-phase12-api-design.md`
before PR 2, and get the user's approval. Fixed means do not reopen it.

| Topic | Fixed | Decide (and record in the ADR) |
|---|---|---|
| Async | axum + tokio live only in `server/`; `core` and `cli` logic stay sync; a test greps `core/Cargo.toml` for tokio | whether `ummp serve` is in `cli` behind a cargo feature or in a separate `ummp-server` binary (the first keeps one install) |
| REST auth | `127.0.0.1` bind, bearer token file mode 0600, constant-time compare, 1 MiB body limit, CORS off | token rotation command (`ummp serve --rotate-token`) yes/no |
| Paths | `allowed_roots` from `ummp.toml`, canonicalize then prefix-check, `resolve_within` for joins | default roots: home only (keep), and how Windows drive roots and symlinks are compared |
| Jobs and SSE | same job and event JSON as the desktop; events ordered per job | how long finished jobs stay queryable (suggest: as long as `jobs.db` keeps them) |
| OpenAPI | one source of truth | `utoipa` generation, or committed `docs/openapi.json` plus a router-paths test (pick the smaller one) |
| MCP | stdio only; write tools need `approve` or `confirm`; no tool returns a key | crate placement: `server/` module `mcp` (default, shares the path check) or a new `mcp/` crate; hand-written JSON-RPC or an SDK (check licence and size first) |
| Action | composite action, builds from source until Phase 13 | cache key and toolchain pin strategy |
| Plugins | out of process, JSON lines, no file writes, `build::process::run` with timeout and scrubbed env, 8 MiB stdout cap, `resolve_within` on returned paths, no `.jar`/`.class` | plugin discovery folder layout and what `plugins check` prints |

ADR numbers: 0016 REST + async runtime, 0017 MCP, 0018 plugin protocol (next free number
after the merge of PR 1 is 0016).

---

## 10. Step-by-step order
1. **D1:**
   - skill check;
   - the §3 questions;
   - the design doc `docs/superpowers/specs/YYYY-MM-DD-phase12-api-design.md`, covering
     §1–§9 and your calls.
2. **D2:** the user approves the design.
   - Write ADR 0015 (keys) now.
   - The others come later: 0016 REST + async runtime, 0017 MCP, 0018 plugin protocol.
3. **P1:** the plan `docs/superpowers/plans/YYYY-MM-DD-phase12-api.md`.
   - One task per milestone. 12.0 may need three tasks: the core file module; desktop +
     UI; CLI.
   - Propose Native execution.
4. **B1:** 12.0, then 12.A.
   1. Run the four root commands and the desktop checks.
   2. Update the docs (§11) for this part.
   3. Open PR 1 via REST and send the short report.
   4. Wait for the merge.
5. **B2:** 12.B + 12.C as PR 2. **B3:** 12.D + 12.E as PR 3. Same rules as B1.
6. After PR 3 is merged: run the final acceptance test in **§15**, record the results in
   the phase file, and write the completion report. **Do not start Phase 13.**

## 11. Docs to update (in each PR, for its part)
- **ADRs:** 0015–0018. Also update `docs/decisions/README.md` and the next ADR number in
  CLAUDE.md.
- **Phase file** `docs/phase/phase-12-api.md`, in the same layout as `phase-11-ui.md`:
  milestones, evidence, mutation checks, real runs, completion report.
- **New docs:**
  - `docs/CLI.md`: commands, exit codes, `--json`, config, `ai-key`
  - `docs/API.md` and the OpenAPI file
  - `docs/MCP.md`
  - `docs/PLUGIN_PROTOCOL.md`
- **Updates:**
  - `PROJECT_STATUS.md`, `CHANGELOG.md`
  - `KNOWN_LIMITATIONS.md`: the credentials file is readable by any process of the same
    user; no AI over REST/MCP; the limits of plugin isolation.
  - `docs/BACKLOG.md`
  - `README.md`: AI key setup (the field in the app, or `ummp ai-key set`), `ummp serve`,
    `ummp mcp`, the Action.
  - `CLAUDE.md`: layout rows for `server/` (and `mcp/` if it is a separate crate),
    `core/src/app/credentials.rs`, and the new test files.
  - `docs/SAD.md`.

## 12. Risks
| Risk | Handling |
|---|---|
| A plain file is easier to steal from than the keyring | It is the user's explicit choice, recorded in ADR 0015. Mitigations: mode 0600 on Unix, user-private `%APPDATA%`, masked display, env override, never logged. Add a KNOWN_LIMITATIONS entry |
| The key leaks through logs, events, JSON or MCP | The ★ leak tests in 12.0 and 12.C; `Secret` masks itself in Debug output |
| The CLI and the desktop app use different data folders | `default_data_dir()` + `UMMP_DATA_DIR`. A test in the desktop crate compares `app_data_dir()` with `default_data_dir()` on the CI platform |
| Async creeps into `core` | Only `server/` depends on tokio. A test searches `core/Cargo.toml` for tokio |
| The CLI refactor changes behavior | The existing CLI tests stay unchanged, and golden JSON pins the output |
| A network surface exposes the file system | Allowed roots, localhost bind, token, security review |

## 13. Backlog candidates
- AI routes for REST and MCP
- MCP over HTTP
- WASM plugins
- webhooks when a job completes
- a Gradle plugin wrapper
- a VS Code extension
- a "test key" button that makes a tiny API call

## 14. Definition of done (Phase 12)
README §7, plus:
- [ ] **12.0:**
  - The AI key is typed in directly (desktop AI screen, Settings, `ummp ai-key set`) and
    saved to `credentials.json`.
  - The CLI and the desktop app share the key; the env var takes precedence.
  - The keyring migration works and the leak tests pass.
  - ADR 0015 is written.
  - The user confirmed on their machine that the key is shared.
- [ ] 12.A–12.E are done, and every ★ test is mutation-checked and recorded.
- [ ] Both CI jobs are green on each PR. The Action has one real run. MCP has been used
      for real once, if the user agreed.
- [ ] The §11 docs are updated.
- [ ] A short Vietnamese report after each PR. Nothing is merged without the user.
- [ ] The §15 final acceptance test has been run on merged `master` and recorded.

---

## 15. Final acceptance test (run once, after all of Phase 12 is merged)

Run it on a fresh clone of `master`, with a fresh data folder (`UMMP_DATA_DIR=<temp>`), and
the synthetic fixtures only (`DemoWidget`). Record every result, with the command and a
short output excerpt, in `docs/phase/phase-12-api.md` under "Final acceptance". A line
without evidence is not a pass. Anything that cannot run in the sandbox (a real desktop, a
real shell, a real MCP client) is listed for the user with exact steps; it is never marked
passed on a guess.

### 15.1 Automatic (sandbox)
1. The four root commands and the desktop checks (`npm ci`, `npm run check`, `npx vitest
   run`, Playwright, `cargo build/test/clippy/fmt` in `ui/src-tauri`). Record the test counts.
2. Both CI jobs green on the merge commit.
3. Every ★ test of 12.0–12.E has a recorded mutation check (break it, see RED, restore).
   List them in a table: test, mutation, result.
4. Dependency boundary: `core/Cargo.toml` and `cli/Cargo.toml` contain no tokio or axum
   (unless the design doc chose the cargo-feature placement; then the default build must
   still have none).
5. Key-leak sweep: save a recognisable fake key, run **every** CLI command with `--json`,
   every REST route, every MCP tool and a plugin call, then search all outputs, logs, job
   files and `jobs.db` for the key text. Expected: not found, apart from `credentials.json`.

### 15.2 End-to-end flows (sandbox, real processes)
| # | Flow | Expected |
|---|---|---|
| 1 | CLI: `ummp --json analyze`, `scan`, `migrate` (plan), `port`, `rollback` on a fixture | envelope `schema: 1`; exit codes 0/2/3/5 as documented; rollback restores the tree byte for byte |
| 2 | Config precedence: the same setting by flag, env and `ummp.toml` | flag beats env beats file; a key in the TOML file is rejected |
| 3 | REST: start `ummp serve` on port 0; call every route with and without the token | 401 without; the SSE events of a port job match the desktop golden events; `migrate` without `reviewed` is refused; a path outside `allowed_roots` is rejected; a body over 1 MiB gives 413 |
| 4 | REST non-loopback: `--listen 0.0.0.0` without `--i-understand-remote` | refused |
| 5 | MCP: JSON-RPC over pipes: `initialize`, `tools/list`, `ummp_plan`, `ummp_port approve:false` (writes nothing), `ummp_port approve:true` (writes), `ummp_rollback` without and with `confirm` | as in §7 |
| 6 | Plugin: the fake plugin renames `DemoWidget` to `DemoGadget` through the engine; then a plugin returning `../../x`; then a hanging plugin; then `ummp_api = 2` | applied and rolled back; rejected; killed at 30 s; refused with a clear message |
| 7 | Same job through the three surfaces (CLI, REST, MCP) on the same fixture | identical `result` JSON (ignoring ids and timestamps) |
| 8 | Parallel use: the desktop-style job and a REST job on one data folder | no database lock errors; both finish |

### 15.3 Needs the user's machine (give the exact steps, wait for the answer)
1. **Key sharing:** save a key in the desktop app, then run `ummp ai-key status` (masked);
   delete it with the CLI and check that the desktop shows "not set"; repeat in the other
   direction. Also check that the environment variable wins.
2. **Completions:** install and try them in real bash, zsh, fish and PowerShell.
3. **Real MCP client:** `claude mcp add ummp -- ummp mcp`, ask Claude Code to port
   `demo-fabric-mod`, and keep a summary of the transcript (only with the user's consent).
4. **GitHub Action:** one real run from `intorbmn/ummp-build-sandbox` on a `push`
   workflow; record the run id and the step summary.
5. **Windows:** `ummp --json analyze` and `ummp serve` on Windows (paths with drive
   letters, `allowed_roots`, the token file); the desktop installer still starts.

### 15.4 Report
End with a table of every §14 box and every §15 line: PASS / FAIL / NOT RUN (with who must
run it). Fix failures in a new small PR (one fix, one test that failed first). Send the user
a very short Vietnamese report. Do not merge, and do not start Phase 13.
