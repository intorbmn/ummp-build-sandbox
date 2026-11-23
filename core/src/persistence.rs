//! Infrastructure: SQLite-backed job persistence.

use crate::jobs::{Job, JobState};
use rusqlite::{params, Connection};
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

fn uuid_from_bytes(b: &[u8]) -> Option<Uuid> {
    let arr: [u8; 16] = b.try_into().ok()?;
    Some(Uuid::from_bytes(arr))
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serialize: {0}")]
    Serialize(String),
    #[error("deserialize: {0}")]
    Deserialize(String),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

pub struct JobDb {
    conn: Connection,
}

impl JobDb {
    pub fn open(path: &Path) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn insert(&self, job: &Job) -> Result<(), DbError> {
        let state =
            serde_json::to_string(&job.state).map_err(|e| DbError::Serialize(e.to_string()))?;
        let log = serde_json::to_string(&job.log).map_err(|e| DbError::Serialize(e.to_string()))?;
        let output =
            serde_json::to_string(&job.output).map_err(|e| DbError::Serialize(e.to_string()))?;
        let timeout = serde_json::to_string(&job.timeout_at)
            .map_err(|e| DbError::Serialize(e.to_string()))?;

        self.conn.execute(
            "INSERT OR REPLACE INTO jobs
             (id, session_id, step, state, retries, log, timeout_at, output)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                job.id.as_bytes(),
                job.session_id.as_bytes(),
                job.step,
                state,
                job.retries,
                log,
                timeout,
                output,
            ],
        )?;
        Ok(())
    }

    pub fn by_session(&self, session_id: Uuid) -> Result<Vec<Job>, DbError> {
        self.query("WHERE session_id = ?1", params![session_id.as_bytes()])
    }

    /// The job with this id, if stored.
    pub fn by_id(&self, id: Uuid) -> Result<Option<Job>, DbError> {
        Ok(self
            .query("WHERE id = ?1", params![id.as_bytes()])?
            .into_iter()
            .next())
    }

    /// Every job in `state`, oldest first.
    pub fn by_state(&self, state: JobState) -> Result<Vec<Job>, DbError> {
        let state = serde_json::to_string(&state).map_err(|e| DbError::Serialize(e.to_string()))?;
        self.query("WHERE state = ?1", params![state])
    }

    /// The most recently written jobs, newest first (a rewritten job counts as newest).
    pub fn recent(&self, limit: usize) -> Result<Vec<Job>, DbError> {
        let mut all = self.query("", [])?;
        all.reverse();
        all.truncate(limit);
        Ok(all)
    }

    fn query(&self, filter: &str, p: impl rusqlite::Params) -> Result<Vec<Job>, DbError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT id, session_id, step, state, retries, log, timeout_at, output
             FROM jobs {filter} ORDER BY rowid"
        ))?;

        let rows: Vec<(
            Vec<u8>,
            Vec<u8>,
            String,
            String,
            i64,
            String,
            String,
            String,
        )> = stmt
            .query_map(p, |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut jobs = Vec::new();
        for (id, sid, step, state, retries, log, timeout, output) in rows {
            jobs.push(Job {
                id: uuid_from_bytes(&id)
                    .ok_or_else(|| DbError::Deserialize("bad id uuid".into()))?,
                session_id: uuid_from_bytes(&sid)
                    .ok_or_else(|| DbError::Deserialize("bad session uuid".into()))?,
                step,
                state: serde_json::from_str(&state)
                    .map_err(|e| DbError::Deserialize(e.to_string()))?,
                retries: u32::try_from(retries)
                    .map_err(|_| DbError::Deserialize("retries out of range".into()))?,
                log: serde_json::from_str(&log).map_err(|e| DbError::Deserialize(e.to_string()))?,
                timeout_at: serde_json::from_str(&timeout)
                    .map_err(|e| DbError::Deserialize(e.to_string()))?,
                output: serde_json::from_str(&output)
                    .map_err(|e| DbError::Deserialize(e.to_string()))?,
            });
        }
        Ok(jobs)
    }

    pub fn delete_by_session(&self, session_id: Uuid) -> Result<u64, DbError> {
        let n = self.conn.execute(
            "DELETE FROM jobs WHERE session_id = ?1",
            params![session_id.as_bytes()],
        )?;
        Ok(n as u64)
    }
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS jobs (
    id           BLOB PRIMARY KEY,
    session_id   BLOB NOT NULL,
    step         TEXT NOT NULL,
    state        TEXT NOT NULL,
    retries      INTEGER NOT NULL DEFAULT 0,
    log          TEXT NOT NULL DEFAULT '[]',
    timeout_at   TEXT DEFAULT 'null',
    output       TEXT DEFAULT 'null'
);

CREATE INDEX IF NOT EXISTS idx_jobs_session ON jobs(session_id);
";

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_insert_and_read() {
        let dir = tempdir().unwrap();
        let db = JobDb::open(&dir.path().join("jobs.db")).unwrap();
        let session_id = Uuid::new_v4();
        let job = Job::new(session_id, "analyze");
        db.insert(&job).unwrap();

        let jobs = db.by_session(session_id).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].step, "analyze");
        assert_eq!(jobs[0].state, JobState::Pending);
    }

    #[test]
    fn test_by_state_and_by_id() {
        let dir = tempdir().unwrap();
        let db = JobDb::open(&dir.path().join("jobs.db")).unwrap();
        let session_id = Uuid::new_v4();
        let mut running = Job::new(session_id, "port");
        running.mark_running();
        let mut cancelled = Job::new(session_id, "build");
        cancelled.mark_cancelled();
        db.insert(&running).unwrap();
        db.insert(&cancelled).unwrap();

        let found = db.by_state(JobState::Running).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, running.id);
        assert_eq!(
            db.by_state(JobState::Cancelled).unwrap()[0].id,
            cancelled.id
        );
        assert!(db.by_state(JobState::Failed).unwrap().is_empty());
        assert_eq!(db.by_id(cancelled.id).unwrap().unwrap().step, "build");
        assert!(db.by_id(Uuid::new_v4()).unwrap().is_none());
    }

    #[test]
    fn test_delete_by_session() {
        let dir = tempdir().unwrap();
        let db = JobDb::open(&dir.path().join("jobs.db")).unwrap();
        let session_id = Uuid::new_v4();
        db.insert(&Job::new(session_id, "a")).unwrap();
        db.insert(&Job::new(session_id, "b")).unwrap();

        let n = db.delete_by_session(session_id).unwrap();
        assert_eq!(n, 2);
        assert!(db.by_session(session_id).unwrap().is_empty());
    }
}
