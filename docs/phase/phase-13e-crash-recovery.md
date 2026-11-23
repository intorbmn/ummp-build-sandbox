# Phase 13.E — Crash recovery

## Delivered in this increment

- Checkpoint snapshots flush copied file contents before publishing the manifest.
- `checkpoints.json` is written through a unique temporary file, `sync_all`, atomic rename, and Unix directory `fsync`.
- The manifest is persisted by `CheckpointStore::snapshot` before `MigrationEngine` begins applying edits. A process interruption therefore leaves a discoverable pre-transform checkpoint.
- Normal checkpoint-store saves use the same atomic persistence path.
- Added `ummp recover <project> --session <UUID>` to restore the latest checkpoint associated with a migration session. `ummp migrate --apply` prints the session UUID before waiting for completion.
- Added tests for manifest durability, recovery after partial file changes, and rejection of a truncated manifest.

## Recovery contract and limitation

Recovery currently supports **rollback to the pre-transform snapshot only**. Continuing is intentionally not offered: UMMP does not persist a deterministic migration edit plan, so replaying after a crash could apply a different edit set. `AppServices::open` automatically detects persisted jobs left in `Running` and marks them `Failed` with an interruption marker. The CLI recovery command still requires the session UUID printed before migration starts; it does not yet present an interactive recovery prompt or support continuation.

The snapshot excludes the existing `SKIP_DIRS` (`.git`, build output, etc.), matching the established rollback contract. This is not a full-disk transaction and cannot restore files intentionally excluded from snapshots.

## Validation

The integrated Phase 13H head passed build/test/lint, desktop and security-audit checks in CI run [#971](https://github.com/intorbmn/Universal-Minecraft-Mod-Porter/actions/runs/45117075173). Do not mark Phase 13.E complete until those checks pass on the final updated commit and the CLI recovery path passes a real process-kill smoke test on a disposable project.

## Usage

After an interrupted `ummp migrate --apply`:

```sh
ummp recover ./path/to/mod --session <session-uuid>
```

Add `--snapshots <directory>` if the checkpoint directory was customized.
