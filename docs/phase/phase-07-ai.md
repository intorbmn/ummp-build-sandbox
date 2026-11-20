# Phase 7 — AI Agent System

Skills used: `superpowers:brainstorming`, `claude-api:claude-api`,
`superpowers:test-driven-development`, `rust-cli-skills:*` (errors, idiomatic, testing),
`superpowers:verification-before-completion`.

## Acceptance criteria (master prompt §Phase 7)
Provider abstraction, structured output, context management, prompt versioning, human
approval, token/cost tracking, retry and fallback, AI evaluation, audit trail; Analysis,
Migration, Review agents (Debug from logs). AI output reaches files only via `FileEdit`s
applied by `MigrationEngine` after a checkpoint (ADR 0005/0006).

## Milestones
| ID | Milestone | Status | Evidence |
|---|---|---|---|
| 7.A | `AiProvider` trait, config, cost tracker, audit log, approval gates | COMPLETED | `core/src/ai/mod.rs` unit tests |
| 7.B | Agents: prompts, JSON schemas, context budget, proposal validation | COMPLETED | `core/src/ai/agents.rs` unit tests (escape, ambiguous match, all-or-nothing per file, whole-file omission) |
| 7.C | Orchestrator: retry/backoff, fallback model, budget, review + approval, checkpointed apply | COMPLETED | `core/tests/ai_eval.rs` (9 scripted eval cases; mutation-checked: approval gate and budget check) |
| 7.D | Providers: Anthropic (structured output, effort, fallback beta, OAuth) and OpenAI-compatible | COMPLETED | `providers/src/lib.rs` (5 tests incl. a real HTTP round trip that checks headers and that the key is not logged) |
| 7.E | CLI `ai-assist`, `ai-debug` | COMPLETED | Smoke test against a local fake API: analysis printed, `../escape.txt` rejected, 1 edit applied, audit has 4 lines and no key, `rollback` then `diff -r` identical |

## Phase 7 completion report
- **Implemented:** 7.A–7.E. Decision: ADR 0010.
- **Evidence:** 116 tests, clippy `--all-targets` and fmt clean on the pinned toolchain.
- **Not verified:** a live call to the real Anthropic API (no key in CI or the sandbox).
- **Known limitations:** `docs/KNOWN_LIMITATIONS.md` → AI agents.
- **Entry condition for Phase 8:** user review of Phase 7.
