//! Finds an installed JDK of the major version a mod needs.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Major version from `java -version` output: `"21.0.2"` gives 21, `"1.8.0_392"` gives 8.
#[must_use]
pub fn parse_java_major(output: &str) -> Option<u32> {
    let version = output.split('"').nth(1)?;
    let mut parts = version.split(['.', '_', '-', '+']);
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

/// Returns the first candidate JDK home whose `bin/java` reports `required`.
#[must_use]
pub fn find_jdk_in(required: u32, candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|home| java_bin(home).is_file() && reported_major(home) == Some(required))
        .cloned()
}

/// Looks in `JAVA_HOME`, then common install folders.
#[must_use]
pub fn find_jdk(required: u32) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = std::env::var_os("JAVA_HOME")
        .map(PathBuf::from)
        .into_iter()
        .collect();
    let mut roots = vec![
        PathBuf::from("/usr/lib/jvm"),
        PathBuf::from("/Library/Java/JavaVirtualMachines"),
        PathBuf::from(r"C:\Program Files\Java"),
        PathBuf::from(r"C:\Program Files\Eclipse Adoptium"),
        PathBuf::from(r"C:\Program Files\Microsoft"),
        PathBuf::from(r"C:\Program Files\Zulu"),
    ];
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        roots.push(PathBuf::from(home).join(".jdks"));
    }
    for root in roots {
        for e in std::fs::read_dir(&root).into_iter().flatten().flatten() {
            let p = e.path();
            // macOS bundles keep the JDK home in Contents/Home.
            let mac = p.join("Contents/Home");
            candidates.push(if mac.is_dir() { mac } else { p });
        }
    }
    find_jdk_in(required, &candidates)
}

fn java_bin(home: &Path) -> PathBuf {
    home.join("bin")
        .join(if cfg!(windows) { "java.exe" } else { "java" })
}

fn reported_major(home: &Path) -> Option<u32> {
    let out = Command::new(java_bin(home)).arg("-version").output().ok()?;
    parse_java_major(&String::from_utf8_lossy(&out.stderr))
        .or_else(|| parse_java_major(&String::from_utf8_lossy(&out.stdout)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modern_and_legacy_versions() {
        assert_eq!(
            parse_java_major("openjdk version \"21.0.2\" 2024-01-16"),
            Some(21)
        );
        assert_eq!(parse_java_major("java version \"1.8.0_392\""), Some(8));
        assert_eq!(
            parse_java_major("openjdk version \"17\" 2021-09-14"),
            Some(17)
        );
        assert_eq!(parse_java_major("no version here"), None);
    }

    #[cfg(unix)]
    #[test]
    fn find_jdk_reads_candidate_home() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let bin = d.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let java = bin.join("java");
        std::fs::write(
            &java,
            "#!/bin/sh\necho 'openjdk version \"21.0.2\" 2024-01-16' >&2\n",
        )
        .unwrap();
        std::fs::set_permissions(&java, std::fs::Permissions::from_mode(0o755)).unwrap();
        let homes = vec![PathBuf::from("/nonexistent/jdk"), d.path().to_path_buf()];
        assert_eq!(find_jdk_in(21, &homes), Some(d.path().to_path_buf()));
        assert_eq!(find_jdk_in(17, &homes), None);
    }
}
