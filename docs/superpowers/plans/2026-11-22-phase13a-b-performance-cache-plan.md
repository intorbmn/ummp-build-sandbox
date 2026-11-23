# Phase 13A–13B Implementation Plan

**Date:** 2026-11-22  
**Branch:** `phase-13a-b-performance-cache`  
**Spec:** `docs/superpowers/specs/2026-11-22-phase13a-b-performance-cache-design.md`  
**Base commit:** `c9da882a41a86ae6449fed0a86b77de79fed3ed1`

## Constraints

- Keep this work isolated to 13A–13B.
- Follow test-first development: add a failing test, implement the smallest change, run the focused test, then the workspace suite.
- Do not modify `master`, merge, or create a PR without the user's instruction.
- Preserve existing SQLite data and all public output behavior.
- No hard benchmark thresholds in CI.
- Do not report a benchmark baseline unless the command was actually run and machine metadata captured.
- Manual workflow is authorized by the user because the repository skills are unavailable in this session.

## Step 0 — Establish a reproducible working baseline

1. Confirm branch head and clean branch ancestry against current master.
2. Inspect the complete KnowledgeBase migration and test patterns, analyzer JAR entry points, scan API, pack analysis path, JobDb session state, and fixture generation scripts.
3. Obtain the current CI result for the base commit.
4. Run baseline Rust checks if an executable repository environment is available: `cargo fmt --all -- --check`, `cargo test --all`, `cargo clippy --all --all-targets -- -D warnings`.
5. Record unavailable checks explicitly. Do not substitute assumptions for local execution.

**Deliverable:** baseline notes in the implementation report; no source changes.

## Step 1 — Add Criterion benchmark infrastructure (13A)

1. Add Criterion as a workspace-appropriate dev dependency and register benchmark targets in `core/Cargo.toml`.
2. Add deterministic fixture-generation helpers for 300 JARs and a 5 MB JAR. Reuse existing fixture content; avoid network dependencies and unnecessary committed binary blobs.
3. Implement benchmark groups for:
   - one-mod analysis;
   - 300-JAR analysis;
   - plan plus dry-run port of `tests/fixtures/forge-src-1.16`;
   - 5 MB JAR remapping;
   - 300-JAR scanning;
   - 300-mod pack inspection if the existing pack API can construct the workload without unrelated network calls.
4. Separate setup/copy/generation from timed code. Use Criterion throughput/bench IDs to make workload size clear.
5. Run the benchmark suite on the available machine. Capture CPU, core count, RAM, OS/kernel, Rust version, command, commit and date.
6. Write measured results to `docs/PERFORMANCE.md`; if the environment cannot run the suite, commit methodology and leave results marked pending rather than fabricate numbers.

**Acceptance:** all benchmark targets compile; selected smoke benchmark executes; no benchmark time assertion is added to regular CI.

## Step 2 — Add deterministic cache identity and SQLite storage (13B foundation)

1. Add a small cache module or narrowly scoped types for:
   - cache kind (analysis / scan);
   - SHA-256 of exact input bytes;
   - UMMP version identity;
   - deterministic rules hash.
2. Implement deterministic rules hashing over sorted normalized relative paths and exact bytes. Include explicit identifiers for compiled-in rule behavior.
3. Add the `analysis_cache` table via the next KnowledgeBase schema migration. Store cache kind, content hash, UMMP version, rules hash, serialized payload and validation metadata.
4. Add parameterized insert/get/delete methods. Bound payload size and validate deserialized data before accepting a hit.
5. Treat malformed, incompatible, or unreadable cache rows as misses; rebuild and replace them. Cache persistence failures must not fail a successful fresh computation.
6. Add unit tests for stable hashing, key separation by cache kind, row round-trip, size/format validation, migration on an existing database and corrupt-row recovery.

**Acceptance:** database migration is backward-compatible; cache errors never corrupt or block normal operations.

## Step 3 — Integrate JAR analysis cache

1. Identify the narrowest shared JAR analysis entry point used by pack analysis.
2. Add a cache-aware wrapper without changing the existing uncached API semantics.
3. Cache only successful complete analysis results; never cache warnings/errors as successful metadata.
4. Ensure cached results preserve deterministic entry ordering and output equality.
5. Add required tests:
   - `second_analyze_hits_cache_and_is_identical`;
   - `rule_change_invalidates_cache`;
   - `ummp_version_change_invalidates_cache`;
   - `corrupt_cache_row_is_ignored_and_rebuilt`.
6. Add tests for different file bytes and different cache kinds.

**Acceptance:** second identical analysis hits cache and produces a structurally identical result; all invalidation cases recompute.

## Step 4 — Integrate scan cache

1. Add a cache-aware scan entry point at the shared scan boundary.
2. Include the exact scan rule identity in the cache key.
3. Cache only complete successful ScanReport values.
4. Test hit/miss, changed bytes, changed rules, corrupt payload and write failure transparency.

**Acceptance:** scan results are identical with cache enabled and disabled.

## Step 5 — Incremental port, only with safe evidence

1. Trace JobDb/session records to establish what proves a previous run completed successfully and which file hashes/rules were used.
2. Add a small persisted per-file manifest only if existing session state can represent this without weakening rollback semantics.
3. Skip a file only when prior successful-session evidence, current content hash and rules identity all match.
4. Always recompute for added/changed/missing/ambiguous files, failed/incomplete sessions, and absent metadata.
5. Add fault and regression tests for unchanged, changed, added, removed and interrupted inputs.
6. If the existing JobDb model cannot safely prove file-level completion, do not force incremental skipping into this milestone; document the blocker and complete safe per-JAR analysis/scan caching first.

**Acceptance:** fail-closed behavior; no changed file can be skipped.

## Step 6 — Verification and documentation

1. Run focused tests after each step.
2. Run `cargo fmt --all -- --check`, `cargo test --all`, `cargo clippy --all --all-targets -- -D warnings`, and compile/run benchmark smoke tests.
3. Review SQL migration behavior against an existing database and a fresh database.
4. Update `docs/PERFORMANCE.md`, phase status, and relevant limitations with measured evidence only.
5. Push branch changes and inspect branch CI. Fix failures before reporting completion.
6. Provide a concise Vietnamese report with commits, tests, benchmark evidence and CI run link. Wait for the user to review before merge.

## Risks / explicit stop conditions

- If the repo's benchmark API requires invasive public API changes, pause and revise the design rather than expanding scope.
- If a pack-inspect benchmark cannot avoid external lookups, benchmark the local-only path or document the omission.
- If incremental session evidence is insufficient, defer file skipping; do not infer success.
- If no local repository execution environment is available, prepare source changes only when the GitHub editing path can preserve and validate exact files; clearly mark tests and CI as unverified until CI runs.

## Execution log (2026-11-22)

- Base commit `c9da882a41a86ae6449fed0a86b77de79fed3ed1` CI run `44928076309` was confirmed successful.
- Added Criterion targets for all six guide workloads and `docs/PERFORMANCE.md`. Measurements remain pending: this session has no local Cargo/Rust toolchain, so no numbers are claimed.
- Added deterministic cache keys, SQLite cache storage, bounded/corrupt-payload handling, and cache integration for both CLI pack inspection and pack migration (JAR descriptors, adapter findings, and static scan results).
- Added tests for deterministic identity, cache hit parity/population, content/rule/version invalidation, corrupt-row rebuild, and creating the cache table when opening an existing database. They have not yet been executed.
- Incremental port remains deferred: current JobDb/session state has no per-file hashes or durable successful-run manifest.
- The current CI workflow's push filter does not include `phase-13a-b-performance-cache`; CI will run on a PR targeting `master`. No PR has been created.
- Root `Cargo.lock` remains tracked but stale for the new Criterion dependency. Regenerate it with the pinned toolchain before the first PR; do not fabricate lockfile entries.
