# ADR 0010 — AI agents return validated search/replace proposals through structured output

**Status:** Accepted (Phase 7)

## Context
ADR 0005 says AI never writes to a project. Phase 7 adds Analysis, Migration, Debug and
Review agents that call a model over HTTP. We need a provider abstraction, a way to get
typed output, and a safe path from model text to file changes.

## Problem
How does model output become a file change without trusting the model, and how do we keep
the core library free of HTTP and vendor code?

## Options
1. Tool calling with a forced `edit_file` tool
2. Free-text unified diffs parsed and applied
3. Structured output (JSON schema) with search/replace proposals, validated in Rust

## Decision
Option 3.
- `core::ai` defines `AiProvider` (sync `complete`) and all agent logic. HTTP providers live in
  the separate crate `ummp-ai-providers` (ureq): Anthropic Messages API and an
  OpenAI-compatible endpoint (local models).
- Anthropic: model `claude-opus-5-5` by default, `output_config.format` with a JSON schema
  (`additionalProperties: false`), explicit `effort`, no forced `tool_choice` (rejected by this
  model), server-side fallback beta on supported models. Refusal and `max_tokens` stops are errors.
- Each proposal is `{path, search, replace}`. Validation: the path must be one of the files sent
  in the prompt, must stay inside the project, and `search` must match exactly once. A file with
  any invalid proposal gets no edits at all.
- Then the Review agent, then an `ApprovalGate` (deny by default when not interactive), then
  `MigrationEngine::apply_edit_set`, which takes a checkpoint first (ADR 0006).
- Prompts are versioned constants (`analysis/1`, ...). File contents are marked as data and
  the system prompt tells the model to ignore instructions inside them.
- Every call is written to an audit JSONL; tokens and cost are tracked per agent and model,
  with prices only from config; a token budget stops the session.

## Consequences
- Edits are deterministic to apply and easy to diff; a hallucinated path or stale context
  fails validation instead of corrupting a file.
- Line-numbered diff edits are not possible; a repeated snippet needs more surrounding text
  to be unique.
- Core tests run with a scripted provider; no network in CI.
- No streaming yet: long requests rely on a 10-minute HTTP timeout.
