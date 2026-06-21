#![no_main]
#![forbid(unsafe_code)]

use libfuzzer_sys::fuzz_target;

// Split the input on `\n` into multiple sub-slices, mirroring the multi-header
// coalescing surface (`parse_cdn_loop` takes a slice of byte-slices). Neither
// the parser nor the counter may panic on arbitrary bytes — they return
// `Ok`/`Err` only; libfuzzer catches any panic/abort.
fuzz_target!(|data: &[u8]| {
    let slices: Vec<&[u8]> = data.split(|&b| b == b'\n').collect();
    if let Ok(parsed) = envoy_filter::cdn_loop::parse_cdn_loop(&slices) {
        let _ = envoy_filter::cdn_loop::count_cdn_id(b"mycdn.example", &parsed);
    }
});
