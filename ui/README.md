# UMMP desktop app

This folder contains the Tauri 2 desktop shell. The UI is a Svelte 5 + TypeScript + Vite
application; all domain logic remains in `ummp_core::app::AppServices`.

## Prerequisites

- Windows 10/11 x64 (the supported packaging target for this phase).
- Node.js 22 and npm.
- Rust stable with the components from the repository toolchain file.
- Microsoft C++ Build Tools (MSVC) and the WebView2 runtime.
- For Linux development/CI: GTK/WebKit development packages and `libdbus-1-dev`
  (the latter is required by the Secret Service keyring backend).

## Run in development

```powershell
cd ui
npm ci
npm run tauri dev
```

For a frontend-only preview:

```powershell
npm run dev
```

The frontend-only preview needs a mocked Tauri bridge for commands; it is not a substitute
for the native desktop app.

## Build installers

```powershell
cd ui
npm ci
npm run tauri build
```

The MSI and NSIS packages are written to:

- `ui/src-tauri/target/release/bundle/msi/`
- `ui/src-tauri/target/release/bundle/nsis/`

The manually dispatched Windows workflow is
[desktop-release.yml](../.github/workflows/desktop-release.yml). These installers are
currently unsigned. Verify the SHA-256 and install only on a test machine until code signing
and the manual Windows smoke test are complete.

## Test

```powershell
cd ui
npm run check
npm run test
npm run e2e
npm audit --omit=dev
```

The desktop Rust crate is a separate Cargo workspace:

```powershell
cd ui/src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
```

The root workspace remains independent:

```powershell
cd ..
cargo fmt --check
cargo clippy --all --all-targets -- -D warnings
cargo test --all
```

## Architecture rules

- `src/lib/api.ts` is the only frontend module allowed to call Tauri `invoke`.
- Long-running work is started by an application command and observed through the
  `ummp://event` job stream; screens never call core engines directly.
- Blocking filesystem/scan work is moved to Tauri's blocking pool.
- API keys and GitHub tokens are stored in the operating-system keyring. The UI can only
  set, delete, and query whether a credential exists; there is no command that returns it.
- The GitHub token is injected as `GH_TOKEN` into GitHub CLI/git child processes only.
- AI proposals remain read-only until the user explicitly approves each file. A/R are the
  keyboard shortcuts for approve/reject.
- In-place migration requires the review checkbox; porting to a new output folder is the
  default and leaves the source untouched.
- Build risk gates are enforced by the core service, not just by disabled UI buttons.
- Modpack reports paginate at 50 rows to keep large reports responsive.
- Both English and Vietnamese strings live in `src/locales/`; missing Vietnamese keys fall
  back to English.

See [Phase 11 implementation notes](../docs/phase/phase-11-ui.md) for the current verification
state and known gaps.
