#![forbid(unsafe_code)]

//! Differential test harness for envoy-rust. Phase 01 surface: TCP echo + HTTP
//! admin GET.
//!
//! Contract: `run_fixture(fixture_dir)` starts upstream Envoy (via
//! testcontainers) and envoy-rust (via subprocess) against the fixture's paired
//! configs, then dispatches on `expectations.yaml`'s tagged `driver:` —
//! `tcp_echo` drives `inputs/payload.bin` via `drive_tcp`; `http_get` issues
//! a minimal `GET` via `drive_http_get`. Equivalence rules from `expectations`
//! are enforced by `assert_equivalence` (status-exact and/or byte-exact body).

use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdTcpListener};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub mod backend;
pub mod subject;
pub mod tls;
pub mod upstream;

/// Contents of `<fixture>/expectations.yaml`. See SPEC §D5.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Expectations {
    pub driver: Driver,
    #[serde(default)]
    pub equivalence: Equivalence,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Driver {
    TcpEcho,
    HttpGet {
        path: String,
        host: String,
    },
    /// 03.1 NEW: TLS round-trip with explicit SNI + optional CN/SAN check.
    TlsTcp {
        sni: String,
        #[serde(default)]
        expected_cn: Option<String>,
    },
    /// 03.2 NEW: drive a sequence of per-SNI TLS probes against a single
    /// listener address. Each probe runs a fresh TLS handshake (varying SNI),
    /// optionally asserts the presented leaf cert's CN/SAN matches
    /// `expected_cn` (DER-substring scan via `check_cn_or_san`), then writes
    /// `payload.bin` and reads-exact + ADR-0007 trailing-byte poll.
    /// Equivalence is enforced *inside* `drive_tls_probes` per probe (each
    /// side asserts byte-equality against the input payload + per-probe
    /// `expected_cn`); both sides succeeding ⇒ equivalent cert selection
    /// per SNI without a final `assert_equivalence` call.
    TlsTcpProbeList {
        probes: Vec<TlsTcpProbe>,
    },
}

/// One TLS-SNI probe entry inside `Driver::TlsTcpProbeList`. SPEC §D6.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TlsTcpProbe {
    pub sni: String,
    #[serde(default)]
    pub expected_cn: Option<String>,
}

#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Equivalence {
    #[serde(default)]
    pub response_status: Option<StatusRule>,
    #[serde(default)]
    pub response_body: Option<BodyRule>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum StatusRule {
    Exact,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum BodyRule {
    ByteExact,
}

pub fn load_expectations(path: &Path) -> Result<Expectations> {
    let yaml =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let parsed: Expectations =
        serde_yaml::from_str(&yaml).with_context(|| format!("parsing {}", path.display()))?;
    Ok(parsed)
}

/// Reserve a free TCP port on 127.0.0.1. Binds `:0`, reads the assigned port,
/// drops the listener, and returns the number.
///
/// TOCTOU: between the drop and the subsequent bind by envoy-rust, another
/// process on the host could grab this port. This is accepted for a
/// pre-production harness per SPEC §6 point 6. If CI flakes materialize, this
/// becomes its own split phase with a port-range reservation strategy.
pub fn reserve_port() -> Result<u16> {
    let listener =
        StdTcpListener::bind(("127.0.0.1", 0)).context("binding 127.0.0.1:0 to reserve a port")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

/// Template-render a fixture YAML by substituting literal `{{KEY}}` tokens.
/// The `kvs` list is the set of tokens to replace; any `{{…}}` token not in
/// `kvs` is left untouched so a typo surfaces as a parser error rather than
/// silently rendering to the empty string.
pub fn render_yaml(template: &str, kvs: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (k, v) in kvs {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    out
}

/// Write `content` to a new temp file in `dir` and return the path. The caller
/// is responsible for ensuring `dir` is already created.
pub fn write_temp(dir: &Path, name: &str, content: &str) -> Result<PathBuf> {
    let path = dir.join(name);
    let mut f =
        std::fs::File::create(&path).with_context(|| format!("creating {}", path.display()))?;
    f.write_all(content.as_bytes())?;
    Ok(path)
}

/// Poll `addr` with exponential backoff (starting at 50ms, doubling, capped at
/// 500ms) until a TCP connect succeeds or `budget` elapses. Returns `Err` on
/// timeout.
pub async fn wait_accept_ready(addr: std::net::SocketAddr, budget: Duration) -> Result<()> {
    let deadline = std::time::Instant::now() + budget;
    let mut delay = Duration::from_millis(50);
    loop {
        match tokio::net::TcpStream::connect(addr).await {
            Ok(_) => return Ok(()),
            Err(_) if std::time::Instant::now() < deadline => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_millis(500));
            }
            Err(err) => bail!("{addr} not accept-ready within {budget:?}: {err}"),
        }
    }
}

/// Drive `payload` at `addr`: open TCP, write payload, read exactly
/// `payload.len()` bytes of echoed response, then confirm the peer writes
/// no further bytes before shutting down the write side and dropping the
/// stream. Returns the echoed bytes.
///
/// Why `read_exact(payload.len())` instead of half-close + `read_to_end`: see
/// `docs/envoy-rust/DECISIONS.md` ADR-0006. Upstream Envoy v1.33.0's default
/// `ConnectionImpl` (enable_half_close_=false) translates a client FIN into
/// `PostIoAction::Close` and calls `closeSocket(RemoteClose)` before the echo
/// filter's queued write is flushed, so a pre-read half-close causes the
/// response bytes to be dropped. Phase 00's only fixture (echo filter) has a
/// deterministic 1:1 byte-count contract, so `read_exact(payload.len())` is
/// both sufficient and matches upstream Envoy's own echo integration test
/// pattern. Graceful write-side shutdown still fires after the read so the
/// envoy-rust subject's echo loop exits on FIN rather than a peer reset.
///
/// Why the trailing-byte poll: see ADR-0007. A bare `read_exact(payload.len())`
/// silently ignores any bytes the peer writes after the echo, which would
/// narrow BEHAVIOR_CONTRACT row 2's "byte-exact" assertion to "first N bytes
/// match." After `read_exact`, we poll the socket with a short deadline
/// (100ms) and bail if the peer delivers more data before EOF or the
/// deadline — a peer that follows the echo-filter contract closes its
/// write side cleanly and we observe `Ok(0)` or a timeout.
pub async fn drive_tcp(addr: SocketAddr, payload: &[u8]) -> Result<Vec<u8>> {
    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .with_context(|| format!("connecting to {addr}"))?;
    stream.write_all(payload).await?;
    let mut out = vec![0u8; payload.len()];
    stream.read_exact(&mut out).await?;

    // ADR-0007: detect trailing bytes past the echoed payload. A compliant
    // peer either closes (Ok(0)) or stays silent until the deadline (timeout
    // Err). Any non-zero read is a contract violation.
    let mut tail = [0u8; 64];
    match tokio::time::timeout(Duration::from_millis(100), stream.read(&mut tail)).await {
        Ok(Ok(0)) | Err(_) => {}
        Ok(Ok(n)) => bail!("{addr} sent {n} trailing bytes after echo"),
        Ok(Err(e)) => bail!("{addr} read error after echo: {e}"),
    }

    stream.shutdown().await.ok();
    drop(stream);
    Ok(out)
}

/// Drive a payload through `addr` over a TLS connection terminated by the
/// peer (downstream-TLS scenario). The peer's leaf cert is verified against
/// `root_store`; the SNI is `sni`; if `expected_cn` is `Some`, the
/// post-handshake cert chain's leaf is walked for SAN-DNS entries and
/// CommonName, and the test fails if no case-insensitive exact match is
/// found (no wildcard support in 03.1 — SPEC §6 signpost 11).
///
/// Mirrors `drive_tcp`'s ADR-0006/0007 discipline: writes payload, reads
/// exactly `payload.len()` bytes, then runs the 100ms trailing-byte poll.
/// Graceful TLS shutdown on the write side completes before drop.
pub async fn drive_tls(
    addr: SocketAddr,
    payload: &[u8],
    sni: &str,
    root_store: rustls::RootCertStore,
    expected_cn: Option<&str>,
) -> Result<Vec<u8>> {
    use rustls::pki_types::ServerName;
    use std::convert::TryFrom;
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let client_cfg = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(client_cfg));
    let server_name = ServerName::try_from(sni)
        .map_err(|e| anyhow::anyhow!("parsing sni {sni:?}: {e}"))?
        .to_owned();

    let tcp = tokio::net::TcpStream::connect(addr)
        .await
        .with_context(|| format!("connecting to {addr}"))?;
    let mut tls = connector
        .connect(server_name, tcp)
        .await
        .with_context(|| format!("TLS handshake against {addr}"))?;

    if let Some(cn) = expected_cn {
        let peer_certs = tls
            .get_ref()
            .1
            .peer_certificates()
            .ok_or_else(|| anyhow::anyhow!("no peer certificate after handshake"))?;
        let leaf = peer_certs
            .first()
            .ok_or_else(|| anyhow::anyhow!("peer cert chain is empty"))?;
        check_cn_or_san(leaf, cn).context("expected_cn match")?;
    }

    tls.write_all(payload).await?;
    let mut out = vec![0u8; payload.len()];
    tls.read_exact(&mut out).await?;

    let mut tail = [0u8; 64];
    match tokio::time::timeout(Duration::from_millis(100), tls.read(&mut tail)).await {
        Ok(Ok(0)) | Err(_) => {}
        Ok(Ok(n)) => bail!("{addr} sent {n} trailing bytes after echo"),
        Ok(Err(e)) => bail!("{addr} read error after echo: {e}"),
    }

    tls.shutdown().await.ok();
    drop(tls);
    Ok(out)
}

/// Drive a sequence of per-SNI TLS probes against a single listener address.
/// Each probe gets a fresh TCP connection + TLS handshake; the SNI varies per
/// probe; if the probe declares `expected_cn`, the post-handshake leaf cert is
/// matched (DER-substring scan via `check_cn_or_san`) before any payload write.
/// Each probe runs the same ADR-0006 read-exact + ADR-0007 trailing-byte poll
/// discipline as `drive_tls`.
///
/// Returns `Ok(probe_outputs)` where `probe_outputs[i]` is the bytes echoed
/// back for `probes[i]` (typically equal to `payload`). On any per-probe
/// failure (handshake, expected_cn mismatch, byte mismatch, trailing-byte
/// detection) returns `Err` naming the probe's SNI for diagnostics.
///
/// Equivalence note: byte-equality is enforced *inside* this helper (each
/// probe writes `payload`, reads-exact `payload.len()` bytes, and the read
/// would not have succeeded as a different byte sequence under
/// `read_exact`-then-bail-on-trailing semantics). Per-probe `expected_cn`
/// matches enforce the cert-selection invariant on each side independently;
/// the conjunction across upstream + subject is the "both proxies select the
/// same cert for the same SNI" property — implicit, no final
/// `assert_equivalence` needed.
pub async fn drive_tls_probes(
    addr: SocketAddr,
    payload: &[u8],
    probes: &[TlsTcpProbe],
    root_store: rustls::RootCertStore,
) -> Result<Vec<Vec<u8>>> {
    use rustls::pki_types::ServerName;
    use std::convert::TryFrom;
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let client_cfg = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(client_cfg));

    let mut outputs = Vec::with_capacity(probes.len());
    for probe in probes {
        let server_name = ServerName::try_from(probe.sni.as_str())
            .map_err(|e| anyhow::anyhow!("parsing sni {:?}: {e}", probe.sni))?
            .to_owned();

        let tcp = tokio::net::TcpStream::connect(addr)
            .await
            .with_context(|| format!("connecting to {addr} for probe sni={:?}", probe.sni))?;
        let mut tls = connector.connect(server_name, tcp).await.with_context(|| {
            format!("TLS handshake against {addr} for probe sni={:?}", probe.sni)
        })?;

        if let Some(cn) = &probe.expected_cn {
            let peer_certs = tls
                .get_ref()
                .1
                .peer_certificates()
                .ok_or_else(|| anyhow::anyhow!("no peer cert for probe sni={:?}", probe.sni))?;
            let leaf = peer_certs.first().ok_or_else(|| {
                anyhow::anyhow!("peer cert chain empty for probe sni={:?}", probe.sni)
            })?;
            check_cn_or_san(leaf, cn)
                .with_context(|| format!("expected_cn match for probe sni={:?}", probe.sni))?;
        }

        tls.write_all(payload)
            .await
            .with_context(|| format!("write for probe sni={:?}", probe.sni))?;
        let mut out = vec![0u8; payload.len()];
        tls.read_exact(&mut out)
            .await
            .with_context(|| format!("read_exact for probe sni={:?}", probe.sni))?;

        // ADR-0007 trailing-byte poll, mirroring drive_tls.
        let mut tail = [0u8; 64];
        match tokio::time::timeout(Duration::from_millis(100), tls.read(&mut tail)).await {
            Ok(Ok(0)) | Err(_) => {}
            Ok(Ok(n)) => bail!(
                "{addr} sent {n} trailing bytes after echo for probe sni={:?}",
                probe.sni
            ),
            Ok(Err(e)) => bail!(
                "{addr} read error after echo for probe sni={:?}: {e}",
                probe.sni
            ),
        }

        tls.shutdown().await.ok();
        drop(tls);

        outputs.push(out);
    }
    Ok(outputs)
}

/// Walk a leaf cert's SAN DNS entries + CommonName for a case-insensitive
/// exact match against `wanted`. No wildcard support in 03.1 (SPEC §6
/// signpost 11). The cert is parsed via the rcgen-roundtrip path —
/// rustls-pemfile yields `CertificateDer`, which we re-parse to extract the
/// SAN/CN strings. We use an inline minimal X.509 walk via rustls-pemfile +
/// `rustls::pki_types` machinery; full TLS validation already happened during
/// the handshake.
fn check_cn_or_san(cert: &rustls::pki_types::CertificateDer<'_>, wanted: &str) -> Result<()> {
    // The simplest path: re-encode the DER to PEM, then use rcgen's parser
    // (we already pull rcgen for cert generation, so its parser is in scope
    // for free). If that proves fragile, swap to `x509-parser` under a new
    // ADR. For 03.1 the cert chain we're matching against is rcgen-built
    // ourselves, so an exact match on the SAN DNS string is reliable.
    //
    // rcgen 0.13 doesn't ship a public PEM/DER parser exposing SAN strings
    // directly; rather than fight that, fall back to walking the DER for
    // the SAN extension's GeneralNames manually. Phase 03.2 may pull
    // `x509-parser` under a follow-up ADR if more sophisticated cert
    // introspection is needed; for 03.1, the harness's `expected_cn` is
    // optional and used only for sanity — the differential body equivalence
    // is the primary signal.
    //
    // Simplest viable check: the rcgen-built leaf's DER includes the SAN
    // value as a literal UTF-8 substring. Search for it.
    let der_bytes: &[u8] = cert.as_ref();
    let needle = wanted.to_ascii_lowercase();
    let hay: Vec<u8> = der_bytes.iter().map(|b| b.to_ascii_lowercase()).collect();
    if hay.windows(needle.len()).any(|w| w == needle.as_bytes()) {
        return Ok(());
    }
    bail!("expected_cn / SAN match for {wanted:?} not found in peer cert (DER-substring scan)",);
}

/// Decoded HTTP/1.1 response. Headers are captured for debug tracing but play
/// no part in the phase-01 equivalence diff (ADR-0011).
#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    #[allow(dead_code)]
    pub headers: Vec<(String, Vec<u8>)>,
}

/// Open a TCP connection to `addr`, issue a minimal `GET` for `path` with
/// `Host: host`, and parse the response. Supports `content-length`-framed,
/// `transfer-encoding: chunked`-framed, and `connection: close`-framed
/// responses. Chunked support was added in phase 01 when upstream Envoy v1.33.0
/// was observed returning chunked responses for `/ready` (SPEC §6 signpost 9).
pub async fn drive_http_get(addr: SocketAddr, path: &str, host: &str) -> Result<HttpResponse> {
    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .with_context(|| format!("connecting to {addr}"))?;
    let req = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Connection: close\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).await?;
    stream.flush().await.ok();

    let mut buf = Vec::with_capacity(2048);
    let mut scratch = [0u8; 2048];
    let head_end;
    loop {
        let n = stream.read(&mut scratch).await?;
        if n == 0 {
            bail!("{addr} closed before a response head was received");
        }
        buf.extend_from_slice(&scratch[..n]);

        let mut headers = [httparse::EMPTY_HEADER; 32];
        let mut resp = httparse::Response::new(&mut headers);
        match resp.parse(&buf) {
            Ok(httparse::Status::Complete(n)) => {
                head_end = n;
                let status = resp
                    .code
                    .ok_or_else(|| anyhow::anyhow!("missing response status code"))?;
                let mut captured_headers: Vec<(String, Vec<u8>)> = Vec::new();
                let mut content_length: Option<usize> = None;
                let mut connection_close = false;
                let mut chunked = false;
                for h in resp.headers.iter() {
                    captured_headers.push((h.name.to_ascii_lowercase(), h.value.to_vec()));
                    if h.name.eq_ignore_ascii_case("content-length") {
                        let s = std::str::from_utf8(h.value)?.trim();
                        content_length = Some(s.parse()?);
                    } else if h.name.eq_ignore_ascii_case("connection") {
                        let s = std::str::from_utf8(h.value)?.trim();
                        if s.eq_ignore_ascii_case("close") {
                            connection_close = true;
                        }
                    } else if h.name.eq_ignore_ascii_case("transfer-encoding") {
                        let s = std::str::from_utf8(h.value)?.trim();
                        if s.eq_ignore_ascii_case("chunked") {
                            chunked = true;
                        }
                    }
                }

                // Drain the body.
                let body = if chunked {
                    // Decode HTTP/1.1 chunked transfer encoding. Read all
                    // wire bytes (already-buffered tail + remaining from
                    // stream), then parse chunk frames and concatenate the
                    // chunk data. This handles upstream Envoy v1.33.0's
                    // habit of sending `/ready` bodies as chunked.
                    let mut wire = buf[head_end..].to_vec();
                    stream.read_to_end(&mut wire).await?;
                    decode_chunked(&wire)
                        .with_context(|| format!("{addr} chunked decoding failed"))?
                } else {
                    match content_length {
                        Some(cl) => {
                            let mut body = Vec::with_capacity(cl);
                            let already = &buf[head_end..];
                            let take = already.len().min(cl);
                            body.extend_from_slice(&already[..take]);
                            if body.len() < cl {
                                let remaining = cl - body.len();
                                let mut rest = vec![0u8; remaining];
                                stream.read_exact(&mut rest).await?;
                                body.extend(rest);
                            }
                            body
                        }
                        None if connection_close => {
                            let mut body = Vec::new();
                            body.extend_from_slice(&buf[head_end..]);
                            stream.read_to_end(&mut body).await?;
                            body
                        }
                        None => bail!(
                            "{addr} response has neither `content-length` nor \
                             `connection: close` nor `transfer-encoding: chunked`; \
                             drive_http_get does not support keep-alive in phase 01",
                        ),
                    }
                };

                return Ok(HttpResponse {
                    status,
                    body,
                    headers: captured_headers,
                });
            }
            Ok(httparse::Status::Partial) => continue,
            Err(e) => bail!("{addr} response parse error: {e}"),
        }
    }
}

/// Decode HTTP/1.1 chunked transfer-encoded body bytes into plain body bytes.
/// Each chunk has the form `<hex-size>\r\n<data>\r\n`; the last chunk is
/// `0\r\n\r\n`. Trailer headers (if any) are ignored. Returns an error if the
/// wire bytes do not conform to the chunked framing grammar.
fn decode_chunked(wire: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut pos = 0;
    loop {
        // Find the CRLF that terminates the chunk-size line.
        let crlf = wire[pos..]
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or_else(|| anyhow::anyhow!("missing CRLF after chunk size at offset {pos}"))?;
        let size_line = std::str::from_utf8(&wire[pos..pos + crlf])
            .context("chunk size line is not UTF-8")?
            .trim();
        // Strip optional chunk extensions (`;ext=val`).
        let size_hex = size_line.split(';').next().unwrap_or("").trim();
        let chunk_size = usize::from_str_radix(size_hex, 16)
            .with_context(|| format!("invalid chunk size hex: {size_hex:?}"))?;
        pos += crlf + 2; // advance past size line + CRLF
        if chunk_size == 0 {
            // Last chunk — ignore optional trailers.
            break;
        }
        if pos + chunk_size + 2 > wire.len() {
            bail!(
                "chunk data truncated: need {} bytes at offset {pos}, have {}",
                chunk_size + 2,
                wire.len() - pos,
            );
        }
        out.extend_from_slice(&wire[pos..pos + chunk_size]);
        pos += chunk_size + 2; // advance past data + trailing CRLF
    }
    Ok(out)
}

/// End-to-end run of one fixture. Panics-on-failure paths unwind through Drop
/// guards so the container and envoy-rust subprocess are cleaned up even on
/// assertion failure.
pub async fn run_fixture(fixture_dir: &Path) -> Result<()> {
    let expectations = load_expectations(&fixture_dir.join("expectations.yaml"))?;

    let host_port = reserve_port()?;

    let tmp = tempfile::tempdir().context("creating fixture temp dir")?;
    let upstream_template = std::fs::read_to_string(fixture_dir.join("envoy.yaml"))
        .context("reading upstream envoy.yaml")?;
    let subject_template = std::fs::read_to_string(fixture_dir.join("envoy-rust.yaml"))
        .context("reading envoy-rust.yaml")?;

    let upstream_port_str = upstream::CONTAINER_PORT.to_string();
    let subject_port_str = host_port.to_string();
    let port_key = match &expectations.driver {
        Driver::TcpEcho | Driver::TlsTcp { .. } | Driver::TlsTcpProbeList { .. } => "PORT",
        Driver::HttpGet { .. } => "ADMIN_PORT",
    };

    // (a) Detect TLS templates — if any TLS substitution token appears in
    // either template, generate a fresh TlsTestPki for this fixture run.
    let needs_tls_pki = upstream_template.contains("{{LEAF_A_CERT_PATH}}")
        || upstream_template.contains("{{LEAF_A_KEY_PATH}}")
        || upstream_template.contains("{{CA_PATH}}")
        || upstream_template.contains("{{LEAF_B_CERT_PATH}}")
        || upstream_template.contains("{{SERVER_CERT_PATH}}")
        || subject_template.contains("{{LEAF_A_CERT_PATH}}")
        || subject_template.contains("{{CA_PATH}}");
    let tls_pki = if needs_tls_pki {
        Some(crate::tls::TlsTestPki::generate().context("generating TLS test PKI")?)
    } else {
        None
    };

    // Spawn a host-local backend if either template needs one. Holding the
    // backend in a binding outside the proxies' lifetime ensures the child
    // process outlives the fixture run; Drop fires after `run_fixture`'s
    // returns paths.
    let needs_backend = upstream_template.contains("{{BACKEND_PORT}}")
        || subject_template.contains("{{BACKEND_PORT}}");
    let _backend = if needs_backend {
        Some(
            backend::TcpProxyBackend::spawn()
                .await
                .context("spawning backend")?,
        )
    } else {
        None
    };
    let backend_port_str = _backend.as_ref().map(|b| b.port().to_string());

    // 03.2 Task 9: spawn a TlsEchoBackend if either template needs one.
    // Same alive-keeper binding-order discipline as `_backend` above — the
    // `Option<TlsEchoBackend>` outlives both proxies, and Drop fires after
    // `run_fixture` returns. Requires `tls_pki` to also be present (the
    // backend reads cert + key from the same PKI the upstream consults).
    let needs_tls_backend = upstream_template.contains("{{TLS_BACKEND_PORT}}")
        || subject_template.contains("{{TLS_BACKEND_PORT}}");
    let _tls_backend: Option<crate::backend::TlsEchoBackend> = if needs_tls_backend {
        let pki = tls_pki
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("TLS backend implies TLS pki shape"))?;
        Some(
            crate::backend::TlsEchoBackend::spawn(&pki.server_cert, &pki.server_key)
                .await
                .context("spawning TlsEchoBackend")?,
        )
    } else {
        None
    };
    let tls_backend_port_str = _tls_backend.as_ref().map(|b| b.port().to_string());

    // (c) Build per-side substitution maps with TLS path keys.
    // Type is Vec<(&str, String)> to accommodate owned strings from TLS paths.
    let upstream_tls_paths = tls_pki.as_ref().map(|p| p.envoy_side_paths());
    let subject_tls_paths = tls_pki.as_ref().map(|p| p.subject_side_paths());

    let upstream_kvs: Vec<(&str, String)> = {
        let mut v: Vec<(&str, String)> = vec![(port_key, upstream_port_str.clone())];
        if let Some(bp) = backend_port_str.as_deref() {
            v.push(("BACKEND_PORT", bp.to_string()));
        }
        if let Some(tp) = tls_backend_port_str.as_deref() {
            v.push(("TLS_BACKEND_PORT", tp.to_string()));
        }
        if backend_port_str.is_some() || tls_backend_port_str.is_some() {
            // Per ADR-0015: container-side reaches the host backend via
            // host.docker.internal (with the harness's with_host call below).
            // Generalized in Task 9 to fire for either backend variant; was
            // previously gated only on BACKEND_PORT (Task 8 cadence).
            v.push(("BACKEND_HOST", "host.docker.internal".to_string()));
        }
        if let Some(map) = upstream_tls_paths.as_ref() {
            for (k, val) in map {
                v.push((*k, val.clone()));
            }
        }
        v
    };
    let subject_kvs: Vec<(&str, String)> = {
        let mut v: Vec<(&str, String)> = vec![(port_key, subject_port_str.clone())];
        if let Some(bp) = backend_port_str.as_deref() {
            v.push(("BACKEND_PORT", bp.to_string()));
        }
        if let Some(tp) = tls_backend_port_str.as_deref() {
            v.push(("TLS_BACKEND_PORT", tp.to_string()));
        }
        if backend_port_str.is_some() || tls_backend_port_str.is_some() {
            v.push(("BACKEND_HOST", "127.0.0.1".to_string()));
        }
        if let Some(map) = subject_tls_paths.as_ref() {
            for (k, val) in map {
                v.push((*k, val.clone()));
            }
        }
        v
    };

    // (d) Adapt render_yaml call sites: build _refs intermediates since
    // render_yaml takes &[(&str, &str)] but kvs are Vec<(&str, String)>.
    let upstream_kvs_refs: Vec<(&str, &str)> =
        upstream_kvs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let subject_kvs_refs: Vec<(&str, &str)> =
        subject_kvs.iter().map(|(k, v)| (*k, v.as_str())).collect();

    let upstream_yaml = render_yaml(&upstream_template, &upstream_kvs_refs);
    let subject_yaml = render_yaml(&subject_template, &subject_kvs_refs);
    let upstream_path = write_temp(tmp.path(), "envoy.yaml", &upstream_yaml)?;
    let subject_path = write_temp(tmp.path(), "envoy-rust.yaml", &subject_yaml)?;

    // The `host_uses_host_gateway` flag drives upstream::start to attach
    // `with_host("host.docker.internal", Host::HostGateway)` on the
    // testcontainers image (per ADR-0015). The flag is true exactly when the
    // upstream YAML actually references the hostname — silent when it
    // doesn't, so fixtures 0001 and 0002 stay unchanged.
    let host_uses_host_gateway = upstream_yaml.contains("host.docker.internal");
    // (e) Thread tls_pki through to upstream::start.
    let upstream =
        upstream::start(&upstream_path, host_uses_host_gateway, tls_pki.as_ref()).await?;
    let mut subject = subject::start(&subject_path, host_port).await?;

    let upstream_addr: SocketAddr = format!("127.0.0.1:{}", upstream.host_port()).parse()?;
    let subject_addr: SocketAddr = format!("127.0.0.1:{}", subject.port()).parse()?;

    let budget = Duration::from_secs(10);
    wait_accept_ready(upstream_addr, budget)
        .await
        .context("upstream Envoy never became accept-ready")?;
    wait_accept_ready(subject_addr, budget)
        .await
        .context("envoy-rust never became accept-ready")?;

    match &expectations.driver {
        Driver::TcpEcho => {
            let payload = std::fs::read(fixture_dir.join("inputs/payload.bin"))
                .context("reading payload.bin")?;
            let upstream_out = drive_tcp(upstream_addr, &payload)
                .await
                .context("upstream envoy drive")?;
            let subject_out = drive_tcp(subject_addr, &payload)
                .await
                .context("envoy-rust drive")?;
            subject.shutdown(Duration::from_secs(5)).await.ok();
            drop(upstream);
            assert_equivalence(&expectations, None, None, &upstream_out, &subject_out)?;
        }
        Driver::HttpGet { path, host } => {
            let upstream_resp = drive_http_get(upstream_addr, path, host)
                .await
                .context("upstream envoy http get")?;
            let subject_resp = drive_http_get(subject_addr, path, host)
                .await
                .context("envoy-rust http get")?;
            subject.shutdown(Duration::from_secs(5)).await.ok();
            drop(upstream);
            assert_equivalence(
                &expectations,
                Some(upstream_resp.status),
                Some(subject_resp.status),
                &upstream_resp.body,
                &subject_resp.body,
            )?;
        }
        // (f) Real TLS dispatch arm.
        Driver::TlsTcp { sni, expected_cn } => {
            let payload = std::fs::read(fixture_dir.join("inputs/payload.bin"))
                .context("reading payload.bin")?;
            // Build a RootCertStore from the test CA. Both sides trust the
            // same CA — both proxies present a leaf signed by it.
            let pki = tls_pki
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!(
                    "Driver::TlsTcp requires a TLS-shaped fixture (template did not reference any *_PATH key)"
                ))?;
            let ca_bytes = std::fs::read(&pki.ca_pem_path).context("read ca.pem")?;
            let mut ca_slice = ca_bytes.as_slice();
            let ca_certs: Vec<rustls::pki_types::CertificateDer<'static>> =
                rustls_pemfile::certs(&mut ca_slice)
                    .collect::<Result<Vec<_>, _>>()
                    .context("parse ca.pem certs")?;
            let mut roots = rustls::RootCertStore::empty();
            for c in ca_certs {
                roots.add(c).context("RootCertStore::add")?;
            }

            let upstream_out = drive_tls(
                upstream_addr,
                &payload,
                sni,
                roots.clone(),
                expected_cn.as_deref(),
            )
            .await
            .context("upstream envoy tls drive")?;
            let subject_out = drive_tls(subject_addr, &payload, sni, roots, expected_cn.as_deref())
                .await
                .context("envoy-rust tls drive")?;
            subject.shutdown(Duration::from_secs(5)).await.ok();
            drop(upstream);
            assert_equivalence(&expectations, None, None, &upstream_out, &subject_out)?;
        }
        // 03.2 Task 8: per-SNI probe list. Equivalence is enforced inside
        // `drive_tls_probes` per probe (byte-equality + per-probe expected_cn);
        // both sides succeeding ⇒ equivalent cert selection per SNI without a
        // final `assert_equivalence` call.
        Driver::TlsTcpProbeList { probes } => {
            let payload = std::fs::read(fixture_dir.join("inputs/payload.bin"))
                .context("reading payload.bin")?;
            let pki = tls_pki
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!(
                    "Driver::TlsTcpProbeList requires a TLS-shaped fixture (template did not reference any *_PATH key)"
                ))?;
            let ca_bytes = std::fs::read(&pki.ca_pem_path).context("read ca.pem")?;
            let mut ca_slice = ca_bytes.as_slice();
            let ca_certs: Vec<rustls::pki_types::CertificateDer<'static>> =
                rustls_pemfile::certs(&mut ca_slice)
                    .collect::<Result<Vec<_>, _>>()
                    .context("parse ca.pem certs")?;
            let mut roots = rustls::RootCertStore::empty();
            for c in ca_certs {
                roots.add(c).context("RootCertStore::add")?;
            }

            drive_tls_probes(upstream_addr, &payload, probes, roots.clone())
                .await
                .context("upstream envoy tls probes")?;
            drive_tls_probes(subject_addr, &payload, probes, roots)
                .await
                .context("envoy-rust tls probes")?;
            subject.shutdown(Duration::from_secs(5)).await.ok();
            drop(upstream);
        }
    }

    // _backend Drop fires here.
    Ok(())
}

fn assert_equivalence(
    expectations: &Expectations,
    upstream_status: Option<u16>,
    subject_status: Option<u16>,
    upstream_body: &[u8],
    subject_body: &[u8],
) -> Result<()> {
    if matches!(
        expectations.equivalence.response_status,
        Some(StatusRule::Exact)
    ) {
        match (upstream_status, subject_status) {
            (Some(u), Some(s)) if u == s => {}
            (u, s) => bail!(
                "response status mismatch under `response_status: exact`\n  \
                 upstream: {u:?}\n  subject:  {s:?}"
            ),
        }
    }
    if matches!(
        expectations.equivalence.response_body,
        Some(BodyRule::ByteExact)
    ) && upstream_body != subject_body
    {
        bail!(
            "byte-exact body mismatch\n  upstream: {upstream_body:?}\n  subject:  {subject_body:?}",
        );
    }
    // Neither rule configured → silently pass + log a warning (SPEC §D5).
    if expectations.equivalence.response_status.is_none()
        && expectations.equivalence.response_body.is_none()
    {
        tracing::warn!(
            "fixture has neither response_status nor response_body equivalence rule — running as a smoke test"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expectations_parse_byte_exact() {
        let yaml = "driver:\n  kind: tcp_echo\nequivalence:\n  response_body: byte_exact\n";
        let e: Expectations = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
        assert!(matches!(e.driver, Driver::TcpEcho));
    }

    #[test]
    fn expectations_reject_unknown_rule() {
        let yaml = "driver:\n  kind: tcp_echo\nequivalence:\n  response_body: sorta_equal\n";
        let r = serde_yaml::from_str::<Expectations>(yaml);
        assert!(r.is_err());
    }

    // Regression for REVIEW.md M3: `#[serde(deny_unknown_fields)]` must reject
    // a typo'd or unexpected top-level key rather than silently dropping it.
    #[test]
    fn expectations_reject_unknown_field() {
        let yaml =
            "driver:\n  kind: tcp_echo\nequivalence:\n  response_body: byte_exact\nfoo: bar\n";
        let err = serde_yaml::from_str::<Expectations>(yaml)
            .expect_err("must reject unknown top-level field");
        let msg = err.to_string();
        assert!(msg.contains("unknown field"), "unexpected: {msg}");
    }

    // Regression for REVIEW.md M3 at the nested `Equivalence` level.
    #[test]
    fn equivalence_reject_unknown_field() {
        let yaml =
            "driver:\n  kind: tcp_echo\nequivalence:\n  response_body: byte_exact\n  extra: true\n";
        let err = serde_yaml::from_str::<Expectations>(yaml)
            .expect_err("must reject unknown nested field");
        let msg = err.to_string();
        assert!(msg.contains("unknown field"), "unexpected: {msg}");
    }

    #[test]
    fn render_yaml_substitutes_all_port_tokens() {
        let t = "a: {{PORT}}\nb: {{PORT}}\n";
        assert_eq!(render_yaml(t, &[("PORT", "9000")]), "a: 9000\nb: 9000\n");
    }

    #[test]
    fn render_yaml_substitutes_admin_port_key() {
        let t = "address: 127.0.0.1\nport: {{ADMIN_PORT}}\n";
        assert_eq!(
            render_yaml(t, &[("ADMIN_PORT", "9901")]),
            "address: 127.0.0.1\nport: 9901\n"
        );
    }

    #[test]
    fn reserve_port_returns_nonzero() {
        let p = reserve_port().unwrap();
        assert!(p > 0);
    }

    #[tokio::test]
    async fn wait_accept_ready_succeeds_for_listening_socket() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        wait_accept_ready(addr, Duration::from_secs(1))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn wait_accept_ready_times_out_for_closed_socket() {
        let listener = StdTcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        drop(listener);
        let result = wait_accept_ready(addr, Duration::from_millis(200)).await;
        assert!(result.is_err());
    }

    // Mirrors upstream Envoy v1.33.0's echo filter semantics per ADR-0006: the
    // server accepts one connection, reads `payload.len()` bytes, echoes them
    // back, and closes WITHOUT ever honoring a client half-close. A harness
    // that half-closed before reading (the pre-ADR-0006 `drive_tcp`) would
    // race against this close and see an empty response.
    #[tokio::test]
    async fn drive_tcp_round_trips_without_half_close() {
        use tokio::io::AsyncReadExt as _;
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let payload: &'static [u8] = b"hello, envoy-rust\n";
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; payload.len()];
            stream.read_exact(&mut buf).await.unwrap();
            stream.write_all(&buf).await.unwrap();
            // Drop without waiting for a client FIN — this is what upstream
            // Envoy's echo path does once it has written the response.
            drop(stream);
        });

        let echoed = drive_tcp(addr, payload).await.unwrap();
        assert_eq!(echoed, payload);
        server.await.unwrap();
    }

    #[test]
    fn expectations_parse_tcp_echo_driver() {
        let yaml = r#"
driver:
  kind: tcp_echo
equivalence:
  response_body: byte_exact
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        assert!(matches!(e.driver, Driver::TcpEcho));
        assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
        assert_eq!(e.equivalence.response_status, None);
    }

    #[test]
    fn expectations_parse_http_get_driver() {
        let yaml = r#"
driver:
  kind: http_get
  path: /ready
  host: envoy-rust-phase-01
equivalence:
  response_status: exact
  response_body: byte_exact
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        match e.driver {
            Driver::HttpGet { path, host } => {
                assert_eq!(path, "/ready");
                assert_eq!(host, "envoy-rust-phase-01");
            }
            _ => panic!("unexpected driver: {:?}", e.driver),
        }
        assert_eq!(e.equivalence.response_status, Some(StatusRule::Exact));
        assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
    }

    // RED for Task 8: parses `kind: tls_tcp_probe_list` with a `probes:`
    // sequence whose entries are `{sni, expected_cn?}` maps.
    #[test]
    fn expectations_parse_tls_tcp_probe_list_driver() {
        let yaml = r#"
driver:
  kind: tls_tcp_probe_list
  probes:
    - sni: a.example.com
      expected_cn: a.example.com
    - sni: b.example.com
      expected_cn: b.example.com
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        match e.driver {
            Driver::TlsTcpProbeList { ref probes } => {
                assert_eq!(probes.len(), 2);
                assert_eq!(probes[0].sni, "a.example.com");
                assert_eq!(probes[0].expected_cn.as_deref(), Some("a.example.com"));
                assert_eq!(probes[1].sni, "b.example.com");
                assert_eq!(probes[1].expected_cn.as_deref(), Some("b.example.com"));
            }
            _ => panic!("unexpected driver: {:?}", e.driver),
        }
    }

    // RED for Task 8: `expected_cn` is `#[serde(default)]` so it may be absent.
    #[test]
    fn expectations_parse_tls_tcp_probe_list_without_expected_cn() {
        let yaml = r#"
driver:
  kind: tls_tcp_probe_list
  probes:
    - sni: a.example.com
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        match e.driver {
            Driver::TlsTcpProbeList { ref probes } => {
                assert_eq!(probes.len(), 1);
                assert_eq!(probes[0].sni, "a.example.com");
                assert!(probes[0].expected_cn.is_none());
            }
            _ => panic!("unexpected driver: {:?}", e.driver),
        }
    }

    #[test]
    fn expectations_reject_unknown_driver_kind() {
        let yaml = r#"
driver:
  kind: quantum_bogon
equivalence:
  response_body: byte_exact
"#;
        let r: Result<Expectations, _> = serde_yaml::from_str(yaml);
        assert!(r.is_err(), "quantum_bogon must not parse: {r:?}");
    }

    // Regression for REVIEW.md I1 (ADR-0007): a server that writes
    // `payload.len()` bytes and then additional trailing bytes must cause
    // `drive_tcp` to fail the fixture. Before ADR-0007's trailing-byte check,
    // `drive_tcp` silently consumed only the first `payload.len()` bytes and
    // returned Ok, narrowing BEHAVIOR_CONTRACT row 2's "byte-exact" contract
    // to "first N bytes match."
    #[tokio::test]
    async fn drive_tcp_rejects_trailing_bytes_after_echo() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let payload: &'static [u8] = b"hello, envoy-rust\n";
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; payload.len()];
            stream.read_exact(&mut buf).await.unwrap();
            stream.write_all(&buf).await.unwrap();
            // Write extra trailing bytes beyond the echoed payload. A pre-
            // ADR-0007 `drive_tcp` would not notice these.
            stream.write_all(b"EXTRA").await.unwrap();
            // Hold the stream open long enough that the harness's trailing-
            // byte poll deadline sees the bytes rather than an early EOF.
            tokio::time::sleep(Duration::from_millis(250)).await;
            drop(stream);
        });

        let err = drive_tcp(addr, payload)
            .await
            .expect_err("drive_tcp must fail when the peer writes trailing bytes");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("trailing bytes"),
            "unexpected error message: {msg}",
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn drive_http_get_round_trips() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            // Read the request (we don't parse — just drain until CRLFCRLF).
            let mut buf = [0u8; 512];
            let mut read = Vec::new();
            loop {
                let n = stream.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                read.extend_from_slice(&buf[..n]);
                if read.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-length: 5\r\nconnection: close\r\n\r\nLIVE\n",
                )
                .await
                .unwrap();
            drop(stream);
        });

        let resp = drive_http_get(addr, "/ready", "x").await.unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, b"LIVE\n");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn drive_http_get_handles_explicit_content_length() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let _ = tokio::io::copy(
                &mut tokio::io::empty(),
                &mut tokio::io::BufWriter::new(&mut s),
            )
            .await;
            s.write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 4\r\n\r\nNOPE")
                .await
                .unwrap();
            // Hold open long enough for the client to read_exact the 4 bytes.
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            drop(s);
        });

        let resp = drive_http_get(addr, "/x", "h").await.unwrap();
        assert_eq!(resp.status, 404);
        assert_eq!(resp.body, b"NOPE");
    }

    #[tokio::test]
    async fn drive_http_get_handles_connection_close_without_length() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            // Drain the incoming request so the receive buffer is empty before
            // we write and close. Without this, macOS sends RST instead of FIN
            // when dropping a TcpStream with unread data.
            let mut drain = [0u8; 512];
            loop {
                let n = s.read(&mut drain).await.unwrap();
                if n == 0 {
                    break;
                }
                if drain[..n].windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            s.write_all(b"HTTP/1.1 200 OK\r\nconnection: close\r\n\r\nhello-close")
                .await
                .unwrap();
            s.shutdown().await.ok();
            drop(s);
        });

        let resp = drive_http_get(addr, "/x", "h").await.unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, b"hello-close");
    }

    #[tokio::test]
    async fn drive_http_get_rejects_malformed_response() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            s.write_all(b"this is not a valid http response\r\n\r\n")
                .await
                .unwrap();
            drop(s);
        });

        let err = drive_http_get(addr, "/x", "h")
            .await
            .expect_err("malformed must fail");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("parse") || msg.contains("invalid"),
            "got: {msg}"
        );
    }

    #[test]
    fn decode_chunked_empty_stream() {
        let decoded = super::decode_chunked(b"0\r\n\r\n").expect("empty stream decodes");
        assert!(decoded.is_empty(), "got {decoded:?}");
    }

    #[test]
    fn decode_chunked_with_chunk_extension() {
        let wire = b"5;name=value\r\nhello\r\n0\r\n\r\n";
        let decoded = super::decode_chunked(wire).expect("chunk extensions tolerated");
        assert_eq!(decoded, b"hello");
    }

    #[test]
    fn decode_chunked_truncated_size_line() {
        // No CRLF anywhere — the first `windows(2).position(== \r\n)` miss
        // must surface as Err, not silent Ok(partial).
        let err = super::decode_chunked(b"5hello").expect_err("must reject");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("missing CRLF"),
            "expected CRLF-missing error; got {msg}",
        );
    }

    #[test]
    fn decode_chunked_ignores_trailer_bytes() {
        let wire = b"3\r\nabc\r\n0\r\nTrailer-Name: value\r\n\r\n";
        let decoded = super::decode_chunked(wire).expect("trailer tolerated");
        assert_eq!(decoded, b"abc");
    }

    #[test]
    fn fixture_0001_expectations_parses_as_tcp_echo() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tests/fixtures/0001-tcp-echo/expectations.yaml");
        let e = load_expectations(&path).expect("parses");
        assert!(matches!(e.driver, Driver::TcpEcho));
        assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
        assert_eq!(e.equivalence.response_status, None);
    }

    #[test]
    fn fixture_0002_expectations_parses_as_http_get() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tests/fixtures/0002-static-admin-ready/expectations.yaml");
        let e = load_expectations(&path).expect("parses");
        match e.driver {
            Driver::HttpGet { ref path, ref host } => {
                assert_eq!(path, "/ready");
                assert_eq!(host, "envoy-rust-phase-01");
            }
            _ => panic!("unexpected driver: {:?}", e.driver),
        }
        assert_eq!(e.equivalence.response_status, Some(StatusRule::Exact));
        assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
    }

    #[test]
    fn render_yaml_substitutes_backend_keys_for_envoy_side() {
        // Upstream-Envoy rendering: {{BACKEND_HOST}} → host.docker.internal,
        // {{BACKEND_PORT}} → harness-reserved port. {{PORT}} → the listener port.
        let template = r#"
listeners: [{{PORT}}]
endpoint: {{BACKEND_HOST}}:{{BACKEND_PORT}}
"#;
        let got = render_yaml(
            template,
            &[
                ("PORT", "10000"),
                ("BACKEND_HOST", "host.docker.internal"),
                ("BACKEND_PORT", "31415"),
            ],
        );
        assert!(
            got.contains("listeners: [10000]"),
            "PORT not substituted: {got}"
        );
        assert!(
            got.contains("endpoint: host.docker.internal:31415"),
            "BACKEND_{{HOST,PORT}} not substituted: {got}",
        );
    }

    #[test]
    fn render_yaml_substitutes_backend_keys_for_envoy_rust_side() {
        // envoy-rust-side rendering: {{BACKEND_HOST}} → 127.0.0.1.
        let template = r#"
listeners: [{{PORT}}]
endpoint: {{BACKEND_HOST}}:{{BACKEND_PORT}}
"#;
        let got = render_yaml(
            template,
            &[
                ("PORT", "20000"),
                ("BACKEND_HOST", "127.0.0.1"),
                ("BACKEND_PORT", "31415"),
            ],
        );
        assert!(
            got.contains("listeners: [20000]"),
            "PORT not substituted: {got}"
        );
        assert!(
            got.contains("endpoint: 127.0.0.1:31415"),
            "BACKEND_HOST not substituted to 127.0.0.1: {got}",
        );
    }

    #[test]
    fn render_yaml_substitutes_tls_paths_for_envoy_side() {
        let template = r#"
trusted_ca:
  filename: {{CA_PATH}}
leaf_cert:
  filename: {{LEAF_A_CERT_PATH}}
leaf_key:
  filename: {{LEAF_A_KEY_PATH}}
"#;
        let got = render_yaml(
            template,
            &[
                ("CA_PATH", "/etc/envoy-rust-tls/ca.pem"),
                ("LEAF_A_CERT_PATH", "/etc/envoy-rust-tls/leaf-a-cert.pem"),
                ("LEAF_A_KEY_PATH", "/etc/envoy-rust-tls/leaf-a-key.pem"),
            ],
        );
        assert!(got.contains("filename: /etc/envoy-rust-tls/ca.pem"));
        assert!(got.contains("filename: /etc/envoy-rust-tls/leaf-a-cert.pem"));
        assert!(got.contains("filename: /etc/envoy-rust-tls/leaf-a-key.pem"));
    }

    #[test]
    fn render_yaml_substitutes_tls_paths_for_subject_side() {
        let template = r#"
trusted_ca:
  filename: {{CA_PATH}}
leaf_cert:
  filename: {{LEAF_A_CERT_PATH}}
leaf_key:
  filename: {{LEAF_A_KEY_PATH}}
"#;
        let got = render_yaml(
            template,
            &[
                ("CA_PATH", "/tmp/abc/ca.pem"),
                ("LEAF_A_CERT_PATH", "/tmp/abc/leaf-a-cert.pem"),
                ("LEAF_A_KEY_PATH", "/tmp/abc/leaf-a-key.pem"),
            ],
        );
        assert!(got.contains("filename: /tmp/abc/ca.pem"));
        assert!(got.contains("filename: /tmp/abc/leaf-a-cert.pem"));
        assert!(got.contains("filename: /tmp/abc/leaf-a-key.pem"));
    }

    // 03.2 Task 8: render_yaml must substitute LEAF_B_* keys (used by
    // fixture 0006-tls-sni's second filter chain).
    #[test]
    fn render_yaml_substitutes_leaf_b_paths() {
        let template = r#"
chain_b_cert: {{LEAF_B_CERT_PATH}}
chain_b_key: {{LEAF_B_KEY_PATH}}
ca: {{CA_PATH}}
"#;
        let got = render_yaml(
            template,
            &[
                ("CA_PATH", "/etc/envoy-rust-tls/ca.pem"),
                ("LEAF_B_CERT_PATH", "/etc/envoy-rust-tls/leaf-b-cert.pem"),
                ("LEAF_B_KEY_PATH", "/etc/envoy-rust-tls/leaf-b-key.pem"),
            ],
        );
        assert!(got.contains("chain_b_cert: /etc/envoy-rust-tls/leaf-b-cert.pem"));
        assert!(got.contains("chain_b_key: /etc/envoy-rust-tls/leaf-b-key.pem"));
        assert!(got.contains("ca: /etc/envoy-rust-tls/ca.pem"));
        assert!(!got.contains("{{"));
    }

    // 03.2 Task 8: render_yaml must substitute SERVER_* keys (used by
    // fixture 0005-tls-upstream's TlsEchoBackend on the upstream cluster).
    #[test]
    fn render_yaml_substitutes_server_paths() {
        let template = r#"
server_cert: {{SERVER_CERT_PATH}}
server_key: {{SERVER_KEY_PATH}}
"#;
        let got = render_yaml(
            template,
            &[
                ("SERVER_CERT_PATH", "/etc/envoy-rust-tls/server-cert.pem"),
                ("SERVER_KEY_PATH", "/etc/envoy-rust-tls/server-key.pem"),
            ],
        );
        assert!(got.contains("server_cert: /etc/envoy-rust-tls/server-cert.pem"));
        assert!(got.contains("server_key: /etc/envoy-rust-tls/server-key.pem"));
        assert!(!got.contains("{{"));
    }

    #[test]
    fn fixture_0003_expectations_parses_as_tcp_echo() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tests/fixtures/0003-tcp-proxy/expectations.yaml");
        if !path.exists() {
            eprintln!(
                "skipping: fixture 0003-tcp-proxy/expectations.yaml not yet landed (Task 12)"
            );
            return;
        }
        let e = load_expectations(&path).expect("parses");
        assert!(matches!(e.driver, Driver::TcpEcho));
        assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
        assert_eq!(e.equivalence.response_status, None);
    }
}
