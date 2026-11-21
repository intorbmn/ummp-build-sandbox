# Phase 10 — Modpack Migration

Design: `docs/superpowers/specs/2026-11-20-phase10-modpack-design.md`.
Plan: `docs/superpowers/plans/2026-11-20-phase10-modpack.md`. Decision: ADR 0013.
Guide: `docs/guides/phase-10-modpack.md`. Skills: brainstorming, writing-plans,
executing-plans (native), TDD, karpathy-guidelines, verification-before-completion
(`rust-cli-skills:*` and `ecc:rust-review` were not loaded as skills in this session; the same
practices were applied by hand: typed errors, no recursion on untrusted input, bounded reads,
explicit review of the diff against the checklist below).

## Milestones
| ID | Milestone | Status | Evidence |
|---|---|---|---|
| 10.A | Pack model and readers: `.mrpack`, CurseForge zip, Prism/MultiMC instance, plain folder | COMPLETED | `core/tests/modpack.rs` 10.A: files + overrides, path traversal, zip-slip, hash mismatch, size caps, CurseForge as unresolved, Prism components, folder majority loader (mutation-checked) |
| 10.B | Parallel analysis, dependency graph, risk gate | COMPLETED | missing/duplicate/cycle/side issues; High-risk JAR refused; parallel result equals sequential (mutation-checked) |
| 10.C | Planner (Keep / Update / Port / Replace / Remove / Manual) + Modrinth index | COMPLETED | 12 plan tests with a fake index, `PanicIndex` proves offline never calls it; `providers/tests/modrinth.rs` 7 tests against a local fake server (mutation-checked) |
| 10.D | Replacement rules and online lookups (`--online`) | COMPLETED | rule + resolved URL only online; JSON cache in `<out>/.ummp-cache/remote.json` |
| 10.E | Config migration through `MigrationEngine` | COMPLETED | text-preserving key renames, checkpoint + rollback (mutation-checked) |
| 10.F | Export (mrpack, CurseForge, Prism, folder), report, `ummp pack inspect|migrate|export` | COMPLETED | `export_round_trips`, `exported_index_has_required_fields`, `report_lists_every_mod_once_with_action_and_reason`, `cli/tests/pack_cli.rs` (exit codes 0/2/3, non-empty output refused) |
| 10.G | Real-world check, timing, docs, PR | COMPLETED | see below |

## Mutation checks (each test went red, then the code was restored)
- 10.A: both `..` guards removed -> `rejects_mrpack_with_path_traversal`; backslash/colon guards
  removed -> same; env fields swapped and loader key changed -> `imports_mrpack_files_and_overrides`.
- 10.B: required dependencies ignored -> 3 graph tests; gate disabled -> `pack_with_high_risk_jar_is_refused`.
- 10.C: mc comparison always equal -> 9 tests; online update never tried -> `plan_updates_when_index_has_target_version`, `offline_mode_never_calls_index`.
- 10.E: target-version check dropped -> `config_rule_renames_key`.
- 10.F: updated mod's sha1 or sha512 replaced -> `export_round_trips`; manual mods kept in the
  output -> `export_round_trips`; `manual_count > 0` threshold changed -> `migrate_exit_3...`;
  `refused` check inverted -> 3 CLI tests.
- A first version of the test fixture used `ab` repeated for every hash, so a truncated or
  swapped hash survived the mutation. Hashes are now derived per project/version.

## Found by the tests on the way
- The CLI test `migrate_exit_2_for_high_risk_jar_unless_allowed` first assumed `--allow-risk`
  drops the JAR. The guide (and `check_risk`) say it means "I reviewed it", so the planner and
  the test now agree: the mod is handled like any other and the report keeps the `High` rating.
- An equivalent mutant in the TOML key-boundary check of the config rename was removed by
  simplifying the condition.

## Real-world check (synthetic pack, offline)
Three real-schema Fabric JARs with a javac-compiled class (`demoa` for 1.21.1, `demob` and
`democ` for 1.20.1) plus `config/`, and the `demo-fabric-mod` source as `demob`:

`ummp pack migrate pack --to 1.21.1 --loader fabric --sources src --output out`

Result, exit code 3: `demoa` keep, `demob` port (source in `out/ported/demob`), `democ` manual
("no version for 1.21.1/fabric"); `out/pack.mrpack`, `out/instance/`, `UMMP_PACK_REPORT.md`
written; `democ` is not in the exported pack. A second run with a Forge JAR in a Fabric pack
and a mod depending on a missing library showed the dependency re-check ("missing fabrica")
and the pack-level conflict list.

Timing (release build, 300 small JARs, one machine): `pack inspect` 0.11 s, `pack migrate`
0.10 s. The fixture JARs are a few KB, so this measures the pipeline, not large-mod I/O.

Not run: `--online` against the real Modrinth API (needs the user's go-ahead; the client is
tested against a local fake server) and a server launch of an exported pack (see BACKLOG).

## Phase 10 completion report
- **Implemented:** 10.A-10.G; all four commands pass (213 tests).
- **Deferred:** `docs/BACKLOG.md` (Phase 10 section); limits: `docs/KNOWN_LIMITATIONS.md`.
- **Entry condition for Phase 11:** user review and merge of the Phase 10 PR.
