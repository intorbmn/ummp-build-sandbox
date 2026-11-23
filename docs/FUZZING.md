# Fuzzing

UMMP keeps fuzzing separate from the required stable CI. The scheduled workflow uses
nightly Rust and runs each target for 10 minutes every Monday (UTC); it can also be
started manually from GitHub Actions. The Phase 13H integration branch additionally runs
the campaign when fuzz harnesses or this workflow change, allowing the unmerged stacked work
to collect fuzz evidence without repeating campaigns for documentation-only commits.

## Targets

| Target | Surface |
|---|---|
| `class_file` | JVM class-file constant-pool parsing and UTF-8 extraction |
| `descriptor` | Fabric/Quilt JSON, Forge/NeoForge TOML, and YAML-shaped `plugin.yml` input |
| `mrpack_index` | Modrinth index parsing and bounded archive extraction |
| `zip_import` | ZIP source staging, including path validation |
| `gradle_log` | Gradle/compiler log classification |
| `crash_report` | Minecraft crash-report parsing |
| `plugin_protocol` | Bounded UTF-8 and single-line JSON plugin response decoding |
| `rest_request` | REST JSON request DTO deserialization for every endpoint body |
| `mcp_request` | MCP tool argument DTO deserialization for every tool |

The descriptor target's YAML arm exercises YAML decoding for a Spigot/Paper-style
`plugin.yml`; UMMP does not currently migrate that descriptor format.

## Local run

Install nightly Rust and cargo-fuzz, then prepare fixture-derived corpora:

```sh
rustup toolchain install nightly
cargo install cargo-fuzz
python3 fuzz/scripts/prepare-corpora.py
cargo +nightly fuzz run class_file fuzz/corpus/class_file -- -max_total_time=60
```

Replace `class_file` with any target in the table. The REST and MCP targets call fuzz-only
entry points that deserialize the same private request structs used by the live handlers;
they do not start a server or execute filesystem operations. The scheduled workflow uses
`-max_total_time=600` (10 minutes) per target. Input-size and archive-expansion bounds
in the harnesses keep fuzz iterations resource-limited.

## Crashes and regressions

libFuzzer saves reproducing inputs under `fuzz/artifacts/<target>/`. Every confirmed
crash must be minimized, reviewed, and added as a regression fixture under
`tests/fixtures/fuzz-regressions/<target>/`, with a normal stable-suite test. Do not
commit untriaged corpus growth or claim a parser is safe merely because a scheduled run
completed without a crash.
