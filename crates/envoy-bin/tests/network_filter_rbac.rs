//! Phase 67.1 backstops: boot the real `envoy-bin` with an
//! `envoy.filters.network.rbac` chain and assert the observable contract the
//! differential fixtures `0072`/`0073` cannot see in-process.
//!
//! The real cross-proxy assertions are the Docker-gated
//! `tests/differential/tests/network_filter_rbac_{deny,allow}.rs`.
//!
//! **ADR-0131 governs the timing here.** Upstream Envoy evaluates network RBAC on
//! the FIRST DOWNSTREAM BYTE (`ONE_TIME_ON_FIRST_BYTE`), not at connection
//! establishment; envoy-rust matches. Every probe below that expects a decision
//! therefore SENDS A BYTE FIRST. A probe that sends nothing is never evaluated —
//! which is itself pinned, by `connection_that_sends_nothing_is_never_evaluated`.
//!
//! **ADR-0132 governs the COMPOSITION here.** That first-byte rule is the RBAC
//! *verdict's* timing, not the terminal filter's. Upstream runs every filter's
//! `onNewConnection` at connection establishment — the terminal filter's included.
//! `echo` (and `http_connection_manager`) have no establishment-time work, so the
//! chain's first-byte gate is observationally correct for them. `direct_response`
//! writes and closes at establishment, so it BYPASSES the chain entirely; the two
//! `*_direct_response_*` probes below are the witnesses. `tcp_proxy` connects
//! upstream at establishment and is REJECTED at config load until phase `67.3`.

use std::io::Write;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

mod common;
use common::{reserve_port, scrape_admin_stats, wait_ready};

fn spawn_envoy_bin(yaml: &str) -> (tokio::process::Child, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("envoy-rust.yaml");
    std::fs::File::create(&cfg)
        .unwrap()
        .write_all(yaml.as_bytes())
        .unwrap();
    let child = tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c")
        .arg(&cfg)
        .env("ENVOY_RUST_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin");
    (child, dir)
}

/// M-2: every downstream read in this file is BOUNDED. Three probes here used to
/// `read_to_end` with no timeout, so a "the connection never closes" regression --
/// exactly what C-1 produced in neighboring configs -- hung CI instead of failing
/// with a useful message. Any read that legitimately completes does so in
/// milliseconds against a loopback listener.
const READ_BUDGET: Duration = Duration::from_secs(5);

/// How long a config REJECTION may take before we conclude the config was in fact
/// ACCEPTED. A rejected config exits non-zero within milliseconds; this bound only
/// has to beat process startup.
const VALIDATE_BUDGET: Duration = Duration::from_secs(20);

/// Run `envoy-bin -c <yaml>` and return (exit-ok, combined output).
///
/// Both streams are captured: `install_tracing`'s `fmt()` subscriber writes the
/// `envoy-rust exited with error` line — which carries the `ConfigError` — to
/// STDOUT, while `main`'s argv/runtime-build failures `eprintln!` to STDERR.
///
/// **Bounded.** A config envoy-bin ACCEPTS makes the binary serve forever, so an
/// unbounded wait here turns "the rejection I asserted is missing" into a hung
/// test rather than a failing one — the exact M-2 failure mode. Past
/// [`VALIDATE_BUDGET`] the child is killed (`kill_on_drop`) and we report
/// `ok = true`: it did not reject. The caller's `assert!(!ok, …)` then fails with
/// a useful message instead of hanging CI.
async fn validate_config(yaml: &str) -> (bool, String) {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("envoy-rust.yaml");
    std::fs::File::create(&cfg)
        .unwrap()
        .write_all(yaml.as_bytes())
        .unwrap();
    let child = tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c")
        .arg(&cfg)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin");
    // On elapse the `wait_with_output` future — which owns `child` — is dropped,
    // and `kill_on_drop` reaps the process.
    match tokio::time::timeout(VALIDATE_BUDGET, child.wait_with_output()).await {
        Ok(out) => {
            let out = out.expect("run envoy-bin");
            let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
            combined.push_str(&String::from_utf8_lossy(&out.stderr));
            (out.status.success(), combined)
        }
        Err(_) => (
            true,
            format!(
                "<envoy-bin did not exit within {VALIDATE_BUDGET:?}: the config was ACCEPTED \
                 and the binary is serving>"
            ),
        ),
    }
}

fn rbac_echo_cfg(port: u16, stat_prefix: &str, rules_block: &str) -> String {
    format!(
        r#"
static_resources:
  listeners:
    - name: rbac_listener
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.rbac
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
                stat_prefix: {stat_prefix}
{rules_block}
            - name: envoy.filters.network.echo
"#
    )
}

fn rbac_echo_cfg_with_admin(
    port: u16,
    admin_port: u16,
    stat_prefix: &str,
    rules_block: &str,
) -> String {
    format!(
        r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: {admin_port}
{}"#,
        rbac_echo_cfg(port, stat_prefix, rules_block)
    )
}

/// ADR-0132 decision 2: `[rbac, direct_response]`. The terminal filter writes its
/// payload at connection ESTABLISHMENT, so the chain is bypassed entirely and the
/// rbac counters — while REGISTERED — never tick. Measured against
/// `envoyproxy/envoy:v1.33.0` (D-3.7).
fn rbac_direct_response_cfg_with_admin(
    port: u16,
    admin_port: u16,
    stat_prefix: &str,
    rules_block: &str,
) -> String {
    format!(
        r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: {admin_port}
static_resources:
  listeners:
    - name: rbac_dr_listener
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.rbac
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
                stat_prefix: {stat_prefix}
{rules_block}
            - name: envoy.filters.network.direct_response
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config
                response:
                  inline_string: "HELLO-DR\n"
"#
    )
}

/// REVIEW.md I-5: `[rbac, http_connection_manager]`. `hcm` does NO
/// establishment-time work (measured, ADR-0132), so it is one of the two terminal
/// filters the chain's first-byte gate models correctly. The HCM routes to a
/// `direct_response` route, so this needs no backend.
fn rbac_hcm_cfg_with_admin(
    port: u16,
    admin_port: u16,
    stat_prefix: &str,
    rules_block: &str,
) -> String {
    format!(
        r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: {admin_port}
static_resources:
  listeners:
    - name: rbac_hcm_listener
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.rbac
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
                stat_prefix: {stat_prefix}
{rules_block}
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok\n" }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#
    )
}

const DENY_ALL: &str = r#"                rules:
                  action: DENY
                  policies:
                    p0:
                      permissions: [{ any: true }]
                      principals: [{ any: true }]"#;

const ALLOW_ALL: &str = r#"                rules:
                  action: ALLOW
                  policies:
                    p0:
                      permissions: [{ any: true }]
                      principals: [{ any: true }]"#;

/// SPEC R-2: DENY writes ZERO bytes and closes with a CLEAN EOF — never an RST.
/// The client's already-sent bytes are DISCARDED (the terminal `echo` never runs).
#[tokio::test]
async fn deny_writes_zero_bytes_and_closes_cleanly_discarding_client_bytes() {
    let port = reserve_port();
    let (_child, _dir) = spawn_envoy_bin(&rbac_echo_cfg(port, "d", DENY_ALL));
    let addr = format!("127.0.0.1:{port}").parse().unwrap();
    wait_ready(addr, Duration::from_secs(10))
        .await
        .expect("listener up");

    let mut s = TcpStream::connect(addr).await.unwrap();
    // ADR-0131: the decision fires on the first downstream byte.
    s.write_all(b"PING-RBAC\n").await.unwrap();
    let mut out = Vec::new();
    tokio::time::timeout(READ_BUDGET, s.read_to_end(&mut out))
        .await
        .expect("DENY must half-close within 5s")
        .expect("clean EOF, not RST");
    assert!(
        out.is_empty(),
        "DENY writes zero bytes; the terminal echo must NOT run. got {out:?}"
    );
}

/// SPEC R-2 / ADR-0124's drain on the DENY path: a client write issued AFTER it
/// observes EOF is ACCEPTED, not reset. A server closing without draining its
/// read half would RST the client.
///
/// DELETE THE DRAIN LOOP IN `envoy_listener::close_with_drain` AND THIS TEST MUST FAIL.
#[tokio::test]
async fn deny_post_eof_client_write_is_accepted_not_reset() {
    let port = reserve_port();
    let (_child, _dir) = spawn_envoy_bin(&rbac_echo_cfg(port, "d", DENY_ALL));
    let addr = format!("127.0.0.1:{port}").parse().unwrap();
    wait_ready(addr, Duration::from_secs(10))
        .await
        .expect("listener up");

    let mut s = TcpStream::connect(addr).await.unwrap();
    // ADR-0131: a byte is required for the decision to be taken at all.
    s.write_all(b"x").await.unwrap();
    let mut out = Vec::new();
    // M-2: bounded. A "never closes" regression must FAIL here, not hang CI.
    tokio::time::timeout(READ_BUDGET, s.read_to_end(&mut out))
        .await
        .expect("DENY must half-close within the read budget")
        .expect("clean EOF");
    assert!(out.is_empty());

    // Two writes: the first may be absorbed locally; a returning RST surfaces on
    // the second. Sleep between them so an RST can land.
    s.write_all(b"y").await.expect("first post-EOF write");
    tokio::time::sleep(Duration::from_millis(50)).await;
    s.write_all(b"y")
        .await
        .expect("second post-EOF write must not be reset");
}

/// SPEC R-2: ALLOW yields to the TERMINAL echo and the payload round-trips.
/// This is the iteration protocol, in-process.
#[tokio::test]
async fn allow_yields_to_the_terminal_echo_filter() {
    let port = reserve_port();
    let (_child, _dir) = spawn_envoy_bin(&rbac_echo_cfg(port, "a", ALLOW_ALL));
    let addr = format!("127.0.0.1:{port}").parse().unwrap();
    wait_ready(addr, Duration::from_secs(10))
        .await
        .expect("listener up");

    let mut s = TcpStream::connect(addr).await.unwrap();
    s.write_all(b"PING-RBAC\n").await.unwrap();
    s.shutdown().await.unwrap();
    let mut out = Vec::new();
    // M-2: bounded — an ALLOW that never yields would otherwise hang CI.
    tokio::time::timeout(READ_BUDGET, s.read_to_end(&mut out))
        .await
        .expect("ALLOW must yield to the terminal echo within the read budget")
        .expect("echo round-trip");
    assert_eq!(out, b"PING-RBAC\n");
}

/// ADR-0131 — the FIRST-BYTE witness, against the real binary.
///
/// Upstream Envoy evaluates network RBAC on the first downstream byte
/// (`ONE_TIME_ON_FIRST_BYTE`), measured: a client that connects and sends nothing
/// is never evaluated, the connection stays OPEN, and NEITHER counter ticks — even
/// on a DENY-all policy. envoy-rust matches by peeking for the first byte.
///
/// REMOVE THE `peek` IN `ChainHandler::handle` AND THIS TEST MUST FAIL: the
/// connection would be denied and closed immediately, and `denied` would read 1.
#[tokio::test]
async fn connection_that_sends_nothing_is_never_evaluated() {
    let port = reserve_port();
    let admin_port = reserve_port();
    let (_child, _dir) =
        spawn_envoy_bin(&rbac_echo_cfg_with_admin(port, admin_port, "n", DENY_ALL));
    let data_addr = format!("127.0.0.1:{port}").parse().unwrap();
    let admin_addr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    wait_ready(admin_addr, Duration::from_secs(10))
        .await
        .expect("admin up");
    wait_ready(data_addr, Duration::from_secs(10))
        .await
        .expect("listener up");

    let mut s = TcpStream::connect(data_addr).await.unwrap();
    // Send NOTHING. The peer must not respond and must not close.
    let mut buf = [0u8; 16];
    let read = tokio::time::timeout(Duration::from_millis(800), s.read(&mut buf)).await;
    assert!(
        read.is_err(),
        "a byte-less connection must stay open and unanswered; got {read:?}",
    );

    let stats = scrape_admin_stats(admin_addr).await;
    assert_eq!(
        stats.get("n.rbac.denied").copied(),
        Some(0),
        "no decision is taken until the first downstream byte (ADR-0131)",
    );
    assert_eq!(stats.get("n.rbac.allowed").copied(), Some(0));
}

/// SPEC R-4 (PLAN-VERIFY W-6) — THE INERTNESS WITNESS, against the real binary.
///
/// `rules` omitted ⇒ the filter is INERT: the connection is ALLOWED and NEITHER
/// counter increments. A naive default `Rules { action: ALLOW }` would tick
/// `allowed` — a STAT divergence with NO body divergence, invisible to a
/// body-only fixture. All four counters are still REGISTERED at 0, so the stat
/// tree matches upstream's shape.
#[tokio::test]
async fn rules_omitted_is_inert_neither_counter_ticks() {
    let port = reserve_port();
    let admin_port = reserve_port();
    let (_child, _dir) =
        spawn_envoy_bin(&rbac_echo_cfg_with_admin(port, admin_port, "norules", ""));
    let data_addr = format!("127.0.0.1:{port}").parse().unwrap();
    let admin_addr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    wait_ready(admin_addr, Duration::from_secs(10))
        .await
        .expect("admin up");
    wait_ready(data_addr, Duration::from_secs(10))
        .await
        .expect("listener up");

    let mut s = TcpStream::connect(data_addr).await.unwrap();
    s.write_all(b"HELLO\n").await.unwrap();
    s.shutdown().await.unwrap();
    let mut out = Vec::new();
    // M-2: bounded — an INERT filter that silently stalled would otherwise hang CI.
    tokio::time::timeout(READ_BUDGET, s.read_to_end(&mut out))
        .await
        .expect("an inert filter must pass the connection through within the read budget")
        .expect("allowed through to echo");
    assert_eq!(out, b"HELLO\n", "an inert filter allows the connection");

    tokio::time::sleep(Duration::from_millis(300)).await;
    let stats = scrape_admin_stats(admin_addr).await;
    for name in [
        "norules.rbac.allowed",
        "norules.rbac.denied",
        "norules.rbac.shadow_allowed",
        "norules.rbac.shadow_denied",
    ] {
        let got = stats
            .get(name)
            .copied()
            .unwrap_or_else(|| panic!("counter {name} must be REGISTERED (stat tree parity)"));
        assert_eq!(got, 0, "INERT: {name} must not tick");
    }
}

/// ADR-0132 decision 2 — THE C-1 REGRESSION WITNESS.
///
/// `[rbac(ALLOW, any), direct_response]`, client sends NOTHING. Upstream Envoy
/// delivers the payload and closes cleanly, because it runs EVERY filter's
/// `onNewConnection` at connection establishment — the TERMINAL filter's
/// included — and defers only the RBAC *verdict* to the first downstream byte.
/// `direct_response` writes and closes before any `onData` can fire, so RBAC
/// never evaluates and no counter ticks.
///
/// Against the pre-fix code this test HANGS (the `ChainHandler` peek waits for a
/// first byte that a well-behaved client of a server-speaks-first protocol never
/// sends), which is why the read is wrapped in a timeout — see M-2.
///
/// RESTORE `wrap_in_chain` ON THE `direct_response` ARM AND THIS TEST MUST FAIL.
#[tokio::test]
async fn direct_response_delivers_payload_to_a_client_that_sends_nothing() {
    let port = reserve_port();
    let admin_port = reserve_port();
    let (_child, _dir) = spawn_envoy_bin(&rbac_direct_response_cfg_with_admin(
        port, admin_port, "dra", ALLOW_ALL,
    ));
    let data_addr = format!("127.0.0.1:{port}").parse().unwrap();
    let admin_addr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    wait_ready(admin_addr, Duration::from_secs(10))
        .await
        .expect("admin up");
    wait_ready(data_addr, Duration::from_secs(10))
        .await
        .expect("listener up");

    let mut s = TcpStream::connect(data_addr).await.unwrap();
    // Send NOTHING. The terminal filter speaks first.
    let mut out = Vec::new();
    tokio::time::timeout(READ_BUDGET, s.read_to_end(&mut out))
        .await
        .expect("direct_response must write its payload without any client byte")
        .expect("clean EOF, not RST");
    assert_eq!(out, b"HELLO-DR\n");

    let stats = scrape_admin_stats(admin_addr).await;
    for name in ["dra.rbac.allowed", "dra.rbac.denied"] {
        let got = stats
            .get(name)
            .copied()
            .unwrap_or_else(|| panic!("counter {name} must be REGISTERED (stat tree parity)"));
        assert_eq!(got, 0, "the chain is bypassed; {name} must never tick");
    }
}

/// ADR-0132 decision 2, the counter-intuitive half: **a DENY policy does NOT
/// suppress the payload.** Measured on upstream Envoy — `[rbac(DENY, any),
/// direct_response]` delivers `HELLO-DR\n` and closes cleanly with all four
/// counters at `0`, whether or not the client sends a byte.
///
/// Both client behaviors are probed: sending a first byte must not change the
/// outcome, because the terminal filter has already written and closed.
#[tokio::test]
async fn deny_does_not_suppress_the_direct_response_payload() {
    let port = reserve_port();
    let admin_port = reserve_port();
    let (_child, _dir) = spawn_envoy_bin(&rbac_direct_response_cfg_with_admin(
        port, admin_port, "drd", DENY_ALL,
    ));
    let data_addr = format!("127.0.0.1:{port}").parse().unwrap();
    let admin_addr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    wait_ready(admin_addr, Duration::from_secs(10))
        .await
        .expect("admin up");
    wait_ready(data_addr, Duration::from_secs(10))
        .await
        .expect("listener up");

    for send_first_byte in [false, true] {
        let mut s = TcpStream::connect(data_addr).await.unwrap();
        if send_first_byte {
            s.write_all(b"X").await.unwrap();
        }
        let mut out = Vec::new();
        tokio::time::timeout(READ_BUDGET, s.read_to_end(&mut out))
            .await
            .unwrap_or_else(|_| {
                panic!("DENY must still deliver (send_first_byte={send_first_byte})")
            })
            .expect("clean EOF, not RST");
        assert_eq!(
            out, b"HELLO-DR\n",
            "a DENY policy must NOT suppress the direct_response payload \
             (send_first_byte={send_first_byte})",
        );
    }

    tokio::time::sleep(Duration::from_millis(300)).await;
    let stats = scrape_admin_stats(admin_addr).await;
    for name in [
        "drd.rbac.allowed",
        "drd.rbac.denied",
        "drd.rbac.shadow_allowed",
        "drd.rbac.shadow_denied",
    ] {
        let got = stats
            .get(name)
            .copied()
            .unwrap_or_else(|| panic!("counter {name} must be REGISTERED (stat tree parity)"));
        assert_eq!(got, 0, "the chain is bypassed; {name} must never tick");
    }
}

/// REVIEW.md I-5 + ADR-0132 decision 1 — the `[rbac, hcm]` composition, ALLOW.
///
/// `http_connection_manager` does no establishment-time work, so the chain's
/// first-byte gate is observationally identical to upstream's model: the verdict
/// is taken on the first request byte, the HCM then serves the request, and
/// `allowed` ticks exactly once.
///
/// This is one of the three compositions that had ZERO coverage when C-1 shipped
/// (`rbac` was paired with `echo` in every fixture and every backstop, and with
/// nothing else, anywhere).
#[tokio::test]
async fn rbac_before_hcm_evaluates_on_the_first_request() {
    let port = reserve_port();
    let admin_port = reserve_port();
    let (_child, _dir) =
        spawn_envoy_bin(&rbac_hcm_cfg_with_admin(port, admin_port, "ha", ALLOW_ALL));
    let data_addr = format!("127.0.0.1:{port}").parse().unwrap();
    let admin_addr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    wait_ready(admin_addr, Duration::from_secs(10))
        .await
        .expect("admin up");
    wait_ready(data_addr, Duration::from_secs(10))
        .await
        .expect("listener up");

    let mut s = TcpStream::connect(data_addr).await.unwrap();
    s.write_all(b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut out = Vec::new();
    tokio::time::timeout(READ_BUDGET, s.read_to_end(&mut out))
        .await
        .expect("ALLOW must yield to the terminal HCM within 5s")
        .expect("clean EOF");
    let text = String::from_utf8_lossy(&out);
    assert!(text.starts_with("HTTP/1.1 200"), "got {text:?}");
    assert!(text.ends_with("ok\n"), "got {text:?}");

    tokio::time::sleep(Duration::from_millis(300)).await;
    let stats = scrape_admin_stats(admin_addr).await;
    assert_eq!(
        stats.get("ha.rbac.allowed").copied(),
        Some(1),
        "ALLOW ticks `allowed` exactly once per connection",
    );
    assert_eq!(stats.get("ha.rbac.denied").copied(), Some(0));
}

/// REVIEW.md I-5 (composition) **and M-3** (the missing in-process `denied == 1`
/// witness — in-process `denied` was previously asserted `== 0` twice and never
/// `== 1`, so the positive tick rode entirely on the Docker-gated fixture `0072`).
///
/// `[rbac(DENY), hcm]`: the HCM never runs, zero bytes are written, the connection
/// closes with a clean EOF, and `denied` reaches exactly 1.
#[tokio::test]
async fn deny_before_hcm_writes_nothing_and_ticks_denied_once() {
    let port = reserve_port();
    let admin_port = reserve_port();
    let (_child, _dir) =
        spawn_envoy_bin(&rbac_hcm_cfg_with_admin(port, admin_port, "hd", DENY_ALL));
    let data_addr = format!("127.0.0.1:{port}").parse().unwrap();
    let admin_addr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    wait_ready(admin_addr, Duration::from_secs(10))
        .await
        .expect("admin up");
    wait_ready(data_addr, Duration::from_secs(10))
        .await
        .expect("listener up");

    let mut s = TcpStream::connect(data_addr).await.unwrap();
    s.write_all(b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut out = Vec::new();
    tokio::time::timeout(READ_BUDGET, s.read_to_end(&mut out))
        .await
        .expect("DENY must half-close within 5s")
        .expect("clean EOF, not RST");
    assert!(
        out.is_empty(),
        "DENY writes zero bytes; the terminal HCM must NOT run — not even a 403. got {out:?}",
    );

    tokio::time::sleep(Duration::from_millis(300)).await;
    let stats = scrape_admin_stats(admin_addr).await;
    assert_eq!(
        stats.get("hd.rbac.denied").copied(),
        Some(1),
        "M-3: the DENY tick needs a Docker-independent witness",
    );
    assert_eq!(stats.get("hd.rbac.allowed").copied(), Some(0));
}

/// SPEC R-1: a chain whose LAST filter is non-terminal is REJECTED at startup.
#[tokio::test]
async fn rbac_alone_is_rejected_at_startup() {
    let port = reserve_port();
    let yaml = format!(
        r#"
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: {{ address: 127.0.0.1, port_value: {port} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.rbac
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
                stat_prefix: sp
"#
    );
    let (ok, output) = validate_config(&yaml).await;
    assert!(!ok, "[rbac] alone must be rejected");
    assert!(output.contains("non-terminal filter"), "got {output}");
}

/// SPEC R-5: ERROR PRECEDENCE. `[echo, rbac]` violates BOTH rules; the
/// TERMINAL-not-last error wins.
#[tokio::test]
async fn echo_before_rbac_reports_the_terminal_error() {
    let port = reserve_port();
    let yaml = format!(
        r#"
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: {{ address: 127.0.0.1, port_value: {port} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
            - name: envoy.filters.network.rbac
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
                stat_prefix: sp
"#
    );
    let (ok, output) = validate_config(&yaml).await;
    assert!(!ok, "[echo, rbac] must be rejected");
    assert!(
        output.contains("must be the last filter"),
        "terminal-not-last must WIN over chain-not-terminated; got {output}",
    );
    assert!(
        !output.contains("non-terminal filter"),
        "wrong error won; got {output}"
    );
}

/// SPEC R-7 / ADR-0130 §2: `filters: []` is ACCEPTED (upstream parity) and must
/// NOT panic. envoy-rust used to crash here with `validator guarantees ≥1 filter`
/// while upstream Envoy accepts the same config and starts.
#[tokio::test]
async fn empty_filter_chain_starts_without_panicking() {
    let admin_port = reserve_port();
    let yaml = format!(
        r#"
admin:
  address:
    socket_address: {{ address: 127.0.0.1, port_value: {admin_port} }}
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: {{ address: 127.0.0.1, port_value: 0 }}
      filter_chains:
        - filters: []
"#
    );
    let (mut child, _dir) = spawn_envoy_bin(&yaml);
    let admin_addr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    wait_ready(admin_addr, Duration::from_secs(10))
        .await
        .expect("admin listener serves despite the empty data chain");
    assert!(
        child.try_wait().unwrap().is_none(),
        "process must still be alive (no panic)"
    );
    child.kill().await.ok();
}

/// ADR-0132 decision 4: `[rbac, tcp_proxy]` is REJECTED AT CONFIG LOAD, fail-loud,
/// until phase `67.3` lands the establishment/data-phase split.
///
/// `tcp_proxy` connects upstream at connection establishment and relays a
/// server-first banner before any downstream byte. Under `ChainHandler`'s
/// first-byte `peek` that composition is a **runtime deadlock** — the client waits
/// for a banner while envoy-rust waits for a byte. Upstream Envoy ACCEPTS this
/// config, so the rejection is a deliberate divergence in the fail-loud direction
/// (`ADR-0049` decision-2 (b)), recorded in `BEHAVIOR_CONTRACT.md` and strictly
/// better than shipping the hang. **`67.3` DELETES this rejection.**
#[tokio::test]
async fn rbac_before_tcp_proxy_is_rejected_at_config_load() {
    let port = reserve_port();
    let yaml = format!(
        r#"
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: {{ address: 127.0.0.1, port_value: {port} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.rbac
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
                stat_prefix: sp
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: {{ address: 127.0.0.1, port_value: 1 }}
"#
    );
    let (ok, output) = validate_config(&yaml).await;
    assert!(!ok, "[rbac, tcp_proxy] must be rejected until 67.3");
    assert!(
        output.contains("envoy.filters.network.tcp_proxy")
            && output.contains("envoy.filters.network.rbac"),
        "the error must name BOTH filters; got {output}",
    );
    assert!(
        output.contains("67.3"),
        "the error must name its owning phase, never be silent (ADR-0132 D4); got {output}",
    );
}

/// ADR-0132 decision 4, the NEGATIVE half: `tcp_proxy` ALONE is still accepted.
/// The rejection is about the COMPOSITION, not about `tcp_proxy`. Guards against
/// a fix that over-rejects and breaks fixture `0003`.
#[tokio::test]
async fn tcp_proxy_alone_is_still_accepted() {
    let admin_port = reserve_port();
    let yaml = format!(
        r#"
admin:
  address:
    socket_address: {{ address: 127.0.0.1, port_value: {admin_port} }}
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: {{ address: 127.0.0.1, port_value: 0 }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: {{ address: 127.0.0.1, port_value: 1 }}
"#
    );
    let (mut child, _dir) = spawn_envoy_bin(&yaml);
    let admin_addr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    wait_ready(admin_addr, Duration::from_secs(10))
        .await
        .expect("a lone tcp_proxy chain must still start");
    assert!(
        child.try_wait().unwrap().is_none(),
        "process must still be alive"
    );
    child.kill().await.ok();
}

/// D1 / SPEC R-3: an EMPTY `stat_prefix` is rejected at startup.
#[tokio::test]
async fn empty_stat_prefix_is_rejected() {
    let (ok, output) = validate_config(&rbac_echo_cfg(reserve_port(), r#""""#, "")).await;
    assert!(!ok, "empty stat_prefix must be rejected");
    assert!(output.contains("stat_prefix"), "got {output}");
}

/// REVIEW.md I-2: a STRUCTURALLY-INVALID `metadata` leaf on a NETWORK `rbac`
/// filter must not be reported as an `"HCM listener"` error — that listener has no
/// HCM at all.
///
/// The guarding comment in `envoy-config`'s `ConfigError` used to claim this was
/// unreachable "because a network rbac filter's `metadata` leaf is rejected
/// outright by `validate_l4_permission` before that error can be reached."
/// **The validation order is the reverse.** `validate_rbac_rules` runs FIRST and
/// validates `Metadata` leaves structurally, so an empty `filter` raises
/// `RbacMetadataMatcherInvalid` before the L4 allow-list walk ever sees the leaf.
///
/// Note the L4 walk must NOT be reordered ahead of `validate_rbac_rules` — the
/// current order is what bounds tree depth before the L4 recursion, pinned by
/// `network_rbac_depth_bound_precedes_the_l4_walk`. The fix is to make the
/// message scope-neutral instead.
#[tokio::test]
async fn structurally_invalid_metadata_leaf_is_not_reported_as_an_hcm_error() {
    let rules = concat!(
        "                rules:\n",
        "                  action: ALLOW\n",
        "                  policies:\n",
        "                    p0:\n",
        r#"                      permissions: [{ metadata: { filter: "", path: [{ key: k }], value: { string_match: { exact: v } } } }]"#,
        "\n                      principals: [{ any: true }]",
    );
    let (ok, output) = validate_config(&rbac_echo_cfg(reserve_port(), "sp", rules)).await;
    assert!(!ok, "a malformed metadata leaf must be rejected");
    assert!(
        output.contains("metadata matcher"),
        "the structural tree validator must fire first, not the L4 walk; got {output}",
    );
    assert!(
        !output.contains("HCM listener"),
        "a network rbac filter has NO HCM; the message must be scope-neutral. got {output}",
    );
    assert!(
        output.contains(r#"listener "rbac_listener""#),
        "the message must still name the listener; got {output}",
    );
}

/// M-7: locks **CF-67-2**'s boundary. `action: LOG` is a real upstream Envoy RBAC
/// action; envoy-rust's `Action` enum has exactly two variants, so `LOG` is
/// rejected at config load rather than silently treated as ALLOW or DENY. The same
/// goes for `enforcement_type` and `delay_deny`, rejected by serde's
/// `deny_unknown_fields`.
///
/// All three are correctly rejected today; nothing pinned it. Whichever phase
/// consumes CF-67-2 will delete the `LOG` case from this test.
#[tokio::test]
async fn log_action_and_unmodeled_rbac_fields_are_rejected() {
    let cases = [
        (
            "LOG",
            concat!(
                "                rules:\n",
                "                  action: LOG\n",
                "                  policies:\n",
                "                    p0:\n",
                "                      permissions: [{ any: true }]\n",
                "                      principals: [{ any: true }]",
            ),
        ),
        (
            "enforcement_type",
            concat!(
                "                enforcement_type: CONTINUOUS\n",
                "                rules:\n",
                "                  action: ALLOW\n",
                "                  policies:\n",
                "                    p0:\n",
                "                      permissions: [{ any: true }]\n",
                "                      principals: [{ any: true }]",
            ),
        ),
        (
            "delay_deny",
            concat!(
                "                delay_deny: 1s\n",
                "                rules:\n",
                "                  action: ALLOW\n",
                "                  policies:\n",
                "                    p0:\n",
                "                      permissions: [{ any: true }]\n",
                "                      principals: [{ any: true }]",
            ),
        ),
    ];
    for (what, rules) in cases {
        let (ok, output) = validate_config(&rbac_echo_cfg(reserve_port(), "sp", rules)).await;
        assert!(!ok, "{what} must be rejected, never silently ignored");
        assert!(
            output.to_lowercase().contains(&what.to_lowercase()),
            "the error must name {what}; got {output}",
        );
    }
}

/// D3 / CF-67-4: the three L4-unevaluable leaves are rejected at startup.
/// `header` in PARITY with upstream Envoy; `url_path` and `metadata` as a
/// deliberate FAIL-LOUD divergence (ADR-0049 decision-2 (b)).
#[tokio::test]
async fn l4_unevaluable_matcher_leaves_are_rejected() {
    let cases = [
        (
            "header",
            r#"[{ header: { name: ":path", exact_match: "/x" } }]"#,
        ),
        ("url_path", r#"[{ url_path: { path: { exact: "/x" } } }]"#),
        (
            "metadata",
            r#"[{ metadata: { filter: f, path: [{ key: k }], value: { string_match: { exact: v } } } }]"#,
        ),
    ];
    for (arm, perms) in cases {
        let rules = format!(
            "                rules:\n                  action: ALLOW\n                  policies:\n                    p0:\n                      permissions: {perms}\n                      principals: [{{ any: true }}]"
        );
        let (ok, output) = validate_config(&rbac_echo_cfg(reserve_port(), "sp", &rules)).await;
        assert!(!ok, "{arm} must be rejected at L4");
        assert!(
            output.contains(arm),
            "error must name the arm {arm}; got {output}"
        );
    }
}

// --- 67.2 Task 5: end-to-end loopback backstops for the L4 matcher arms --------
//
// A client connecting over loopback has `peer_addr.ip() == 127.0.0.1`, and the
// listener binds `127.0.0.1:{port}`, so `local_addr.{ip(),port()}` are EXACT.
// This is the in-process witness the SPEC's §2 rationale requires (the IP/port
// arms are not host-deterministic under the Docker differential harness, so 67.2
// ships NO new differential fixture). Rules blocks mirror `ALLOW_ALL`'s exact
// 16-space indentation, spliced into `rbac_echo_cfg`.

/// Build an `action: ALLOW` rules block with the given `permissions`/`principals`
/// flow-lists, at `rbac_echo_cfg`'s required indentation.
fn allow_rules(permissions: &str, principals: &str) -> String {
    format!(
        "                rules:\n                  action: ALLOW\n                  policies:\n                    p0:\n                      permissions: {permissions}\n                      principals: {principals}"
    )
}

/// 67.2 D6: `direct_remote_ip: 127.0.0.0/8` matches a loopback client ⇒ ALLOW,
/// the echo terminal round-trips the payload.
#[tokio::test]
async fn direct_remote_ip_loopback_allows_end_to_end() {
    let port = reserve_port();
    let rules = allow_rules(
        "[{ any: true }]",
        "[{ direct_remote_ip: { address_prefix: 127.0.0.0, prefix_len: 8 } }]",
    );
    let (_child, _dir) = spawn_envoy_bin(&rbac_echo_cfg(port, "dr", &rules));
    let addr = format!("127.0.0.1:{port}").parse().unwrap();
    wait_ready(addr, Duration::from_secs(10))
        .await
        .expect("listener up");

    let mut s = TcpStream::connect(addr).await.unwrap();
    s.write_all(b"ping").await.unwrap();
    s.shutdown().await.unwrap();
    let mut out = Vec::new();
    tokio::time::timeout(READ_BUDGET, s.read_to_end(&mut out))
        .await
        .expect("ALLOW must yield to the terminal echo within the read budget")
        .expect("echo round-trip");
    assert_eq!(out, b"ping", "loopback peer ∈ 127.0.0.0/8 ⇒ ALLOW ⇒ echo");
}

/// 67.2 D6: a `direct_remote_ip` range that EXCLUDES loopback ⇒ no policy match ⇒
/// inverse of ALLOW = DENY: zero bytes, clean EOF (the 67.1 DENY wire shape).
#[tokio::test]
async fn direct_remote_ip_non_loopback_denies_end_to_end() {
    let port = reserve_port();
    let rules = allow_rules(
        "[{ any: true }]",
        "[{ direct_remote_ip: { address_prefix: 10.0.0.0, prefix_len: 8 } }]",
    );
    let (_child, _dir) = spawn_envoy_bin(&rbac_echo_cfg(port, "dr2", &rules));
    let addr = format!("127.0.0.1:{port}").parse().unwrap();
    wait_ready(addr, Duration::from_secs(10))
        .await
        .expect("listener up");

    let mut s = TcpStream::connect(addr).await.unwrap();
    // ADR-0131: the decision fires on the first downstream byte.
    s.write_all(b"ping").await.unwrap();
    let mut out = Vec::new();
    tokio::time::timeout(READ_BUDGET, s.read_to_end(&mut out))
        .await
        .expect("DENY must half-close within the read budget")
        .expect("clean EOF, not RST");
    assert!(
        out.is_empty(),
        "loopback peer ∉ 10.0.0.0/8 ⇒ DENY ⇒ zero bytes. got {out:?}",
    );
}

/// 67.2 D6: `destination_port` bound to the listener port ⇒ ALLOW; a rule naming a
/// DIFFERENT port ⇒ DENY. `local_addr.port()` is exactly the bound listener port.
#[tokio::test]
async fn destination_port_end_to_end() {
    // ALLOW: the rule names the listener's own port.
    let port = reserve_port();
    let allow = allow_rules(
        &format!("[{{ destination_port: {port} }}]"),
        "[{ any: true }]",
    );
    let (_child, _dir) = spawn_envoy_bin(&rbac_echo_cfg(port, "dp", &allow));
    let addr = format!("127.0.0.1:{port}").parse().unwrap();
    wait_ready(addr, Duration::from_secs(10))
        .await
        .expect("listener up");
    let mut s = TcpStream::connect(addr).await.unwrap();
    s.write_all(b"ping").await.unwrap();
    s.shutdown().await.unwrap();
    let mut out = Vec::new();
    tokio::time::timeout(READ_BUDGET, s.read_to_end(&mut out))
        .await
        .expect("ALLOW must yield to the terminal echo")
        .expect("echo round-trip");
    assert_eq!(out, b"ping", "local port matches ⇒ ALLOW ⇒ echo");

    // DENY: listener binds `port2`, but the rule names a DIFFERENT reserved port.
    let port2 = reserve_port();
    let wrong = reserve_port();
    let deny = allow_rules(
        &format!("[{{ destination_port: {wrong} }}]"),
        "[{ any: true }]",
    );
    let (_child2, _dir2) = spawn_envoy_bin(&rbac_echo_cfg(port2, "dp2", &deny));
    let addr2 = format!("127.0.0.1:{port2}").parse().unwrap();
    wait_ready(addr2, Duration::from_secs(10))
        .await
        .expect("listener up");
    let mut s2 = TcpStream::connect(addr2).await.unwrap();
    s2.write_all(b"ping").await.unwrap();
    let mut out2 = Vec::new();
    tokio::time::timeout(READ_BUDGET, s2.read_to_end(&mut out2))
        .await
        .expect("DENY must half-close within the read budget")
        .expect("clean EOF, not RST");
    assert!(
        out2.is_empty(),
        "local port {port2} ≠ rule port {wrong} ⇒ DENY ⇒ zero bytes. got {out2:?}",
    );
}
