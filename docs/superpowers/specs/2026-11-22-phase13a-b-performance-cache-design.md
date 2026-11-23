# Phase 13A–13B Design: Performance and Incremental Cache

**Date:** 2026-11-22  
**Branch:** `phase-13a-b-performance-cache`  
**Base:** `c9da882a41a86ae6449fed0a86b77de79fed3ed1`  
**Status:** Design and implementation plan approved by the user; implementation in progress  
**Execution:** Manual implementation; required repository skills are unavailable in this session.

## Intent

Establish reproducible performance measurements and reduce repeated work without changing UMMP's observable analysis, scan, or migration results.

## Goals

- Add Criterion benchmarks for the Phase 13A workloads.
- Record reproducible baseline metadata and measured results in `docs/PERFORMANCE.md`.
- Cache per-JAR analysis and scan results in the existing SQLite knowledge base.
- Invalidate cache entries when file bytes, UMMP version, or relevant rules change.
- Treat malformed or obsolete cache entries as misses and rebuild them safely.
- Make repeated analysis results identical to uncached results.
- Add incremental port behavior only where the existing session model can prove a file is unchanged and the previous session completed successfully.

## Non-goals

- No speculative micro-optimizations before profiling.
- No cache for mutable project-wide planning or AI output.
- No cache reuse across different UMMP versions or rule sets.
- No changes to user-facing output schemas unless required for a cache-transparent implementation.
- No security audit, fuzzing, crash recovery, installers, update checker, or RC work in this branch.

## Approaches considered

1. **SQLite cache in KnowledgeBase (selected).** One transactional store, schema migration, easy invalidation and corruption handling; consistent with ADR 0003.
2. **Filesystem cache files.** Simple serialization, but adds path lifecycle, atomic-write, cleanup and cross-process consistency concerns.
3. **In-memory cache only.** Low complexity, but does not persist between CLI runs and does not satisfy incremental workflows.

## Design

### Cache identity

A cache identity is the tuple:

- SHA-256 of the exact input file bytes;
- UMMP package/workspace version;
- deterministic hash of the relevant rule inputs.

Rule hashing must be stable across machines: hash normalized relative rule paths and their exact bytes in sorted path order. Include built-in rule/version identifiers where behavior is compiled into code. A rules hash must not depend on filesystem timestamps, absolute paths, directory iteration order, or locale.

### Storage and behavior

- Add an `analysis_cache` table through the existing KnowledgeBase schema migration mechanism.
- Store cache kind, the three identity components, serialized result, and enough metadata to validate the row.
- Use parameterized SQL and bounded serialized payload reads.
- A valid hit returns the same domain result as a fresh computation.
- Missing, version-mismatched, rules-mismatched, hash-mismatched, undecodable, or structurally invalid rows are cache misses. Recompute and replace the bad row; never fail the user operation solely because a cache row is corrupt.
- Cache writes must be transactional. A failed write must not turn a successful fresh analysis into a failed operation.
- Do not cache errors or partial results.
- Cache keys must distinguish analysis from scan results.

### Incremental port

Use the existing JobDb/session history as the source of truth. Reuse a prior file result only when the last relevant session completed successfully and both the file content hash and relevant rules identity match. Never skip a changed, missing, newly added, or ambiguous file. If existing session metadata cannot establish this safely, defer that sub-path and document the limitation rather than infer unchanged state.

### Benchmarks

Add Criterion benchmarks for:

- analyze one mod;
- analyze 300 JARs using deterministic corpus copies;
- plan plus dry-run port of `tests/fixtures/forge-src-1.16`;
- remap a 5 MB JAR;
- scan 300 JARs;
- inspect a 300-mod folder when the Phase 10 pack-inspect API supports it.

Fixtures must be deterministic and generated without network access. Benchmark setup and fixture generation should be outside timed sections where possible. Include both cold-cache and warm-cache measurements for cached workloads. Do not commit generated 300-JAR or 5 MB artifacts unless repository conventions require them; prefer a deterministic generator.

### Performance reporting

`docs/PERFORMANCE.md` records CPU, core count, RAM, OS/kernel, Rust version, benchmark command, commit SHA, and date. Report actual measured values and Criterion comparison output. If measurements cannot be run in the available environment, record the exact reason and leave the baseline explicitly pending. Performance targets are proposed only after baseline measurement and user agreement.

## Testing

Required cache tests from the guide:

- `second_analyze_hits_cache_and_is_identical`
- `rule_change_invalidates_cache`
- `ummp_version_change_invalidates_cache`
- `corrupt_cache_row_is_ignored_and_rebuilt`

Also cover scan cache hit/miss, cache write failure transparency, deterministic rules hashing, and incremental-port safety (successful previous session, changed file, failed/incomplete prior session). Benchmark targets must compile and execute in CI without timing thresholds.

## Risks and mitigations

- **Stale results:** include content, UMMP version and rule identity in every key.
- **Corrupt cache:** treat decode/validation failures as misses and rebuild.
- **Benchmark noise:** record machine metadata; avoid hard timing assertions in CI.
- **Unsafe incremental skips:** require successful session evidence; fail closed to recomputation.
- **Schema compatibility:** add a forward-only migration and preserve existing KnowledgeBase data.

## Decisions / calls

- User-approved: all three release platforms, cargo-dist, unsigned builds, security audit at RC (recorded for later Phase 13 milestones).
- Selected here: SQLite cache, Criterion, deterministic rule hashing, conservative incremental reuse.
- Skills: no repository skills were available; the user explicitly authorized manual work. Follow the same test-first and verification discipline manually.
- Split: this branch covers only 13A–13B. Later Phase 13 milestones remain separate branches/PRs as required by the guide.
