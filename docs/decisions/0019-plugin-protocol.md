# ADR 0019: Out-of-process plugin protocol and trust boundary

**Status:** Accepted  
**Date:** 2026-11-21  
**Scope:** plugin discovery, execution and proposed edits

## Context

UMMP needs third-party adapters and rules without loading arbitrary plugin code into the core process. Plugins are untrusted executables and must not receive AI keys or GitHub/REST tokens. Every project write must continue through the checkpointed MigrationEngine boundary (ADR 0005).

ADR 0018 is already assigned to the project license. This decision uses the next available number, 0019, without renumbering history.

## Decision

- Plugins are out-of-process executables described by `ummp-plugin.toml` with `name`, `version`, `kind`, `command` and `ummp_api = 1`.
- Installed plugins are immediate child directories of `<data_dir>/plugins/`.
- The protocol is one JSON request per stdin line and one JSON response per stdout line. Methods are `hello`, `findings` and `edits`.
- UMMP sends bounded project file contents over stdin. Secret-like files are excluded.
- The process uses a scrubbed environment, a 30-second per-call timeout, process-tree termination and an 8 MiB stdout limit.
- Every returned path must refer to a supplied existing project file, resolve within the canonical project root, and not be a `.jar` or `.class` file. Returned original text must match current disk content.
- Proposed edits are converted to `EditSet` and applied only by `MigrationEngine` after a checkpoint.
- `ummp plugins list` lists manifests under the default plugin directory; `ummp plugins check <dir>` validates the manifest, executable and API handshake.

## Consequences

- An API mismatch is refused with a clear error; API 1 is the only supported version.
- The protocol itself never writes project files and never passes process environment secrets.
- Timeout and output limits bound a misbehaving child process.
- This is not an OS sandbox. A malicious executable still runs with the user's permissions and may access files or processes available to that user. Users must install only trusted plugins.
- Stderr is drained separately and is not interpreted as protocol data.

## Alternatives rejected

- In-process dynamic libraries: rejected because they share UMMP's address space and can corrupt it.
- WASM: explicitly deferred by the Phase 12 guide.
- Plugins writing project files directly: rejected because all edits must remain reviewable and checkpointed.
