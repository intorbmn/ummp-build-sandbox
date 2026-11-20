//! Classifies Gradle/javac/Mixin build output and parses Minecraft crash reports.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Compile,
    MissingSymbol,
    Dependency,
    JavaVersion,
    Mixin,
    Crash,
    Timeout,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LogError {
    pub kind: ErrorKind,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CrashInfo {
    pub description: String,
    pub exception: String,
    pub suspected_mods: Vec<String>,
}

/// Classified errors found in build or game log text, in order, without duplicates.
#[must_use]
pub fn analyze_log(text: &str) -> Vec<LogError> {
    let mut out: Vec<LogError> = Vec::new();
    for line in text.lines() {
        if let Some(e) = classify(line.trim()) {
            if !out.contains(&e) {
                out.push(e);
            }
        }
    }
    out
}

fn classify(line: &str) -> Option<LogError> {
    let plain = |kind, message: &str| LogError {
        kind,
        file: None,
        line: None,
        message: message.to_string(),
    };
    // javac: `<path>:<line>: error: <message>`
    if let Some((loc, msg)) = line.split_once(": error: ") {
        let (path, num) = loc.rsplit_once(':')?;
        let kind = if msg.starts_with("cannot find symbol") {
            ErrorKind::MissingSymbol
        } else if msg.starts_with("invalid source release")
            || msg.starts_with("invalid target release")
        {
            ErrorKind::JavaVersion
        } else {
            ErrorKind::Compile
        };
        return Some(LogError {
            kind,
            file: Some(project_relative(path)),
            line: num.parse().ok(),
            message: msg.to_string(),
        });
    }
    // kotlinc: `e: file:///<path>:<line>:<col> <message>`
    if let Some(rest) = line.strip_prefix("e: ") {
        let rest = rest.strip_prefix("file://").unwrap_or(rest);
        let (loc, msg) = rest.split_once(' ').unwrap_or((rest, ""));
        let mut parts = loc.rsplitn(3, ':');
        let (_col, num, path) = (parts.next(), parts.next(), parts.next());
        let kind = if msg.starts_with("Unresolved reference") {
            ErrorKind::MissingSymbol
        } else {
            ErrorKind::Compile
        };
        return Some(LogError {
            kind,
            file: path.map(project_relative),
            line: num.and_then(|n| n.parse().ok()),
            message: msg.to_string(),
        });
    }
    let l = line.trim_start_matches(['>', ' ']);
    if l.starts_with("Could not find ")
        || (l.starts_with("Could not resolve ")
            && !l.contains("all files")
            && !l.contains("all dependencies"))
    {
        return Some(plain(ErrorKind::Dependency, l));
    }
    if l.contains("Unsupported class file major version")
        || l.contains("compiled by a more recent version of the Java")
    {
        return Some(plain(ErrorKind::JavaVersion, l));
    }
    if (l.contains("Mixin apply") && l.contains("failed")) || l.contains("MixinApplyError") {
        return Some(plain(ErrorKind::Mixin, l));
    }
    if l.contains("---- Minecraft Crash Report ----") || l.contains("Exception in server tick loop")
    {
        return Some(plain(ErrorKind::Crash, l));
    }
    None
}

/// Trims an absolute path to the part from `src/` on, so errors read the same everywhere.
fn project_relative(path: &str) -> String {
    let p = path.replace('\\', "/");
    match p.find("/src/") {
        Some(i) => p[i + 1..].to_string(),
        None => p,
    }
}

/// Parses a `---- Minecraft Crash Report ----` file.
#[must_use]
pub fn parse_crash_report(text: &str) -> Option<CrashInfo> {
    if !text.contains("---- Minecraft Crash Report ----") {
        return None;
    }
    let mut lines = text.lines().map(str::trim_end);
    let description = lines
        .by_ref()
        .find_map(|l| l.strip_prefix("Description: "))
        .unwrap_or_default()
        .to_string();
    let exception = lines
        .by_ref()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default()
        .to_string();
    let mut suspected_mods = Vec::new();
    let mut in_section = false;
    for l in text.lines() {
        if l.trim_start().starts_with("Suspected Mod") {
            in_section = true;
        } else if in_section && l.trim().is_empty() {
            break;
        }
        if in_section && !l.trim_start().starts_with("at ") {
            if let Some(id) = l
                .split_once('(')
                .and_then(|(_, r)| r.split_once(')'))
                .map(|(id, _)| id)
            {
                suspected_mods.push(id.to_string());
            }
        }
    }
    Some(CrashInfo {
        description,
        exception,
        suspected_mods,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/logs")
            .join(name);
        std::fs::read_to_string(p).unwrap()
    }

    #[test]
    fn compile_errors_have_file_and_line() {
        let e = analyze_log(&fixture("gradle-compile.log"));
        assert_eq!(e.len(), 2, "{e:?}");
        assert_eq!(e[0].kind, ErrorKind::Compile);
        assert_eq!(
            e[0].file.as_deref(),
            Some("src/main/java/com/example/demo/DemoWidget.java")
        );
        assert_eq!(e[0].line, Some(12));
        assert_eq!(e[0].message, "';' expected");
        assert_eq!(e[1].kind, ErrorKind::MissingSymbol);
        assert_eq!(e[1].line, Some(20));
    }

    #[test]
    fn dependency_error_names_coordinate() {
        let e = analyze_log(&fixture("gradle-deps.log"));
        assert_eq!(e.len(), 1, "{e:?}");
        assert_eq!(e[0].kind, ErrorKind::Dependency);
        assert!(e[0].message.contains("com.example.demo:demolib:1.0.0"));
    }

    #[test]
    fn java_version_mismatch() {
        let e = analyze_log(&fixture("gradle-java.log"));
        assert!(e.iter().any(|x| x.kind == ErrorKind::JavaVersion), "{e:?}");
    }

    #[test]
    fn mixin_failure() {
        let e = analyze_log(&fixture("mixin-fail.log"));
        assert!(
            e.iter()
                .any(|x| x.kind == ErrorKind::Mixin && x.message.contains("DemoWidgetMixin")),
            "{e:?}"
        );
    }

    #[test]
    fn empty_log_has_no_errors() {
        assert!(analyze_log("").is_empty());
    }

    #[test]
    fn crash_report_fields() {
        let c = parse_crash_report(&fixture("crash-report.txt")).unwrap();
        assert_eq!(c.description, "Exception in server tick loop");
        assert!(c.exception.starts_with("java.lang.NullPointerException"));
        assert_eq!(c.suspected_mods, vec!["demowidget".to_string()]);
        assert!(parse_crash_report("plain log").is_none());
    }
}
