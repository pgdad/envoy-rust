#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let _ = envoy_http2::grpc::decode_health_check_response(data);
});
