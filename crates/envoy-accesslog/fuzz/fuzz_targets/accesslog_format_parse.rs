#![no_main]
#![forbid(unsafe_code)]

use libfuzzer_sys::fuzz_target;

// §7.4 fuzz target over the access-log command-operator format-string parser
// (`envoy_accesslog::parse_format`). The parser must NEVER panic on arbitrary
// input — it returns `Ok`/`Err` only; libfuzzer catches any panic/abort.
fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = envoy_accesslog::parse_format(s);
    }
});
