//! Infrastructure: job state persistence.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobState {
    Pending,
    Running,
    Succeeded,
    Failed,
    Skipped,
    TimedOut,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: Uuid,
    pub session_id: Uuid,
    pub step: String,
    pub state: JobState,
    pub retries: u32,
    pub log: Vec<String>,
    pub timeout_at: Option<chrono::DateTime<chrono::Utc>>,
    pub output: Option<serde_json::Value>,
}

impl Job {
    pub fn new(session_id: Uuid, step: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            step: step.to_string(),
            state: JobState::Pending,
            retries: 0,
            log: Vec::new(),
            timeout_at: None,
            output: None,
        }
    }

    pub fn log_line(&mut self, line: &str) {
        self.log.push(line.to_string());
    }

    pub const fn mark_running(&mut self) {
        self.state = JobState::Running;
    }

    pub fn mark_succeeded(&mut self, output: serde_json::Value) {
        self.state = JobState::Succeeded;
        self.output = Some(output);
    }

    pub const fn mark_failed(&mut self) {
        self.state = JobState::Failed;
    }

    pub const fn mark_cancelled(&mut self) {
        self.state = JobState::Cancelled;
    }

    pub const fn retry(&mut self) -> bool {
        if self.retries < 3 {
            self.retries += 1;
            self.state = JobState::Pending;
            true
        } else {
            self.state = JobState::TimedOut;
            false
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JobStore {
    pub jobs: Vec<Job>,
}

impl JobStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, job: Job) {
        self.jobs.push(job);
    }

    pub fn for_session(&self, session_id: Uuid) -> Vec<&Job> {
        self.jobs
            .iter()
            .filter(|j| j.session_id == session_id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_new() {
        let session_id = Uuid::new_v4();
        let job = Job::new(session_id, "analyze");
        assert_eq!(job.session_id, session_id);
        assert_eq!(job.step, "analyze");
        assert_eq!(job.state, JobState::Pending);
        assert_eq!(job.retries, 0);
    }

    #[test]
    fn test_job_transitions() {
        let session_id = Uuid::new_v4();
        let mut job = Job::new(session_id, "transform");

        job.mark_running();
        assert_eq!(job.state, JobState::Running);

        job.mark_succeeded(serde_json::json!({"ok": true}));
        assert_eq!(job.state, JobState::Succeeded);
        assert_eq!(job.output, Some(serde_json::json!({"ok": true})));

        job.mark_failed();
        assert_eq!(job.state, JobState::Failed);
    }

    #[test]
    fn test_job_retry() {
        let session_id = Uuid::new_v4();
        let mut job = Job::new(session_id, "build");

        assert!(job.retry());
        assert_eq!(job.retries, 1);
        assert_eq!(job.state, JobState::Pending);

        for _ in 0..3 {
            job.retry();
        }
        assert_eq!(job.retries, 3);
        assert!(!job.retry());
        assert_eq!(job.state, JobState::TimedOut);
    }

    #[test]
    fn test_job_store() {
        let session_id = Uuid::new_v4();
        let mut store = JobStore::new();
        store.add(Job::new(session_id, "a"));
        store.add(Job::new(session_id, "b"));
        store.add(Job::new(Uuid::new_v4(), "c"));

        let jobs = store.for_session(session_id);
        assert_eq!(jobs.len(), 2);
    }
}
