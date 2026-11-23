//! Child-process runner: scrubbed environment, merged output log, timeout that kills the
//! whole process tree, and an optional "watch for a line, then send input" hook.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

/// Environment variables passed to builds; everything else (API keys, tokens) is dropped.
const ENV_ALLOW: &[&str] = &[
    "PATH",
    "JAVA_HOME",
    "HOME",
    "USERPROFILE",
    "TEMP",
    "TMP",
    "SystemRoot",
    "GRADLE_USER_HOME",
];

/// What to run.
#[derive(Debug, Clone)]
pub struct ProcSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub timeout: Duration,
}

/// When a line containing `pattern` appears, write `send` to stdin, wait up to `grace`
/// for the process to exit, then kill it.
#[derive(Debug, Clone)]
pub struct Watch {
    pub pattern: String,
    pub send: String,
    pub grace: Duration,
}

/// How the process ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcStatus {
    Exited(i32),
    TimedOut,
    WatchMatched,
}

/// The allow-listed subset of the current environment.
#[must_use]
pub fn scrubbed_env() -> Vec<(String, String)> {
    scrubbed_env_from(std::env::vars())
}

/// Filters an arbitrary environment iterator through the same allow-list.
pub(crate) fn scrubbed_env_from<I>(vars: I) -> Vec<(String, String)>
where
    I: IntoIterator<Item = (String, String)>,
{
    vars.into_iter()
        .filter(|(key, _)| {
            ENV_ALLOW
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(key))
        })
        .collect()
}

/// Runs `spec`, writing stdout and stderr to `log`.
///
/// # Errors
/// Spawn or log-file I/O failures.
pub fn run(spec: &ProcSpec, log: &Path, watch: Option<&Watch>) -> io::Result<ProcStatus> {
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args)
        .current_dir(&spec.cwd)
        .env_clear()
        .envs(spec.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let mut child = cmd.spawn()?;
    let (tx, rx) = mpsc::channel::<String>();
    for out in [
        child
            .stdout
            .take()
            .map(|o| Box::new(o) as Box<dyn io::Read + Send>),
        child
            .stderr
            .take()
            .map(|e| Box::new(e) as Box<dyn io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);

    let mut log = io::BufWriter::new(std::fs::File::create(log)?);
    let mut deadline = Instant::now() + spec.timeout;
    let mut matched = false;
    loop {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(line) => {
                writeln!(log, "{line}")?;
                if let Some(w) = watch.filter(|w| !matched && line.contains(&w.pattern)) {
                    matched = true;
                    deadline = Instant::now() + w.grace;
                    if let Some(stdin) = child.stdin.as_mut() {
                        // The process may already be gone; that is not an error here.
                        let _ = stdin
                            .write_all(w.send.as_bytes())
                            .and_then(|()| stdin.flush());
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                kill_tree(&mut child);
                log.flush()?;
                return Ok(if matched {
                    ProcStatus::WatchMatched
                } else {
                    ProcStatus::TimedOut
                });
            }
        }
    }
    log.flush()?;
    if matched {
        kill_tree(&mut child);
        return Ok(ProcStatus::WatchMatched);
    }
    let status = child.wait()?;
    Ok(ProcStatus::Exited(status.code().unwrap_or(-1)))
}

/// Runs a bounded JSON-lines exchange with separate stdout and stderr pipes.
///
/// # Errors
/// Returns an I/O error when spawning, reading, or exceeding the output limit.
pub fn run_with_input(
    spec: &ProcSpec,
    input: &[u8],
    max_stdout_bytes: usize,
) -> io::Result<(ProcStatus, Vec<u8>)> {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    enum ReaderDone {
        Stdout,
        Stderr,
    }

    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args)
        .current_dir(&spec.cwd)
        .env_clear()
        .envs(spec.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let mut child = cmd.spawn()?;

    let overflow = Arc::new(AtomicBool::new(false));
    let (done_tx, done_rx) = mpsc::channel();
    let stdout = child.stdout.take().expect("piped stdout");
    let stdout_flag = Arc::clone(&overflow);
    let stdout_tx = done_tx.clone();
    let stdout_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take(max_stdout_bytes as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > max_stdout_bytes {
            stdout_flag.store(true, Ordering::SeqCst);
        }
        let _ = stdout_tx.send(ReaderDone::Stdout);
        Ok::<_, io::Error>(bytes)
    });
    let stderr = child.stderr.take().expect("piped stderr");
    let stderr_tx = done_tx.clone();
    let stderr_reader = std::thread::spawn(move || {
        let mut stderr = stderr;
        let result = io::copy(&mut stderr, &mut io::sink()).map(|_| ());
        let _ = stderr_tx.send(ReaderDone::Stderr);
        result
    });
    drop(done_tx);

    let mut stdin = child.stdin.take().expect("piped stdin");
    let request = input.to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&request));

    let deadline = Instant::now() + spec.timeout;
    let mut exit_status = None;
    let mut stdout_done = false;
    let mut stderr_done = false;
    let mut timed_out = false;
    loop {
        if overflow.load(Ordering::SeqCst) {
            kill_tree(&mut child);
            break;
        }
        if exit_status.is_none() {
            if let Some(exit) = child.try_wait()? {
                exit_status = Some(ProcStatus::Exited(exit.code().unwrap_or(-1)));
            }
        }
        if exit_status.is_some() && stdout_done && stderr_done {
            break;
        }
        if Instant::now() >= deadline {
            kill_tree(&mut child);
            timed_out = true;
            break;
        }
        match done_rx.recv_timeout(Duration::from_millis(10)) {
            Ok(ReaderDone::Stdout) => stdout_done = true,
            Ok(ReaderDone::Stderr) => stderr_done = true,
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {}
        }
    }

    let output = stdout_reader
        .join()
        .map_err(|_| io::Error::other("stdout reader panicked"))??;
    stderr_reader
        .join()
        .map_err(|_| io::Error::other("stderr reader panicked"))??;
    if let Err(error) = writer
        .join()
        .map_err(|_| io::Error::other("stdin writer panicked"))?
    {
        if error.kind() != io::ErrorKind::BrokenPipe {
            return Err(error);
        }
    }
    if overflow.load(Ordering::SeqCst) || output.len() > max_stdout_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "child stdout limit exceeded",
        ));
    }
    let status = if timed_out {
        ProcStatus::TimedOut
    } else {
        exit_status.unwrap_or(ProcStatus::TimedOut)
    };
    Ok((status, output))
}

/// Kills the child and everything it started. Errors are ignored: the processes may
/// already have exited.
fn kill_tree(child: &mut Child) {
    let pid = child.id().to_string();
    #[cfg(unix)]
    let _ = Command::new("kill")
        .args(["-KILL", "--", &format!("-{pid}")])
        .status();
    #[cfg(windows)]
    let _ = Command::new("taskkill")
        .args(["/T", "/F", "/PID", &pid])
        .status();
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn sh(script: &str, cwd: &Path, timeout: Duration) -> ProcSpec {
        ProcSpec {
            program: "sh".into(),
            args: vec!["-c".into(), script.into()],
            cwd: cwd.to_path_buf(),
            env: scrubbed_env(),
            timeout,
        }
    }

    #[test]
    fn exit_code_is_returned() {
        let d = tempfile::tempdir().unwrap();
        let log = d.path().join("log");
        let st = run(
            &sh("echo hi; exit 3", d.path(), Duration::from_secs(10)),
            &log,
            None,
        )
        .unwrap();
        assert_eq!(st, ProcStatus::Exited(3));
        assert!(std::fs::read_to_string(&log).unwrap().contains("hi"));
    }

    #[test]
    fn env_is_scrubbed() {
        let d = tempfile::tempdir().unwrap();
        let log = d.path().join("log");
        assert!(
            std::env::var("CARGO_MANIFEST_DIR").is_ok(),
            "cargo sets this for tests"
        );
        let st = run(&sh("env", d.path(), Duration::from_secs(10)), &log, None).unwrap();
        assert_eq!(st, ProcStatus::Exited(0));
        let out = std::fs::read_to_string(&log).unwrap();
        assert!(!out.contains("CARGO_MANIFEST_DIR"), "{out}");
        assert!(out.contains("PATH="), "{out}");
    }

    #[test]
    fn timeout_kills_grandchild() {
        let d = tempfile::tempdir().unwrap();
        let log = d.path().join("log");
        let script = "sleep 100 & echo $! > pid; trap '' TERM; sleep 100";
        let st = run(&sh(script, d.path(), Duration::from_secs(1)), &log, None).unwrap();
        assert_eq!(st, ProcStatus::TimedOut);
        let pid = std::fs::read_to_string(d.path().join("pid")).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        // A killed process may linger as a zombie until its new parent reaps it; that is dead.
        let stat = std::fs::read_to_string(format!("/proc/{}/stat", pid.trim()));
        let alive = stat.map_or_else(
            |_| {
                std::process::Command::new("kill")
                    .args(["-0", pid.trim()])
                    .status()
                    .unwrap()
                    .success()
            },
            |st| !st.contains(") Z "),
        );
        assert!(!alive, "grandchild {pid} survived");
    }

    #[test]
    fn watch_sends_stop() {
        let d = tempfile::tempdir().unwrap();
        let log = d.path().join("log");
        let script =
            "echo 'Done (1s)!'; read l; [ \"$l\" = stop ] && echo stopped && exit 0; exit 9";
        let w = Watch {
            pattern: "Done (".into(),
            send: "stop\n".into(),
            grace: Duration::from_secs(5),
        };
        let st = run(
            &sh(script, d.path(), Duration::from_secs(10)),
            &log,
            Some(&w),
        )
        .unwrap();
        assert_eq!(st, ProcStatus::WatchMatched);
        assert!(std::fs::read_to_string(&log).unwrap().contains("stopped"));
    }
}
