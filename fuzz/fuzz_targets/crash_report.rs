#![no_main]

use libfuzzer_sys::fuzz_target;
use ummp_core::build::logs::parse_crash_report;

fuzz_target!(|data: &[u8]| {
    if data.len() > 1024 * 1024 {
        return;
    }
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = parse_crash_report(text);
    }
});
