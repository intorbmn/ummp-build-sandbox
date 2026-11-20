# ADR 0004 — Tauri for the desktop UI

**Status:** Accepted, implementation deferred to Phase 11

## Context
A desktop UI is planned, but the CLI comes first (MVP is CLI-only).

## Problem
Which UI technology keeps the Rust core reusable and the bundle small?

## Options
1. Tauri (Rust backend, web frontend)
2. Electron
3. Native Rust GUI (egui, iced)

## Decision
Tauri. The UI calls application services only; it never contains migration logic.

## Consequences
- The same application services serve CLI, UI and a future REST/MCP API.
- Requires a web frontend stack (chosen in Phase 11).

## Alternatives rejected
- **Electron:** large bundle, second runtime.
- **Native Rust GUI:** more UI code to write; no reuse of web components such as a diff viewer.
