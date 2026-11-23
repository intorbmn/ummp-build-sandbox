//! Application: bytecode migration (Phase 6.F) for JAR-only mods.
//!
//! Remaps class names in compiled classes without running any external tool:
//! only `CONSTANT_Utf8` entries of the constant pool change (internal names,
//! descriptors and generic signatures), so every index and attribute stays
//! valid. Class files are moved to their new paths inside the JAR. Member
//! (method/field) renames need member mappings and are out of scope.
//!
//! The input JAR is never modified: the result is written to a new file.

use crate::migration::RenameRule;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use thiserror::Error;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

/// Per-entry and per-archive caps for untrusted input.
pub const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 100_000;

#[derive(Debug, Error)]
pub enum BytecodeError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("invalid class file {0}: {1}")]
    InvalidClass(String, String),
    #[error("refusing to write: {0}")]
    Refused(String),
}

/// Internal-name map built from qualified rules (`a.b.C` -> `a/b/C`).
#[derive(Debug, Clone, Default)]
pub struct ClassMap {
    map: BTreeMap<String, String>,
}

impl ClassMap {
    pub fn from_rules(rules: &[RenameRule]) -> Self {
        let map = rules
            .iter()
            .filter(|r| r.is_qualified())
            .map(|r| (r.from.replace('.', "/"), r.to.replace('.', "/")))
            .collect();
        Self { map }
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Remap one Utf8 constant: an exact internal name (or an inner class of
    /// one), or every `L<name>;` / `L<name>$` / `L<name><` inside descriptors
    /// and generic signatures.
    fn remap_utf8(&self, s: &str) -> Option<String> {
        if let Some(to) = self.map.get(s) {
            return Some(to.clone());
        }
        if let Some((outer, inner)) = s.split_once('$') {
            if let Some(to) = self.map.get(outer) {
                return Some(format!("{to}${inner}"));
            }
        }
        if !s.contains('L') {
            return None;
        }
        let mut out = s.to_string();
        for (from, to) in &self.map {
            out = out
                .replace(&format!("L{from};"), &format!("L{to};"))
                .replace(&format!("L{from}$"), &format!("L{to}$"))
                .replace(&format!("L{from}<"), &format!("L{to}<"));
        }
        (out != s).then_some(out)
    }

    /// New path of a `.class` entry, if its class is mapped.
    fn remap_entry_name(&self, name: &str) -> Option<String> {
        let class = name.strip_suffix(".class")?;
        self.remap_utf8(class).map(|c| format!("{c}.class"))
    }
}

fn u16_at(b: &[u8], i: usize, name: &str) -> Result<usize, BytecodeError> {
    b.get(i..i + 2)
        .map(|s| usize::from(u16::from_be_bytes([s[0], s[1]])))
        .ok_or_else(|| BytecodeError::InvalidClass(name.into(), "truncated".into()))
}

/// Remap class names in one class file. Returns `None` when nothing changed.
pub fn remap_class(
    bytes: &[u8],
    map: &ClassMap,
    name: &str,
) -> Result<Option<Vec<u8>>, BytecodeError> {
    let bad = |m: &str| BytecodeError::InvalidClass(name.into(), m.into());
    if bytes.get(0..4) != Some(&[0xCA, 0xFE, 0xBA, 0xBE]) {
        return Err(bad("missing CAFEBABE magic"));
    }
    let count = u16_at(bytes, 8, name)?;
    let mut out = bytes[..10].to_vec();
    let mut i = 10;
    let mut changed = false;
    let mut index = 1;
    while index < count {
        let tag = *bytes.get(i).ok_or_else(|| bad("truncated constant pool"))?;
        let size = match tag {
            1 => {
                let len = u16_at(bytes, i + 1, name)?;
                let raw = bytes
                    .get(i + 3..i + 3 + len)
                    .ok_or_else(|| bad("truncated Utf8"))?;
                // Class names are ASCII; non-UTF-8 (modified UTF-8) entries are kept.
                if let Some(new) = std::str::from_utf8(raw)
                    .ok()
                    .and_then(|s| map.remap_utf8(s))
                {
                    let new_len = u16::try_from(new.len()).map_err(|_| bad("Utf8 too long"))?;
                    out.push(1);
                    out.extend_from_slice(&new_len.to_be_bytes());
                    out.extend_from_slice(new.as_bytes());
                    changed = true;
                    i += 3 + len;
                    index += 1;
                    continue;
                }
                3 + len
            }
            7 | 8 | 16 | 19 | 20 => 3,
            15 => 4,
            3 | 4 | 9 | 10 | 11 | 12 | 17 | 18 => 5,
            5 | 6 => {
                index += 1; // 8-byte constants take two slots
                9
            }
            t => return Err(bad(&format!("unknown constant tag {t}"))),
        };
        out.extend_from_slice(bytes.get(i..i + size).ok_or_else(|| bad("truncated"))?);
        i += size;
        index += 1;
    }
    out.extend_from_slice(&bytes[i..]);
    Ok(changed.then_some(out))
}

/// All `CONSTANT_Utf8` strings of a class file (class names, member names, descriptors and
/// string literals), lossily decoded. Used by the risk scan.
///
/// # Errors
/// [`BytecodeError::InvalidClass`] when the file is not a well-formed class file.
pub fn utf8_constants(bytes: &[u8], name: &str) -> Result<Vec<String>, BytecodeError> {
    let bad = |m: &str| BytecodeError::InvalidClass(name.into(), m.into());
    if bytes.get(0..4) != Some(&[0xCA, 0xFE, 0xBA, 0xBE]) {
        return Err(bad("missing CAFEBABE magic"));
    }
    let count = u16_at(bytes, 8, name)?;
    let (mut i, mut index, mut out) = (10, 1, Vec::new());
    while index < count {
        let tag = *bytes.get(i).ok_or_else(|| bad("truncated constant pool"))?;
        let size = match tag {
            1 => {
                let len = u16_at(bytes, i + 1, name)?;
                let raw = bytes
                    .get(i + 3..i + 3 + len)
                    .ok_or_else(|| bad("truncated Utf8"))?;
                out.push(String::from_utf8_lossy(raw).into_owned());
                3 + len
            }
            7 | 8 | 16 | 19 | 20 => 3,
            15 => 4,
            3 | 4 | 9 | 10 | 11 | 12 | 17 | 18 => 5,
            5 | 6 => {
                index += 1;
                9
            }
            t => return Err(bad(&format!("unknown constant tag {t}"))),
        };
        i += size;
        index += 1;
    }
    Ok(out)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemapStats {
    pub classes_changed: usize,
    pub entries_moved: usize,
    pub signatures_dropped: usize,
}

/// Remap `input` into a new JAR at `output`, applying `stages` in order.
/// Signature files are dropped because remapped classes no longer match them.
pub fn remap_jar(
    input: &Path,
    output: &Path,
    stages: &[ClassMap],
) -> Result<RemapStats, BytecodeError> {
    if output.exists() {
        return Err(BytecodeError::Refused(format!(
            "{} already exists",
            output.display()
        )));
    }
    let mut archive = ZipArchive::new(File::open(input)?)?;
    if archive.len() > MAX_ENTRIES {
        return Err(BytecodeError::Refused("too many entries".into()));
    }
    // create_new: fails instead of replacing a file created concurrently.
    let out_file = File::options().write(true).create_new(true).open(output)?;
    let mut writer = ZipWriter::new(out_file);
    let mut stats = RemapStats::default();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name()?.replace('\\', "/");
        if entry.is_dir() {
            writer.add_directory(name, SimpleFileOptions::default())?;
            continue;
        }
        let upper = name.to_ascii_uppercase();
        if upper.starts_with("META-INF/")
            && [".SF", ".RSA", ".DSA", ".EC"]
                .iter()
                .any(|ext| upper.ends_with(ext))
        {
            stats.signatures_dropped += 1;
            continue;
        }
        if entry.size() > MAX_ENTRY_BYTES {
            return Err(BytecodeError::Refused(format!("{name} is too large")));
        }
        let method = entry.compression();
        let mut data = Vec::new();
        (&mut entry).take(MAX_ENTRY_BYTES).read_to_end(&mut data)?;

        let mut new_name = name.clone();
        if Path::new(&name).extension().is_some_and(|e| e == "class") {
            let mut class_changed = false;
            for map in stages {
                if let Some(remapped) = remap_class(&data, map, &name)? {
                    data = remapped;
                    class_changed = true;
                }
                if let Some(n) = map.remap_entry_name(&new_name) {
                    new_name = n;
                }
            }
            stats.classes_changed += usize::from(class_changed);
            stats.entries_moved += usize::from(new_name != name);
        }
        writer.start_file(
            new_name,
            SimpleFileOptions::default().compression_method(method),
        )?;
        writer.write_all(&data)?;
    }
    writer.finish()?;
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal valid class file: `com/example/old/DemoWidget` extends Object,
    /// one field of its own type, a Long constant (two slots), and an
    /// unrelated class name that must not change.
    fn demo_class() -> Vec<u8> {
        let mut b = vec![0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 52];
        let utf8 = |b: &mut Vec<u8>, s: &str| {
            b.push(1);
            b.extend_from_slice(&u16::try_from(s.len()).unwrap().to_be_bytes());
            b.extend_from_slice(s.as_bytes());
        };
        // constant_pool_count = 10 (entries 1..=9, Long uses 2 slots)
        b.extend_from_slice(&10u16.to_be_bytes());
        utf8(&mut b, "com/example/old/DemoWidget"); // #1
        b.extend_from_slice(&[7, 0, 1]); // #2 Class #1
        utf8(&mut b, "java/lang/Object"); // #3
        b.extend_from_slice(&[7, 0, 3]); // #4 Class #3
        b.extend_from_slice(&[5, 0, 0, 0, 0, 0, 0, 0, 42]); // #5-6 Long
        utf8(&mut b, "Lcom/example/old/DemoWidget;"); // #7 descriptor
        utf8(&mut b, "self"); // #8
        utf8(&mut b, "com/example/old/DemoWidgetExtra"); // #9 must not change
                                                         // access, this, super, interfaces=0
        b.extend_from_slice(&[0, 0x21, 0, 2, 0, 4, 0, 0]);
        // fields_count=1: access, name #8, descriptor #7, attributes=0
        b.extend_from_slice(&[0, 1, 0, 0x02, 0, 8, 0, 7, 0, 0]);
        b.extend_from_slice(&[0, 0, 0, 0]); // methods=0, attributes=0
        b
    }

    /// Bytes after the constant pool in `demo_class`.
    const DEMO_TAIL_LEN: usize = 22;

    fn demo_map() -> ClassMap {
        ClassMap::from_rules(&[RenameRule {
            from: "com.example.old.DemoWidget".into(),
            to: "com.example.demo.DemoWidget".into(),
            kind: "move".into(),
            confidence: 1.0,
            version: "1.17".into(),
        }])
    }

    /// All Utf8 constants, in pool order.
    fn utf8_constants(bytes: &[u8]) -> Vec<String> {
        let count = usize::from(u16::from_be_bytes([bytes[8], bytes[9]]));
        let (mut i, mut idx, mut out) = (10, 1, Vec::new());
        while idx < count {
            match bytes[i] {
                1 => {
                    let len = usize::from(u16::from_be_bytes([bytes[i + 1], bytes[i + 2]]));
                    out.push(String::from_utf8(bytes[i + 3..i + 3 + len].to_vec()).unwrap());
                    i += 3 + len;
                }
                7 => i += 3,
                5 | 6 => {
                    i += 9;
                    idx += 1;
                }
                t => panic!("tag {t}"),
            }
            idx += 1;
        }
        out
    }

    #[test]
    fn test_remap_class_constant_pool() {
        let original = demo_class();
        let out = remap_class(&original, &demo_map(), "DemoWidget.class")
            .unwrap()
            .unwrap();
        assert_eq!(
            utf8_constants(&out),
            [
                "com/example/demo/DemoWidget",
                "java/lang/Object",
                "Lcom/example/demo/DemoWidget;",
                "self",
                "com/example/old/DemoWidgetExtra",
            ]
        );
        // Everything after the constant pool is byte-identical.
        assert_eq!(
            out[out.len() - DEMO_TAIL_LEN..],
            original[original.len() - DEMO_TAIL_LEN..]
        );
    }

    #[test]
    fn test_unmapped_class_unchanged_and_bad_magic_rejected() {
        assert!(remap_class(&demo_class(), &ClassMap::default(), "x")
            .unwrap()
            .is_none());
        assert!(remap_class(b"not a class", &demo_map(), "x").is_err());
    }

    #[test]
    fn test_inner_class_and_signature() {
        let m = demo_map();
        assert_eq!(
            m.remap_utf8("com/example/old/DemoWidget$Inner").unwrap(),
            "com/example/demo/DemoWidget$Inner"
        );
        assert_eq!(
            m.remap_utf8("Ljava/util/List<Lcom/example/old/DemoWidget;>;")
                .unwrap(),
            "Ljava/util/List<Lcom/example/demo/DemoWidget;>;"
        );
    }

    #[test]
    fn test_remap_jar_moves_class_and_keeps_input() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.jar");
        {
            let mut w = ZipWriter::new(File::create(&input).unwrap());
            w.start_file(
                "com/example/old/DemoWidget.class",
                SimpleFileOptions::default(),
            )
            .unwrap();
            w.write_all(&demo_class()).unwrap();
            w.start_file("META-INF/DEMO.SF", SimpleFileOptions::default())
                .unwrap();
            w.write_all(b"sig").unwrap();
            w.start_file("assets/demo/lang/en_us.json", SimpleFileOptions::default())
                .unwrap();
            w.write_all(b"{}").unwrap();
            w.finish().unwrap();
        }
        let before = std::fs::read(&input).unwrap();
        let output = dir.path().join("out.jar");
        let stats = remap_jar(&input, &output, &[demo_map()]).unwrap();
        assert_eq!(
            stats,
            RemapStats {
                classes_changed: 1,
                entries_moved: 1,
                signatures_dropped: 1
            }
        );
        assert_eq!(std::fs::read(&input).unwrap(), before);
        let mut out = ZipArchive::new(File::open(&output).unwrap()).unwrap();
        let mut class = Vec::new();
        out.by_name("com/example/demo/DemoWidget.class")
            .unwrap()
            .read_to_end(&mut class)
            .unwrap();
        assert_eq!(utf8_constants(&class)[0], "com/example/demo/DemoWidget");
        assert!(out.by_name("META-INF/DEMO.SF").is_err());
        assert!(out.by_name("assets/demo/lang/en_us.json").is_ok());
        // never overwrites
        assert!(matches!(
            remap_jar(&input, &output, &[demo_map()]),
            Err(BytecodeError::Refused(_))
        ));
    }
}
