//! Self-contained perf micro-benchmarks for the per-request-allocation trims
//! (uses only the crate's public API; no external bench dep). Run with:
//!   cargo test -p envoy-http1 --release --test perf_bench -- --ignored --nocapture
//!
//! These are `#[ignore]`d so the normal `cargo test` run never pays for them.

use bytes::Bytes;
use envoy_http1::{Http1Response, Response};
use std::time::Instant;

/// Total calls per single-thread run.
const N_SINGLE: u64 = 20_000_000;
/// Per-thread calls in the contended run.
const N_PER_THREAD: u64 = 5_000_000;

#[test]
#[ignore]
fn bench_date_single_thread() {
    // warm the cache
    let _ = envoy_http1::date::now_imf_fixdate();
    let start = Instant::now();
    let mut acc = 0usize;
    for _ in 0..N_SINGLE {
        acc += envoy_http1::date::now_imf_fixdate().len();
    }
    let elapsed = start.elapsed();
    std::hint::black_box(acc);
    let ns_per = elapsed.as_nanos() as f64 / N_SINGLE as f64;
    let mops = N_SINGLE as f64 / elapsed.as_secs_f64() / 1e6;
    println!(
        "DATE single-thread: {N_SINGLE} calls in {:?} = {ns_per:.1} ns/call, {mops:.2} Mops/s",
        elapsed
    );
}

#[test]
#[ignore]
fn bench_date_contended() {
    // The headline metric: aggregate throughput with N worker threads all
    // calling now_imf_fixdate() concurrently. A global Mutex serializes them;
    // a thread-local cache does not.
    for threads in [1usize, 2, 4, 8, 16] {
        // warm each thread's cache implicitly via the first iteration
        let start = Instant::now();
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                std::thread::spawn(move || {
                    let mut acc = 0usize;
                    for _ in 0..N_PER_THREAD {
                        acc += envoy_http1::date::now_imf_fixdate().len();
                    }
                    acc
                })
            })
            .collect();
        let mut total = 0usize;
        for h in handles {
            total += h.join().unwrap();
        }
        std::hint::black_box(total);
        let elapsed = start.elapsed();
        let ops = threads as u64 * N_PER_THREAD;
        let mops = ops as f64 / elapsed.as_secs_f64() / 1e6;
        println!(
            "DATE contended: {threads:>2} threads x {N_PER_THREAD} = {ops} calls in {:>10?} = {mops:>7.2} Mops/s",
            elapsed
        );
    }
}

/// A representative small proxied response (the shape the bench backend returns).
fn sample_response() -> Response {
    Response {
        status: 200,
        reason: None,
        headers: vec![
            ("server".to_string(), "envoy-rust".to_string()),
            (
                "date".to_string(),
                "Sun, 06 Nov 1994 08:49:37 GMT".to_string(),
            ),
            ("content-length".to_string(), "13".to_string()),
            ("content-type".to_string(), "text/plain".to_string()),
            ("connection".to_string(), "keep-alive".to_string()),
        ],
        body: Bytes::from_static(b"hello, world\n"),
    }
}

const N_RESP: u64 = 10_000_000;

#[test]
#[ignore]
fn bench_response_write_fresh_vs_reused() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let resp = sample_response();

    // A) fresh allocation per response (old `write_to` behavior).
    let a = rt.block_on(async {
        let mut sink = tokio::io::sink();
        let start = Instant::now();
        for _ in 0..N_RESP {
            Http1Response::write_to(&resp, &mut sink).await.unwrap();
        }
        start.elapsed()
    });

    // B) one reused per-connection buffer (new `write_to_buf` behavior).
    let b = rt.block_on(async {
        let mut sink = tokio::io::sink();
        let mut buf: Vec<u8> = Vec::new();
        let start = Instant::now();
        for _ in 0..N_RESP {
            Http1Response::write_to_buf(&resp, &mut sink, &mut buf)
                .await
                .unwrap();
        }
        start.elapsed()
    });

    let a_ns = a.as_nanos() as f64 / N_RESP as f64;
    let b_ns = b.as_nanos() as f64 / N_RESP as f64;
    println!("RESPONSE write_to     (fresh Vec/resp): {a_ns:.1} ns/resp");
    println!("RESPONSE write_to_buf (reused buffer):  {b_ns:.1} ns/resp");
    println!(
        "RESPONSE reuse speedup: {:.2}x ({:.1}% less time)",
        a_ns / b_ns,
        (a_ns - b_ns) / a_ns * 100.0
    );
}
