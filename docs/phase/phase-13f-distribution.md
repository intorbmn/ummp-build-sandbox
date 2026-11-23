# Phase 13.F — Distribution

## Decision
- Ship the standalone `ummp` CLI from GitHub Releases; MCP users do not need Rust.
- Pin cargo-dist 0.34.0 in `dist-workspace.toml` and generate the release workflow.
- Targets: Windows x64 (MSVC), Linux x64 (GNU), macOS ARM64.
- Formats/installers: Windows ZIP + MSI, shell + PowerShell installers, Linux/macOS tar.gz.
- Code signing is not configured. Windows SmartScreen and macOS Gatekeeper warnings remain expected.
- The desktop Tauri installer remains a separate manual workflow and is not bundled into the CLI release.

## Release process
1. Review the changelog and all crate versions.
2. Run the generated cargo-dist plan on pull requests.
3. On an approved release commit, create a pre-release tag such as `v1.0.0-rc.1`.
4. Let `.github/workflows/release.yml` build, checksum and attach platform artifacts to GitHub Releases.
5. Download artifacts manually when the environment cannot retrieve GitHub release assets; record the workflow run ID.
6. Install the Windows ZIP/MSI on a clean Windows x64 machine and run `ummp --version`, `ummp mcp`, and the demo mod port smoke test before declaring the RC verified.

## Current evidence and limits
- Workspace CLI/core/providers/server and the Tauri Rust crate are versioned `1.0.0-rc.1`; the root `Cargo.lock` and desktop lockfile are tracked and updated.
- `dist-workspace.toml` pins cargo-dist 0.34.0 and configures the approved targets/installers.
- Release workflow is generated from cargo-dist 0.34.0 and runs its plan on pull requests.
- No release tag has been pushed and no binary artifact has been installed by a user. This PR therefore delivers release configuration, not a verified release.
- Windows code signing and macOS notarization remain out of scope until credentials are provided.

## Acceptance
- [x] cargo-dist plan passes on the PR (integrated Release plan run [#67](https://github.com/intorbmn/Universal-Minecraft-Mod-Porter/actions/runs/45117081636)).
- [ ] RC dry-run/pre-release workflow completes and artifacts/checksums are attached.
- [ ] User downloads and installs the Windows artifact on a machine without Rust.
- [ ] `claude mcp add ummp -- <path>\\ummp.exe mcp` connects successfully.
- [ ] Demo mod port smoke test passes.
