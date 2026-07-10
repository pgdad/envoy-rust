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

/// Run `envoy-bin -c <yaml>` to completion and return (exit-ok, combined output).
///
/// Both streams are captured: `install_tracing`'s `fmt()` subscriber writes the
/// `envoy-rust exited with error` line — which carries the `ConfigError` — to
/// STDOUT, while `main`'s argv/runtime-build failures `eprintln!` to STDERR.
///
/// Used only by the negative-config tests: a rejected config exits non-zero fast.
/// **Never call this on a valid config** — the binary would serve forever.
fn validate_config(yaml: &str) -> (bool, String) {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("envoy-rust.yaml");
    std::fs::File::create(&cfg)
        .unwrap()
        .write_all(yaml.as_bytes())
        .unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c")
        .arg(&cfg)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run envoy-bin");
    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), combined)
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
    tokio::time::timeout(Duration::from_secs(5), s.read_to_end(&mut out))
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
    s.read_to_end(&mut out).await.expect("clean EOF");
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
    s.read_to_end(&mut out).await.expect("echo round-trip");
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
    s.read_to_end(&mut out)
        .await
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
    let (ok, output) = validate_config(&yaml);
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
    let (ok, output) = validate_config(&yaml);
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

/// D1 / SPEC R-3: an EMPTY `stat_prefix` is rejected at startup.
#[tokio::test]
async fn empty_stat_prefix_is_rejected() {
    let (ok, output) = validate_config(&rbac_echo_cfg(reserve_port(), r#""""#, ""));
    assert!(!ok, "empty stat_prefix must be rejected");
    assert!(output.contains("stat_prefix"), "got {output}");
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
        let (ok, output) = validate_config(&rbac_echo_cfg(reserve_port(), "sp", &rules));
        assert!(!ok, "{arm} must be rejected at L4");
        assert!(
            output.contains(arm),
            "error must name the arm {arm}; got {output}"
        );
    }
}
