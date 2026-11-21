# ADR 0013 — Pack model, opt-in online index, hashing crates

**Status:** Accepted (Phase 10)

## Context
Phase 10 migrates whole modpacks. Packs list files by URL and hash (mrpack), by ids (CurseForge)
or hold JARs (folders, Prism). Version lookups need a network service; the user wants offline by default.

## Decision
- A `Pack` model (`core/src/modpack/`) keeps entries that may or may not have a local JAR, so URL-only
  and unresolved entries survive an export. Readers and writers are per format.
- `ModIndex` is a trait in core; `ModrinthIndex` lives in `providers/` (the only HTTP crate), runs only with
  `--online`, sends the `User-Agent` Modrinth requires, retries a 429 once. Core tests use a fake index.
- UMMP never downloads JARs; exported mrpack indexes carry URL + hash and the launcher downloads them.
- Hashing: RustCrypto `sha1` + `sha2` (pure Rust, no system deps; sha1 is needed because mrpack and the
  Modrinth API use it — for lookup and integrity of listed files, not for security decisions).
- Lookup cache: a JSON file in the output folder, not a KB table, to avoid a schema change for data that is
  cheap to refetch and tied to one run.
- Untrusted input: every path from a pack goes through `resolve_within`; archive size caps count bytes read.

## Consequences
New dependencies `sha1`, `sha2`. CurseForge ids cannot be resolved without an API key (BACKLOG).
Next ADR number: 0014.
