#![forbid(unsafe_code)]

//! `tcp-echo-server` — a minimal localhost-only echo server for the envoy-rust
//! differential harness. Sub-phase 02.2's fixture 0003 will dial it; sub-phase
//! 02.1 lands it in isolation so its own tests run under 02.1's CI gate before
//! composition with `TcpProxyBackend` in 02.2 (SPEC §D3).

fn main() {
    // Populated in Task 10.
    unimplemented!("tcp-echo-server runtime lands in Task 10");
}
