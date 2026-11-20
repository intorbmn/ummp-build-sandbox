//! Parallel per-JAR analysis (descriptor, malware-risk scan, technologies) and the risk gate.

use super::{Pack, PackEntry, PackError};
use crate::analyzer::{inspect_jar, ModAnalysis};
use crate::filesystem::LocalFileSystem;
use crate::scan::{scan_path, Finding, Severity};
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Copy, Default)]
pub struct AnalyzeOptions {
    /// Worker threads; `0` means `available_parallelism()`.
    pub threads: usize,
}

struct EntryResult {
    mod_info: Option<ModAnalysis>,
    risk: Option<Severity>,
    findings: Vec<Finding>,
    technologies: Vec<String>,
    warning: Option<String>,
}

/// Caps for the temporary extraction used to detect technologies.
const MAX_TECH_ENTRY_BYTES: u64 = 8 * 1024 * 1024;
const MAX_TECH_TOTAL_BYTES: u64 = 128 * 1024 * 1024;

fn analyze_entry(e: &PackEntry) -> Option<EntryResult> {
    let jar = e.local.as_ref()?;
    let mut r = EntryResult {
        mod_info: None,
        risk: None,
        findings: Vec::new(),
        technologies: Vec::new(),
        warning: None,
    };
    match inspect_jar(jar, &LocalFileSystem) {
        Ok(a) => r.mod_info = Some(a),
        Err(err) => r.warning = Some(format!("{}: descriptor unreadable: {err}", e.path)),
    }
    match scan_path(jar) {
        Ok(s) => {
            r.risk = s.risk();
            r.findings = s.findings;
        }
        Err(err) => {
            let w = format!("{}: scan failed: {err}", e.path);
            r.warning = Some(r.warning.map_or_else(|| w.clone(), |p| format!("{p}; {w}")));
        }
    }
    r.technologies = technologies(jar);
    Some(r)
}

/// Unpacks a JAR (capped, safe names only) and runs the technology adapters over it.
fn technologies(jar: &Path) -> Vec<String> {
    let Ok(dir) = tempfile::tempdir() else {
        return Vec::new();
    };
    let Ok(file) = std::fs::File::open(jar) else {
        return Vec::new();
    };
    let Ok(mut archive) = zip::ZipArchive::new(file) else {
        return Vec::new();
    };
    let mut total = 0u64;
    for i in 0..archive.len() {
        let Ok(mut entry) = archive.by_index(i) else {
            continue;
        };
        if entry.is_dir() {
            continue;
        }
        let Some(dest) = entry
            .enclosed_name()
            .map(Path::to_path_buf)
            .and_then(|rel| crate::filesystem::resolve_within(dir.path(), &rel).ok())
        else {
            continue;
        };
        let mut buf = Vec::new();
        if (&mut entry)
            .take(MAX_TECH_ENTRY_BYTES + 1)
            .read_to_end(&mut buf)
            .is_err()
            || buf.len() as u64 > MAX_TECH_ENTRY_BYTES
        {
            continue;
        }
        total += buf.len() as u64;
        if total > MAX_TECH_TOTAL_BYTES {
            break;
        }
        if let Some(parent) = dest.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                continue;
            }
        }
        let _ = std::fs::write(&dest, &buf);
    }
    let mut techs: Vec<String> = crate::adapter::AdapterRegistry::default()
        .scan_dir(dir.path())
        .into_iter()
        .map(|f| f.tech)
        .collect();
    techs.sort();
    techs.dedup();
    techs
}

/// Reads every local JAR's descriptor, risk findings and technologies, in parallel.
/// Results are applied in entry order, so the outcome does not depend on the thread count.
///
/// # Errors
/// Currently none; unreadable JARs become pack warnings.
pub fn analyze_pack(pack: &mut Pack, opts: &AnalyzeOptions) -> Result<(), PackError> {
    let n = if opts.threads == 0 {
        std::thread::available_parallelism().map_or(1, usize::from)
    } else {
        opts.threads
    };
    let todo: Vec<usize> = pack
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.local.is_some() && e.kind == super::EntryKind::Mod)
        .map(|(i, _)| i)
        .collect();
    let chunk = todo.len().div_ceil(n.max(1)).max(1);
    let entries = &pack.entries;
    let mut results: Vec<(usize, EntryResult)> = std::thread::scope(|s| {
        // Every thread must be spawned before the first join, so the handles are collected.
        #[allow(clippy::needless_collect)]
        let handles: Vec<_> = todo
            .chunks(chunk)
            .map(|idxs| {
                s.spawn(move || {
                    idxs.iter()
                        .filter_map(|&i| analyze_entry(&entries[i]).map(|r| (i, r)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect::<Vec<_>>()
    });
    results.sort_by_key(|(i, _)| *i);
    for (i, r) in results {
        if let Some(w) = r.warning {
            pack.warnings.push(w);
        }
        let e = &mut pack.entries[i];
        e.mod_info = r.mod_info;
        e.risk = r.risk;
        e.findings = r.findings;
        e.technologies = r.technologies;
    }
    Ok(())
}

/// The risk gate: a High finding in any JAR stops the run unless the user passed `--allow-risk`.
///
/// # Errors
/// [`PackError::Refused`] naming the JARs.
pub fn gate(pack: &Pack, allow_risk: bool) -> Result<(), PackError> {
    if allow_risk {
        return Ok(());
    }
    let bad: Vec<&str> = pack
        .entries
        .iter()
        .filter(|e| e.risk == Some(Severity::High))
        .map(|e| e.path.as_str())
        .collect();
    if bad.is_empty() {
        Ok(())
    } else {
        Err(PackError::Refused(format!(
            "High malware risk in {}; review the findings or pass --allow-risk",
            bad.join(", ")
        )))
    }
}
