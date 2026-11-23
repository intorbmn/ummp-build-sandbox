//! Application: migration session state machine.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Pending,
    Running,
    Failed,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub source_minecraft: String,
    pub target_minecraft: String,
    pub target_loader: String,
    pub ai_model: Option<String>,
    pub toolchain_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationSession {
    pub id: Uuid,
    pub project_path: PathBuf,
    pub config: SessionConfig,
    pub state: SessionState,
    pub current_step: Option<String>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl MigrationSession {
    pub fn new(project_path: PathBuf, config: SessionConfig) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_path,
            config,
            state: SessionState::Pending,
            current_step: None,
            started_at: now,
            updated_at: now,
        }
    }

    pub fn start(&mut self, step: &str) {
        self.state = SessionState::Running;
        self.current_step = Some(step.to_string());
        self.updated_at = chrono::Utc::now();
    }

    pub fn succeed(&mut self) {
        self.state = SessionState::Completed;
        self.updated_at = chrono::Utc::now();
    }

    pub fn fail(&mut self) {
        self.state = SessionState::Failed;
        self.updated_at = chrono::Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> SessionConfig {
        SessionConfig {
            source_minecraft: "1.20.1".into(),
            target_minecraft: "1.21".into(),
            target_loader: "fabric".into(),
            ai_model: None,
            toolchain_version: "1.0".into(),
        }
    }

    #[test]
    fn test_session_new() {
        let s = MigrationSession::new(PathBuf::from("."), config());
        assert_eq!(s.state, SessionState::Pending);
        assert!(s.current_step.is_none());
    }

    #[test]
    fn test_session_lifecycle() {
        let mut s = MigrationSession::new(PathBuf::from("."), config());
        s.start("analyze");
        assert_eq!(s.state, SessionState::Running);
        assert_eq!(s.current_step, Some("analyze".into()));

        s.succeed();
        assert_eq!(s.state, SessionState::Completed);
    }

    #[test]
    fn test_session_fail() {
        let mut s = MigrationSession::new(PathBuf::from("."), config());
        s.start("transform");
        s.fail();
        assert_eq!(s.state, SessionState::Failed);
    }
}
