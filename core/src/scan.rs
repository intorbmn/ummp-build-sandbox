//! Static malware-risk scan (Phase 8, ADR 0011).
//!
//! Rules run over source files, build scripts and class-file constant-pool strings,
//! including JARs nested in JARs. A scan reports indicators; finding nothing is not proof that a mod is safe.

use crate::bytecode::utf8_constants;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::io::{Cursor, Read};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub rule: String,
    pub severity: Severity,
    pub file: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ScanReport {
    pub findings: Vec<Finding>,
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
}

impl ScanReport {
    /// The highest severity found, if any.
    #[must_use]
    pub fn risk(&self) -> Option<Severity> {
        self.findings.iter().map(|f| f.severity).max()
    }

    #[must_use]
    pub fn to_text(&self) -> String {
        let Some(risk) = self.risk() else {
            return "No risk indicators found (this is not proof the mod is safe).\n".into();
        };
        let mut out = format!(
            "Risk: {risk:?} ({} finding(s)). Review before running this mod.\n",
            self.findings.len()
        );
        for f in &self.findings {
            let _ = write!(
                out,
                "- [{:?}] {}: {}\n    {}\n",
                f.severity, f.rule, f.file, f.evidence
            );
        }
        out
    }
}

const RULES: &str = include_str!("scan_rules.toml");
const MAX_ENTRY: u64 = 64 << 20;
const MAX_DEPTH: u32 = 3;
const TEXT_EXT: &[&str] = &["java", "kt", "gradle", "kts", "properties"];
const NATIVE_EXT: &[&str] = &["dll", "so", "dylib", "jnilib"];
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".gradle",
    "build",
    "target",
    "run",
    "out",
    "node_modules",
];

#[derive(Deserialize)]
struct RuleFile {
    rule: Vec<Rule>,
}

#[derive(Deserialize)]
struct Rule {
    id: String,
    severity: Severity,
    description: String,
    files: Vec<String>,
    all: Vec<Vec<String>>,
}

struct Scanner {
    rules: Vec<Rule>,
    findings: Vec<Finding>,
}

fn ext(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

impl Scanner {
    fn add(&mut self, rule: &str, severity: Severity, file: &str, evidence: String) {
        self.findings.push(Finding {
            rule: rule.into(),
            severity,
            file: file.into(),
            evidence,
        });
    }

    /// Applies the rules for `kind` and the encoded-blob check to one file's text.
    fn check_text(&mut self, text: &str, kind: &str, file: &str) {
        let mut hits = Vec::new();
        for r in self
            .rules
            .iter()
            .filter(|r| r.files.iter().any(|f| f == kind))
        {
            let matched: Option<Vec<&str>> = r
                .all
                .iter()
                .map(|group| {
                    group
                        .iter()
                        .find(|item| item.split("&&").all(|part| text.contains(part)))
                        .map(|item| item.trim())
                })
                .collect();
            if let Some(m) = matched {
                let shown: Vec<String> = m
                    .iter()
                    .map(|x| x.replace('\n', " ").trim().to_string())
                    .collect();
                hits.push((
                    r.id.clone(),
                    r.severity,
                    format!("{} ({})", r.description, shown.join(" + ")),
                ));
            }
        }
        for (id, sev, ev) in hits {
            self.add(&id, sev, file, ev);
        }
        let longest = text
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '='))
            .map(str::len)
            .max()
            .unwrap_or(0);
        if longest >= 2000 {
            self.add(
                "encoded-blob",
                Severity::Medium,
                file,
                format!("Encoded data block of {longest} characters"),
            );
        }
    }

    fn check_file(&mut self, name: &str, bytes: &[u8], file: &str, depth: u32) {
        match ext(name).as_str() {
            "class" => match utf8_constants(bytes, file) {
                Ok(strings) => {
                    let text = format!("\n{}\n", strings.join("\n"));
                    self.check_text(&text, "class", file);
                }
                Err(e) => self.add("unreadable-class", Severity::Low, file, e.to_string()),
            },
            "jar" | "zip" if depth < MAX_DEPTH => {
                if let Err(e) = self.scan_jar(bytes, &format!("{file}!/"), file, depth + 1) {
                    self.add("unreadable-jar", Severity::Low, file, e.to_string());
                }
            }
            e if NATIVE_EXT.contains(&e) => self.add(
                "native-library",
                Severity::High,
                file,
                "Native library: runs outside the JVM; confirm it is expected".into(),
            ),
            e if TEXT_EXT.contains(&e) => self.check_text(&String::from_utf8_lossy(bytes), e, file),
            _ => {}
        }
    }

    fn scan_jar(
        &mut self,
        bytes: &[u8],
        prefix: &str,
        label: &str,
        depth: u32,
    ) -> Result<(), ScanError> {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
        let mut simple_names = Vec::new();
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().to_string();
            let mut data = Vec::new();
            entry.by_ref().take(MAX_ENTRY).read_to_end(&mut data)?;
            if let Some(class) = name.strip_suffix(".class") {
                let simple = class.rsplit('/').next().unwrap_or(class);
                simple_names.push(simple.split('$').next().unwrap_or(simple).len());
            }
            self.check_file(&name, &data, &format!("{prefix}{name}"), depth);
        }
        let short = simple_names.iter().filter(|&&n| n <= 2).count();
        if simple_names.len() >= 10 && short * 10 > simple_names.len() * 6 {
            self.add(
                "obfuscation",
                Severity::Medium,
                label,
                format!(
                    "{short} of {} classes have 1-2 character names",
                    simple_names.len()
                ),
            );
        }
        Ok(())
    }

    fn scan_dir(&mut self, root: &Path, dir: &Path) -> Result<(), ScanError> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let ty = entry.file_type()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if ty.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    self.scan_dir(root, &path)?;
                }
            } else if ty.is_file() {
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if rel == "gradle/wrapper/gradle-wrapper.jar" {
                    // The Gradle wrapper downloads and runs Gradle by design, so content rules
                    // would always fire. Its integrity is a checksum question instead.
                    self.add(
                        "gradle-wrapper",
                        Severity::Medium,
                        &rel,
                        "Gradle wrapper JAR: not inspected; verify it against Gradle's published \
                         wrapper checksums (GitHub builds run gradle/actions/wrapper-validation)"
                            .into(),
                    );
                    continue;
                }
                let e = ext(&name);
                if e == "class"
                    || e == "jar"
                    || e == "zip"
                    || TEXT_EXT.contains(&e.as_str())
                    || NATIVE_EXT.contains(&e.as_str())
                {
                    let bytes = std::fs::read(&path)?;
                    self.check_file(&name, &bytes, &rel, 0);
                }
            }
        }
        Ok(())
    }
}

/// Scans a project directory or a JAR file.
///
/// # Errors
/// I/O errors, or a top-level JAR that is not a valid zip.
pub fn scan_path(path: &Path) -> Result<ScanReport, ScanError> {
    let rules = toml::from_str::<RuleFile>(RULES)
        .map_err(|e| std::io::Error::other(format!("embedded scan rules: {e}")))?
        .rule;
    let mut s = Scanner {
        rules,
        findings: Vec::new(),
    };
    if path.is_dir() {
        s.scan_dir(path, path)?;
    } else {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let bytes = std::fs::read(path)?;
        if matches!(ext(&name).as_str(), "jar" | "zip") {
            s.scan_jar(&bytes, "", &name, 0)?;
        } else {
            s.check_file(&name, &bytes, &name, 0);
        }
    }
    s.findings.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.rule.cmp(&b.rule))
    });
    Ok(ScanReport {
        findings: s.findings,
    })
}
