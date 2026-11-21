# Local REST API (Phase 12.B)

\`ummp serve\` starts an Axum service backed by the same \`AppServices\` used by desktop and CLI. Core remains synchronous; handlers use \`tokio::task::spawn_blocking\`.

## Start

\`\`\`sh
ummp serve --port 7432
\`\`\`

The default bind is \`127.0.0.1\`. UMMP creates \`<data_dir>/server-token\` (0600 on Unix) and prints the bearer token once. Keep it private. Every route except \`GET /v1/health\` requires:

\`\`\`http
Authorization: Bearer <token>
\`\`\`

A non-loopback bind is refused unless both \`--listen 0.0.0.0\` and \`--i-understand-remote\` are supplied. CORS is disabled and request bodies are limited to 1 MiB. Token rotation is not a CLI feature in this milestone; stop the server and remove \`server-token\` to generate a new token on next start.

## Allowed paths

\`ummp.toml\` may contain \`allowed_roots = ["/path/to/mods"]\`. If omitted, only the current user's home directory is allowed. Every request path is canonicalized, symlinks are resolved, and the result must be within one configured root. Relative roots resolve from the process working directory. Windows comparisons are case-insensitive.

## Routes

All routes are under \`/v1\`. Only health is public.

| Method | Route | Body / result |
|---|---|---|
| GET | \`/health\` | Public health status |
| POST | \`/analyze\` | \`{path}\` → \`AnalysisView\` |
| POST | \`/scan\` | \`{path}\` → \`RiskView\` |
| POST | \`/plan\` | \`{path, from?, to, loader?, min_confidence?}\` → \`PlanView\` |
| POST | \`/port\` | \`PortRequest\` → \`{job}\` |
| POST | \`/migrate\` | \`PlanRequest + reviewed:true\` → \`{job}\` |
| POST | \`/pack\` | \`PackRequest\` → \`{job}\` |
| POST | \`/build\` | \`BuildJobRequest + accept_risk\` → \`{job}\` |
| GET | \`/jobs/{id}\` | \`JobView\` |
| GET | \`/jobs/{id}/events\` | SSE stream with desktop-compatible event JSON |
| POST | \`/jobs/{id}/cancel\` | \`202 Accepted\` |
| POST | \`/sessions/{project}/rollback\` | \`{checkpoint?}\` → restored checkpoint |
| GET | \`/openapi.json\` | OpenAPI 3.0 document |

\`/v1/migrate\` refuses requests unless \`reviewed\` is \`true\`. \`/v1/plan\` never writes. Build requests preserve the core risk gate: \`accept_risk\` acknowledges High risk; Medium-risk local builds still require \`confirm_medium\`. Jobs remain queryable as long as core retains them in \`jobs.db\`.

There are no AI routes. Credentials and server tokens are never returned in API responses or logs. The source-of-truth OpenAPI document is \`docs/openapi.json\`; integration tests assert that its path set matches the router.
