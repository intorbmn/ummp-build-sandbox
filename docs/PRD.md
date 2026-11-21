# PRD — Universal Minecraft Mod Porter (UMMP)

## Product
A desktop/CLI tool that analyzes Minecraft mod projects, plans cross-version / cross-loader migrations, applies transformations, builds, and verifies the result — with an AI assistant for ambiguous cases.

## Goals
- Migrate mods between Minecraft versions, mod loaders (Forge, Fabric, NeoForge, Quilt, Bukkit/Paper), and platforms.
- Reduce manual porting effort from days to hours.
- Provide evidence (build, test, launch) that the migrated mod actually works.

## Non-goals (v1)
- Modpack hosting or distribution.
- Real-time multiplayer testing automation.
- Decompilation as a first-class shipped feature (wrap existing tools).
- Supporting every Minecraft version simultaneously.

## Users
- Mod developers porting across versions/loaders.
- Modpack maintainers updating large mod lists.
- CI pipelines running automated port checks.

## Key features (phased)
1. Project import & analysis (structure, loader, version, deps, build system).
2. Compatibility knowledge base (version/loader mappings, breaking changes, recipes).
3. Migration pipeline: detect → plan → transform → build → test → review → export.
4. AI agent system for analysis, patching, debugging, review (with human approval gate).
5. Build & test engine (Java/Gradle/Maven runner, game launcher, log/crash analysis).
6. Plugin/adapter system for new technologies without touching core.

## Success metrics
- A representative test corpus of mods migrates end-to-end with build+launch evidence.
- Migration sessions are checkpointable and rollbackable.
- AI-generated changes are diffed and require approval before writing.