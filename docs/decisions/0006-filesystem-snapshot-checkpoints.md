# ADR 0006 — File-copy snapshots for checkpoint/rollback

**Status:** Accepted (Phase 3 gap closed alongside Phase 6)

## Context
Phase 3 shipped `CheckpointStore`, which only recorded checkpoint metadata. Nothing was
saved or restored, so the acceptance criterion "rollback works" was not met. Phase 6
transforms need a guaranteed exact restore.

## Problem
How do we snapshot a project before a step and restore it byte-for-byte?

## Options
1. Copy the project tree into `<snapshot_root>/<checkpoint-id>/`
2. Require the project to be a git repository and use commits/stash
3. Copy-on-write filesystem features (btrfs/APFS clones)
4. Store only the files a step will touch

## Decision
Option 1 for the MVP, with a skip list (`.git`, `build`, `.gradle`, `target`, `out`,
`run`). Restore deletes files created after the snapshot and rewrites every snapshot
file. Snapshots live outside the project directory.

## Consequences
- Works on every OS and for projects that are not git repositories.
- Disk use is proportional to source size. Build outputs are skipped, and mod sources are small.
- Large binary assets are copied in full. Option 4 can replace this later behind the
  same `snapshot`/`restore` API.

## Alternatives rejected
- **Git-based:** many imported mods are plain archives; the tool must not touch a user's git state.
- **CoW clones:** not portable across Windows/Linux/macOS filesystems.
