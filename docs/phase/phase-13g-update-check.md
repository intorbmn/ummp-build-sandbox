# Phase 13.G — Update check

## Behaviour
- `ummp update --check` queries the latest stable release at the project's GitHub Releases API endpoint.
- The CLI prints the newer tag and release URL; it never downloads, installs or replaces binaries.
- Automatic checks are opt-in via `check_updates = true` in `ummp.toml` or `UMMP_CHECK_UPDATES=true`.
- When enabled, the automatic check is throttled to once per 24 hours. Network and malformed-response errors are silent in the background.
- No telemetry or user identifiers are sent. The request uses a generic User-Agent and GitHub's public API.

## Tests
- Newer stable release is reported from a local fake HTTP server.
- Same and older releases are not reported.
- Stable 1.0.0 is newer than 1.0.0-rc.1.
- Unreachable endpoint returns an error for explicit checks.
- Configuration defaults to disabled and supports explicit opt-in.

## Acceptance
- [x] Stable CI passes provider tests, CLI tests, Clippy and formatting (latest integrated run [#971](https://github.com/intorbmn/Universal-Minecraft-Mod-Porter/actions/runs/45117075173)).
- [ ] Verify `ummp update --check` against GitHub Releases.
- [ ] Verify automatic checks make no request when disabled and are throttled when enabled.
