# Phase 13.D — Fuzzing

## Scope
Add bounded, fixture-seeded fuzz harnesses for untrusted UMMP input surfaces. Keep fuzzing on
nightly Rust in a separate workflow so the required stable CI remains unchanged.

## Delivered
| Target | Surface |
|---|---|
| `class_file` | JVM class-file parser and constant-pool UTF-8 reader |
| `descriptor` | Fabric/Quilt JSON, Forge/NeoForge TOML, YAML decoding for plugin.yml |
| `mrpack_index` | Modrinth index and bounded archive import |
| `zip_import` | Source ZIP staging and path validation |
| `gradle_log` | Gradle/compiler log analyzer |
| `crash_report` | Crash-report parser |
| `plugin_protocol` | Bounded plugin JSON Lines response decoder |
| `rest_request` | REST request DTO deserialization |
| `mcp_request` | MCP tool argument DTO deserialization |

- Added `fuzz/Cargo.toml`, nine `cargo-fuzz` targets, and
  `fuzz/scripts/prepare-corpora.py` to generate seed corpora from checked-in fixtures.
- Added `.github/workflows/fuzz.yml`: push to the original fuzz branch builds the harness matrix; scheduled/manual runs execute each target for 600 seconds and upload crash artifacts. The Phase 13H integration branch also runs campaigns when fuzz harnesses or this workflow change.
- Added fuzz-only REST/MCP decoder entry points that deserialize the same request DTOs as
  production handlers without starting a server or invoking filesystem operations.
- Extracted the bounded plugin response decoder so it can be exercised directly.
- Added `docs/FUZZING.md` with local-run and crash-triage instructions.

## Verification
- Stable CI run [#814](https://github.com/intorbmn/Universal-Minecraft-Mod-Porter/actions/runs/45004353816):
  `build-test-lint`, `desktop`, and `security-audit` all completed successfully.
- The stable run validates the normal workspaces; it is not evidence that the separate
  nightly fuzz campaign has completed.
- The fuzz workflow is configured for a weekly Monday 03:17 UTC schedule and manual dispatch. The Phase 13H branch runs the campaign when `fuzz/**` or the workflow changes, avoiding repeated campaigns for documentation-only commits. The first completed 10-minute-per-target campaign remains pending; no crash-free claim is made.
- The `plugin.yml` harness currently exercises YAML decoding only. UMMP does not yet migrate
  Spigot/Paper descriptors.

## Acceptance status
Implementation and stable-CI verification are complete. Mark the milestone fully verified
only after the scheduled/manual fuzz workflow completes all nine 10-minute campaigns and
any discovered crash is fixed with a stable regression test.

## PR
PR #25 is stacked on PR #24, which is stacked on PR #23. Merge only after the preceding
Phase 13 PRs have been reviewed and merged.
