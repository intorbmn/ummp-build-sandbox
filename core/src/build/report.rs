//! Build score and Markdown report.

use super::{BuildOutcome, StageStatus};
use crate::scan::ScanReport;
use std::fmt::Write as _;

/// `(passed, run)` stages; skipped stages are not counted as run.
#[must_use]
pub fn score(o: &BuildOutcome) -> (u32, u32) {
    let count = |want: fn(StageStatus) -> bool| {
        u32::try_from(o.stages.iter().filter(|s| want(s.status)).count()).unwrap_or(u32::MAX)
    };
    (
        count(|s| s == StageStatus::Passed),
        count(|s| s != StageStatus::Skipped),
    )
}

/// Markdown report: score, stages, classified errors, tests, crash, scan, next steps.
#[must_use]
pub fn to_markdown(o: &BuildOutcome, scan: &ScanReport) -> String {
    let (passed, run) = score(o);
    let mut md = format!(
        "# UMMP build report\n\n**Score: {passed}/{run} stages passed**\n\nWork folder: `{}`\n\n\
         | Stage | Status | Log |\n|---|---|---|\n",
        o.workdir.display()
    );
    for s in &o.stages {
        let _ = writeln!(
            md,
            "| {} | {:?} | `{}` |",
            s.stage.name(),
            s.status,
            s.log.display()
        );
    }
    if let Some((ok, total)) = o.tests {
        let _ = writeln!(md, "\n{ok}/{total} tests passed");
    }
    let errors: Vec<_> = o.stages.iter().flat_map(|s| &s.errors).collect();
    if !errors.is_empty() {
        md.push_str("\n## Errors\n");
        for e in errors {
            let at = match (&e.file, e.line) {
                (Some(f), Some(l)) => format!(" `{f}:{l}`"),
                (Some(f), None) => format!(" `{f}`"),
                _ => String::new(),
            };
            let _ = writeln!(md, "- {:?}{at}: {}", e.kind, e.message);
        }
    }
    if let Some(c) = &o.crash {
        let _ = write!(
            md,
            "\n## Crash\n{}\n\n`{}`\n\nSuspected mods: {}\n",
            c.description,
            c.exception,
            c.suspected_mods.join(", ")
        );
    }
    let _ = write!(md, "\n## Malware risk scan\n{}", scan.to_text());
    if let Some(failed) = o
        .stages
        .iter()
        .find(|s| matches!(s.status, StageStatus::Failed | StageStatus::TimedOut))
    {
        let _ = write!(
            md,
            "\n## Next step\nAsk the AI for fixes:\n\n```\nummp ai-debug <project> --log {} \
             --from <ver> --to <ver> --loader <loader>\n```\n",
            failed.log.display()
        );
    }
    md
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::logs::{ErrorKind, LogError};
    use crate::build::{Stage, StageResult};
    use std::path::PathBuf;

    fn outcome() -> BuildOutcome {
        BuildOutcome {
            stages: vec![
                StageResult {
                    stage: Stage::Build,
                    status: StageStatus::Passed,
                    log: PathBuf::from("logs/build.log"),
                    errors: vec![],
                },
                StageResult {
                    stage: Stage::Test,
                    status: StageStatus::Failed,
                    log: PathBuf::from("/w/logs/test.log"),
                    errors: vec![LogError {
                        kind: ErrorKind::Compile,
                        file: Some("src/main/java/com/example/demo/DemoWidget.java".into()),
                        line: Some(3),
                        message: "';' expected".into(),
                    }],
                },
                StageResult {
                    stage: Stage::Server,
                    status: StageStatus::Skipped,
                    log: PathBuf::new(),
                    errors: vec![],
                },
            ],
            tests: Some((4, 5)),
            workdir: PathBuf::from("/w"),
            crash: None,
        }
    }

    #[test]
    fn score_excludes_skipped() {
        assert_eq!(score(&outcome()), (1, 2));
    }

    #[test]
    fn markdown_contains_score_and_ai_debug_hint() {
        let md = to_markdown(&outcome(), &ScanReport::default());
        assert!(md.contains("1/2"), "{md}");
        assert!(md.contains("DemoWidget.java:3"), "{md}");
        assert!(md.contains("4/5 tests passed"), "{md}");
        assert!(md.contains("ummp ai-debug"), "{md}");
        assert!(md.contains("/w/logs/test.log"), "{md}");
        assert!(md.contains("not proof"), "{md}");
    }
}
