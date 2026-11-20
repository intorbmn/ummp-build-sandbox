# Phase 10 — Modpack Migration: design

Status: approved by the user on 2026-11-20 (questions answered, decisions 1-5 approved, Native execution).
Source of truth for scope: `docs/guides/phase-10-modpack.md`. This file records the calls made on top of it.

## Intent
Users ask to move a whole pack, not one mod. UMMP imports a pack, finds its mods, builds the
dependency graph, decides per mod (keep / update / port / replace / remove / manual), migrates configs,
and exports a new pack. It never downloads JARs and never trusts a pack's paths.

## Goals
- Read and write 4 formats: Modrinth `.mrpack`, CurseForge zip, Prism/MultiMC instance, plain folder (user's answer: all four, read and write).
- `ummp pack inspect | migrate | export`, offline by default; `--online` enables read-only Modrinth lookups.
- Every pack is scanned for malware risk first; High stops the run unless `--allow-risk` (exit 2).
- Deterministic reports; parallel analysis.

## Non-goals
Porting closed-source JARs of a whole pack; launcher integration; downloading JARs.

## Anti-goals
Silently dropping a mod; writing outside the output folder; shipping third-party JARs inside an exported pack; a decision without a reason string.

## Approaches
1. **`modpack/` module with a Pack model, `ModIndex` trait, FakeIndex tests (picked).** Represents URL-only entries and unresolved CurseForge files, so a pack can be exported again.
2. Unzip, then run `analyze` on `mods/`: cannot represent entries without a JAR, so no re-export. Rejected.
3. One generic reader for all formats: harder to guard paths per format. Rejected.

## Calls made (user approved 1-5 on 2026-11-20)
1. **Hashing:** RustCrypto `sha1` + `sha2` (ADR 0013). Cost if wrong: swap one crate.
2. **API cache:** JSON file `<out>/.ummp-cache/remote.json`, not a KB table (no schema change).
3. **Writers:** mrpack and folder fully; Prism fully (`instance.cfg`, `mmc-pack.json`, `minecraft/mods`); CurseForge writes `manifest.json` only for entries that came with `projectID`/`fileID`, everything else local goes to `overrides/mods/`. New CurseForge ids need an API key (BACKLOG).
4. **Order:** mrpack + folder first, then CurseForge + Prism.
5. **Environment:** built and tested in the sandbox (`RUSTUP_TOOLCHAIN=stable`); the `D:\Claude\ummp-packs\...` root is the default in code only.

Additional calls (mine, each cheap to change):
- **mrpack hashes:** the spec lists hashes for files that are downloaded, so there is no JAR to compare. A file is hash-checked when the zip also bundles it at `files[].path`; hashes must be well-formed hex (40 / 128 chars) either way.
- **Remote entries without a JAR:** no descriptor is readable offline. If the target equals the pack's own mc + loader they are `Keep`; otherwise `Update` with `--online` (by hash), else `Manual`.
- **"Supports target":** the analyzer keeps only the first version of a range, so a mod supports the target when its detected mc equals the target exactly and its loader matches. Ranges with a higher upper bound are under-reported (KNOWN_LIMITATIONS).
- **Port:** `port::port` outputs source, not a JAR. `Port` writes `<out>/ported/<mod-id>/`; the mod is left out of the pack until the user builds it (`ummp build`). BACKLOG: build and bundle automatically.
- **Replace:** rows in `core/src/pack_config_rules.toml`. Resolved to a URL only with `--online`; offline it is recorded and counts as "left to the user" (exit 3).
- **Side overrides:** `overrides/`, then `client-overrides/`, then `server-overrides/` are merged into one root (later wins, a warning is recorded). Export writes them into `overrides/`.
- **Archive caps:** per-entry 256 MiB, total 2 GiB, 20 000 entries; enforced on bytes actually read, not on header sizes.
- **Loader version for export:** `--loader-version`, else the source pack's version when the loader is unchanged, else error.
- **Exit codes:** 0 done; 2 refused (risk); 3 done but Manual/unresolved Replace remain.

## Design
`core/src/modpack/`: `mod.rs` (Pack, PackEntry, import, limits, error), `formats.rs` (readers/writers), `graph.rs`, `plan.rs` (+ `ModIndex`, `RemoteVersion`), `config.rs`, `report.rs`, `analyze.rs` (parallel analysis + risk gate, split out of `mod.rs`). `providers/src/modrinth.rs`: `ModrinthIndex` (ureq). `cli`: `pack` group. Data: `core/src/pack_config_rules.toml`.
Decision rules and the dependency re-check loop follow guide §6 exactly.

## Testing
Guide §7 test names, test-first; fixtures built at test time with real hashes; ★ tests mutation-checked and recorded in the phase file. CLI tests run the built binary. 10.G: synthetic pack Fabric 1.20.1 → 1.21.1 offline, plus a 300-JAR timing run.

## Left to the builder
Report wording, internal helper names, exact `--help` text.
