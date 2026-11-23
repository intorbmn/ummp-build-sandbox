# Security design and threat model

## Security boundary

Treat every mod JAR, archive, repository, descriptor, build log, plugin, network response, REST/MCP request, and AI proposal as untrusted input. A successful parse or static scan is not proof that an artifact is safe.

The trusted computing base includes the UMMP CLI/core, desktop shell, local REST/MCP servers, release workflow, pinned Rust toolchain, and the operating system account running UMMP. Local builds and plugins execute with that account's permissions. UMMP does not claim to sandbox arbitrary code at the OS level.

## Assets

- User source trees, generated output, checkpoints, and migration reports.
- API keys, access tokens, credentials, and private source files.
- Integrity of the knowledge database, cache, job state, and release artifacts.
- Availability of the user's machine and UMMP services.

## Threats and current controls

| Attack surface | Threat | Current control / residual risk |
|---|---|---|
| JAR and ZIP import | Zip-slip, path traversal, symlinks, oversized entries, archive bombs | Use enclosed_name, resolve_within, bounded reads, and extraction limits at every archive boundary. Verify each reader; a missing limit is a defect. |
| Class and descriptor parsers | Malformed or deeply nested input, panic, excessive allocation | Validate lengths before allocation; fuzz parsers and turn every crash into a regression fixture. |
| Source repositories | Malicious Gradle scripts or wrapper code | Local builds execute user code as the current OS user. Prefer the GitHub build backend for untrusted projects. Process timeouts and environment scrubbing are not a sandbox. |
| Plugins | Malicious executable, path escape, secret access, hung or oversized output | Manifest and executable paths are checked; environment is scrubbed; process timeout and protocol output cap apply. Plugins are not OS-sandboxed and retain the user's OS permissions. Install only trusted plugins. |
| REST API | Unauthorized local access, remote exposure, path races | Keep the API local by default. Remote binding is explicit and has no TLS; do not expose it to untrusted networks. Path canonicalization has a TOCTOU gap where noted in KNOWN_LIMITATIONS. |
| MCP | Unsafe tool invocation or unintended writes | Require the configured approval and allowed-root gates for write operations. Treat MCP clients and request payloads as untrusted. |
| AI output | Prompt injection, malicious or incorrect edits | AI output is a proposal; validate edits and require human approval before applying. Use checkpoints and review diffs. |
| Secrets | Key leakage through logs, child processes, files, or reports | Do not log credentials. Child processes receive an allow-listed environment. Credential-file storage is currently plaintext with restrictive Unix permissions, not encryption; see KNOWN_LIMITATIONS. |
| Dependencies and releases | Known vulnerable dependency, license incompatibility, compromised artifact | Run cargo audit and cargo deny check in CI; pin toolchains/actions where practical; distribute release checksums and provenance when configured. |

## Required implementation rules

1. Every archive reader must reject paths that escape the extraction root and enforce per-entry and aggregate size limits before writing.
2. Every external process must use the shared process runner, a bounded timeout, a scrubbed environment, and bounded output where output is captured.
3. Never log secrets. Error messages and diagnostics must not include credential values.
4. Do not use unwrap() or expect() on untrusted-input-derived values in production code. Prefer typed errors and checked conversions.
5. Treat cache rows and persisted job/session data as untrusted, versioned input. Corrupt data must fail closed or be discarded and rebuilt safely.
6. Keep write operations behind explicit approval and root/path validation.

## Verification plan

- cargo audit checks RustSec advisories.
- cargo deny check checks advisories, license policy, duplicate versions, and sources.
- Fuzz targets cover bytecode/class parsing, descriptors, .mrpack indexes, ZIP import, Gradle logs, crash reports, and plugin protocol decoding. Fuzzing runs separately on nightly so stable CI remains deterministic.
- Full repository security review is reserved for the release-candidate commit. Confirmed findings require a regression test and fix; unresolved findings must be documented in docs/KNOWN_LIMITATIONS.md.

## Incident response

Report vulnerabilities privately using the process in the root SECURITY.md. Do not publish exploit details before maintainers have had a reasonable opportunity to investigate and coordinate a fix.
