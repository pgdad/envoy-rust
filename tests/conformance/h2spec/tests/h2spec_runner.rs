#![forbid(unsafe_code)]

//! h2spec conformance runner. Spawns envoy-bin against an HCM HTTP2 config,
//! runs h2spec via subprocess, parses output, asserts ≥95% pass rate +
//! every failing test enumerated in known-failures.txt.
//!
//! When `which h2spec` fails locally the test eprintln!-skips per phase 05.2
//! SPEC §3 D7. CI provisions the binary at Task 14.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::net::TcpStream;
use tokio::process::Command;

const PASS_RATE_GATE: f64 = 0.95;

#[tokio::test(flavor = "multi_thread")]
async fn h2spec_pass_rate_gate() {
    let outcome = run_h2spec_gate().await;
    if let Err(e) = outcome {
        // h2spec binary unavailable → eprintln!-skip per SPEC §3 D7. Don't
        // fail the test locally; CI provisions the binary at Task 14.
        if e.to_string().contains("h2spec not found") {
            eprintln!("h2spec_runner: {} — skipping locally", e);
            return;
        }
        panic!("h2spec gate failed: {e:#}");
    }
}

async fn run_h2spec_gate() -> Result<()> {
    // Locate the h2spec binary.
    let h2spec = locate_h2spec().context("h2spec not found")?;

    // Locate envoy-bin. Sibling-crate tests cannot use `env!("CARGO_BIN_EXE_*")`
    // — that env var is only defined for tests inside the crate that owns the
    // binary target. Walk `target/<profile>/envoy-bin` instead, mirroring the
    // pattern used by `tests/differential/src/subject.rs::locate_envoy_bin`.
    let envoy_bin = locate_envoy_bin()?;

    // Reserve a port + render the YAML.
    let port = reserve_port()?;
    let yaml_template =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("h2spec.yaml"))?;
    let yaml = yaml_template.replace("{{PORT}}", &port.to_string());

    let dir = tempfile::tempdir()?;
    let cfg = dir.path().join("envoy-rust.yaml");
    std::fs::write(&cfg, yaml)?;

    // Spawn envoy-bin.
    let child = Command::new(&envoy_bin)
        .arg("-c")
        .arg(&cfg)
        .env("ENVOY_RUST_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("spawn envoy-bin at {}", envoy_bin.display()))?;

    // Wait for accept-readiness.
    let listener_addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if TcpStream::connect(listener_addr).await.is_ok() {
            break;
        }
        if std::time::Instant::now() >= deadline {
            bail!("listener never became ready at {listener_addr}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Run h2spec.
    let output = Command::new(&h2spec)
        .args(["-h", "127.0.0.1", "-p", &port.to_string()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("run h2spec")?;

    drop(child); // SIGKILL envoy-bin via kill_on_drop.

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    eprintln!("=== h2spec stdout ===\n{stdout}\n=== h2spec stderr ===\n{stderr}");

    // Parse h2spec's summary line. h2spec emits a final summary of the form
    //   `<N> tests, <M> passed, <K> skipped, <L> failed`
    // (verified against CI run 25294002788). See `parse_h2spec_output` below
    // for the section-context derivation that recovers full failure IDs.
    let (passed, failed, failures) = parse_h2spec_output(&stdout)?;
    let total = passed + failed;
    let pass_rate = if total == 0 {
        0.0
    } else {
        passed as f64 / total as f64
    };

    eprintln!("h2spec: passed={passed} failed={failed} total={total} pass_rate={pass_rate:.4}");

    // Read known-failures.txt.
    let kf_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("known-failures.txt");
    let kf_text = std::fs::read_to_string(&kf_path)?;
    let known_failures: std::collections::BTreeSet<String> = kf_text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .map(|l| l.split('#').next().unwrap().trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    // Gate (a): overall pass rate.
    anyhow::ensure!(
        pass_rate >= PASS_RATE_GATE,
        "h2spec pass rate {pass_rate:.4} below gate {PASS_RATE_GATE}",
    );

    // Gate (b): every failing test must be in known-failures.txt.
    let unexpected: Vec<&String> = failures
        .iter()
        .filter(|t| !known_failures.contains(*t))
        .collect();
    anyhow::ensure!(
        unexpected.is_empty(),
        "h2spec regressed on unlisted tests: {unexpected:?}",
    );

    // Gate (c): every test in known-failures.txt must actually fail.
    let stale: Vec<&String> = known_failures
        .iter()
        .filter(|t| !failures.contains(*t))
        .collect();
    anyhow::ensure!(
        stale.is_empty(),
        "known-failures.txt has stale entries (now passing): {stale:?} — trim the file",
    );

    Ok(())
}

fn locate_h2spec() -> Result<PathBuf> {
    // (1) Try `which h2spec` on PATH.
    if let Ok(out) = std::process::Command::new("which").arg("h2spec").output()
        && out.status.success()
    {
        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !path.is_empty() {
            return Ok(PathBuf::from(path));
        }
    }
    // (2) Try project-internal `tools/h2spec`. From this crate's manifest dir
    // (`tests/conformance/h2spec`) the workspace root is three parents up, so
    // `../../../tools/h2spec` resolves to `<workspace_root>/tools/h2spec`.
    let project_tool = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("tools")
        .join("h2spec");
    if project_tool.exists() {
        return Ok(project_tool);
    }
    bail!("h2spec not found on PATH or in tools/")
}

/// Locate the envoy-bin binary built by `cargo test --workspace`. Sibling-crate
/// tests cannot declare envoy-bin as a dependency on stable Rust (artifact
/// dependencies require nightly), so we compute the path by convention:
/// `<workspace_root>/target/<profile>/envoy-bin`, honoring `CARGO_TARGET_DIR`.
/// Mirrors `tests/differential/src/subject.rs::locate_envoy_bin`.
fn locate_envoy_bin() -> Result<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    // tests/conformance/h2spec → repo root is three parents up.
    let workspace_root = manifest
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .context("walking up from CARGO_MANIFEST_DIR to workspace root")?;
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root.join("target"));
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let mut bin = target_dir.join(profile).join("envoy-bin");
    if cfg!(windows) {
        bin.set_extension("exe");
    }
    if !bin.exists() {
        bail!(
            "envoy-bin not found at {}; run `cargo build -p envoy-bin` or `cargo test --workspace`",
            bin.display(),
        );
    }
    Ok(bin)
}

fn reserve_port() -> Result<u16> {
    let l = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    let p = l.local_addr()?.port();
    drop(l);
    Ok(p)
}

/// Parse h2spec's terminal output. Returns (passed, failed, failures) where
/// `failures` is a sorted set of failing test IDs.
///
/// h2spec's actual output (verified against the CI run `25294002788` log) is
/// hierarchical: section headings of the form `<digits>(.<digits>)*\. <title>`
/// announce a context, and per-test lines `× <N>: <description>` (failures)
/// or `✔ <N>: <description>` (passes — note U+2714 HEAVY CHECK MARK, NOT the
/// U+2713 CHECK MARK the original parser scraped) appear underneath. The full
/// test ID is `<section>/<N>`, e.g., section `3.5` + test `2` ⇒ `3.5/2`.
///
/// Counts come from the canonical summary line, emitted (twice) as:
///   `<N> tests, <M> passed, <K> skipped, <L> failed`
/// Per-line marker scraping is unreliable for counts (sub-tests nested under
/// a parent test, header ornamentation), so we read the summary instead and
/// only use marker scraping to recover failing test IDs.
fn parse_h2spec_output(stdout: &str) -> Result<(usize, usize, std::collections::BTreeSet<String>)> {
    let mut current_section: Option<String> = None;
    let mut failures = std::collections::BTreeSet::new();

    for line in stdout.lines() {
        let trimmed = line.trim_start();

        // Section heading: "3.5. HTTP/2 Connection Preface"
        if let Some(section) = parse_section_heading(trimmed) {
            current_section = Some(section);
            continue;
        }

        // Failure line: "× 2: Sends invalid connection preface"
        if let Some(rest) = trimmed.strip_prefix("× ")
            && let Some(num_str) = rest.split(':').next()
            && !num_str.is_empty()
            && num_str.chars().all(|c| c.is_ascii_digit())
            && let Some(section) = &current_section
        {
            failures.insert(format!("{section}/{num_str}"));
        }
    }

    let (passed, failed) = parse_summary_line(stdout)
        .ok_or_else(|| anyhow::anyhow!("h2spec summary line not found in output"))?;

    Ok((passed, failed, failures))
}

/// Parse a section heading line of the form `<digits>(.<digits>)*\. <title>`.
/// Returns the dotted-numeric prefix (e.g., `"3.5"`) if matched.
fn parse_section_heading(line: &str) -> Option<String> {
    // Look for the first ". " (dot+space) separating prefix from title.
    let dot_space = line.find(". ")?;
    let prefix = &line[..dot_space];
    if prefix.is_empty() {
        return None;
    }
    if !prefix.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    if prefix.starts_with('.') || prefix.ends_with('.') {
        return None;
    }
    if prefix.contains("..") {
        return None;
    }
    Some(prefix.to_string())
}

/// Parse the summary line `<N> tests, <M> passed, <K> skipped, <L> failed`.
/// Returns `(passed, failed)`. Returns `None` if no such line is present.
fn parse_summary_line(stdout: &str) -> Option<(usize, usize)> {
    for line in stdout.lines() {
        let trimmed = line.trim();
        if !trimmed.contains(" tests,") || !trimmed.contains(" passed") {
            continue;
        }
        let mut passed = None;
        let mut failed = None;
        for chunk in trimmed.split(',') {
            // Each chunk is "<N> <label>".
            let (num, label) = chunk.trim().split_once(' ')?;
            let n: usize = num.parse().ok()?;
            match label.trim() {
                "passed" => passed = Some(n),
                "failed" => failed = Some(n),
                _ => {}
            }
        }
        if let (Some(p), Some(f)) = (passed, failed) {
            return Some((p, f));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_summary_line_extracts_pass_fail_counts() {
        let line = "146 tests, 144 passed, 1 skipped, 1 failed";
        assert_eq!(parse_summary_line(line), Some((144, 1)));
    }

    #[test]
    fn parse_h2spec_output_extracts_section_failure_ids() {
        // Excerpt synthesized from CI run 25294002788's h2spec output.
        let stdout = "\
Hypertext Transfer Protocol Version 2 (HTTP/2)
  3. Starting HTTP/2
    3.5. HTTP/2 Connection Preface
      using source address 127.0.0.1:55152
      × 2: Sends invalid connection preface
        -> The endpoint MUST terminate the TCP connection.
           Expected: GOAWAY Frame (Error Code: PROTOCOL_ERROR)
                     Connection closed
             Actual: Error: read tcp ...: read: connection reset by peer

Finished in 0.2086 seconds
146 tests, 144 passed, 1 skipped, 1 failed
";
        let (passed, failed, failures) = parse_h2spec_output(stdout).expect("parse");
        assert_eq!(passed, 144);
        assert_eq!(failed, 1);
        assert!(
            failures.contains("3.5/2"),
            "expected 3.5/2 in failures, got {failures:?}"
        );
        assert_eq!(failures.len(), 1);
    }
}
