# Phase 13 — Optimization & Production Readiness (DRAFT)

## Intent
Turn the tool into a release candidate people can install and trust.

## Work items
| Area | Plan |
|---|---|
| Performance | Benchmarks (criterion) for analyze/scan/migrate on a 200-mod corpus; profile hot paths (zip reading, tokenizer) |
| Memory | Stream JAR entries instead of reading whole files where possible; cap sizes everywhere (scan already caps entries) |
| Caching | Content-hash cache of analysis/scan results per JAR (SQLite, existing persistence layer) |
| Incremental analysis | Re-analyze only changed files (mtime + hash) |
| Parallel execution | rayon over mods/files; bounded by CPU; deterministic output order |
| Security audit | Full `claude-security` scan + `cargo audit`/`cargo deny`; fuzz the class-file parser, zip handling, descriptor parsers (cargo-fuzz) |
| Dependency audit | License + advisory check in CI (`cargo deny`) |
| Crash recovery | Resume interrupted jobs from `jobs`/`persistence`; checkpoints already allow rollback |
| UX | Error messages with next steps; consistent progress output |
| Installer | Windows MSI/winget, Linux tarball/deb, macOS dmg via cargo-dist |
| Update system | Signed release manifests; opt-in update check |

## Deliverables mapping
Release Candidate (v1.0.0-rc.1), Security Report, Performance Report, Production Readiness
Checklist (`docs/PRODUCTION_CHECKLIST.md`).

## Open questions
- Which platforms get installers first (Windows likely)?
- Code signing certificate for Windows (cost)?

## Risks
Fuzzing will find parser bugs — budget time to fix them.
