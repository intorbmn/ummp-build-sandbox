# BACKLOG — not done yet, to revisit

Items deferred from finished phases. Each line says what is missing and why it was left.
Move an item into a phase file when it is picked up.

## Monetization (user decision, 2026-11-21) — after all phases are finished
- [ ] **Voluntary support with early access.** GitHub Sponsors / Ko-fi / Patreon; supporters
  get pre-release builds early.
- [ ] **Open source core + paid Pro edition.** Decide which features are Pro (candidates:
  batch modpack porting, continuously updated version rule sets, built-in AI without a
  user key) and how Pro is licensed/delivered.
- [ ] **Pick the project license first** (it constrains the Pro model: permissive MIT/Apache
  vs AGPL or dual licensing). Record it in an ADR before the first public release.
- [ ] Check the Minecraft EULA / Mojang commercial guidelines and mod licenses before charging:
  sell the tool only, never ported mods or Mojang code. Not legal advice; the user verifies.

## From the Windows app test (user, 2026-11-21) — after all phases are finished
The user's order: finish every planned phase first, then pick these up.
- [ ] **JAR input.** The app and CLI do not take a `.jar` as a port input: JARs only get a
  metadata-only analysis and closed JARs are never ported (KNOWN_LIMITATIONS). Decide what
  "support .jar" means (accept and analyze in the app, or decompile/remap) in an ADR first.
- [ ] **Downgrade.** Porting to an older Minecraft version is not supported. Add it as a
  migration direction with its own rules and tests.
- [ ] **More target versions.** Convert to many other Minecraft versions, not only the
  current pairs; chain steps when no direct rule set exists.
- [ ] **Heavy test mod.** Need a large real mod source (many files, long port) so a job runs
  long enough to test cancel by hand; cancel was not tested because jobs ended too fast.
  Keep it outside the repo (untrusted input, real names); a synthetic large fixture can
  be generated for CI.
- [ ] **More AI providers.** Settings offers only Anthropic and OpenAI(-compatible). Add other
  providers (for example Gemini, OpenRouter, Ollama/local) behind the `providers/` crate.
  Entering a key works (confirmed by the user).

## Phase 12 — CLI, API & Integration (planned)
- [ ] **Final AI key storage.** Phase 12.0 saves AI API keys to `credentials.json` behind
  the `AiKeyStore` trait, as an interim solution (user, 2026-11-20: "lưu vào file chỉ là
  tạm thời"). The user decides the final storage later. When they do, add a new
  `AiKeyStore` implementation, migrate existing keys, and supersede ADR 0015.

## Phase 11 — Desktop UI
- [ ] Capture and commit 2–3 real Tauri/Playwright screenshots after the first verified Windows
  launch; no synthetic mockups should be presented as runtime screenshots.
- [ ] Complete a manual Windows smoke test: MSI install/uninstall, NSIS install/uninstall,
  Credential Manager set/read/delete, file/folder picker, local and GitHub build risk gates,
  AI approve/reject, modpack export, output-folder opener, cancel, and rollback.
- [ ] Sign MSI/NSIS packages and publish checksums; current installers are unsigned.
- [ ] Add macOS and Linux packaging/release workflows and validate Keychain/Secret Service.
- [ ] Add auto-update and release-channel selection.
- [ ] Add custom OpenAI-compatible base URL, local Ollama, model selection, and optional
  prompt customization to Settings/AI assist.
- [ ] Reopen recent sessions into a complete job/event timeline (currently only a summary list).
- [ ] Replace bounded 200-line log tail with virtual scrolling and historical paging.
- [ ] Accessibility audit with keyboard-only, screen reader, high contrast, and reduced motion.
- [ ] Add drag-and-drop for JAR/source ZIP/project folders and a persistent recent-path picker.
- [ ] Test very large unified diffs and reports with real-world 1,000+ file projects.

## Phase 10 — Modpack Migration
- [ ] CurseForge API resolution of unresolved entries (needs an API key).
- [ ] Build and bundle ported mods automatically into the exported pack.
- [ ] Prism/MultiMC export refinements (icons, java settings, memory).
- [ ] Launch an exported pack: server start (`pack-server.yml` in the sandbox repo) and a
  client launch.
- [ ] KubeJS / CraftTweaker script migration; `pack_format` bumps for data and resource packs.
- [ ] Pack diff (old vs new) in the report.
- [ ] Add required dependencies of Update versions; report upper bounds of version ranges.
- [ ] Real replacement and config-rule data (today one synthetic sample each).

## Phase 9 — Advanced Technology Adapters
- [ ] Build templates for more Minecraft versions (only 1.21.1 today) and for Forge
  (Fabric → Forge converts descriptor/entry, but the build needs a ForgeGradle template).
- [ ] `quilt.mod.json` as a conversion **target** (Quilt is treated as Fabric).
- [ ] Automatic port of loader API semantics (events, registries, networking, config): today
  each use is listed as `assisted`; a KB of `<from>-><to>` mappings would cover common ones.
- [ ] Dependency id mapping between loaders (KB `kind = "dependency"`), e.g. API libraries.
- [ ] Rendering/shader/networking adapters beyond detection (need version-diff data).
- [ ] Native code build (C/C++/Rust) and JNI ABI checks; JAR-only native libraries.
- [ ] Kotlin entry classes: the bridge is Java and calls the Kotlin class; `ModInitializer`
  stripping is Java-only.
- [ ] Mixin refmap handling when mapping namespaces differ (intermediary/yarn ↔ Mojang).
- [ ] Access widener fields from AT (AT has no field descriptor; left as commented lines).
- [ ] `port` from a Git URL (today: folder or zip).
- [x] Demo `gradle-wrapper.jar` validated by `gradle/actions/wrapper-validation` on GitHub.
- [ ] Validate `gradle-wrapper.jar` checksums locally too (today: Medium finding, `--yes`).

## Phase 8 — Build & Testing Engine
- [x] Real GitHub run verified (2026-11-19, 3/3).
- [ ] Delete old `ummp/*` branches in the build repo automatically (they pile up today).
- [ ] Per-stage logs when artifact storage is blocked (fallback gives stage results only).
- [ ] Minecraft **client** launch check (needs a display/GPU; e.g. Xvfb on Linux runners).
- [ ] **Maven** builds (only Gradle is supported).
- [ ] **Native** builds (C/C++/Rust parts of a mod; JNI).
- [ ] **Docker/VM isolation** for `--local` builds (today: copy folder, scrubbed env, timeout).
- [ ] Download a missing **JDK** automatically (today: error with install hint).
- [ ] Build **JAR-only** mods by dropping them into a real server (today: `scan` only).
- [ ] Run a mod's own **in-game integration tests** (GameTest).
- [ ] **Dynamic/behavioral** malware analysis (today: static rules only).
- [ ] Scan rules: compare native libraries against what the mod declares; signature
  database of known malware families.
- [ ] Windows process-tree kill (`taskkill /T`) is implemented but only tested on Linux CI.
- [ ] Proxy support for local builds: the env allow-list drops `HTTPS_PROXY`/`JAVA_TOOL_OPTIONS`.

## Phase 7 — AI agents
- [ ] Streaming responses; live API test (no key in CI).
- [ ] Agent memory across sessions.

## Phase 6 — Migration engine
- [ ] Real mapping dataset in the knowledge base (risks R1/R4).
- [ ] Method/field renames in bytecode and Mixin injector targets (member mappings).


## Phase 11 — Desktop UI follow-up
- [ ] Replace the default Tauri icons with approved UMMP branding.
- [ ] Add screenshots after a verified Windows desktop launch.
- [ ] Record a Windows install and sample-mod smoke test before treating Phase 11 as accepted.
- [ ] Consider interrupting active Gradle/GitHub build processes; current job cancellation is checked between steps only.
