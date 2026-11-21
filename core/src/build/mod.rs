//! Build & Testing Engine (Phase 8): runs a mod's Gradle build, tests and a dedicated-server
//! launch check, locally or on GitHub Actions, and reports the result.

pub mod github;
pub mod java;
pub mod local;
pub mod logs;
pub mod process;
pub mod report;

use logs::{CrashInfo, LogError};
use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Build,
    Test,
    Server,
}

impl Stage {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Test => "test",
            Self::Server => "server",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    Passed,
    Failed,
    Skipped,
    TimedOut,
}

#[derive(Debug, Clone, Serialize)]
pub struct StageResult {
    pub stage: Stage,
    pub status: StageStatus,
    /// Log file of this stage; empty for skipped stages.
    pub log: PathBuf,
    pub errors: Vec<LogError>,
}

#[derive(Debug, Clone)]
pub struct BuildRequest {
    pub project: PathBuf,
    pub mod_name: String,
    pub stages: Vec<Stage>,
    /// Required Java major version; `None` uses whatever Java the build tool finds.
    pub java: Option<u32>,
    /// Limit for each stage.
    pub timeout: Duration,
}

#[derive(Debug, Clone, Serialize)]
pub struct BuildOutcome {
    pub stages: Vec<StageResult>,
    /// `(passed, total)` unit tests from JUnit XML, when any ran.
    pub tests: Option<(u32, u32)>,
    pub workdir: PathBuf,
    pub crash: Option<CrashInfo>,
}

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Tool(String),
}

/// Where a build runs.
pub trait BuildBackend {
    /// Runs the requested stages in order, stopping after the first failure.
    ///
    /// # Errors
    /// Setup failures (copying the project, talking to GitHub); build failures are
    /// reported in the outcome instead.
    fn run(&self, req: &BuildRequest) -> Result<BuildOutcome, BuildError>;
}
