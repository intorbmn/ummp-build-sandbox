#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Some((&route, body)) = data.split_first() {
        ummp_server::fuzz_decode_rest_request(route, body);
    }
});
