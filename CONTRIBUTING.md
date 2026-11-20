# Contributing to Universal Minecraft Mod Porter

## Development Setup

```bash
# Install Rust toolchain (stable)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone the repo
git clone https://github.com/intorbmn/Universal-Minecraft-Mod-Porter.git
cd Universal-Minecraft-Mod-Porter

# Build
cargo build --all

# Run tests
cargo test --all

# Check formatting and linting
cargo fmt --all --check
cargo clippy --all -- -D warnings
```

## Project Structure

- `core/` — Core engine (domain types, filesystem abstraction, event bus, session/job management)
- `cli/` — Command-line interface entry point
- `docs/` — Architecture documents, roadmap, per-phase specs

## Coding Standards

- `#[forbid(unsafe_code)]` — No unsafe blocks
- Clippy pedantic + nursery lints enabled
- 100-line source limit per file (documented, enforced in PR review)
- Follow modular monolith layer rules documented in `docs/SAD.md`

## Commit Messages

Use Conventional Commits:

```
feat: add new migration step
fix: resolve path resolution bug
docs: update phase 2 status
test: add unit tests for job store
chore: update CI workflow
```

## Pull Requests

1. Fork and create a feature branch
2. Ensure all tests pass: `cargo test --all`
3. Run `cargo clippy --all` with no warnings
4. Update relevant documentation if changing behavior
5. Link any related issues

## Architecture Rules

See `docs/SAD.md` for strict layering rules:
- Domain → Application → Infrastructure (no backward arrows)
- No module coupling (use traits/interfaces)
- Sandbox untrusted JARs before processing
