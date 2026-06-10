# CI Flake Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One PR that eliminates the four recurring intermittent CI failure classes on `main` and the imminent Node-20 actions deprecation, so green pushes stop randomly failing.

**Architecture:** All four flake classes live in the differential test harness (`tests/differential/`) or `.github/workflows/ci.yml`. Fixes are surgical: dedup port reservation, reorder access-log polling vs. subject SIGKILL, add an admin-stats warm-up gate for file-based-xDS fixtures, pre-pull the Envoy image with retries, and bump `actions/checkout`. No production-crate (`crates/`) code changes.

**Tech Stack:** Rust (stable 1.95 per `rust-toolchain.toml`), tokio, GitHub Actions, Docker (testcontainers).

---

## Background: evidence per failure class

All recent failures are intermittent — every failing commit's successor run passed. Root causes, each tied to a specific CI run:

| # | Symptom (CI run) | Root cause (verified in code/logs) |
|---|---|---|
| 1 | `envoy-rust never became accept-ready … within 10s` (26861955222, 26797777933, 26761458559) | Run 26861955222's log shows the data listener AND admin listener both got port **40875**; envoy-rust exited with `Address already in use`. `reserve_port()` (`tests/differential/src/lib.rs:921`) binds `:0`, drops the listener, and can hand out the **same port twice** in one fixture. Its own doc comment predicts this: "If CI flakes materialize, this becomes its own split phase." They have. Secondary problem: after the child exits, the harness still waits the full 10s and reports a misleading timeout instead of the bind error. |
| 2 | `access log mismatch: line count mismatch: envoy=1 envoy-rust=0` (27059869720 — the most recent failure) | In the `Http1WithAccessLog` arm (`tests/differential/src/lib.rs:~3790`), `subject.shutdown()` sends **SIGKILL** (`subject.rs:34-44`) *before* the 5s non-empty-file poll. envoy-rust's access-log emit is a fire-and-forget async task; if SIGKILL wins the race the line is lost forever and the post-kill poll can never succeed. (A previous mitigation — polling for non-empty instead of exists — fixed a different race, run 26375100437, but left this one.) |
| 3 | `upstream: expected status 200 for GET /, got 503` in `xds_file_based_cds_fixture` (26862683687, 26862493718) | File-based CDS/EDS clusters warm asynchronously (STRICT_DNS resolve) after the listener is accept-ready. The harness drives the measured request immediately after `wait_accept_ready`, racing cluster warm-up. Data-plane retry probes are NOT an option: these fixtures assert exact `downstream_rq_total` / `upstream_rq_total` counter values, so the gate must use the admin endpoint only. |
| 4 | `failed to pull the image 'envoyproxy/envoy:v1.33.0' … Docker responded with status code 500 … Client.Timeout` (26858671660) | The image is pulled lazily inside the test by testcontainers, with no retry. Docker Hub timeouts/rate limits kill the run. |
| 5 | Annotation on every run: `actions/checkout@v4` runs on Node 20; **GitHub forces Node 24 starting 2026-06-16** (six days away) | Stale action major. Bump to `actions/checkout@v5` (Node-24 drop-in) in both jobs. |

**Out of scope:** Historical clippy failures (e.g. run 26716702937) were genuine lint errors in pushed code, fixed by follow-up commits — CI working as intended, nothing to change. Cross-*process* port TOCTOU (another host process stealing a reserved port) remains accepted-risk per SPEC §6 point 6; this plan fixes only the observed intra-process double-handout.

## File Structure

- Modify: `tests/differential/src/lib.rs` — `reserve_port` dedup, subject-aware accept-ready wait, access-log poll reorder, `wait_file_nonempty` + `clusters_warm_from_stats_text` helpers, warm-up gate call site; unit tests in the existing `mod tests`.
- Modify: `tests/differential/src/subject.rs` — add `try_exit_status()`; unit test in-module.
- Modify: `.github/workflows/ci.yml` — checkout bump, image pre-pull step, h2spec curl retry.

Work on a branch off `main`: `git switch -c ci-flake-fixes`.

---

### Task 1: `reserve_port` must never hand out the same port twice in one test process

**Files:**
- Modify: `tests/differential/src/lib.rs:914-927` (`reserve_port`)
- Test: same file, existing `mod tests` (near `reserve_port_returns_nonzero`, ~line 5471)

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn reserve_port_with_skips_port_already_handed_out() {
    // Simulate the kernel returning the same ephemeral port twice in a row
    // (CI run 26861955222: data + admin listener both got 40875).
    let mut calls = 0u32;
    let first = reserve_port_with(|| {
        calls += 1;
        Ok(61001)
    })
    .unwrap();
    assert_eq!(first, 61001);
    let second = reserve_port_with(|| {
        calls += 1;
        // Kernel hands back 61001 again; helper must reject it and retry.
        Ok(if calls <= 2 { 61001 } else { 61002 })
    })
    .unwrap();
    assert_eq!(second, 61002);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p differential --lib reserve_port_with_skips_port_already_handed_out`
Expected: COMPILE ERROR — `reserve_port_with` not found.

- [ ] **Step 3: Implement**

Replace the body of `reserve_port` (keep its doc comment, but update the TOCTOU paragraph to note the intra-process dedup now in place and cite run 26861955222):

```rust
static RESERVED_PORTS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<u16>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));

pub fn reserve_port() -> Result<u16> {
    reserve_port_with(|| {
        let listener = StdTcpListener::bind(("127.0.0.1", 0))
            .context("binding 127.0.0.1:0 to reserve a port")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        Ok(port)
    })
}

/// Core of `reserve_port` with an injectable ephemeral-port allocator so the
/// dedup logic is unit-testable. Ports are never returned to the set: a test
/// process reserves a few dozen ports at most.
fn reserve_port_with(mut bind_ephemeral: impl FnMut() -> Result<u16>) -> Result<u16> {
    for _ in 0..64 {
        let port = bind_ephemeral()?;
        if RESERVED_PORTS.lock().unwrap().insert(port) {
            return Ok(port);
        }
    }
    bail!("64 consecutive ephemeral-port reservations were duplicates of already-handed-out ports")
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p differential --lib reserve_port`
Expected: PASS (both the new test and `reserve_port_returns_nonzero`).

- [ ] **Step 5: Commit**

```bash
git add tests/differential/src/lib.rs
git commit -m "fix(differential): dedup reserve_port handouts — same port was issued for data+admin listeners (CI run 26861955222)"
```

### Task 2: Fail fast with the real error when envoy-rust exits before accept-ready

**Files:**
- Modify: `tests/differential/src/subject.rs` (add `try_exit_status`)
- Modify: `tests/differential/src/lib.rs:~2993-2999` (subject accept-ready wait in `run_fixture`)
- Test: `tests/differential/src/subject.rs` in-module

- [ ] **Step 1: Write the failing test** (in `subject.rs`; add a `#[cfg(test)] mod tests` if absent — the struct's private fields are reachable in-module)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn try_exit_status_reports_exited_child() {
        let child = Command::new("sh").args(["-c", "exit 0"]).spawn().unwrap();
        let mut s = Subject { child: Some(child), port: 0 };
        // Poll until the child is reaped (bounded).
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while s.try_exit_status().is_none() {
            assert!(std::time::Instant::now() < deadline, "child never exited");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn try_exit_status_none_for_running_child() {
        let child = Command::new("sleep").arg("5").spawn().unwrap();
        let mut s = Subject { child: Some(child), port: 0 };
        assert!(s.try_exit_status().is_none());
        s.shutdown(Duration::from_secs(5)).await.unwrap();
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p differential --lib try_exit_status`
Expected: COMPILE ERROR — no method `try_exit_status`.

- [ ] **Step 3: Implement** (in `impl Subject`)

```rust
/// Non-blocking liveness probe: `Some(status)` once the child has exited.
/// Used by the harness to abort the accept-ready wait immediately (with the
/// real exit reason) instead of timing out for 10s against a dead process.
pub fn try_exit_status(&mut self) -> Option<std::process::ExitStatus> {
    self.child.as_mut().and_then(|c| c.try_wait().ok().flatten())
}
```

Then in `lib.rs` `run_fixture`, replace:

```rust
wait_accept_ready(subject_addr, budget)
    .await
    .context("envoy-rust never became accept-ready")?;
```

with:

```rust
// Like wait_accept_ready, but bail immediately if the subject process has
// already exited (e.g. a listener bind failure) — the connect loop would
// otherwise burn the whole budget and mask the real error.
let subject_deadline = std::time::Instant::now() + budget;
loop {
    if let Some(status) = subject.try_exit_status() {
        bail!("envoy-rust exited before accept-ready: {status}");
    }
    match tokio::net::TcpStream::connect(subject_addr).await {
        Ok(_) => break,
        Err(_) if std::time::Instant::now() < subject_deadline => {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Err(err) => {
            bail!("envoy-rust {subject_addr} not accept-ready within {budget:?}: {err}")
        }
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p differential --lib`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add tests/differential/src/subject.rs tests/differential/src/lib.rs
git commit -m "fix(differential): fail fast with real exit error when envoy-rust dies before accept-ready"
```

### Task 3: Poll the envoy-rust access-log file BEFORE SIGKILLing the subject

**Files:**
- Modify: `tests/differential/src/lib.rs` `Http1WithAccessLog` arm (~3790-3892) + new helper
- Test: `lib.rs` `mod tests`

- [ ] **Step 1: Write the failing tests for the extracted helper**

```rust
#[tokio::test]
async fn wait_file_nonempty_true_for_existing_content() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("log");
    std::fs::write(&p, "line\n").unwrap();
    assert!(wait_file_nonempty(&p, Duration::from_millis(200)).await);
}

#[tokio::test]
async fn wait_file_nonempty_false_when_budget_expires() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("log");
    std::fs::write(&p, "").unwrap();
    assert!(!wait_file_nonempty(&p, Duration::from_millis(200)).await);
}

#[tokio::test]
async fn wait_file_nonempty_true_when_content_arrives_mid_poll() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("log");
    std::fs::write(&p, "").unwrap();
    let p2 = p.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        std::fs::write(&p2, "line\n").unwrap();
    });
    assert!(wait_file_nonempty(&p, Duration::from_secs(2)).await);
}
```

(If `tempfile` is not already a dev-dependency of the differential crate, use `std::env::temp_dir()` + a unique filename instead — do not add a new dependency.)

- [ ] **Step 2: Run to verify failure** — `cargo test -p differential --lib wait_file_nonempty` → COMPILE ERROR.

- [ ] **Step 3: Implement helper + reorder the arm**

```rust
/// Poll `path` until it exists with len > 0, or `budget` expires.
/// Returns whether the file became non-empty. Non-fatal by design: callers
/// fall through to the byte-level assertion, which reports the real diff.
async fn wait_file_nonempty(path: &std::path::Path, budget: Duration) -> bool {
    let deadline = std::time::Instant::now() + budget;
    loop {
        if path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
```

In the `Http1WithAccessLog` arm, change the sequencing. Currently:

```rust
subject.shutdown(Duration::from_secs(5)).await.ok();
drop(upstream);
/* status/body/header asserts */
/* 5s both-files non-empty poll + 100ms settle */
```

New order — wait for envoy-rust's line while the process is still alive, only then SIGKILL; keep the upstream-container drop before its file wait (docker stop → SIGTERM → Envoy flushes its buffered sink on shutdown):

```rust
// envoy-rust's access-log emit is a fire-and-forget task that runs after the
// response completes; subject.shutdown() is SIGKILL (subject.rs TODO on
// graceful drain). Wait for the line to land BEFORE killing the process —
// CI run 27059869720 lost the race (`envoy=1 envoy-rust=0`) because the old
// post-shutdown poll could never observe a write from a dead process.
let envoy_rust_path = std::path::PathBuf::from(&expected_access_log_paths.envoy_rust);
wait_file_nonempty(&envoy_rust_path, std::time::Duration::from_secs(5)).await;
subject.shutdown(Duration::from_secs(5)).await.ok();
drop(upstream);

/* status/body/header asserts — unchanged, keep in place */

// Envoy-side flush is driven by container stop (SIGTERM) above.
let envoy_path = std::path::PathBuf::from(&expected_access_log_paths.envoy);
wait_file_nonempty(&envoy_path, std::time::Duration::from_secs(5)).await;
// One final yield to let the OS flush any in-flight bytes that crossed the
// metadata-len threshold but haven't fully landed.
tokio::time::sleep(std::time::Duration::from_millis(100)).await;
```

Delete the old combined poll block (the `deadline` / `both_nonempty` loop). Preserve the historical comment about run 26375100437 by folding it into the new comment block.

- [ ] **Step 4: Run tests**

Run: `cargo test -p differential --lib wait_file_nonempty` → PASS.
Run (requires Docker): `cargo test -p differential --test access_log_file_sink` → PASS.

- [ ] **Step 5: Commit**

```bash
git add tests/differential/src/lib.rs
git commit -m "fix(differential): wait for envoy-rust access-log emit before SIGKILL (CI run 27059869720)"
```

### Task 4: Admin-stats warm-up gate for file-based-xDS fixtures

**Files:**
- Modify: `tests/differential/src/lib.rs` — parser + gate helper + call site after the accept-ready waits (~line 3000)
- Test: `lib.rs` `mod tests`

Constraint: these fixtures assert exact data-plane counters (`downstream_rq_total`, `upstream_rq_total`), so the gate must only touch the **admin** endpoint, never the data plane.

- [ ] **Step 1: Write the failing parser tests**

```rust
#[test]
fn clusters_warm_requires_at_least_one_cluster() {
    assert!(!clusters_warm_from_stats_text("server.live: 1\n"));
}

#[test]
fn clusters_warm_false_when_any_membership_unhealthy() {
    let s = "cluster.a.membership_healthy: 1\ncluster.b.membership_healthy: 0\n";
    assert!(!clusters_warm_from_stats_text(s));
}

#[test]
fn clusters_warm_true_when_all_memberships_healthy() {
    let s = "cluster.a.membership_healthy: 1\ncluster.b.membership_healthy: 2\nserver.live: 1\n";
    assert!(clusters_warm_from_stats_text(s));
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p differential --lib clusters_warm` → COMPILE ERROR.

- [ ] **Step 3: Implement**

```rust
/// Parse Envoy admin `/stats` plain text: warm iff at least one
/// `cluster.<name>.membership_healthy` gauge exists and ALL such gauges
/// are >= 1.
fn clusters_warm_from_stats_text(stats: &str) -> bool {
    let mut saw_any = false;
    for line in stats.lines() {
        let Some((name, value)) = line.split_once(':') else { continue };
        if name.starts_with("cluster.") && name.trim_end().ends_with(".membership_healthy") {
            saw_any = true;
            if value.trim().parse::<u64>().map(|v| v >= 1) != Ok(true) {
                return false;
            }
        }
    }
    saw_any
}

/// File-based xDS warm-up gate (CI runs 26862683687 / 26862493718: upstream
/// answered 503 because the CDS-supplied STRICT_DNS cluster had not resolved
/// when the measured request fired). Polls admin `/stats` until clusters
/// report healthy membership. Budget expiry is deliberately NON-FATAL: the
/// measured drive then fails with exactly the diff it would have produced
/// without the gate, so the gate cannot mask a real differential bug.
async fn wait_clusters_warm(admin_addr: SocketAddr, budget: Duration) {
    let deadline = std::time::Instant::now() + budget;
    while std::time::Instant::now() < deadline {
        // drive_http_get signature is (addr: SocketAddr, path: &str, host: &str)
        // — see lib.rs:1313. Any host literal works for the admin listener.
        if let Ok(resp) = drive_http_get(admin_addr, "/stats", "localhost").await
            && clusters_warm_from_stats_text(&String::from_utf8_lossy(&resp.body))
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}
```

Call site in `run_fixture`, immediately after the two accept-ready waits. **Upstream side only**: envoy-rust gates `membership_healthy` emission on `health_checks` being configured (absent in these fixtures — fixture 0029's expectations comment documents this), so a subject-side wait would always burn its full budget; both observed 503 failures were upstream-side anyway.

```rust
// Gate only fixtures whose clusters arrive via file-based xDS; static-cluster
// fixtures warm synchronously and some intentionally drive 503s. Upstream
// (real Envoy) side only — envoy-rust does not emit membership_healthy
// without active health checks.
if (upstream_cds_path.is_some() || upstream_eds_path.is_some()) && needs_admin_port {
    if let Some(p) = upstream.host_admin_port() {
        wait_clusters_warm(format!("127.0.0.1:{p}").parse()?, Duration::from_secs(10)).await;
    }
}
```

Implementation notes for the executor:
- `upstream.host_admin_port() -> Option<u16>` and `needs_admin_port` already exist in `run_fixture` scope (see `lib.rs:2236`, `lib.rs:2241`, `lib.rs:3313`). Both flaky fixtures (0026 CDS, 0029 EDS) have `{{ADMIN_PORT}}` in both templates, so the gate fires for them.
- `grep -rn "http.admin\|listener.admin" tests/fixtures/` DOES hit fixture 0011-admin-stats-prometheus's Prometheus name allow-list — harmless, since 0011 uses no file-based CDS/EDS and the gate never runs there. No extra scoping needed.

- [ ] **Step 4: Run tests**

Run: `cargo test -p differential --lib clusters_warm` → PASS.
Run (requires Docker): `cargo test -p differential --test xds_file_based_cds --test xds_file_based_eds` → PASS.

- [ ] **Step 5: Commit**

```bash
git add tests/differential/src/lib.rs
git commit -m "fix(differential): admin-gated cluster warm-up wait for file-based xDS fixtures (503 flake)"
```

### Task 5: Pre-pull the upstream Envoy image in CI with retries

**Files:**
- Modify: `.github/workflows/ci.yml` (insert step between `install h2spec` and the test step)

- [ ] **Step 1: Add the step**

```yaml
      - name: pre-pull upstream Envoy image (Docker Hub flake guard)
        run: |
          set -euo pipefail
          # Tag is greped from the harness source so CI can never drift from
          # tests/differential/src/upstream.rs (IMAGE_TAG).
          TAG="$(grep -oP 'IMAGE_TAG: &str = "\K[^"]+' tests/differential/src/upstream.rs)"
          IMG="envoyproxy/envoy:${TAG}"
          for i in 1 2 3 4 5; do
            if docker pull "$IMG"; then exit 0; fi
            echo "docker pull $IMG failed (attempt $i); sleeping $((i*10))s"
            sleep $((i*10))
          done
          echo "giving up after 5 attempts"
          exit 1
```

- [ ] **Step 2: Verify locally**

Run: `TAG="$(grep -oP 'IMAGE_TAG: &str = "\K[^"]+' tests/differential/src/upstream.rs)"; echo "$TAG"`
Expected: `v1.33.0`

Run: `command -v actionlint >/dev/null && actionlint .github/workflows/ci.yml || echo "actionlint not installed — rely on CI"`

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: pre-pull envoyproxy/envoy image with retries (Docker Hub timeout flake, run 26858671660)"
```

### Task 6: Bump `actions/checkout` v4 → v5 and harden the h2spec download

**Files:**
- Modify: `.github/workflows/ci.yml:22` and `:67` (both jobs)
- Modify: `.github/workflows/ci.yml:47` (h2spec curl)

- [ ] **Step 1: Edit**

Replace both `uses: actions/checkout@v4` with `uses: actions/checkout@v5` (first Node-24 major; GitHub forces Node 24 on 2026-06-16 and removes Node 20 from runners on 2026-09-16 — see the deprecation annotation on every current run).

In the h2spec step, change the curl invocation to retry transient failures:

```yaml
          curl -fsSL --retry 5 --retry-all-errors "https://github.com/summerwind/h2spec/releases/download/v${H2SPEC_VERSION}/h2spec_linux_amd64.tar.gz" \
            | sudo tar xz -C /usr/local/bin
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: checkout@v5 (Node 24 forced 2026-06-16) + curl retries for h2spec download"
```

### Task 7: Full verification + PR

- [ ] **Step 1: Full local gate** (mirrors ci.yml; Docker required for the differential tests)

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
```

Expected: all green. If any differential test fails, debug before pushing — do not weaken assertions.

- [ ] **Step 2: Flake-targeted soak** — rerun the four previously-flaky tests 5× each:

```bash
for i in 1 2 3 4 5; do
  cargo test -p differential \
    --test access_log_file_sink \
    --test xds_file_based_cds \
    --test upstream_circuit_breaker_budgets \
    --test admin_stats_prometheus || exit 1
done
```

Expected: 5/5 green.

- [ ] **Step 3: Push branch and open PR**

```bash
git push -u origin ci-flake-fixes
gh pr create --title "Fix CI flake classes: port-reservation collision, access-log SIGKILL race, xDS warm-up race, Docker pull retries, checkout@v5" --body "$(cat <<'EOF'
## Summary
Eliminates every recurring intermittent CI failure class observed on main, each tied to a specific failing run:

- **reserve_port duplicate handout** (run 26861955222: data + admin listener both got 40875 → `Address already in use` → misleading 10s accept-ready timeout). Now deduped via a process-global reserved set; the harness also fails fast with the child's real exit status.
- **Access-log SIGKILL race** (run 27059869720: `envoy=1 envoy-rust=0`). The harness now waits for envoy-rust's fire-and-forget emit to land before SIGKILLing the subject.
- **File-based xDS warm-up race** (runs 26862683687/26862493718: upstream 503 before its CDS STRICT_DNS cluster resolved). Added an admin-/stats-gated `membership_healthy` wait; non-fatal on budget expiry so it cannot mask real differential bugs; admin-only so exact data-plane counter assertions are untouched.
- **Docker Hub pull timeout** (run 26858671660). CI pre-pulls `envoyproxy/envoy` (tag greped from `upstream.rs` to prevent drift) with 5 retries.
- **Node-20 actions deprecation** (forced Node 24 on 2026-06-16): `actions/checkout` v4 → v5; h2spec download gets curl retries.

## Test plan
- [ ] `cargo fmt` / `clippy -D warnings` / `build` / `cargo test --workspace` green locally (Docker available)
- [ ] 5× soak of the four previously-flaky differential tests green
- [ ] CI green on this PR

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 4: Watch the PR's CI run to completion**

```bash
gh run watch --exit-status "$(gh run list --branch ci-flake-fixes --limit 1 --json databaseId --jq '.[0].databaseId')"
```

Expected: both jobs green, and the run annotations no longer include the checkout Node-20 deprecation warning.
