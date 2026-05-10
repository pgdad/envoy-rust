//! In-process backstop for fixture 0002's `/ready` semantics post-admin-migration.
//! Spawns envoy-bin via CARGO_BIN_EXE_envoy-bin against a fixture-0002-style
//! admin-only bootstrap, drives a `GET /ready` HTTP/1.1 request, asserts a
//! 200 "LIVE\n" response. Independent of Docker availability; runs under
//! plain `cargo test --workspace`.

use std::io::Read;
use std::net::TcpStream as StdTcpStream;
use std::process::{Command, Stdio};
use std::time::Duration;

const ADMIN_BOOTSTRAP_YAML: &str = r#"node:
  id: backstop
  cluster: backstop
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 0
static_resources:
  listeners: []
  clusters: []
"#;

#[test]
fn admin_ready_returns_200_post_migration() {
    let dir = tempfile::tempdir().expect("tempdir");
    let yaml_path = dir.path().join("admin_ready.yaml");
    std::fs::write(&yaml_path, ADMIN_BOOTSTRAP_YAML).expect("write yaml");

    let bin = env!("CARGO_BIN_EXE_envoy-bin");
    let mut child = Command::new(bin)
        .arg("-c")
        .arg(&yaml_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn envoy-bin");

    // Scrape stdout for the `envoy-rust listening (admin) addr=127.0.0.1:NNNNN` line.
    // (`tracing_subscriber::fmt()` writes to stdout by default.)
    let stdout = child.stdout.as_mut().expect("stdout captured");
    let port = scrape_admin_port(stdout).expect("admin port from log");

    // Drive GET /ready.
    let req = b"GET /ready HTTP/1.1\r\nHost: x\r\n\r\n";
    let resp = drive_request(("127.0.0.1", port), req).expect("drive /ready");
    let s = std::str::from_utf8(&resp).unwrap();
    assert!(
        s.starts_with("HTTP/1.1 200 OK\r\n"),
        "expected 200 OK, got: {s}"
    );
    assert!(s.ends_with("LIVE\n"), "expected LIVE\\n body, got: {s}");

    // SPEC §3 D3 "non-negotiable mirroring" guard — all 4 standard admin
    // headers preserved across the migration (the deleted in-package
    // crates/envoy-bin/src/admin.rs::render_response emitted these too).
    assert!(
        s.contains("server: envoy-rust\r\n"),
        "missing server header: {s}"
    );
    assert!(
        s.contains("cache-control: no-cache, max-age=0\r\n"),
        "missing cache-control: {s}"
    );
    assert!(
        s.contains("x-content-type-options: nosniff\r\n"),
        "missing x-content-type-options: {s}"
    );
    assert!(s.contains("date: "), "missing date header: {s}");

    // SIGKILL — matches the 04.x / 05.x integration-test posture
    // (phase-02.2 REVIEW M1 awareness-only carryforward).
    let _ = child.kill();
    let _ = child.wait();
}

fn scrape_admin_port(stdout: &mut std::process::ChildStdout) -> Option<u16> {
    use std::io::BufRead;
    let reader = std::io::BufReader::new(stdout);
    for line in reader.lines() {
        let line = line.ok()?;
        if line.contains("listening (admin)") {
            // Look for "addr=127.0.0.1:<port>" in the line.
            if let Some(pos) = line.find("127.0.0.1:") {
                let tail = &line[pos + "127.0.0.1:".len()..];
                let port_str: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(port) = port_str.parse() {
                    return Some(port);
                }
            }
        }
    }
    None
}

fn drive_request(addr: (&str, u16), req: &[u8]) -> std::io::Result<Vec<u8>> {
    use std::io::Write;
    let mut s = StdTcpStream::connect_timeout(
        &format!("{}:{}", addr.0, addr.1).parse().unwrap(),
        Duration::from_secs(5),
    )?;
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    s.write_all(req)?;
    s.shutdown(std::net::Shutdown::Write)?;
    let mut buf = Vec::new();
    s.read_to_end(&mut buf)?;
    Ok(buf)
}
