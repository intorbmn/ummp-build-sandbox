# ADR 0018: Project license — Apache-2.0 core, separate proprietary Pro

**Status:** Accepted  
**Date:** 2026-11-21  
**Scope:** whole repository; monetization (see `docs/BACKLOG.md`)

## Context

The user chose to fund UMMP with voluntary support (early access for supporters) and an
open-source core plus a paid Pro edition, and asked Claude to pick the license. There was
no `LICENSE` file; the Rust crates declared `MIT` in `Cargo.toml` without a license text.
The user is the sole copyright holder, so relicensing needs no outside consent.

## Options

- **MIT:** simplest, but no explicit patent grant and no trademark clause.
- **Apache-2.0:** permissive like MIT, adds an explicit patent grant and says the license does
  not grant rights to the project's names/trademarks (section 6). Common for Rust tools.
- **AGPL-3.0 (+ dual license):** stops others from shipping closed forks or hosted services,
  but scares off contributors and mod authors and makes the Pro split harder to reason about.

## Decision

The public repository is licensed **Apache-2.0**. Pro features live in separate code (a
separate private repository or crate) under a proprietary license and are not committed
here. The name "UMMP" / "Universal Minecraft Mod Porter" is kept for official builds;
forks may use the code but not present themselves as the official product.

## Consequences

- `LICENSE` holds the Apache-2.0 text; every crate declares `license = "Apache-2.0"`.
- Anyone may fork the core, including commercially. The Pro edition must earn its price
  with features and convenience, not by locking the core.
- External contributions are accepted under Apache-2.0 (inbound = outbound); no CLA for now.
  Revisit if Pro ever needs to include contributed core code under other terms.
- The license covers UMMP's own code only. It grants nothing for Minecraft, Mojang code, or
  mods users port; users remain bound by the Minecraft EULA and each mod's license.
- Not legal advice; the user may review it with someone qualified before the first release.
