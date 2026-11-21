# Phase 13 — Production Readiness (guide for Claude Code)

Read [README.md](README.md) first. Rough history: `docs/drafts/phase-13-production.md`.

## 0. Skills
- **Always:** the README §3 set.
- **Phase-specific:**
  - `claude-security:claude-security` (full repository audit; dispatch the
    `claude-security:claude-security` agent only with the user's consent because it uses
    many tokens)
  - `rust-cli-skills:rust-performance`, `rust-cli-skills:rust-memory`
  - `rust-cli-skills:rust-distribution` (installers, releases)
  - `ecc:benchmark-methodology`
  - `ecc:architecture-decision-records`

## 1. Goal (master prompt, Phase 13)
Make UMMP safe, fast and installable for real users:
- benchmarks and performance targets;
- caching and incremental runs;
- a full security audit with fixes;
- fuzzing of every untrusted-input parser;
- crash recovery for interrupted migrations;
- installers and signed releases;
- an update check;
- a release candidate with complete user docs.

## 2. Questions for the user (defaults in brackets)
1. Platforms for release binaries? [Windows x64 (MSI + zip), Linux x64 (tar.gz), macOS
   arm64 (tar.gz, unsigned)]
2. Release tool? [`cargo-dist`, which generates the GitHub Release workflow and
   installers]
3. Code signing on Windows? [Not in this phase unless the user has a certificate; record
   the SmartScreen warning in KNOWN_LIMITATIONS]
4. May the full security scan run (expensive)? [Yes, once, on the RC commit]
5. Update check? [Opt-in at first run; it asks GitHub Releases for the latest tag; no
   telemetry]

## 3. Milestones

### 13.A Benchmarks and targets
- `benches/` with `criterion` (a new dev-dependency; record it in the design doc).
  Benchmarks:
  - analyze 1 mod
  - analyze 300 JARs (corpus copies)
  - plan + dry-run port of `tests/fixtures/forge-src-1.16`
  - remap a 5 MB JAR
  - scan 300 JARs
  - pack inspect of a 300-mod folder (if Phase 10 is merged)
- Write the baseline numbers (machine, OS, commit) in `docs/PERFORMANCE.md`.
- Set targets with the user, for example: 300-JAR analyze under 30 s on a 4-core laptop.
- **Profiling:** `cargo flamegraph`, or `perf` on Linux. Optimize only what the profile
  shows. Keep before/after numbers.

### 13.B Caching and incremental runs
- **Cache key:** sha256 of the file bytes + UMMP version + rules hash. Cache the per-JAR
  analysis and the scan results in the KB (a new table `analysis_cache`, migration in
  `knowledge.rs`).
- **Incremental port:** skip files whose hash and rules are unchanged since the last
  successful session (session metadata in `JobDb`).
- **Tests:**
  - ★ `second_analyze_hits_cache_and_is_identical`
  - ★ `rule_change_invalidates_cache`
  - `ummp_version_change_invalidates_cache`
  - `corrupt_cache_row_is_ignored_and_rebuilt`

### 13.C Security audit and hardening
- Run `cargo audit` and `cargo deny check`, and add both to CI (`deny.toml`: licenses
  allow-list, ban duplicate major versions where practical, advisories).
- Full scan with `claude-security` on the RC commit. Fix every confirmed finding with a
  test (RED → GREEN). Write unresolved ones to KNOWN_LIMITATIONS with a reason.
- Update the existing threat model `docs/SECURITY.md` (from Phase 1; read it first) and add
  a `SECURITY.md` at the root (how to report
  vulnerabilities). Cover:
  - untrusted JARs and zips (zip-slip, bombs, huge class files);
  - untrusted Gradle builds (local builds run the user's code: document it, and the
    default `ummp build` backend for untrusted input should be GitHub);
  - plugins (Phase 12);
  - the REST/MCP surface;
  - AI output (approval gate);
  - secrets (keyring, token files).
- **Checklist** (grep for each and add a test where one is missing):
  - every archive reader has a size cap and `enclosed_name` + `resolve_within`;
  - every process uses `build::process::run` with a timeout;
  - no secret is ever logged;
  - no `unwrap()` on input-derived data in non-test code
    (`clippy::unwrap_used` for `core` as a deny lint, with a justification at each
    remaining call).

### 13.D Fuzzing
- `fuzz/` with `cargo-fuzz` (it runs on nightly in CI only, as a separate scheduled
  workflow; the stable CI is unaffected).
- **Targets:**
  - class file parser (`bytecode`)
  - descriptor parsers (fabric.mod.json, mods.toml, plugin.yml)
  - `.mrpack` index
  - zip import (`port`, `modpack`)
  - Gradle log analyzer
  - crash report parser
  - the plugin protocol decoder
- **Seed corpora** from `tests/fixtures`. Run each target for 10 minutes in the scheduled
  job.
- Every crash found becomes a regression test in the normal suite, with the input file
  checked in under `tests/fixtures/fuzz-regressions/`.

### 13.E Crash recovery
- If the process dies during `MigrationEngine::run` (kill -9 or a power loss), the next
  start must detect the unfinished session. A session marked `Running` in `JobDb` with no
  live process means it was interrupted.
- It then offers `ummp recover <session>`: roll back to the checkpoint, or continue.
- Checkpoint writes must be durable: write to a temp file, fsync, rename; fsync the
  directory on Unix. Follow `rust-filesystem`.
- **Tests:**
  - ★ simulate an interruption (a fault-injection hook in tests:
    `RunOptions::fail_after_edits(n)` behind `#[cfg(test)]` or a test-only feature)
  - ★ `recover` rolls back to a tree identical to the original
  - a checkpoint file truncated mid-write is detected and refused

### 13.F Distribution
- `cargo dist init`: targets from question 1, MSI for Windows, shell/PowerShell installers.
  The release workflow runs on tag `v*`.
- Version `1.0.0-rc.1`: bump all crates. `CHANGELOG.md` goes from Unreleased to the
  version.
- Before the real tag, do a dry run on the sandbox repo or with a pre-release tag. The
  artifact download is blocked in this environment, so record the run id and let the user
  download it.
- **The user installs** the Windows build on their machine and runs `ummp port` on the demo
  mod. Their confirmation is the real-world check.

### 13.G Update check
- `ummp update --check`, plus an opt-in check at most once a day when the user enabled it.
  It calls `GET https://api.github.com/repos/intorbmn/universal-minecraft-mod-porter/releases/latest`
  through `providers/`.
- It prints the new version and the release URL. It never downloads or installs anything
  by itself.
- **Tests** (fake server): newer, same, older, network error (silent), disabled (no call).

### 13.H RC documentation
- `docs/USER_GUIDE.md` in English and `docs/USER_GUIDE.vi.md` in Vietnamese (the user is
  Vietnamese). Cover:
  - install;
  - porting a mod source;
  - porting a pack;
  - reading the report;
  - the build backends;
  - the AI assistant;
  - rollback/recover;
  - troubleshooting (Java versions, Gradle, Maven 429, risk refusals).
- All screenshots come from the real app (Phase 11).

## 4. Docs (same PR(s))
- ADRs: caching, release tooling, recovery protocol.
- `docs/PERFORMANCE.md`, `docs/SECURITY.md`, `SECURITY.md`, user guides.
- Phase file, STATUS, CHANGELOG (release section), README (install section with the
  release links), CLAUDE.md, KNOWN_LIMITATIONS, BACKLOG.
- Phase 13 is big. Propose to the user to split it into 2–3 PRs:
  - performance + caching;
  - security + fuzz + recovery;
  - distribution + update + docs.

## 5. Risks
- **Fuzzing needs nightly:** keep it out of the required CI.
- **cargo-dist versions change fast:** pin the version in the generated workflow.
- **Over-optimizing:** targets first, profile second, change third.

## 6. Backlog candidates
- Windows code signing
- macOS notarization
- package managers (winget, scoop, Homebrew, AUR)
- telemetry (opt-in, only if the user asks)
- delta updates

## 7. Definition of done
README §7, plus:
- the benchmarks are recorded and the targets met, or the misses explained;
- the audit findings are fixed or documented;
- the fuzz targets run;
- the recovery tests pass;
- RC binaries are built by the release workflow and installed by the user;
- the user guides exist in both languages.
