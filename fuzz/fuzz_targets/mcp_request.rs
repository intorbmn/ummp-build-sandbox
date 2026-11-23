#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Some((&tool, body)) = data.split_first() {
        ummp_server::mcp::fuzz_decode_mcp_request(tool, body);
    }
});
