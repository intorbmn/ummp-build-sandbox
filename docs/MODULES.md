# MODULES — Module Catalog

| ID | Module | Layer | Interfaces | Depends on | Status |
|---|---|---|---|---|---|
| CORE-01 | Project Manager | Application | IProjectManager | FileSystem, Metadata | COMPLETED |
| CORE-02 | File System Abstraction | Infrastructure | IFileSystem | — | COMPLETED |
| CORE-03 | Project Metadata | Domain | ProjectMeta | — | COMPLETED |
| CORE-04 | Migration Session | Application | ISession | Event Bus, Checkpoint | COMPLETED |
| CORE-05 | Event Bus | Infrastructure | IEventBus | — | COMPLETED |
| CORE-06 | Checkpoint / Rollback | Infrastructure | ICheckpoint | FileSystem | COMPLETED |
| CORE-07 | Job Persistence | Infrastructure | IJobStore | Database | COMPLETED |
| ENG-01 | Analyzer Engine | Application | IAnalyzer | JAR, Source, Build | COMPLETED |
| ENG-02 | Compatibility Engine | Application | ICompatibility | Knowledge Base | COMPLETED |
| ENG-03 | Migration Engine | Application | IMigration | Analyzer, Compatibility, Transform | COMPLETED |
| ENG-04 | Build Engine | Application | IBuild | Toolchain, Sandbox | PLANNED |
| ENG-05 | Testing Engine | Application | ITestRunner | Build, Launcher | PLANNED |
| AI-01 | AI Provider Abstraction | Infrastructure | IAIProvider | — | DONE (Phase 7) |
| AI-02 | Agent Orchestrator | Application | IAgent | AI-01, Context | DONE (Phase 7) |
| PLG-01 | Plugin Registry | Infrastructure | IPluginRegistry | — | PLANNED |
| PLG-02 | Plugin Loader | Infrastructure | IPluginLoader | PLG-01 | PLANNED |
| PLG-03 | Plugin SDK | — | SDK docs + API | — | PLANNED |
## Implementation map
| ID | Code |
|---|---|
| CORE-01 | `core/src/project.rs` |
| CORE-02 | `core/src/filesystem.rs` (`FileSystem`, `resolve_within`) |
| CORE-03 | `core/src/metadata.rs` |
| CORE-04 | `core/src/session.rs` |
| CORE-05 | `core/src/events.rs` |
| CORE-06 | `core/src/checkpoint.rs` (`snapshot`, `restore`) |
| CORE-07 | `core/src/jobs.rs`, `core/src/persistence.rs` |
| ENG-01 | `core/src/analyzer.rs` |
| ENG-02 | `core/src/knowledge.rs` |
| ENG-03 | `core/src/migration.rs` (planner, engine), `descriptor.rs`, `build_script.rs`, `resources.rs`, `mixin.rs`, `bytecode.rs` |
