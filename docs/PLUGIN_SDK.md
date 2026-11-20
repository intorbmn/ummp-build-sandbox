# PLUGIN_SDK — Plugin Architecture

## Concepts
- **Adapter**: converts between systems/APIs (e.g. Fabric loader API → core interface).
- **Plugin**: extends application functionality (new AI provider, decompiler, mapping source).
- **Provider**: implements a service (AI, mapping, decompiler) behind an interface.
- **Migration Recipe**: reusable transformation rule set.

## Components
| Component | Purpose |
|---|---|
| Plugin API | Rust traits + versioned ABI plugins extend. |
| Plugin Registry | discover, register, validate, version-check plugins. |
| Plugin Loader | load at startup or dynamically; capability discovery. |
| Plugin Lifecycle | init → enable → run → suspend → disable → unload. |
| Dependency Manager | plugin-to-plugin deps, version constraints. |
| Capability Discovery | advertise what a plugin can do. |
| Compatibility Validation | check plugin vs core API version, loader, Minecraft version. |
| Permission System | declare required permissions; enforce at runtime. |
| Sandboxing | plugins run in isolated processes where feasible; resource limits. |
| Configuration | per-plugin config with schema validation. |

## Rules
- Plugins never touch core internals directly; they go through the registry and approved interfaces.
- A plugin may be disabled without affecting others.
- Plugin API is versioned; incompatible plugins are rejected with a clear error.

## SDK
See `plugins/sdk/` for the starter template and examples.