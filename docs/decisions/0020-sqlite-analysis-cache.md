# ADR 0020: SQLite cache for deterministic per-JAR analysis

- **Status:** Accepted
- **Date:** 2026-11-22
- **Decision owners:** UMMP maintainers

## Context

Phase 13 requires repeated mod analysis and static scanning to avoid re-reading and re-processing unchanged JARs. UMMP already uses SQLite through `KnowledgeBase` (ADR 0003). Analysis and scan results are deterministic for a fixed JAR byte sequence, UMMP version, and rule implementation.

## Problem

The cache must survive process restarts, invalidate stale results reliably, tolerate corrupt rows, and never turn a successful fresh analysis into a failed migration.

## Options

1. **SQLite in `KnowledgeBase`.** Reuses the existing database lifecycle and transaction semantics.
2. **One cache file per JAR.** Requires path lifecycle, atomic file replacement, cleanup, and collision handling.
3. **In-memory cache.** Cannot reuse results across CLI invocations.

## Decision

Use a dedicated `analysis_cache` SQLite table in the existing knowledge database. Each key contains:

- cache kind (`analysis` or `scan`);
- SHA-256 of the exact JAR bytes;
- the `ummp-core` package version;
- a deterministic SHA-256 of the relevant rules and an explicit behavior identifier.

Rule inputs are hashed in normalized relative-path order, with exact file bytes and length delimiters. Absolute paths, timestamps, locale, and directory iteration order are excluded.

Payloads are JSON, capped at 64 MiB when read and written. Writes use a single atomic `INSERT OR REPLACE` statement. Missing, oversized, malformed, or incompatible payloads are cache misses. A malformed row is deleted before rebuilding. Callers treat cache read/write failures as non-fatal and compute the result normally.

Only complete successful JAR analysis and scan results are cached. Cache hits restore the current JAR path rather than reusing a path serialized by a previous run.

Incremental file skipping is not part of this decision. It requires durable per-file hashes and evidence that the prior session completed successfully; the current JobDb/session model does not yet provide that evidence.

## Consequences

- Repeated pack migrations can reuse descriptor, technology-adapter, and scan results for identical JAR bytes.
- Rule or UMMP version changes naturally invalidate old entries.
- The cache shares the knowledge database backup and lifecycle.
- Old cache rows may remain after version/rule changes; they are harmless but may require future cleanup.
- First-run cache writes are best-effort and do not change migration success semantics.

## Alternatives rejected

Filesystem sidecars and process-local caches were rejected because they add lifecycle complexity or do not persist between runs. Incremental porting is deferred rather than guessing that a file is unchanged.
