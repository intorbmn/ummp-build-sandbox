#![no_main]

use libfuzzer_sys::fuzz_target;
use std::fs::File;
use ummp_core::modpack::formats::read_mrpack;
use ummp_core::modpack::{ImportOptions, Limits};
use zip::ZipArchive;

fuzz_target!(|data: &[u8]| {
    if data.len() > 2 * 1024 * 1024 {
        return;
    }
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let archive_path = temp.path().join("input.mrpack");
    if std::fs::write(&archive_path, data).is_err() {
        return;
    }
    let Ok(file) = File::open(&archive_path) else {
        return;
    };
    let Ok(archive) = ZipArchive::new(file) else {
        return;
    };
    let options = ImportOptions {
        limits: Limits {
            max_entry_bytes: 1024 * 1024,
            max_total_bytes: 2 * 1024 * 1024,
            max_entries: 128,
        },
        ..ImportOptions::default()
    };
    let _ = read_mrpack(archive, &temp.path().join("work"), &options);
});
