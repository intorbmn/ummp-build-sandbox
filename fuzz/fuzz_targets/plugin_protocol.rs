#![no_main]

use libfuzzer_sys::fuzz_target;
use ummp_core::plugins::decode_plugin_response;

fuzz_target!(|data: &[u8]| {
    let _ = decode_plugin_response(data);
});
