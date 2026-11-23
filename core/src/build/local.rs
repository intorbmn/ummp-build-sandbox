//! Runs a build on this machine, in a fresh copy of the project.

use super::java::find_jdk;
use super::logs::{analyze_log, parse_crash_report, ErrorKind, LogError};
use super::process::{run, scrubbed_env, ProcSpec, ProcStatus, Watch};
use super::{
    BuildBackend, BuildError, BuildOutcome, BuildRequest, Stage, StageResult, StageStatus,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Folders never copied into the build folder (VCS data and old build output).
const SKIP: &[&str] = &[".git", ".gradle", "build", "run", "out", "target"];

/// Builds in `<workdir>/<mod>-<yyyymmdd-hhmmss>/`.
#[derive(Debug, Clone)]
pub struct LocalBackend {
    pub workdir: PathBuf,
    /// Gradle to use when the project has no wrapper; `None` searches PATH.
    pub gradle: Option<PathBuf>,
}

/// `D:\Claude\ummp-builds` on Windows, `~/ummp-builds` elsewhere.
#[must_use]
pub fn default_workdir() -> PathBuf {
    if cfg!(windows) {
        return PathBuf::from(r"D:\Claude\ummp-builds");
    }
    std::env::var_os("HOME")
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
        .join("ummp-builds")
}

/// Creates `<workdir>/<name>-<timestamp>[-n]/`, never reusing an existing folder.
///
/// # Errors
/// I/O errors creating the folder.
pub fn new_build_dir(workdir: &Path, name: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(workdir)?;
    let base = format!("{name}-{}", chrono::Local::now().format("%Y%m%d-%H%M%S"));
    for n in 1..1000 {
        let dir = workdir.join(if n == 1 {
            base.clone()
        } else {
            format!("{base}-{n}")
        });
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("could not create a new build folder"))
}

/// Copies a project tree, skipping [`SKIP`] folders and symlinks.
///
/// # Errors
/// I/O errors.
pub fn copy_project(from: &Path, to: &Path) -> std::io::Result<()> {
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let ty = e.file_type()?;
        let dest = to.join(e.file_name());
        if ty.is_dir() {
            if !SKIP.contains(&e.file_name().to_string_lossy().as_ref()) {
                std::fs::create_dir_all(&dest)?;
                copy_project(&e.path(), &dest)?;
            }
        } else if ty.is_file() {
            std::fs::copy(e.path(), dest)?;
        }
    }
    Ok(())
}

fn which(name: &str) -> Option<PathBuf> {
    let exts: &[&str] = if cfg!(windows) {
        &[".bat", ".cmd", ".exe"]
    } else {
        &[""]
    };
    std::env::split_paths(&std::env::var_os("PATH")?)
        .flat_map(|dir| exts.iter().map(move |e| dir.join(format!("{name}{e}"))))
        .find(|p| p.is_file())
}

impl LocalBackend {
    /// The project's wrapper, else the configured Gradle, else `gradle` on PATH.
    fn gradle(&self, dir: &Path) -> Option<PathBuf> {
        let wrapper = dir.join(if cfg!(windows) {
            "gradlew.bat"
        } else {
            "gradlew"
        });
        if wrapper.is_file() {
            return Some(wrapper);
        }
        self.gradle
            .as_ref()
            .map_or_else(|| which("gradle"), |p| p.is_file().then(|| p.clone()))
    }
}

fn failed(stage: Stage, message: String, kind: ErrorKind) -> StageResult {
    StageResult {
        stage,
        status: StageStatus::Failed,
        log: PathBuf::new(),
        errors: vec![LogError {
            kind,
            file: None,
            line: None,
            message,
        }],
    }
}

const fn skipped(stage: Stage) -> StageResult {
    StageResult {
        stage,
        status: StageStatus::Skipped,
        log: PathBuf::new(),
        errors: vec![],
    }
}

/// `(passed, total)` from JUnit XML files under `results` `<testsuite tests=".." failures=".." errors="..">`.
pub(crate) fn junit_counts(results: &Path) -> Option<(u32, u32)> {
    fn attr(tag: &str, name: &str) -> u32 {
        tag.split(&format!(" {name}=\""))
            .nth(1)
            .and_then(|r| r.split('"').next())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }
    fn walk(dir: &Path, acc: &mut (u32, u32)) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, acc);
            } else if p.extension().is_some_and(|x| x == "xml") {
                let text = std::fs::read_to_string(&p).unwrap_or_default();
                if let Some(tag) = text
                    .split("<testsuite")
                    .nth(1)
                    .and_then(|t| t.split('>').next())
                {
                    let total = attr(tag, "tests");
                    let bad = attr(tag, "failures") + attr(tag, "errors");
                    acc.0 += total.saturating_sub(bad);
                    acc.1 += total;
                }
            }
        }
    }
    let mut acc = (0, 0);
    walk(results, &mut acc);
    (acc.1 > 0).then_some(acc)
}

pub(crate) fn newest_crash(reports: &Path) -> Option<super::logs::CrashInfo> {
    let newest = std::fs::read_dir(reports)
        .ok()?
        .flatten()
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())?;
    parse_crash_report(&std::fs::read_to_string(newest.path()).ok()?)
}

impl BuildBackend for LocalBackend {
    fn run(&self, req: &BuildRequest) -> Result<BuildOutcome, BuildError> {
        let dir = new_build_dir(&self.workdir, &req.mod_name)?;
        copy_project(&req.project, &dir)?;
        let logs = dir.join("ummp-logs");
        std::fs::create_dir_all(&logs)?;
        let mut env = scrubbed_env();
        let mut stages = Vec::new();
        let mut setup_error = None;
        if let Some(v) = req.java {
            match find_jdk(v) {
                Some(home) => {
                    env.retain(|(k, _)| k != "JAVA_HOME");
                    env.push(("JAVA_HOME".into(), home.to_string_lossy().into_owned()));
                }
                None => {
                    setup_error = Some((
                        format!("JDK {v} not found; install it or set JAVA_HOME"),
                        ErrorKind::JavaVersion,
                    ));
                }
            }
        }
        let gradle = self.gradle(&dir);
        if gradle.is_none() && setup_error.is_none() {
            setup_error = Some((
                "no Gradle found: add the Gradle wrapper (gradlew) to the project or install Gradle".into(),
                ErrorKind::Other,
            ));
        }
        for &stage in &req.stages {
            if stages
                .iter()
                .any(|s: &StageResult| s.status != StageStatus::Passed)
            {
                stages.push(skipped(stage));
                continue;
            }
            if let Some((msg, kind)) = setup_error.take() {
                stages.push(failed(stage, msg, kind));
                continue;
            }
            let Some(program) = gradle.clone() else {
                continue;
            };
            let task = match stage {
                Stage::Build => vec!["build", "-x", "test"],
                Stage::Test => vec!["test"],
                Stage::Server => vec!["runServer"],
            };
            let mut args: Vec<String> = task.into_iter().map(String::from).collect();
            args.extend(["--no-daemon".into(), "--console=plain".into()]);
            let watch = (stage == Stage::Server).then(|| Watch {
                pattern: "Done (".into(),
                send: "stop\n".into(),
                grace: Duration::from_secs(30),
            });
            if stage == Stage::Server {
                std::fs::create_dir_all(dir.join("run"))?;
                std::fs::write(dir.join("run/eula.txt"), "eula=true\n")?;
            }
            let log = logs.join(format!("{}.log", stage.name()));
            let spec = ProcSpec {
                program,
                args,
                cwd: dir.clone(),
                env: env.clone(),
                timeout: req.timeout,
            };
            let st = run(&spec, &log, watch.as_ref())?;
            let mut errors = analyze_log(&std::fs::read_to_string(&log).unwrap_or_default());
            let status = match st {
                ProcStatus::Exited(0) | ProcStatus::WatchMatched => StageStatus::Passed,
                ProcStatus::Exited(_) => StageStatus::Failed,
                ProcStatus::TimedOut => {
                    errors.push(LogError {
                        kind: ErrorKind::Timeout,
                        file: None,
                        line: None,
                        message: format!(
                            "{} did not finish within {}s",
                            stage.name(),
                            req.timeout.as_secs()
                        ),
                    });
                    StageStatus::TimedOut
                }
            };
            stages.push(StageResult {
                stage,
                status,
                log,
                errors,
            });
        }
        Ok(BuildOutcome {
            stages,
            tests: junit_counts(&dir.join("build/test-results")),
            crash: newest_crash(&dir.join("run/crash-reports")),
            workdir: dir,
        })
    }
}
