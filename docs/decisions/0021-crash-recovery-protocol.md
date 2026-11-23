# ADR 0021: Durable pre-transform checkpoint recovery

## Status

Accepted

## Context

A process can terminate after the migration engine has started modifying files. Previously, the checkpoint manifest was saved by the caller only after the migration completed, so a crash during apply could leave the snapshot on disk without a durable index entry.

## Problem

Recovery must identify the pre-transform snapshot after process death, and checkpoint metadata must not be left half-written by power loss or process termination.

## Options

1. Persist checkpoint metadata only after the migration completes.
2. Persist a complete manifest atomically as soon as the snapshot is complete, before any project edits.
3. Persist the full edit plan and support deterministic continuation.

## Decision

Choose option 2 for this increment. Flush snapshot file contents, sync snapshot directories on Unix, then publish `checkpoints.json` using a temporary file, `sync_all`, atomic rename, and parent-directory `fsync` on Unix. Reuse the same atomic path for subsequent checkpoint-store updates.

Expose `ummp recover <project> --session <UUID>` to restore the latest checkpoint belonging to that session. Recovery is rollback-only until the exact migration edit plan and its dependencies can be persisted and safely replayed.

## Consequences

- A completed snapshot becomes recoverable before the first migration write.
- A truncated manifest is rejected rather than silently treated as an empty store.
- Recovery requires the session UUID printed by the CLI.
- Startup detects persisted `Running` jobs with no surviving process and marks them `Failed` with an interruption marker.
- The CLI still requires the session UUID; it does not yet show an interactive recovery prompt.
- The existing snapshot exclusions remain unchanged.

## Alternatives rejected

- Continuing from a partially applied migration without a persisted edit plan could produce non-repeatable results.
- Silently ignoring malformed checkpoint metadata could make recovery appear successful while losing the only recovery index.
