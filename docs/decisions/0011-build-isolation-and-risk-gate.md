# ADR 0011 — Build on disposable GitHub runners by default; gate local builds on a static risk scan

**Status:** Accepted (Phase 8)

## Context
Phase 8 builds, tests and starts mods, including mods from other sources. A Gradle build
script is code, and so is the mod: building or launching runs untrusted code. The user wants
GitHub builds when possible, otherwise local builds in a new folder under `D:\Claude`, and a
warning when a mod may contain malware.

## Options
1. GitHub Actions only
2. Local only
3. One `BuildBackend` trait with both; GitHub by default, `--local` on request

## Decision
Option 3.
- **GitHub backend:** UMMP copies the project, drops the mod's own workflows, adds a generated
  workflow (`permissions: contents: read`, no secrets) and pushes it to branch
  `ummp/<mod>-<timestamp>` of a separate build repo. It drives `git` and `gh` as subprocesses,
  waits for the run, and downloads one artifact with `stages.json`, logs, test results and
  crash reports.
- **Local backend:** a fresh copy in `<workdir>/<mod>-<timestamp>/`, an environment allow-list
  (no API keys or tokens), a timeout per stage that kills the whole process tree, and the
  server stopped with `stop` after `Done (`.
- **Risk gate:** `ummp scan` runs before every build. Static rules (TOML, embedded) over class
  constant pools, sources and build scripts, recursing into nested JARs. Local: High risk is
  refused without `--i-accept-risk`, and Medium asks. GitHub: warn only.
- The scan never says "safe". The clean result reads "No risk indicators found (this is not
  proof the mod is safe)".

## Consequences
- The safe path (GitHub) is slower and needs `gh` plus a build repo. A private repo uses
  Actions minutes.
- Local isolation is process-level only; Docker/VM isolation is in `docs/BACKLOG.md`.
- Static rules have false positives (native libraries, reflection) and miss novel
  obfuscated malware.
