# SECURITY — Security Design

## Principle
Every JAR, repository, plugin, and file input is untrusted until verified.

## Controls
| Area | Control |
|---|---|
| Threat modeling | identify per-step attack surface (import, build, AI) before implementation. |
| Zero Trust | plugins and external code run with least privilege; no implicit trust. |
| Sandboxing | builds/tests/imports run in isolated processes; resource limits (CPU, memory, time, disk). |
| Permission model | plugins declare permissions; core enforces. |
| Secret management | API keys in env/secret store, never in config files or logs. |
| Dependency scanning | scan downloaded deps for known issues; verify checksums. |
| Artifact verification | verify mod JAR signatures where available; flag unsigned. |
| Supply chain | pin toolchain versions; reproducible build records. |
| Safe file handling | path traversal checks, size limits, no writes outside project/workspace. |
| Network isolation | network access only to declared endpoints; block by default. |
| Audit logging | log all writes, AI actions, approvals, rejections. |

## AI safety
AI proposals are diffs; writes require human approval; all AI actions are audited; AI cannot modify its own rules.

## Incident response
Diagnostic reports include security-relevant events; secrets never appear in logs.