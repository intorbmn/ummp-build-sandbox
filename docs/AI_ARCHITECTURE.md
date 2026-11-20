# AI_ARCHITECTURE — AI Agent System

## Design principle
AI assists; it never writes directly. Every AI-generated change is a diff requiring human approval, with checkpoint and rollback.

## Components
| Component | Purpose |
|---|---|
| AI Provider Abstraction | `IAIProvider`: chat, tool calling, streaming, cost. Pluggable (OpenAI-compatible, local, etc.). |
| Model Router | picks model per task (cheap for classify, strong for patch). |
| Tool Calling | agents invoke UMMP tools (analyze, search knowledge, propose patch). |
| Context Management | trim/summarize context per agent; token budget. |
| Memory Management | short-term session memory; long-term persisted learnings. |
| Prompt Versioning | prompts are versioned assets; changes are tracked. |
| Structured Output | agents return typed results (JSON schema), not free text. |
| Agent State | each agent tracks its state; resumable. |
| Human Approval | write-actions require approval gate. |
| Token & Cost Tracking | per-session, per-agent, per-task cost accounting. |
| Retry and Fallback | transient errors retry; provider failover. |
| AI Evaluation | eval suite of analysis/patch tasks with scored outcomes. |
| Audit Trail | every AI action logged with prompt, tool calls, output, approval. |

## Agents (phased)
| Agent | Phase | Responsibility |
|---|---|---|
| Orchestrator | 7 | route tasks, manage state, aggregate results. |
| Analysis | 7 | interpret project, detect issues, suggest plan. |
| Migration | 6-7 | propose source/build transformations. |
| Debug | 8 | analyze logs/crashes, suggest fixes. |
| Review | 7 | review diffs for correctness before approval. |
| Mixin / Rendering / Native / Testing | 9+ | specialized agents for those tech areas. |

## Implemented (Phase 7, ADR 0010)
| Requirement | Where |
|---|---|
| Provider abstraction | `core::ai::AiProvider`; `ummp-ai-providers`: `AnthropicProvider`, `OpenAiCompatibleProvider` |
| Structured output | JSON schemas in `ai/agents.rs`; Anthropic `output_config.format`, OpenAI `response_format` |
| Model routing | `AiConfig.agents` (model, effort, max_tokens per agent) |
| Context management | `build_context`: character budget, whole files omitted, never truncated |
| Prompt versioning | `PROMPT_*` constants, recorded in each audit entry |
| Human approval | `ApprovalGate`: CLI prompt, `--approve-reviewed`, `DenyAll` when not a terminal |
| Cost tracking | `CostTracker` per agent and model; prices from config only |
| Retry and fallback | exponential backoff on transient errors, then `fallback_model` |
| Evaluation | `core/tests/ai_eval.rs` (scripted provider) |
| Audit trail | `<snapshots>/ai-audit.jsonl` |

Not built: tool calling (agents get context up front), long-term memory, streaming.

## Guardrails
- AI outputs are proposals only.
- File writes go through the checkpoint/rollback system.
- All writes are diffed and logged.
- AI cannot modify its own prompts or approval rules.