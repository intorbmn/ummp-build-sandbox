# ADR 0002 — Modular monolith with layered dependencies

**Status:** Accepted (Phase 1)

## Context
The product will grow many adapters (loaders, versions, languages, AI providers) but is
built by a small team and runs on one machine.

## Problem
How to split the system so new technologies can be added without rewriting the core,
without paying the operational cost of distributed services?

## Options
1. Modular monolith (one process, modules behind traits)
2. Microservices from day one
3. Unstructured single crate

## Decision
Modular monolith. Dependency direction: Presentation → Application → Domain;
Infrastructure implements Application/Domain traits. Domain modules (`metadata`,
`session`) have no I/O dependencies.

## Consequences
- One binary, one test run, no network between modules.
- Module boundaries are enforced by traits (`FileSystem`, `IAnalyzer`, `SourceTransformer`)
  and, once modules grow, by splitting into separate crates.
- A module can later be lifted into a service by putting a transport behind its trait.

## Alternatives rejected
- **Microservices:** deploy and debugging cost with no scaling need today.
- **Single unstructured crate:** fast at first, but GUI/AI/loader code would leak into the core.
