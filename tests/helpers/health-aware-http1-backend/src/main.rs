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
//! `--retry-script` (16 Task 6): stateful knob. For each listed path, returns
//! 503 (body `fail\n`) for the first N requests to that path from a given
//! source IP, then 200 (body `ok\n`) for all subsequent requests from that
//! source IP. Each distinct client source IP gets its own independent
//! fail-then-succeed sequence — required because the differential harness
//! shares one backend between both proxies (envoy-rust from 127.0.0.1, Envoy-
//! in-Docker from the Docker bridge/NAT address); without per-source keying the
//! two proxies would share a single counter and only the first proxy would ever
//! see 503s. Backed by a per-path `Mutex<HashMap<IpAddr, u64>>` counter map.
//! Takes precedence over `--per-path` and all default arms for paths listed in
//! the retry-script map.
//!
//! All response shaping is hand-rolled (no framework) so the backend stays
//! transparent — no hidden header behavior. Connection: close per response
//! (the active-HC probe uses a fresh connection per probe).

use std::collections::HashMap;
use std::env;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

/// Per-source-IP request counter for a single retry-script path.
/// Maps peer IpAddr → number of requests seen so far from that source.
/// Each key is created on first contact; the Mutex allows concurrent
/// connections (from different source IPs) to bump their own counters
/// without blocking each other for longer than one map insert+increment.
type SourceCounters = Mutex<HashMap<IpAddr, u64>>;

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
    /// 16 Task 6: per-path retry-script map. Value is (fail_count, per-source counters).
    /// For paths in this map: return 503 for the first `fail_count` requests from a
    /// given source IP, then 200 thereafter. The inner map keys are peer IpAddrs;
    /// each source IP gets its own independent fail-then-succeed sequence.
    retry_script: HashMap<String, (u64, SourceCounters)>,
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
        let peer_ip = peer.ip();
        let cfg = Arc::clone(&cfg);
        tokio::spawn(async move {
            if let Err(e) = serve(stream, cfg, peer_ip).await {
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
    let mut retry_script: HashMap<String, (u64, SourceCounters)> = HashMap::new();
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
/// Returns a map of path → (fail_count, SourceCounters).
/// N is the per-source-IP fail count — each distinct client source IP gets its
/// own independent fail-then-succeed sequence. Required because the differential
/// harness shares one backend between both proxies (envoy-rust from 127.0.0.1;
/// Envoy-in-Docker from the Docker bridge/NAT address); without per-source
/// keying the two proxies would share a single counter and only the first proxy
/// would ever see the configured 503 failures.
fn parse_retry_script(s: &str) -> Result<HashMap<String, (u64, SourceCounters)>> {
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
        out.insert(path.to_string(), (fail_count, Mutex::new(HashMap::new())));
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

async fn serve(mut stream: TcpStream, cfg: Arc<Config>, peer_ip: IpAddr) -> Result<()> {
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
            if let Some((fail_count, counters)) = cfg.retry_script.get(&path) {
                // 16 Task 6 (amended): retry-script arm. Counter is keyed per
                // (source IP, path) so each distinct client (e.g. envoy-rust on
                // 127.0.0.1 vs. Envoy-in-Docker on the bridge NAT address) gets
                // its own independent fail-then-succeed sequence. The Mutex guard
                // is held only for the duration of the counter bump — negligible
                // contention in a test helper. The pre-increment value (prev) is
                // the zero-based index of this request from this source:
                // 0 = first request, 1 = second, …. Return 503 for indices 0
                // through fail_count-1, then 200 thereafter.
                let prev = {
                    let mut map = counters.lock().expect("retry-script counter lock");
                    let cnt = map.entry(peer_ip).or_insert(0);
                    let prev = *cnt;
                    *cnt += 1;
                    prev
                };
                if prev < *fail_count {
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
        // Per-source semantics: each source IP gets its own independent counter;
        // the Mutex'd HashMap starts empty (no IPs have made requests yet).
        let m = parse_retry_script("/retry-success=fail:1").expect("parse");
        assert_eq!(m.len(), 1);
        let (fail_count, counters) = m.get("/retry-success").expect("entry present");
        assert_eq!(*fail_count, 1);
        assert!(
            counters.lock().unwrap().is_empty(),
            "counter map must start empty before any requests"
        );
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
