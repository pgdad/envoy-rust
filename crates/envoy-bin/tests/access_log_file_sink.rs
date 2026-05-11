//! In-process integration backstop for 06.2's access-log file sink.
//!
//! Spawns envoy-bin via CARGO_BIN_EXE_envoy-bin against an HCM
//! config with an `access_log:` block writing to a tempdir path;
//! drives a single GET / over HTTP/1.1; reads the access-log file
//! post-request; asserts the line tokens.
//!
//! Runs without Docker. Mirrors phase-04.1's http1_direct_response.rs,
//! phase-05.2's http2_direct_response.rs, and phase-06.1's
//! admin_ready.rs structure.

use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context as _, Result};
use tempfile::tempdir;

const ENVOY_BIN: &str = env!("CARGO_BIN_EXE_envoy-bin");

fn pick_free_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let port = l.local_addr().expect("local_addr").port();
    drop(l);
    port
}

fn write_yaml_config(dir: &std::path::Path, listener_port: u16, access_log_path: &str) -> PathBuf {
    let yaml = format!(
        r#"
node: {{ id: it-06.2, cluster: it-06.2 }}
static_resources:
  listeners:
    - name: http1_listener
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: {listener_port} }} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: {access_log_path}
                route_config:
                  name: r
                  virtual_hosts:
                    - name: v
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response: {{ status: 200, body: {{ inline_string: "ok\n" }} }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#,
        listener_port = listener_port,
        access_log_path = access_log_path,
    );
    let yaml_path = dir.join("envoy-rust.yaml");
    std::fs::write(&yaml_path, yaml).expect("write yaml");
    yaml_path
}

fn wait_for_port(addr: SocketAddr, deadline: std::time::Instant) -> Result<TcpStream> {
    loop {
        match TcpStream::connect_timeout(&addr, Duration::from_millis(100)) {
            Ok(s) => return Ok(s),
            Err(_) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(e).context("port did not open"),
        }
    }
}

#[test]
fn access_log_file_sink_in_process() -> Result<()> {
    let dir = tempdir().expect("tempdir");
    let access_log_path = dir.path().join("access.log");
    let access_log_path_str = access_log_path.to_str().expect("utf8 path");
    let listener_port = pick_free_port();
    let yaml_path = write_yaml_config(dir.path(), listener_port, access_log_path_str);

    // Spawn envoy-bin.
    let mut child = std::process::Command::new(ENVOY_BIN)
        .arg("-c")
        .arg(&yaml_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("spawn envoy-bin")?;

    let result: Result<()> = (|| {
        let addr: SocketAddr = format!("127.0.0.1:{}", listener_port).parse().unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut stream = wait_for_port(addr, deadline)?;

        // Drive one GET /.
        let request = b"GET / HTTP/1.1\r\nHost: envoy-rust.test\r\nConnection: close\r\n\r\n";
        stream.write_all(request).context("write request")?;
        let mut response = Vec::new();
        stream.read_to_end(&mut response).context("read response")?;

        // Verify response: 200 OK + body contains "ok\n".
        let resp_str = String::from_utf8_lossy(&response);
        assert!(resp_str.contains("200"), "response: {}", resp_str);
        assert!(
            resp_str.contains("ok\n") || resp_str.contains("ok"),
            "response: {}",
            resp_str
        );

        // Give the synchronous-after-write emission a brief moment
        // (the HCM dispatches sink.emit().await before returning to
        // the keep-alive loop, but the OS file flush is on close).
        // We don't close the FileSink explicitly; envoy-bin holds the
        // FileSink open for the listener's lifetime. The OS write
        // should have made the bytes durable via the kernel buffer
        // before our std::fs::read_to_string call below.
        std::thread::sleep(Duration::from_millis(100));

        let log_contents =
            std::fs::read_to_string(&access_log_path).context("read access log file")?;
        let lines: Vec<&str> = log_contents.lines().collect();
        assert_eq!(
            lines.len(),
            1,
            "expected 1 access-log line; got {}: {:?}",
            lines.len(),
            log_contents
        );
        let line = lines[0];
        // Per the Envoy default format with fixture-0012's surface.
        assert!(
            line.contains("\"GET / HTTP/1.1\" 200 - 0 3 "),
            "access log line: {}",
            line
        );

        Ok(())
    })();

    // Clean up the child.
    let _ = child.kill();
    let _ = child.wait();

    result
}
