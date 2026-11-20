# ADR 0001 — Rust as the primary language

**Status:** Accepted (Phase 1)

## Context
UMMP reads untrusted JARs and repositories, drives external toolchains (Java, Gradle,
Maven, native compilers) and must ship as a desktop app on Windows, Linux and macOS.

## Problem
Which language should the core engine, CLI and desktop backend use?

## Options
1. Rust
2. Java/Kotlin (same ecosystem as the mods)
3. TypeScript/Node
4. Python

## Decision
Rust for core, CLI and the desktop backend.

## Consequences
- Memory safety and `unsafe_code = "forbid"` at the workspace level when parsing untrusted archives.
- Single static binary per platform; no runtime to install for the tool itself.
- JVM-side analysis (bytecode, decompilation) is done by external tools wrapped as adapters,
  not reimplemented in Rust.
- Contributors who only know Java face a learning curve.

## Alternatives rejected
- **Java/Kotlin:** convenient for bytecode libraries (ASM), but couples the tool to a JVM
  version and makes process isolation of the mod's own JVM harder to reason about.
- **Node:** weak story for sandboxing child processes; large desktop bundles.
- **Python:** runtime distribution and packaging pain on Windows.
