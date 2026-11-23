#![no_main]

use libfuzzer_sys::fuzz_target;
use std::fs::File;
use ummp_core::port::stage_source;
use zip::ZipArchive;

const MAX_INPUT: usize = 2 * 1024 * 1024;
const MAX_EXPANDED: u64 = 2 * 1024 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_INPUT {
        return;
    }
    let Ok(temp) = tempfile::tempdir() else {
        return;
    };
    let archive_path = temp.path().join("input.zip");
    if std::fs::write(&archive_path, data).is_err() {
        return;
    }
    let Ok(file) = File::open(&archive_path) else {
        return;
    };
    let Ok(mut archive) = ZipArchive::new(file) else {
        return;
    };
    if archive.len() > 128 {
        return;
    }
    let mut expanded = 0_u64;
    for index in 0..archive.len() {
        let Ok(entry) = archive.by_index(index) else {
            return;
        };
        if entry.size() > MAX_EXPANDED || entry.compressed_size() > MAX_INPUT as u64 {
            return;
        }
        let Some(next) = expanded.checked_add(entry.size()) else {
            return;
        };
        expanded = next;
        if expanded > MAX_EXPANDED {
            return;
        }
    }
    let _ = stage_source(&archive_path, &temp.path().join("staging"));
});
