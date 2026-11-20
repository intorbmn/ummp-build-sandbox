# ADR 0014 — Desktop application boundary, credential storage, and workflow UI

**Status:** Accepted  
**Date:** 2026-11-20  
**Supersedes:** None  
**Related:** ADR 0004, 0005, 0006, 0010, 0011, 0013

## Context

Phase 11 turns the existing application services into a desktop workflow. The desktop must
not fork migration, AI, pack, or build logic into the web view. It also handles API credentials
and local build execution, so the boundary between UI state, persistent settings, and secrets
must be explicit.

## Problem

A desktop shell needs to:
- expose analysis, risk scan, planning, diff review, jobs, cancellation, reports, rollback,
  build, AI assistance, and pack migration;
- stream progress without blocking the window thread;
- store provider keys and GitHub credentials without serializing them into settings or events;
- keep AI and in-place migration behind explicit human approval;
- support English and Vietnamese and remain responsive with large logs and pack reports.

## Options

1. Put migration and orchestration logic in Svelte components.
2. Add a parallel desktop-only service layer with duplicated logic.
3. Keep `ummp_core::app::AppServices` as the single application boundary and make Tauri
   commands thin adapters.
4. Store credentials in `settings.json`, environment variables, or the OS credential store.

## Decision

- Tauri 2 + Svelte 5 + TypeScript + Vite is the desktop shell (ADR 0004).
- `ui/src-tauri/src/commands.rs` adapts Tauri commands to `AppServices`; the frontend's
  only `invoke` boundary is `ui/src/lib/api.ts`.
- Long-running scans, planning, build setup, AI setup, and rollback work use Tauri's
  blocking worker pool. Jobs emit the existing core event envelope through
  `ummp://event`; job state remains persisted by the core.
- The OS keyring is the only persistent secret store. The desktop uses keyring-rs with
  Windows Credential Manager, macOS Keychain, and Linux Secret Service backends. Settings
  contain only non-secret preferences. The UI has set/delete/has operations but no
  get-secret command.
- A GitHub token is passed to the GitHub CLI and its git credential helper through the
  `GH_TOKEN` environment variable for the duration of child processes. It is never added
  to command arguments, logs, job results, or settings.
- AI output remains a proposal. Every file diff waits for an explicit Approve/Reject decision;
  A and R are keyboard shortcuts, not implicit approval.
- In-place migration requires a review acknowledgement and relies on the existing checkpoint
  and rollback service. Porting to a separate output folder is the default.
- Build risk checks are repeated by core before execution; UI acknowledgements are not the
  security boundary.
- Pack reports paginate at 50 rows. Job logs render a bounded tail. Long unchanged diff runs
  collapse while retaining their original line numbers.
- English is the fallback locale when a Vietnamese key is missing.

## Consequences

- The desktop remains a client of the existing application layer; the CLI and desktop share
  the same migration/build/pack behavior.
- The UI can be tested with golden JSON and mocked Tauri IPC without duplicating core logic.
- OS keyring availability is required to use stored AI/GitHub credentials. If the keyring is
  locked or unavailable, the command returns a safe error and does not fall back to plaintext.
- GitHub build authentication can use a keyring token or the user's existing `gh auth login`.
- Local builds still execute untrusted project scripts with the user's account privileges;
  the risk gate is a warning/acknowledgement, not a sandbox.
- Phase 11 initially packages Windows only. Installers are unsigned until a separate signing
  setup is approved.

## Alternatives rejected

- A second migration engine in Svelte: creates behavioral drift and bypasses core safeguards.
- Storing secrets in `settings.json`, local storage, or UI state across sessions: increases
  exposure and makes accidental serialization likely.
- Returning a stored secret to the web view: unnecessary for the UI and creates an avoidable
  exfiltration path.
- Automatically accepting AI proposals: violates ADR 0005 and the existing review gate.
- Running scans and filesystem work on the Tauri command thread: risks freezing the window.
