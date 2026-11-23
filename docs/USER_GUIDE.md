# UMMP User Guide

UMMP creates a separate output copy for porting and pack migration. Keep a backup, read every report, and review changes before building. A successful command is not a gameplay compatibility guarantee.

## 1. Install

### Windows x64
1. Download the Windows ZIP or MSI from [GitHub Releases](https://github.com/intorbmn/Universal-Minecraft-Mod-Porter/releases).
2. Extract the ZIP or run the MSI. The CLI is ummp.exe.
3. In PowerShell, run:

~~~powershell
C:/tools/ummp/ummp.exe --version
C:/tools/ummp/ummp.exe --help
~~~

The release candidate is unsigned. SmartScreen may warn. Verify the release checksum and only run a build you trust.

### Linux x64 / macOS ARM64
Download the matching tar.gz archive, verify its checksum, extract it, then run ./ummp --version. Add the binary directory to PATH if desired.

### Claude Code MCP
Register the extracted CLI with Claude Code:

~~~powershell
claude mcp add ummp -- C:/tools/ummp/ummp.exe mcp
claude mcp list
~~~

MCP uses stdio. Let the MCP client manage the process; do not expose it as a network service.

## 2. Port a mod source project

UMMP ports source folders or source ZIPs, not closed JARs. A compiled JAR can be inspected and scanned, but cannot be structurally ported without source.

~~~sh
ummp analyze ./MyMod
ummp scan ./MyMod
ummp port ./MyMod --to 1.21.1 --loader neoforge
~~~

Optional inputs include --from 1.20.1, --output ./out/MyMod, --zip, --kb mappings.db or --mappings mappings.json. Run ummp port --help for the full option list.

Read UMMP_REPORT.md in the output folder. Resolve manual items and review every changed file before building. Loader conversion is partly structural; unsupported loader APIs may remain manual work.

## 3. Inspect and migrate a modpack

Inspect without writing:

~~~sh
ummp pack inspect ./MyPack.mrpack
~~~

Migrate to a target version and loader:

~~~sh
ummp pack migrate ./MyPack.mrpack --to 1.21.1 --loader neoforge --format mrpack,folder
~~~

Add --online for read-only Modrinth version lookup. Supply mod source projects with --sources ./mod-sources. UMMP does not download mod JARs; unresolved updates and replacements remain in the report. Review UMMP_PACK_REPORT.md, especially risk, missing dependencies, side mismatches and manual actions.

## 4. Read the report

Check source and target versions/loaders, changed files and diffs, confidence and rationale, warnings and skipped files, manual items, unresolved dependencies, risk findings, and build/test/server results if requested.

A successful command means the defined pipeline completed. It does not prove gameplay correctness or that every mod API was migrated.

## 5. Build backends

The default backend uses GitHub Actions:

~~~sh
ummp build ./ported-mod --repo YOUR_GITHUB_USER/ummp-build-sandbox
~~~

Authenticate with gh auth login and use a repository you control. UMMP creates a build branch and reports build, test and dedicated-server stages.

For a local build:

~~~sh
ummp build ./ported-mod --local
~~~

Local build scripts run with your OS user permissions. Prefer GitHub builds for untrusted projects. A local Medium-risk build requires --yes; High-risk local builds require --i-accept-risk. Do not bypass a refusal unless you reviewed the finding and accept the consequences.

## 6. AI assistance

Set a provider key using hidden terminal input:

~~~sh
ummp ai-key set --provider anthropic
ummp ai-key status
ummp ai-assist ./MyMod --from 1.20.1 --to 1.21.1 --loader neoforge --mappings mappings.json
~~~

Review every diff. UMMP applies only changes that pass validation and approval, with a checkpoint before writing. --approve-reviewed skips the prompt only for changes approved by the review agent; it does not approve rejected proposals. AI output can be wrong or incompatible.

For build/crash-log suggestions:

~~~sh
ummp ai-debug ./MyMod --log build.log --from 1.20.1 --to 1.21.1 --loader neoforge
~~~

## 7. Roll back or recover

Restore the latest checkpoint after an applied migration:

~~~sh
ummp rollback ./MyMod
~~~

Recover an interrupted migration using the session UUID printed before the migration waits:

~~~sh
ummp recover ./MyMod --session SESSION-UUID
~~~

Phase 13E recovery is rollback-only. It does not resume or replay migration. Snapshot exclusions such as .git, build output and configured skip directories are outside rollback guarantees. Keep a separate backup.

## 8. Update checks

~~~sh
ummp update --check
~~~

This prints a newer release tag and URL; it never downloads or installs anything. Automatic checks are disabled by default. Opt in with check_updates = true in ummp.toml or UMMP_CHECK_UPDATES=true. When enabled, checks run at most once per 24 hours. Background network errors are silent; no telemetry is sent.

## 9. Troubleshooting

### Java version mismatch
Check java -version and ./gradlew --version (Windows: gradlew.bat --version). Install the JDK required by the target Minecraft/loader, set JAVA_HOME and ensure the Gradle toolchain agrees. As a rule of thumb, Minecraft 1.18–1.20.4 commonly uses Java 17 and 1.20.5+ uses Java 21; older targets may require older runtimes. Follow the exact loader/project requirements.

### Gradle wrapper or build failure
Run the wrapper --version command, confirm wrapper files exist, and inspect the first meaningful compiler/configuration error. Preserve logs before cleaning build output. Prefer GitHub builds for untrusted source because local Gradle scripts run with your user permissions.

### Maven Central / HTTP 429
A 429 is rate limiting, not necessarily a source-code error. Wait before retrying, avoid rapid repeated builds, and check whether the project uses a repository mirror or authenticated package proxy. Never put access tokens in build files or logs.

### Risk refusal
Run ummp scan PATH and inspect the finding and affected file. High risk is a safety gate, not a build failure. Do not bypass it just to make the command pass. If you consciously accept the risk, use only the acknowledgement documented by that command and keep the report.

### No source / closed JAR
UMMP can inspect and scan a JAR, but migration requires a source project. Obtain source from its maintainer or leave the mod as a manual item.

### Update check cannot reach GitHub
The manual command reports the failure. Automatic checks fail silently and retry no more than once per day. Check network/proxy access to api.github.com; UMMP never installs updates in the background.

## 10. Important limitations

- Loader conversion is structural; many loader-specific APIs still require manual changes.
- A server-start check is not a gameplay test.
- Static risk scanning cannot prove a mod is safe.
- Closed JARs are not ported and UMMP does not download mod binaries.
- Recovery is rollback-only and excludes configured snapshot skip directories.
- Unsigned releases may trigger SmartScreen or Gatekeeper warnings.
