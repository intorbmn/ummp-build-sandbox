# ADR 0009 — Remap class names in-process instead of running an external remapper

**Status:** Accepted (Phase 6.F)

## Context
JAR-only mods need class renames applied to bytecode. Tools such as tiny-remapper or
SpecialSource can do this, but they run on a JVM, and Phase 8, which provides the
sandboxed process runner, does not exist yet.

## Problem
How do we migrate class names in compiled mods now, safely, on any OS?

## Options
1. Run an external remapper as a subprocess
2. Rewrite `CONSTANT_Utf8` entries of the constant pool in Rust
3. Defer bytecode migration to Phase 8

## Decision
Option 2 for class-level renames. Only Utf8 constants change (internal names,
descriptors and signatures), so every constant-pool index and attribute stays valid. The
input JAR is never modified, and the output is created exclusively (`create_new`).

## Consequences
- No subprocess, no JVM requirement, and no untrusted code runs.
- Verified against a real JVM in `core/tests/bytecode_jvm.rs`: the test compiles with
  javac, remaps, then compiles and runs code against the new names.
- Member (method/field) renames and string-literal reflection are not handled. Those
  need member mappings plus an external remapper (option 1) in Phase 8.
- JAR signatures are dropped, because the remapped classes cannot match them.

## Alternatives rejected
- **External tool now:** needs the Phase 8 sandbox and a JVM on the user's machine.
- **Defer:** leaves JAR-only mods with no migration path at all.
