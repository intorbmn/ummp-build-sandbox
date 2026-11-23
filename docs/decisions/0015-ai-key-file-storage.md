# ADR 0015 — Interim file-backed AI API key storage

**Status:** Accepted (interim)  
**Date:** 2026-11-20  
**Supersedes:** The AI-provider-key storage portion of ADR 0014 only  
**Related:** ADR 0004, 0005, 0010, 0014

## Context

Phase 11 stores Anthropic and OpenAI-compatible API keys in the operating-system keyring.
The user requested that AI keys be entered directly in the AI workflow and saved automatically
so that the desktop app and CLI can share them. The user explicitly clarified that saving
keys to a file is temporary, not a final storage preference.

## Problem

How can the desktop app and CLI share AI provider keys now, while keeping the storage
implementation replaceable and minimizing accidental disclosure?

The design must preserve the existing GitHub-token keyring behavior, keep credentials out
of settings and all machine-readable output, and avoid coupling callers to a JSON file.

## Options

1. Keep AI keys exclusively in the OS keyring. This is safer at rest, but does not meet the
   requested interim file-backed shared workflow without introducing a second mechanism.
2. Put keys directly in `settings.json` or TOML. Rejected because these files represent
   ordinary preferences/configuration and are likely to be copied, inspected or serialized.
3. Use a dedicated `credentials.json` file behind an `AiKeyStore` trait, with a single
   file-backed implementation for now. This meets the requested interim behavior and
   isolates future storage changes.
4. Add a new permanent encrypted vault or platform-specific credential abstraction now.
   Rejected because the user has not selected a final storage backend.

## Decision

- Add `core/src/app/credentials.rs` with an `AiKeyStore` trait exposing only
  `get`, `set`, `delete` and `masked`. Callers depend on this trait and do not parse
  credentials JSON or construct credential file paths.
- Implement the trait with `FileKeyStore`, backed by `credentials.json` in UMMP's
  shared per-user application data directory. This is the only implementation in Phase 12.0.
- Use the same data directory for CLI and desktop:
  - Windows: `%APPDATA%/io.github.intorbmn.ummp`
  - macOS: `~/Library/Application Support/io.github.intorbmn.ummp`
  - Linux: `$XDG_DATA_HOME/io.github.intorbmn.ummp`, or
    `~/.local/share/io.github.intorbmn.ummp`
  - `UMMP_DATA_DIR` overrides the default for portable use and tests.
- Store a versioned JSON object with schema 1 and separate Anthropic/OpenAI-compatible
  key fields plus the optional OpenAI-compatible base URL. Read-modify-write must preserve
  the other provider's credentials.
- Write through a temporary file in the same directory, flush it, then atomically replace
  the destination. On Unix, restrict the file to mode `0600` before writing secret bytes.
  On Windows, use the private per-user application-data directory and do not broaden ACLs.
- Validate by trimming surrounding whitespace and rejecting empty values, values longer
  than 512 characters, or control characters. An Anthropic key without the `sk-ant-`
  prefix produces a warning but is not rejected.
- Mask stored keys. For normal-length values show the first seven and last four characters
  separated by an ellipsis; for short values show only an ellipsis and the last two characters.
  No status or command returns the full stored key.
- Resolve credentials in this order: `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` environment
  variable, then `AiKeyStore`, then no credential. Environment values are never persisted
  automatically.
- Keep the GitHub token in the existing OS keyring. Remove only AI key names from the
  desktop keyring allow-list.
- Migrate an existing keyring AI key once, only when the corresponding file entry is absent:
  write it to the file first, then delete the keyring entry. If keyring access fails, log a
  sanitized warning and continue startup without exposing the value.
- Desktop AI screen accepts the key directly. Starting AI assist saves the entered key
  before starting the job. After successful save, clear the input state and display only
  the masked value with Change/Delete actions. Settings exposes the same status and controls.
- CLI provides `ummp ai-key set`, `status` and `delete`. `set` reads a hidden terminal
  prompt or one line from stdin; a key is never accepted as a command-line argument.
- UI/CLI language says the key is “saved”; only `ai-key status` reveals the storage path.
  No key may appear in logs, events, settings, JSON, error messages, REST/MCP responses,
  plugin environments or child-process arguments.

## Security and limitations

This is an interim compromise, not equivalent to an OS keyring. A process running as the
same user can read the credentials file. Unix mode 0600 and the private per-user application
data directory reduce exposure but do not encrypt the file. Do not claim otherwise.

Tests use synthetic credentials only. Automated tests must verify that raw keys never enter
settings, job records, events, CLI output (except masked status), UI state after save, or
error messages. Never use a real API key in CI.

## Consequences

- A key entered once is available to both desktop and CLI when both resolve the same data
  directory.
- Storage can be replaced by adding another `AiKeyStore` implementation; callers and UI
  commands need not change.
- Environment credentials remain convenient for CI and automation and override saved keys.
- Existing GitHub-token behavior is unchanged.
- The credentials file is easier to copy or read than an OS keyring; this limitation is
  explicit and temporary.

## Revisit trigger

Revisit this ADR when the user chooses the final AI key storage backend. Record the selected
backend in a new ADR, implement a new `AiKeyStore`, migrate existing credentials safely,
and remove or retain `FileKeyStore` according to the user's decision. The open BACKLOG item
is **Final AI key storage**. Do not choose the permanent backend without the user's approval.

## Alternatives rejected

- Storing keys in `settings.json`, `ummp.toml`, UI local storage or logs.
- Moving the GitHub token to the credentials file without user approval.
- Returning raw keys to the Svelte web view after they have been saved.
- Making the file-backed implementation a permanent commitment.
- Encrypting with a locally stored application secret, which would not provide a meaningful
  boundary against another process running as the same user.
