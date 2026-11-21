# TESTING_STRATEGY — Testing Strategy

## Pyramid
1. **Unit** — pure logic (parsing, planning, transforms, compatibility lookups). Fast, `cargo test`.
2. **Component** — each engine behind its interface, with stub adapters.
3. **Integration** — pipeline steps wired together with real fixtures.
4. **Contract** — interface/version compatibility tests between core and adapters.
5. **Regression** — test corpus of real mods; run full pipeline; assert build/test outcomes.
6. **Compatibility** — matrix across Minecraft versions, loaders, languages.
7. **Game launch** — sandboxed client/server launch; log/crash analysis.
8. **Multiplayer** — Phase 10+.

## Test corpus (fixtures)
Representative mods covering: Forge legacy, Forge modern, Fabric, NeoForge, Quilt, Bukkit/Paper, Java-only, Kotlin, mixin-heavy, rendering-heavy, networking-heavy, native/JNI, modpack projects.

## Evidence rule
A module is not COMPLETED without: unit tests, integration tests, build passing, and known limitations documented.

## Non-trivial logic
Every branch, loop, parser, and security-relevant path gets a test or assert-based self-check.