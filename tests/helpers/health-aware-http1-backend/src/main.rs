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
//!
//! All response shaping is hand-rolled (no framework) so the backend stays
//! transparent — no hidden header behavior. Connection: close per response
//! (the active-HC probe uses a fresh connection per probe).

use std::env;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Debug, Clone)]
struct Config {
    port: u16,
    healthz_status: u16,
    data_status: u16,
    data_body: Vec<u8>,
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
            other => bail!("unknown arg: {other}"),
        }
    }
    Ok(Config {
        port: port.context("--port is required")?,
        healthz_status,
        data_status,
        data_body,
    })
}

async fn serve(mut stream: TcpStream, cfg: Arc<Config>) -> Result<()> {
    let mut buf = vec![0u8; 8192];
    let mut filled = 0;
    let head_end = loop {
        let n = stream.read(&mut buf[filled..]).await?;
        if n == 0 {
            bail!("EOF before headers complete");
        }
        filled += n;
        if let Some(pos) = buf[..filled].windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
        if filled == buf.len() {
            bail!("request headers too large");
        }
    };
    let mut headers_storage = [httparse::EMPTY_HEADER; 32];
    let mut req = httparse::Request::new(&mut headers_storage);
    req.parse(&buf[..head_end])?;
    let path = req.path.unwrap_or("/").to_string();
    let (status, body): (u16, Vec<u8>) = if path == "/healthz" {
        (cfg.healthz_status, Vec::new())
    } else {
        (cfg.data_status, cfg.data_body.clone())
    };
    let resp = format!(
        "HTTP/1.1 {status} {reason}\r\nserver: health-aware-http1-backend\r\ncontent-length: {len}\r\ncontent-type: text/plain\r\nconnection: close\r\n\r\n",
        status = status,
        reason = status_reason(status),
        len = body.len(),
    );
    stream.write_all(resp.as_bytes()).await?;
    stream.write_all(&body).await?;
    let _ = stream.shutdown().await;
    Ok(())
}

fn status_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        503 => "Service Unavailable",
        _ => "OK",
    }
}
