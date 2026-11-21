# ADR 0005 — AI never writes to a project directly

**Status:** Accepted (Phase 1, enforced from Phase 7)

## Context
AI agents will propose patches. A wrong patch can silently break mod logic (risks R11, R12).

## Problem
How do we let AI help without letting it corrupt a user project?

## Options
1. AI agents receive write access to the project directory
2. AI agents return proposed edits; a gate applies them

## Decision
Option 2. Agents return edits in the same `FileEdit` form the migration engine uses.
Edits are applied only by the migration engine, after a checkpoint, and only once
approved by a human (or a policy the user has set explicitly).

## Consequences
- Every AI change has a diff, a checkpoint and a rollback path, exactly like a
  rule-based change.
- The AI layer needs no file-system access at all.

## Alternatives rejected
- **Direct write access:** impossible to audit, and no guaranteed rollback.
