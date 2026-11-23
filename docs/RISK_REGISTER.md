# RISK_REGISTER

| # | Risk | Probability | Impact | Severity | Mitigation | Fallback | Owner |
|---|---|---|---|---|---|---|---|
| R1 | Minecraft API changes between versions | High | High | Critical | Versioned mapping DB; re-run analyzer on new versions; community updates | Manual review mode; flag unmapped APIs | Compatibility Engine |
| R2 | Missing source code (JAR-only mods) | High | High | High | Bytecode/decompiler adapter; IR from bytecode; document limitations | Report gaps; skip with warning |
| R3 | Obfuscated JARs | High | Medium | High | Use official mappings where available; Mojang MCP; accept partial | Flag obfuscated symbols as unknown |
| R4 | Incomplete mappings | Medium | High | High | Multiple mapping sources; confidence scoring; human review for low-confidence | Mark unknown; do not auto-transform |
| R5 | Complex mixins | Medium | High | High | Mixin IR + pattern library; test on real mod corpus | Suggest manual migration |
| R6 | Rendering pipeline changes | Medium | Medium | High | Abstract rendering adapters; version-specific adapters | Disable rendering-related transforms |
| R7 | Incompatible native dependencies | Medium | High | High | Detect native libs; sandbox build; isolate failures | Skip native; report |
| R8 | Missing old loader docs | Medium | Medium | Medium | Community knowledge base; fallback to decompiled APIs | Flag as unverified |
| R9 | Custom classloaders | Low | High | High | Detect and flag; do not auto-migrate | Manual migration only |
| R10 | Deleted dependencies | Medium | Medium | High | Dependency graph; suggest replacements from compatibility DB | Report missing; continue |
| R11 | AI incorrect logic changes | Medium | High | Critical | Human approval gate; diff review; eval suite; rollback | Reject patch; flag for manual |
| R12 | AI regressions | Medium | High | Critical | Regression tests after every AI patch; rollback | Revert to checkpoint |
| R13 | High API cost | Medium | Medium | Medium | Model router (cheap model for classify); token budget; cache | Local/offline provider |
| R14 | Sandbox insufficient | Low | High | High | Process isolation; resource limits; audit | Run untrusted code only in CI, not desktop |
| R15 | Un-testable mods | Medium | Medium | Medium | Test corpus expansion; launch-based evidence where possible | Mark as unverified |
| R16 | Client/server differences | Medium | High | High | Run both client and server; compare logs | Flag divergence |
| R17 | License forbids redistribution | Medium | High | High | License detection; refuse to bundle; warn user | Export source only |