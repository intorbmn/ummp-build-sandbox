# ADR 0007 — Token-aware text rewriting for the first source transformer

**Status:** Accepted (Phase 6 MVP)

## Context
Phase 6 must rename APIs (`Mapping.from_api` → `Mapping.to_api`) in Java/Kotlin sources.
Mappings hold either simple names (`DemoWidget`) or fully-qualified names (`net.minecraft.block.Block`).

## Problem
How do we apply renames without a full Java/Kotlin parser?

## Options
1. Plain substring replace
2. Token-aware rewrite: match whole identifiers or dotted paths, skip string literals and comments
3. Full AST rewrite (tree-sitter / JavaParser via an adapter)

## Decision
Option 2 behind the `SourceTransformer` trait. It never matches inside a longer
identifier (`DemoWidget` does not touch `DemoWidgetEntity`). By default it leaves strings and
comments untouched, because those are often registry IDs or prose.

## Consequences
- No new heavy dependencies; deterministic and fast.
- It cannot resolve imports or types: a simple-name mapping renames every `DemoWidget`
  token in scope, whichever `DemoWidget` it refers to. To limit this, mappings below
  `MigrationPlanner::min_confidence` are reported, not applied.
- An AST transformer (option 3) can be added later as another `SourceTransformer`.

## Alternatives rejected
- **Substring replace:** corrupts longer identifiers (`DemoWidgetEntity` → `DemoGadgetEntity`).
- **Full AST now:** a large dependency and per-language adapters before the pipeline is proven.
