# Phase 8 design — Build & Testing Engine + malware risk scan

## Intent
After a migration, the user wants to know whether the mod actually builds, passes its tests
and starts a server, for their own mods and for mods from other sources. Builds run on
GitHub when possible (isolated, has internet), otherwise on the user's machine in a new
folder under `D:\Claude`. Mods from other sources may be malicious, so UMMP scans them and
reports risk before anything runs. Anything not done goes into `docs/BACKLOG.md`.

## Goals
- `ummp build <path>`: build, unit tests, dedicated-server launch check, then a report with a
  success score (stages passed / stages run) and classified errors.
- Backends: GitHub Actions (default), local (`--local`).
- `ummp scan <path|jar>`: static malware-risk report. Runs automatically before every build.
- Log and crash analyzers usable on their own (and their output feeds `ai-debug --log`).

## Non-goals (go to `docs/BACKLOG.md`)
Client launch (needs a display/GPU), Maven, native builds, Docker/VM isolation for local
builds, downloading JDKs, running a mod's own integration tests in-game, dynamic/behavioral
malware analysis.

## Anti-goals
- Running a high-risk mod on the user's machine without an explicit, informed override.
- Claiming a mod is safe. The scan reports findings; "no findings" is not "safe".
- Leaking secrets to a build: no API keys in the build environment, no secrets in the
  workflow.

## Approaches (picked: C)
- A. GitHub only: safe and has internet, but needs GitHub and is slower.
- B. Local only: fast, but untrusted code runs on the user's machine.
- **C. Both behind one `BuildBackend` trait, GitHub by default.** The user picked this.

## Design
### Core (`core/src/build/`)
- `BuildBackend` trait: `run(&BuildRequest) -> Result<BuildOutcome>`. `BuildRequest` holds the
  project dir, the stages (`build`, `test`, `server`), the Java version and a timeout.
  `BuildOutcome` holds the result of each stage, the log paths and the artifact paths.
- `process::run_with_timeout`: spawns a child process with a scrubbed environment (keeps only
  PATH, JAVA_HOME, HOME/USERPROFILE, TEMP, SystemRoot), kills the whole process tree on
  timeout, and streams output to a log file.
- `LocalBackend`:
  - copies the project to `<workdir>/<mod>-<yyyymmdd-hhmmss>/`. The default workdir is
    `D:\Claude\ummp-builds` on Windows and `~/ummp-builds` elsewhere; `--workdir` overrides it.
  - runs `gradlew` (falling back to `gradle`) for `build`, then `test`, then `runServer`.
  - server stage: writes `eula=true` into the run dir. Success is a `Done (` line in the log
    within the timeout; the server is then sent `stop` and killed if it does not exit.
- `GithubBackend` (drives the `gh` CLI as a subprocess):
  - pushes the project plus a generated `.github/workflows/ummp-build.yml` to branch
    `ummp/<mod>-<timestamp>` of a build repo (`--repo owner/name`, default from config).
  - the workflow has `permissions: contents: read` and no secrets; it uses `setup-java` with the
    required version, runs the same stages and uploads logs plus test-results as an artifact.
  - UMMP polls the run, downloads the artifact, and produces the same `BuildOutcome`.
- Java toolchain: required version from the analyzer (fallback: from the target Minecraft
  version through the KB). Local: find a JDK via JAVA_HOME and common install paths and check
  it with `java -version`; if none matches, the build fails early with a clear message.
- `log_analyzer`: classifies Gradle output (compile error with file:line, missing symbol,
  dependency resolution, Java version mismatch, Mixin apply failure, timeout).
- `crash_analyzer`: parses Minecraft crash reports (description, top exception, suspected
  mods) and `latest.log` errors.
- JUnit XML from `build/test-results` gives the test counts.
- Report: Markdown/JSON with each stage, the score, the classified errors, and next steps
  (including the `ummp ai-debug --log …` command).

### Malware risk scan (`core/src/scan.rs`)
Static rules over source files, build scripts, and class files (constant-pool strings) in
JARs, nested JARs included up to a depth limit. Each finding has a severity, the rule, the
file and the evidence. Examples:
- **High:** process execution (`Runtime.exec`, `ProcessBuilder`) combined with network
  access; class loading from bytes or URLs (`defineClass`, `URLClassLoader`); access to the
  game session token; browser/Discord credential paths; webhook or paste-site URLs; build
  scripts that download and execute code; nested JARs or native libraries (.dll/.so/.dylib)
  that are not in the descriptor.
- **Medium:** heavy obfuscation (most class names 1–2 chars), large base64/encrypted blobs,
  reflection on string-built names, writing to startup folders or other mods' JARs.
- **Low:** plain network use, file deletion outside the game folder.

Overall risk is the highest severity found. Policy:
- **Local build:** High risk is refused unless `--i-accept-risk` is given (the report is
  shown first); Medium asks for confirmation.
- **GitHub build:** never blocked, only warned (the runner is disposable).

The rule set is data (a TOML file embedded at compile time) so it can grow without code
changes.

### CLI
`ummp scan`, `ummp build [--local] [--repo] [--workdir] [--stages] [--timeout]`.

## Testing
- Process runner: timeout kills a sleeping child process tree; env scrubbing (a test child
  prints its env).
- Local backend end to end: a small Gradle fixture with no plugins and no network (plain
  `java` plugin) builds and tests with the installed Gradle; the server stage runs against a
  fake server main class that prints `Done (` and exits on `stop`.
- GitHub backend: tests against a fake `gh` script (records its arguments, returns canned
  JSON and an artifact zip); the generated workflow is a golden file.
- Analyzers: real-format Gradle failure logs and crash reports (synthetic mod names) → golden
  classifications.
- Scan: fixture JARs built by `generate.py` with real `.class` files containing each risky
  pattern, plus a clean JAR; mutation-check a High rule.

## Calls made while writing (check these)
- The fixture defines its own `runServer` JavaExec task, so the server stage is tested
  without Minecraft or network access.
- JAR-only mods (no source) get `scan` only; `build` refuses them with a clear message.
  Dropping the JAR into a real server goes to the backlog.

## Left to the builder
Module boundaries inside `build/`, exact rule wording, report layout, the poll interval.

## Open (needs the user)
A real end-to-end GitHub run needs a build repo on the user's account (e.g.
`intorbmn/ummp-build-sandbox`, private or public). Until one exists, the GitHub backend is
tested only against the fake `gh`.
