//! Parallel per-JAR analysis (descriptor, malware-risk scan, technologies) and the risk gate.

use super::{Pack, PackEntry, PackError};
use crate::analyzer::{inspect_jar, ModAnalysis};
use crate::cache::{rules_sha256, CacheKey, CacheKind};
use crate::filesystem::LocalFileSystem;
use crate::knowledge::KnowledgeBase;
use crate::scan::{scan_path, Finding, Severity};
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Copy, Default)]
pub struct AnalyzeOptions {
    /// Worker threads; `0` means `available_parallelism()`.
    pub threads: usize,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CachedJarAnalysis {
    mod_info: ModAnalysis,
    technologies: Vec<String>,
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

/// Cached per-JAR analysis used by pack migration. Cache I/O is best-effort:
/// a cache failure never prevents a fresh analysis from completing.
pub fn analyze_pack_cached(
    pack: &mut Pack,
    opts: &AnalyzeOptions,
    kb: &KnowledgeBase,
) -> Result<(), PackError> {
    let n = if opts.threads == 0 {
        std::thread::available_parallelism().map_or(1, usize::from)
    } else {
        opts.threads
    };
    let todo: Vec<usize> = pack
        .entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.local.is_some() && entry.kind == super::EntryKind::Mod)
        .map(|(index, _)| index)
        .collect();

    let mut results = Vec::new();
    let mut misses = Vec::new();
    for &index in &todo {
        let entry = &pack.entries[index];
        let Some(jar) = entry.local.as_deref() else {
            continue;
        };
        let Ok(bytes) = std::fs::read(jar) else {
            misses.push(index);
            continue;
        };
        let analysis_rules = rules_sha256(
            "jar-descriptor-and-adapters-v1",
            [("descriptor-parser", b"v1".as_slice())],
        );
        let analysis_key = CacheKey::new(CacheKind::Analysis, &bytes, analysis_rules);
        let scan_rules = rules_sha256(
            "static-scan-engine-v1",
            [(
                "scan_rules.toml",
                include_bytes!("../scan_rules.toml").as_slice(),
            )],
        );
        let scan_key = CacheKey::new(CacheKind::Scan, &bytes, scan_rules);

        let cached_analysis = kb
            .cache_get::<CachedJarAnalysis>(&analysis_key)
            .ok()
            .flatten()
            .filter(|value| !value.mod_info.metadata.mod_id.trim().is_empty());
        let cached_scan = kb
            .cache_get::<crate::scan::ScanReport>(&scan_key)
            .ok()
            .flatten();
        if let (Some(mut cached_analysis), Some(scan)) = (cached_analysis, cached_scan) {
            // The serialized result is content-based, but this path is location-specific.
            cached_analysis.mod_info.jar_path = Some(jar.to_path_buf());
            results.push((
                index,
                EntryResult {
                    mod_info: Some(cached_analysis.mod_info),
                    risk: scan.risk(),
                    findings: scan.findings,
                    technologies: cached_analysis.technologies,
                    warning: None,
                },
            ));
        } else {
            misses.push(index);
        }
    }

    let chunk = misses.len().div_ceil(n.max(1)).max(1);
    let entries = &pack.entries;
    let computed: Vec<(usize, EntryResult)> = std::thread::scope(|scope| {
        #[allow(clippy::needless_collect)]
        let handles: Vec<_> = misses
            .chunks(chunk)
            .map(|indices| {
                scope.spawn(move || {
                    indices
                        .iter()
                        .filter_map(|&index| {
                            analyze_entry(&entries[index]).map(|result| (index, result))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap_or_default())
            .collect()
    });

    // Persist only complete successful results. Cache failures are intentionally non-fatal.
    for (index, result) in &computed {
        if result.warning.is_some() || result.mod_info.is_none() {
            continue;
        }
        let Some(jar) = pack.entries[*index].local.as_deref() else {
            continue;
        };
        let Ok(bytes) = std::fs::read(jar) else {
            continue;
        };
        let analysis_key = CacheKey::new(
            CacheKind::Analysis,
            &bytes,
            rules_sha256(
                "jar-descriptor-and-adapters-v1",
                [("descriptor-parser", b"v1".as_slice())],
            ),
        );
        let scan_key = CacheKey::new(
            CacheKind::Scan,
            &bytes,
            rules_sha256(
                "static-scan-engine-v1",
                [(
                    "scan_rules.toml",
                    include_bytes!("../scan_rules.toml").as_slice(),
                )],
            ),
        );
        if let Some(mod_info) = &result.mod_info {
            let _ = kb.cache_put(
                &analysis_key,
                &CachedJarAnalysis {
                    mod_info: mod_info.clone(),
                    technologies: result.technologies.clone(),
                },
            );
        }
        let _ = kb.cache_put(
            &scan_key,
            &crate::scan::ScanReport {
                findings: result.findings.clone(),
            },
        );
    }
    results.extend(computed);
    results.sort_by_key(|(index, _)| *index);
    for (index, result) in results {
        apply_entry_result(&mut pack.entries[index], result, &mut pack.warnings);
    }
    Ok(())
}

fn apply_entry_result(entry: &mut PackEntry, result: EntryResult, warnings: &mut Vec<String>) {
    if let Some(warning) = result.warning {
        warnings.push(warning);
    }
    entry.mod_info = result.mod_info;
    entry.risk = result.risk;
    entry.findings = result.findings;
    entry.technologies = result.technologies;
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

#[cfg(test)]
mod cache_tests {
    use super::*;
    use crate::modpack::{EntryKind, PackFormat};
    use tempfile::tempdir;

    #[test]
    fn second_analyze_hits_cache_and_is_identical() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/fabric-a.jar");
        let work = tempdir().unwrap();
        let jar = work.path().join("fabric-a.jar");
        std::fs::copy(source, &jar).unwrap();
        let kb = KnowledgeBase::open(&work.path().join("knowledge.db")).unwrap();

        let make_pack = || {
            let mut entry = PackEntry::new("mods/fabric-a.jar", EntryKind::Mod);
            entry.local = Some(jar.clone());
            Pack {
                name: "cache-test".into(),
                version: "1".into(),
                format: PackFormat::Folder,
                mc: None,
                loader: None,
                loader_version: None,
                root: work.path().to_path_buf(),
                entries: vec![entry],
                overrides: Vec::new(),
                warnings: Vec::new(),
            }
        };

        let mut first = make_pack();
        analyze_pack_cached(&mut first, &AnalyzeOptions::default(), &kb).unwrap();
        let bytes = std::fs::read(&jar).unwrap();
        let analysis_key = CacheKey::new(
            CacheKind::Analysis,
            &bytes,
            rules_sha256(
                "jar-descriptor-and-adapters-v1",
                [("descriptor-parser", b"v1".as_slice())],
            ),
        );
        let scan_key = CacheKey::new(
            CacheKind::Scan,
            &bytes,
            rules_sha256(
                "static-scan-engine-v1",
                [(
                    "scan_rules.toml",
                    include_bytes!("../scan_rules.toml").as_slice(),
                )],
            ),
        );
        assert!(kb
            .cache_get::<CachedJarAnalysis>(&analysis_key)
            .unwrap()
            .is_some());
        assert!(kb
            .cache_get::<crate::scan::ScanReport>(&scan_key)
            .unwrap()
            .is_some());

        let expected = serde_json::to_value(&first.entries[0]).unwrap();

        let mut second = make_pack();
        analyze_pack_cached(&mut second, &AnalyzeOptions::default(), &kb).unwrap();
        let actual = serde_json::to_value(&second.entries[0]).unwrap();
        assert_eq!(actual, expected);
        assert!(second.entries[0].mod_info.is_some());
    }
}
