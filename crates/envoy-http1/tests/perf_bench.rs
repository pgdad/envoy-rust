//! Self-contained perf micro-benchmarks for the per-request-allocation trims
//! (uses only the crate's public API; no external bench dep). Run with:
//!   cargo test -p envoy-http1 --release --test perf_bench -- --ignored --nocapture
//!
//! These are `#[ignore]`d so the normal `cargo test` run never pays for them.

use bytes::Bytes;
use envoy_http1::{Http1Response, Response};
use std::io::IoSlice;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;
use tokio::io::AsyncWrite;

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

/// A discarding sink that advertises vectored support (like `TcpStream`) and
/// consumes every byte offered without copying it anywhere. It isolates the
/// *userspace* serialization cost: the coalesced path pays a body memcpy into
/// its scratch buffer regardless of the sink, while the vectored path does not.
/// The syscall shape is identical to production (one `writev` per response),
/// so what this bench measures is exactly the memcpy that vectored I/O removes.
struct NullVecSink {
    bytes: u64,
}

impl AsyncWrite for NullVecSink {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        self.bytes += buf.len() as u64;
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<std::io::Result<usize>> {
        let n: usize = bufs.iter().map(|s| s.len()).sum();
        self.bytes += n as u64;
        Poll::Ready(Ok(n))
    }

    fn is_write_vectored(&self) -> bool {
        true
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/// A byte-faithful copy of the **pre-vectored** `write_to_buf`: it always
/// coalesces head + body into one scratch buffer — the body memcpy this phase
/// removes above the threshold — then issues one write. Mirrors the real prior
/// code path (single up-front reserve including the body; the same per-field
/// status-line building), so the below-threshold comparison against the shipped
/// writer is honest parity and the above-threshold delta is purely the memcpy.
/// (The bench only uses status 200, whose canonical reason is `OK`.)
async fn coalesced_write_to_buf<W>(
    resp: &Response,
    w: &mut W,
    buf: &mut Vec<u8>,
) -> std::io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;
    let reason = resp.reason.unwrap_or("OK");
    buf.clear();
    buf.reserve(
        64 + resp
            .headers
            .iter()
            .map(|(n, v)| n.len() + v.len() + 4)
            .sum::<usize>()
            + resp.body.len(),
    );
    buf.extend_from_slice(b"HTTP/1.1 ");
    buf.extend_from_slice(resp.status.to_string().as_bytes());
    buf.push(b' ');
    buf.extend_from_slice(reason.as_bytes());
    buf.extend_from_slice(b"\r\n");
    for (name, value) in &resp.headers {
        buf.extend_from_slice(name.as_bytes());
        buf.extend_from_slice(b": ");
        buf.extend_from_slice(value.as_bytes());
        buf.extend_from_slice(b"\r\n");
    }
    buf.extend_from_slice(b"\r\n");
    buf.extend_from_slice(&resp.body); // the body memcpy the vectored path elides
    w.write_all(buf).await?;
    w.flush().await?;
    Ok(())
}

fn response_with_body(len: usize) -> Response {
    Response {
        status: 200,
        reason: None,
        headers: vec![
            ("server".to_string(), "envoy-rust".to_string()),
            (
                "date".to_string(),
                "Sun, 06 Nov 1994 08:49:37 GMT".to_string(),
            ),
            ("content-length".to_string(), len.to_string()),
            (
                "content-type".to_string(),
                "application/octet-stream".to_string(),
            ),
            ("connection".to_string(), "keep-alive".to_string()),
        ],
        body: Bytes::from(vec![b'x'; len]),
    }
}

#[test]
#[ignore]
fn bench_response_write_coalesced_vs_vectored() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    // `old` is the pre-change coalesced writer (always memcpy the body). `new`
    // is the shipped `write_to_buf`: it coalesces below VECTORED_BODY_THRESHOLD
    // (1 KiB) — so small sizes are parity — and goes vectored at/above it, where
    // eliding the body memcpy is a clear win. Same-or-better on every size.
    println!("RESPONSE old (coalesced) vs new (adaptive: coalesce <1KiB, writev >=1KiB):");
    println!(
        "{:>10}  {:>12}  {:>12}  {:>8}",
        "body", "old", "new", "speedup"
    );
    for &size in &[
        13usize, 256, 512, 768, 1024, 1536, 2048, 4096, 65_536, 1_048_576,
    ] {
        let resp = response_with_body(size);
        // Total bytes copied bounded to ~256 MiB per config so large bodies
        // stay fast; small bodies get enough iterations to be stable.
        let iters: u64 = (256 * 1024 * 1024 / size.max(1)).clamp(50_000, 3_000_000) as u64;

        let coalesced = rt.block_on(async {
            let mut sink = NullVecSink { bytes: 0 };
            let mut buf: Vec<u8> = Vec::new();
            let start = Instant::now();
            for _ in 0..iters {
                coalesced_write_to_buf(&resp, &mut sink, &mut buf)
                    .await
                    .unwrap();
            }
            std::hint::black_box(sink.bytes);
            start.elapsed()
        });

        let vectored = rt.block_on(async {
            let mut sink = NullVecSink { bytes: 0 };
            let mut buf: Vec<u8> = Vec::new();
            let start = Instant::now();
            for _ in 0..iters {
                Http1Response::write_to_buf(&resp, &mut sink, &mut buf)
                    .await
                    .unwrap();
            }
            std::hint::black_box(sink.bytes);
            start.elapsed()
        });

        let c_ns = coalesced.as_nanos() as f64 / iters as f64;
        let v_ns = vectored.as_nanos() as f64 / iters as f64;
        let label = if size >= 1024 {
            format!("{} KiB", size / 1024)
        } else {
            format!("{size} B")
        };
        println!(
            "{:>10}  {:>9.1} ns  {:>9.1} ns  {:>7.2}x",
            label,
            c_ns,
            v_ns,
            c_ns / v_ns
        );
    }
}
