# Universal Minecraft Mod Porter (UMMP)

A tool to analyze, plan, and migrate Minecraft mods across versions, mod loaders, and platforms.

## Status
Phases 0-10 complete; Phase 11 (desktop UI) is implemented on `phase-11-ui`; automated CI is green. Windows installer/runtime validation and native desktop screenshots remain pending. Rough designs for 10-14 are in [docs/drafts](docs/drafts/README.md). Deferred work: [BACKLOG.md](docs/BACKLOG.md). See
[PROJECT_STATUS.md](docs/PROJECT_STATUS.md) and [KNOWN_LIMITATIONS.md](docs/KNOWN_LIMITATIONS.md).

## Documentation
| Doc | Purpose |
|---|---|
| [PRD.md](docs/PRD.md) | Product requirements |
| [SAD.md](docs/SAD.md) | Software architecture |
| [ROADMAP.md](docs/ROADMAP.md) | Phase roadmap |
| [MODULES.md](docs/MODULES.md) | Module catalog |
| [DEPENDENCY_GRAPH.md](docs/DEPENDENCY_GRAPH.md) | Module dependencies |
| [TECH_STACK.md](docs/TECH_STACK.md) | Technology decisions |
| [PLUGIN_SDK.md](docs/PLUGIN_SDK.md) | Plugin architecture |
| [AI_ARCHITECTURE.md](docs/AI_ARCHITECTURE.md) | AI agent architecture |
| [MIGRATION_PIPELINE.md](docs/MIGRATION_PIPELINE.md) | Migration workflow |
| [DATABASE_SCHEMA.md](docs/DATABASE_SCHEMA.md) | Data model |
| [SECURITY.md](docs/SECURITY.md) | Security design |
| [TESTING_STRATEGY.md](docs/TESTING_STRATEGY.md) | Testing strategy |
| [RISK_REGISTER.md](docs/RISK_REGISTER.md) | Risk register |
| [PROJECT_STATUS.md](docs/PROJECT_STATUS.md) | Progress tracking |
| [phase-11-ui.md](docs/phase/phase-11-ui.md) | Phase 11 implementation and verification record |
| [ui/README.md](ui/README.md) | Desktop app setup and developer workflow |
| [guides/](docs/guides/README.md) | Step-by-step guides for the remaining phases (10–14) |
| [BACKLOG.md](docs/BACKLOG.md) | Deferred work |
| [KNOWN_LIMITATIONS.md](docs/KNOWN_LIMITATIONS.md) | Known limitations |
| [decisions/](docs/decisions/README.md) | Architecture Decision Records |
| [CLAUDE.md](CLAUDE.md) | Workflow rules and skills per phase for AI-assisted work |

## Desktop app preview (Windows)

The desktop shell is in active development. It currently provides the Home → Analysis → Target
→ Plan/Diff → Run → Report workflow, plus Settings, AI approval, modpack migration, and build
screens. It is not yet a signed production release; use the source build until the Windows
installer workflow has passed CI and a manual smoke test.

Requirements: Windows 10/11 x64, Node.js 22, Rust stable, Microsoft C++ Build Tools, and the
WebView2 runtime. From a clone:

```powershell
cd ui
npm ci
npm run tauri dev
```

Create MSI and NSIS installers with:

```powershell
npm run tauri build
```

Installers are emitted under `ui/src-tauri/target/release/bundle/`. The manual release workflow
is [Desktop release](.github/workflows/desktop-release.yml). Do not install an unsigned
installer on a production machine. Runtime screenshots and the Windows smoke-test record will
be added after the first verified Windows launch.

## CLI quick start
```sh
cargo build --release

# Detect loader, Minecraft/Java versions, dependencies, build system
ummp analyze path/to/mod            # add --json for machine-readable output

# Preview a migration (nothing is written)
ummp migrate path/to/mod --from 1.16.5 --to 1.17.1 --loader forge --mappings mappings.json

# Write it (a checkpoint is taken first), then undo if needed
ummp migrate path/to/mod --from 1.16.5 --to 1.17.1 --loader forge --mappings mappings.json --apply --report report.md
ummp rollback path/to/mod

# JAR-only mod: remap class names into a new JAR (input is not modified)
ummp remap-jar mod.jar mod-1.17.jar --from 1.16.5 --to 1.17.1 --loader forge --mappings mappings.json

# AI assistance (needs ANTHROPIC_API_KEY; proposals are reviewed, approved, then applied with a checkpoint)
ummp ai-assist path/to/mod --from 1.16.5 --to 1.17.1 --loader forge --mappings mappings.json
ummp ai-debug path/to/mod --log build.log --from 1.16.5 --to 1.17.1 --loader forge

# Malware-risk scan (exit code 2 on High risk); runs automatically before every build
ummp scan path/to/mod-or.jar
# Build + tests + dedicated-server start on GitHub Actions (needs `gh auth login` and a build repo)
ummp build path/to/mod --repo you/ummp-build-sandbox
# ...or on this machine, in a new folder under D:\Claude\ummp-builds (Windows) or ~/ummp-builds
ummp build path/to/mod --local

# Port a mod: source folder or .zip in, ported source out (the original is not touched).
# Output goes to D:\Claude\ummp-ports\<mod>-<mc>-<loader>-<time> on Windows, ~/ummp-ports elsewhere.
ummp port path/to/mod-src --to 1.21.1 --loader neoforge --zip
# Modpacks: .mrpack, CurseForge zip, Prism instance or folder. Output goes to
# D:\Claude\ummp-packs\<pack>-<mc>-<loader>-<time> on Windows, ~/ummp-packs elsewhere.
ummp pack inspect path/to/pack.mrpack
ummp pack migrate path/to/pack.mrpack --to 1.21.1 --loader neoforge [--online] [--sources DIR]
ummp pack export path/to/instance --to-format mrpack
# exit codes: 0 done, 2 refused (High-risk JAR; review it, then --allow-risk), 3 done, mods left to you
ummp adapters    # technologies UMMP detects or migrates (see docs/ADAPTER_SDK.md)
# local model: --provider openai --base-url http://localhost:11434/v1 ; settings: --ai-config ai.json
```
`--mappings` takes a JSON list of `{version, loader, from_api, to_api, kind, source, confidence}`
(the knowledge-base export format); `--kb` takes a knowledge-base SQLite file instead.
Checkpoints go to `<project>.ummp-snapshots/` next to the project unless `--snapshots` is set.

See [ROADMAP.md](docs/ROADMAP.md) for the development plan.