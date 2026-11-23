# ADR 0008 — Rename simple names after a package path

**Status:** Accepted (Phase 6). Supersedes the "after a dot" rule of ADR 0007.

## Context
ADR 0007 renamed a simple-name rule (`DemoWidget -> DemoGadget`) after a `.` only inside
`import` statements. That missed fully qualified uses in code, such as
`com.example.DemoWidget.create()`.

## Problem
Without type information, `x.DemoWidget` may be a type reached through a package or a
member of an object. When should the name after the dot be renamed?

## Options
1. Only in imports (ADR 0007)
2. When everything before the name is lowercase, i.e. looks like a package path
3. Always

## Decision
Option 2, chosen by the maintainer. `should_rewrite` receives the dotted `qualifier`
before the name and renames when every segment of it starts with a lowercase ASCII letter.

## Consequences
- `com.example.DemoWidget` is renamed; `Outer.DemoWidget` and `foo().DemoWidget` are not.
- A lowercase variable (`x.DemoWidget`) cannot be told apart from a package and is
  renamed too. Prefer qualified mappings where this matters.

## Alternatives rejected
- **Imports only:** misses qualified references in code.
- **Always:** renames unrelated members that share the name.
