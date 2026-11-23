# Phase 14 — Community & Ecosystem (guide for Claude Code)

Read [README.md](README.md) first. Rough history: `docs/drafts/phase-14-community.md`.

## Status update (2026-11-22) — read first, it wins where the text below differs
- **Do not start Phase 14 yet.** §2 requires the Phase 13 releases. On `master`, Phase 13 is
  IN_PROGRESS: 13.A–13.D are merged (PRs #23, #24, #25: benchmarks, SQLite analysis
  cache ADR 0020, security audit + `security-audit` CI job, nightly fuzzing of 9 parsers).
  13.E crash recovery, 13.F distribution (prebuilt `ummp` on GitHub Releases, decided by
  the user), 13.G update check and 13.H RC docs are still open. Start only after Phase 13
  is COMPLETED and the user says so.
- **Next ADR number: 0021** (0018 license, 0019 plugin protocol, 0020 analysis cache).
- **`Cargo.lock` is stale** (`cargo metadata --locked` fails on `master`). Phase 13.F must
  commit a current lock before releases; Phase 14 must not work around it.
- **Answers already given by the user:**
  - Project code license is **Apache-2.0** (ADR 0018). Question 4 (license for community
    data: CC0 or CC-BY-4.0) is still open; ask, do not choose.
  - Monetization: voluntary support with early access, plus an open-source core with a
    separate proprietary **Pro** edition (`docs/BACKLOG.md`). Keep Pro code out of this
    repo. Community KB, recipes and the plugin index stay open and free; do not gate any
    Phase 14 feature behind Pro without asking.
- **Inputs from Phase 12:** the plugin SDK (14.B) builds on the out-of-process JSON Lines
  protocol in `core/src/plugins.rs` and ADR 0019 (env scrub, 30 s timeout, 8 MiB output
  cap, edits only through `MigrationEngine`). Plugins are not OS-sandboxed: the SDK docs and
  the plugin index (14.E) must say so plainly, and indexed plugins need the same review as
  community recipes.
- **Not in this phase** (BACKLOG, after all phases): `.jar` port input, downgrade, more
  target versions, more AI providers, final AI key storage, Pro features.

## 0. Skills
- **Always:** the README §3 set.
- **Phase-specific:**
  - `example-skills:doc-coauthoring` (contributor and plugin-SDK docs)
  - `pr-review-toolkit:review-pr` (review workflow for community recipes)
  - `ecc:api-design` (KB schema versioning)
  - `ecc:architecture-decision-records`

## 1. Goal (master prompt, Phase 14)
Let other people extend UMMP without changing its code:
- contributor documentation;
- a plugin SDK with docs and a template;
- a community knowledge base (mappings, compatibilities, replacement and config rules) that
  is validated before it is accepted;
- shareable migration "recipes";
- a plugin index;
- an issue-reporting command that never sends anything without the user's confirmation;
- community regression testing;
- a documentation website.

## 2. Depends on
- Phase 12 plugin protocol.
- Phase 13 releases (users install binaries).
- If either is missing, stop and tell the user.

## 3. Questions for the user (defaults in brackets)
1. Where does the community KB live? [A separate public repo
   `intorbmn/ummp-knowledge` with TOML/JSON rule files, validated by its CI using
   `ummp kb validate`]
2. Who reviews contributions? [The user (maintainer), with UMMP's validation + a
   `review-pr` checklist; no automatic merges]
3. Docs website? [mdBook built from `docs/`, published with GitHub Pages]
4. License for community data? [CC0 or CC-BY-4.0 for rules; the code keeps the repo's
   license. Ask; do not choose for them.]

## 4. Milestones

### 14.A Contributor docs
- `CONTRIBUTING.md`:
  - setup (README §2 of the guides);
  - the four CI commands;
  - the TDD rule;
  - fixtures are real formats;
  - synthetic names;
  - ADR process;
  - PR template.
- `.github/PULL_REQUEST_TEMPLATE.md` and `.github/ISSUE_TEMPLATE/` (bug, port request,
  rule contribution).
- `CODE_OF_CONDUCT.md` (Contributor Covenant, by reference/link).
- **Review:** have a fresh subagent (or the user) follow `CONTRIBUTING.md` from a clean
  clone. Every step must work as written. Record the result.

### 14.B Plugin SDK
- A crate `ummp-plugin-sdk` (workspace member). It has:
  - the protocol types (serde) from `docs/PLUGIN_PROTOCOL.md`;
  - a `run(adapter: impl PluginAdapter)` main loop that handles stdin/stdout;
  - helpers to build edits.
- `templates/plugin/`, a minimal adapter plugin that uses the SDK and renames
  `DemoWidget` → `DemoGadget`. Its tests run in CI.
- Rewrite the existing `docs/PLUGIN_SDK.md` (Phase 1 architecture notes) and fold in
  `docs/ADAPTER_SDK.md` (Phase 9 in-tree adapters), with `doc-coauthoring`: concepts, a tutorial (build, test
  and install the template), the protocol reference, security rules (plugins are untrusted;
  UMMP validates everything).
- **Tests:**
  - ★ the template plugin, built in CI, is loaded by `ummp plugins check` and its edit is
    applied through the engine
  - ★ SDK protocol types round-trip with the server-side types; share one definition or
    test against golden JSON

### 14.C Community knowledge base
- **Schema:** versioned TOML files, one per rule set. They hold mappings, compatibilities,
  `[[replacement]]` and config rules, using the same shapes as the embedded TOML files so
  the core needs no second parser. Each has a `schema = 1` header.
- **`ummp kb validate <dir>`** checks:
  - the schema;
  - duplicates and conflicts with the built-in rules;
  - names that match a deny-list pattern of real APIs in test data (warning only);
  - every rule has a test case (`[[case]] input/expected`), which is run.
- **`ummp kb import <dir|url-of-release-asset>`** imports into the local KB. It shows a
  summary and asks for confirmation; `--yes` exists for CI.
- **Template repo layout** for `ummp-knowledge`: `rules/`, `cases/`, a CI workflow that
  installs the release binary and runs `ummp kb validate`. Create the repo only after the
  user agrees, with `add_repo`/`gh api`.
- **Tests:**
  - ★ `validate_rejects_rule_without_case`
  - ★ `validate_runs_cases_and_reports_failure_with_file_line`
  - `import_requires_confirmation`
  - `schema_version_mismatch_is_refused`

### 14.D Recipes
- A recipe is a named, shareable set of options plus rules for a common migration (for
  example "Forge 1.20.1 → NeoForge 1.21.1, keep Mixin config"). The file is
  `recipe.toml`.
- **Commands:**
  - `ummp recipe list`
  - `ummp port --recipe <name|file>`
  - `ummp recipe validate`
- Recipes go through the same review as KB rules (14.C), using the `review-pr` checklist.

### 14.E Plugin index
- `plugins.toml` in `ummp-knowledge`. Each entry has name, repo, version, sha256 of the
  release asset, `ummp_api`.
- **`ummp plugins search|install <name>`:**
  - downloads the asset;
  - checks the sha256;
  - shows the manifest and asks before installing;
  - never runs anything during install.
- **Tests** (fake server):
  - ★ hash mismatch refused
  - install asks for confirmation
  - incompatible `ummp_api` refused

### 14.F Issue reporting
- `ummp report-issue <session>` builds a bug report:
  - UMMP version, OS, command;
  - the report markdown;
  - the log tail with paths anonymized (home dir → `~`) and secrets redacted.
- It **shows the full text and asks** before opening the browser on a pre-filled GitHub
  issue URL, or before creating the issue with `gh` if the user prefers. It never attaches
  mod source code.
- **Tests:**
  - ★ redaction (token-like strings, home paths)
  - ★ nothing is sent without confirmation (a fake opener records calls)

### 14.G Community regression testing
- A scheduled workflow (nightly) in the main repo:
  - ports every case in `ummp-knowledge/cases` and the fixture mods;
  - builds them on GitHub runners;
  - posts a summary to the job summary;
  - opens or updates a tracking issue on regressions (only if the user enables it).
- Keep it outside the required PR checks.

### 14.H Documentation website
- mdBook in `book/` built from `docs/` (`SUMMARY.md`): user guide (en/vi), CLI, API, MCP,
  plugin SDK, KB contributions, ADRs, architecture.
- Publish with GitHub Pages via a workflow on master. Check the links in CI
  (`mdbook build` + a link checker).

## 5. Docs (same PR(s))
- ADRs: KB schema/versioning, plugin index trust model, report-issue privacy.
- Phase file, STATUS (all phases done → project status "1.0"), CHANGELOG, README (links to
  the book, contributing, plugins), CLAUDE.md, KNOWN_LIMITATIONS, BACKLOG.
- Propose to the user to split into PRs:
  - 14.A + 14.B;
  - 14.C + 14.D;
  - 14.E + 14.F;
  - 14.G + 14.H.

## 6. Risks
- **Supply chain:** community plugins and rules are untrusted. Hashes, confirmations,
  process isolation and engine-only writes are the defenses; document them in
  `docs/SECURITY.md`.
- **Real Minecraft names in community data** are allowed in the community KB (that is its
  purpose), but never in this repo's tests and fixtures. Say so in `CONTRIBUTING.md`.
- **Maintainer load:** keep validation automatic so that review is about intent, not
  format.

## 7. Backlog candidates
- a web UI for browsing recipes
- signing of plugin releases (minisign/sigstore)
- translations of the book beyond en/vi

## 8. Definition of done
README §7, plus:
- a contributor can follow `CONTRIBUTING.md` from a clean clone;
- the template plugin works end to end;
- `ummp kb validate` guards the community repo;
- the book is published;
- the user has reviewed every user-facing text.
