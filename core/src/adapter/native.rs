//! Native-library report: which operating systems a mod ships native libraries for, and
//! libraries it loads (`System.loadLibrary`, JNA `Native.load`) but does not bundle.

use super::{AdapterContext, Handling, Level, TechAdapter, TechFinding};
use std::collections::BTreeSet;

const OSES: [&str; 3] = ["windows", "linux", "macos"];

/// `(os, library name)` for a native library file name, e.g. `libdemo.so` → `("linux", "demo")`.
fn native_lib(name: &str) -> Option<(&'static str, String)> {
    let (stem, ext) = name.rsplit_once('.')?;
    let os = match ext.to_ascii_lowercase().as_str() {
        "dll" => "windows",
        "so" => "linux",
        "dylib" | "jnilib" => "macos",
        _ => return None,
    };
    let lib = if os == "windows" {
        stem
    } else {
        stem.strip_prefix("lib").unwrap_or(stem)
    };
    Some((os, lib.to_string()))
}

/// The first string literal after `call` on `line`.
fn loaded_name<'a>(line: &'a str, call: &str) -> Option<&'a str> {
    let rest = &line[line.find(call)? + call.len()..];
    let start = rest.find('"')? + 1;
    let len = rest[start..].find('"')?;
    Some(&rest[start..start + len])
}

fn missing(present: &BTreeSet<&str>) -> Vec<&'static str> {
    OSES.iter()
        .copied()
        .filter(|o| !present.contains(o))
        .collect()
}

pub struct NativeAdapter;

impl TechAdapter for NativeAdapter {
    fn id(&self) -> &'static str {
        "native-libs"
    }

    fn level(&self) -> Level {
        Level::Report
    }

    fn findings(&self, cx: &AdapterContext<'_>) -> Vec<TechFinding> {
        let bundled: Vec<(&'static str, String, String)> = cx
            .files
            .iter()
            .filter_map(|rel| {
                let name = rel.file_name()?.to_string_lossy();
                let (os, lib) = native_lib(&name)?;
                Some((os, lib, rel.to_string_lossy().replace('\\', "/")))
            })
            .collect();
        let mut out = Vec::new();
        if let Some((_, _, first)) = bundled.first() {
            let present: BTreeSet<&str> = bundled.iter().map(|(o, _, _)| *o).collect();
            let gaps = missing(&present);
            if !gaps.is_empty() {
                out.push(TechFinding {
                    tech: self.id().into(),
                    handling: Handling::Manual,
                    file: first.clone(),
                    line: None,
                    message: format!(
                        "native libraries bundled for {}; missing: {}",
                        present.iter().copied().collect::<Vec<_>>().join(", "),
                        gaps.join(", ")
                    ),
                });
            }
        }
        let sources = cx.files.iter().filter(|r| {
            matches!(
                r.extension().and_then(|e| e.to_str()),
                Some("java" | "kt" | "scala" | "groovy")
            )
        });
        for rel in sources {
            let Some(text) = cx.text(rel) else { continue };
            for (i, line) in text.lines().enumerate() {
                let Some(lib) = loaded_name(line, "System.loadLibrary(")
                    .or_else(|| loaded_name(line, "Native.load("))
                else {
                    continue;
                };
                let present: BTreeSet<&str> = bundled
                    .iter()
                    .filter(|(_, l, _)| l == lib)
                    .map(|(o, _, _)| *o)
                    .collect();
                let gaps = missing(&present);
                if !gaps.is_empty() {
                    out.push(TechFinding {
                        tech: self.id().into(),
                        handling: Handling::Manual,
                        file: rel.to_string_lossy().replace('\\', "/"),
                        line: u32::try_from(i + 1).ok(),
                        message: format!(
                            "library '{lib}' is loaded but not bundled for: {}",
                            gaps.join(", ")
                        ),
                    });
                }
            }
        }
        out
    }
}
