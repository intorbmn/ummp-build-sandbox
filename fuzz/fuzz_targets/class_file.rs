#![no_main]

use libfuzzer_sys::fuzz_target;
use ummp_core::bytecode::{remap_class, utf8_constants, ClassMap};

fuzz_target!(|data: &[u8]| {
    if data.len() > 1024 * 1024 {
        return;
    }
    let map = ClassMap::default();
    let _ = remap_class(data, &map, "fuzz.class");
    let _ = utf8_constants(data, "fuzz.class");
});
