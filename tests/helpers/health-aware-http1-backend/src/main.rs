#![forbid(unsafe_code)]

//! 12.2 D7.1: synthetic health-aware HTTP/1.1 backend for the active-HC
//! differential fixture. Serves a configurable per-path status — by default,
//! 200 on `/` and 503 on `/healthz` (the discriminating health-check signal).
//! This is the project's first synthetic-backend harness primitive (the
//! 06.3 REVIEW I2 down-payment).
//!
//! CLI:
//!   health-aware-http1-backend --port <PORT> [--healthz-status 503]
//!     [--data-status 200] [--data-body "ok\n"]
//!     [--per-path PATH=STATUS[,PATH=STATUS,...]]
//!     [--retry-script PATH=fail:N[,PATH=fail:N,...]]
//!
//! `--per-path` (13.1 D8) takes precedence over the `/healthz` special-case and
//! over the default-path arm; bodies for per-path responses are deterministic
//! per-class bytes (see `per_class_body`).
//!
//! `--retry-script` (16 Task 6, amended): stateful knob with CYCLIC-WINDOW
//! semantics. For each listed path, maintains ONE global request counter
//! (an `AtomicU64`, NOT keyed by source IP) and partitions the request stream
//! into repeating windows of length `N+1`: within each window the first `N`
//! requests return 503 (body `fail\n`) and the final request returns 200
//! (body `ok\n`). Concretely, for request index `idx` (0-based): if
//! `idx % (N+1) < N` → 503, else → 200. For `fail:1` this alternates
//! 503, 200, 503, 200, ….
//!
//! Why cyclic windows instead of a per-source-IP one-shot counter: the
//! differential harness shares one backend between both proxies and drives
//! them SEQUENTIALLY. On macOS, Docker Desktop NATs every container→host
//! connection to source IP 127.0.0.1 — identical to envoy-rust's source IP —
//! so per-source keying is not viable (both proxies collapse into one bucket,
//! and the first-driven proxy burns the entire fail budget). Cyclic windows
//! are NAT-immune: because each proxy's upstream attempts for a single
//! downstream request are consecutive, and the proxies are driven one after
//! the other, each proxy's retry sequence lands in its own fresh window and
//! sees the same fail-then-succeed pattern.
//!
//! Takes precedence over `--per-path` and all default arms for paths listed in
//! the retry-script map.
//!
//! All response shaping is hand-rolled (no framework) so the backend stays
//! transparent — no hidden header behavior. Connection: close per response
//! (the active-HC probe uses a fresh connection per probe).

use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Debug)]
struct Config {
    port: u16,
    healthz_status: u16,
    data_status: u16,
    data_body: Vec<u8>,
    per_path: HashMap<String, u16>,
    /// 16 Task 6 (amended): per-path retry-script map. Value is
    /// `(fail_count, global_counter)`. For paths in this map, requests are
    /// partitioned into repeating windows of length `fail_count + 1`: the
    /// first `fail_count` requests in each window return 503, the last
    /// returns 200 (see `serve`). The counter is a single global `AtomicU64`
    /// per path — NOT keyed by source IP (Docker Desktop NAT collapses all
    /// container source IPs to 127.0.0.1, so per-source keying is not viable).
    retry_script: HashMap<String, (u64, AtomicU64)>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();
    let cfg = parse_args()?;
    let listener = TcpListener::bind(("0.0.0.0", cfg.port))
        .await
        .with_context(|| format!("binding 0.0.0.0:{}", cfg.port))?;
    tracing::info!(port = cfg.port, "health-aware-http1-backend listening");
    let cfg = Arc::new(cfg);
    loop {
        let (stream, peer) = listener.accept().await.context("accept")?;
        let cfg = Arc::clone(&cfg);
        tokio::spawn(async move {
            if let Err(e) = serve(stream, cfg).await {
                tracing::debug!(error=?e, %peer, "connection ended");
            }
        });
    }
}

fn parse_args() -> Result<Config> {
    let mut port: Option<u16> = None;
    let mut healthz_status: u16 = 503;
    let mut data_status: u16 = 200;
    let mut data_body: Vec<u8> = b"ok\n".to_vec();
    let mut per_path: HashMap<String, u16> = HashMap::new();
    let mut retry_script: HashMap<String, (u64, AtomicU64)> = HashMap::new();
    let args: Vec<String> = env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                port = Some(args[i + 1].parse().context("parsing --port")?);
                i += 2;
            }
            "--healthz-status" => {
                healthz_status = args[i + 1].parse().context("parsing --healthz-status")?;
                i += 2;
            }
            "--data-status" => {
                data_status = args[i + 1].parse().context("parsing --data-status")?;
                i += 2;
            }
            "--data-body" => {
                data_body = args[i + 1].as_bytes().to_vec();
                i += 2;
            }
            "--per-path" => {
                per_path = parse_per_path(&args[i + 1])?;
                i += 2;
            }
            "--retry-script" => {
                retry_script = parse_retry_script(&args[i + 1])?;
                i += 2;
            }
            other => bail!("unknown arg: {other}"),
        }
    }
    Ok(Config {
        port: port.context("--port is required")?,
        healthz_status,
        data_status,
        data_body,
        per_path,
        retry_script,
    })
}

/// 13.1 D8: parse `--per-path` flag value: `PATH=STATUS[,PATH=STATUS,...]`.
/// Returns a map; on malformed input returns Err.
fn parse_per_path(s: &str) -> Result<HashMap<String, u16>> {
    let mut out = HashMap::new();
    for entry in s.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let (path, status) = entry
            .split_once('=')
            .with_context(|| format!("per-path entry missing '=': {entry:?}"))?;
        let status: u16 = status
            .parse()
            .with_context(|| format!("per-path status not numeric: {status:?}"))?;
        out.insert(path.to_string(), status);
    }
    Ok(out)
}

/// 16 Task 6 (amended): parse `--retry-script` flag value:
/// `PATH=fail:N[,PATH=fail:N,...]`.
/// Returns a map of path → (fail_count, global counter).
/// N is the per-window fail count: requests to a scripted path are partitioned
/// into repeating windows of length `N+1`, with the first `N` returning 503
/// and the last returning 200. The counter is a single global `AtomicU64`
/// (NOT source-IP keyed): Docker Desktop NAT collapses all container source
/// IPs to 127.0.0.1, so per-source keying cannot distinguish the two proxies.
fn parse_retry_script(s: &str) -> Result<HashMap<String, (u64, AtomicU64)>> {
    let mut out = HashMap::new();
    for entry in s.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let (path, spec) = entry
            .split_once('=')
            .with_context(|| format!("retry-script entry missing '=': {entry:?}"))?;
        let fail_count: u64 = spec
            .strip_prefix("fail:")
            .with_context(|| format!("retry-script spec must be `fail:N`, got: {spec:?}"))?
            .parse()
            .with_context(|| format!("retry-script fail count not numeric in: {spec:?}"))?;
        out.insert(path.to_string(), (fail_count, AtomicU64::new(0)));
    }
    Ok(out)
}

/// 13.1 D8: deterministic per-class body bytes per PLAN-time lock-in #11.
/// 2xx → empty body (preserves existing `--data-body` semantics; per-path 2xx is unusual);
/// 3xx → `"moved\n"`; 4xx-404 → `"not found\n"`; 5xx-500 → `"server error\n"`;
/// 5xx-503 → `"service unavailable\n"`. Other codes → empty body (defensive default).
fn per_class_body(status: u16) -> &'static [u8] {
    match status {
        301 => b"moved\n",
        404 => b"not found\n",
        500 => b"server error\n",
        503 => b"service unavailable\n",
        _ => b"",
    }
}

async fn serve(mut stream: TcpStream, cfg: Arc<Config>) -> Result<()> {
    // 13.1 Task 7 (D9.1): HTTP/1.1 keep-alive support. The 12.2 D7.1
    // single-shot shape (one request, write response with
    // `connection: close`, shutdown) was fine for fixture 0019's
    // active-HC probe (one request per conn) but breaks fixture 0020's
    // H1-pool-reuse assertion (the H1 pool needs the backend to keep
    // the upstream conn alive across N requests so `upstream_cx_total`
    // can read 1 instead of N — the discriminating observable per
    // parent-13 SPEC §6.2 item-iv).
    //
    // Policy: HTTP/1.1 default is keep-alive UNLESS the request carries
    // `Connection: close`. When the request says close (or signals EOF
    // between requests), the loop exits and the helper shuts down the
    // conn. Otherwise the response carries `connection: keep-alive` and
    // the loop reads the next request on the same conn.
    let mut buf: Vec<u8> = Vec::with_capacity(8192);
    loop {
        // Read until we have a full request head (`\r\n\r\n` terminator).
        let head_end = loop {
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos + 4;
            }
            let mut chunk = [0u8; 4096];
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                // Clean EOF between requests — peer closed; this is fine.
                if buf.is_empty() {
                    return Ok(());
                }
                bail!("EOF before headers complete");
            }
            buf.extend_from_slice(&chunk[..n]);
            if buf.len() > 8192 {
                bail!("request headers too large");
            }
        };
        let mut headers_storage = [httparse::EMPTY_HEADER; 32];
        let mut req = httparse::Request::new(&mut headers_storage);
        req.parse(&buf[..head_end])?;
        let path = req.path.unwrap_or("/").to_string();
        // Detect request-side `Connection: close` (case-insensitive). HTTP/1.1
        // default is keep-alive when the header is absent or carries any
        // non-`close` token (e.g. `keep-alive`).
        let request_wants_close = req.headers.iter().any(|h| {
            h.name.eq_ignore_ascii_case("connection")
                && std::str::from_utf8(h.value)
                    .map(|s| s.eq_ignore_ascii_case("close"))
                    .unwrap_or(false)
        });
        // Drop the consumed bytes from the buffer; any pipelined bytes
        // beyond `head_end` carry forward into the next request's window.
        buf.drain(..head_end);
        let (status, body): (u16, Vec<u8>) =
            if let Some((fail_count, counter)) = cfg.retry_script.get(&path) {
                // 16 Task 6 (amended): retry-script arm with CYCLIC-WINDOW
                // semantics. A single global per-path counter (NOT source-IP
                // keyed — Docker Desktop NAT collapses all container source IPs
                // to 127.0.0.1, so per-source keying cannot tell the two proxies
                // apart). `idx` is the 0-based global request index for this path
                // (fetch_add returns the pre-increment value). Requests are
                // partitioned into repeating windows of length `fail_count + 1`:
                // within a window the first `fail_count` requests return 503 and
                // the last returns 200. For fail:1 this alternates 503,200,503,…
                // Because the harness drives the two proxies sequentially and
                // each proxy's upstream attempts for one downstream request are
                // consecutive, each proxy's retry sequence lands in its own fresh
                // window and observes the same fail-then-succeed pattern.
                // CAUTION (latent fragility): if the differential keep-alive
                // driver is ever refactored to drive the two proxies in
                // PARALLEL (e.g. tokio::join), the windows would interleave and
                // retry fixtures using this knob would silently flake.
                let idx = counter.fetch_add(1, Ordering::Relaxed);
                if idx % (*fail_count + 1) < *fail_count {
                    (503u16, b"fail\n".to_vec())
                } else {
                    (200u16, b"ok\n".to_vec())
                }
            } else if let Some(&s) = cfg.per_path.get(&path) {
                // 13.1 D8: per-path mapping wins over default/healthz arms.
                (s, per_class_body(s).to_vec())
            } else if path == "/healthz" {
                (cfg.healthz_status, Vec::new())
            } else {
                (cfg.data_status, cfg.data_body.clone())
            };
        let conn_value = if request_wants_close {
            "close"
        } else {
            "keep-alive"
        };
        let resp = format!(
            "HTTP/1.1 {status} {reason}\r\nserver: health-aware-http1-backend\r\ncontent-length: {len}\r\ncontent-type: text/plain\r\nconnection: {conn_value}\r\n\r\n",
            status = status,
            reason = status_reason(status),
            len = body.len(),
        );
        stream.write_all(resp.as_bytes()).await?;
        if !body.is_empty() {
            stream.write_all(&body).await?;
        }
        stream.flush().await?;
        if request_wants_close {
            let _ = stream.shutdown().await;
            return Ok(());
        }
    }
}

fn status_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        301 => "Moved Permanently",
        404 => "Not Found",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "OK",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_per_path_parses_multiple_entries() {
        let m = parse_per_path("/301=301,/404=404,/500=500").expect("parse");
        assert_eq!(m.get("/301"), Some(&301u16));
        assert_eq!(m.get("/404"), Some(&404u16));
        assert_eq!(m.get("/500"), Some(&500u16));
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn parse_per_path_rejects_malformed() {
        assert!(parse_per_path("notakvpair").is_err());
        assert!(parse_per_path("/x=notanumber").is_err());
    }

    #[test]
    fn per_class_body_returns_deterministic_bytes() {
        assert_eq!(per_class_body(301), b"moved\n".as_slice());
        assert_eq!(per_class_body(404), b"not found\n".as_slice());
        assert_eq!(per_class_body(500), b"server error\n".as_slice());
        assert_eq!(per_class_body(503), b"service unavailable\n".as_slice());
        // Other codes fall through to the empty body (deterministic; tests rely on this).
        assert_eq!(per_class_body(200), b"".as_slice());
    }

    #[test]
    fn parse_retry_script_parses_single_entry() {
        // Cyclic-window semantics: one global counter per path, starting at 0
        // (no requests seen yet).
        let m = parse_retry_script("/retry-success=fail:1").expect("parse");
        assert_eq!(m.len(), 1);
        let (fail_count, counter) = m.get("/retry-success").expect("entry present");
        assert_eq!(*fail_count, 1);
        assert_eq!(
            counter.load(Ordering::Relaxed),
            0,
            "counter must start at 0 before any requests"
        );
    }

    #[test]
    fn retry_script_cyclic_window_modulo_for_fail_1() {
        // fail:1 → window length 2 → alternating 503,200,503,200,…
        // The serve() arm computes `idx % (N+1) < N`; replicate it here.
        let fail_count: u64 = 1;
        let expect_503 = |idx: u64| idx % (fail_count + 1) < fail_count;
        assert!(expect_503(0), "idx 0 → 503");
        assert!(!expect_503(1), "idx 1 → 200");
        assert!(expect_503(2), "idx 2 → 503");
        assert!(!expect_503(3), "idx 3 → 200");
    }

    #[test]
    fn retry_script_cyclic_window_modulo_for_fail_2() {
        // fail:2 → window length 3 → 503,503,200 repeating.
        let fail_count: u64 = 2;
        let expect_503 = |idx: u64| idx % (fail_count + 1) < fail_count;
        assert!(expect_503(0));
        assert!(expect_503(1));
        assert!(!expect_503(2));
        assert!(expect_503(3));
        assert!(expect_503(4));
        assert!(!expect_503(5));
    }

    #[test]
    fn parse_retry_script_parses_multiple_entries() {
        let m = parse_retry_script("/a=fail:2,/b=fail:5").expect("parse");
        assert_eq!(m.len(), 2);
        assert_eq!(m.get("/a").expect("a").0, 2);
        assert_eq!(m.get("/b").expect("b").0, 5);
    }

    #[test]
    fn parse_retry_script_rejects_missing_fail_prefix() {
        assert!(parse_retry_script("/x=3").is_err());
        assert!(parse_retry_script("/x=success:1").is_err());
    }

    #[test]
    fn parse_retry_script_rejects_non_numeric_count() {
        assert!(parse_retry_script("/x=fail:abc").is_err());
    }
}
