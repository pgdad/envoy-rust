# Phase 68 — Active TCP Health Checking (`tcp_health_check`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. TDD (superpowers:test-driven-development) is mandatory on every task: write the failing test, run it red, implement minimally, run it green, commit.

**Goal:** Land active **TCP** health checking (`envoy.config.core.v3.HealthCheck.tcp_health_check`) as the upstream-robustness family's second health-check checker type, behaviorally equivalent to `envoyproxy/envoy:v1.33.0` under the differential contract — reusing the phase-12 scheduler, `EndpointHealth` state machine, ejection, `pick()` exclusion, and `cluster.<n>.health_check.*`/`membership_*` stat tree unchanged.

**Architecture:** The config layer (`crates/envoy-config`) adds a `TcpHealthCheck { send: Option<HealthCheckPayload>, receive: Vec<HealthCheckPayload> }` sub-message + a `HealthCheckPayload { text (hex) | binary (base64) }` decoded fail-loud at validate time. The health layer (`crates/envoy-health`) adds an L4 probe (`tcp_probe_once`/`tcp_probe_loop`) that connects, optionally writes `send`, then scans inbound bytes for `receive` — the whole probe bounded by ONE `timeout(hc.timeout, ...)` (measured: the HC `timeout`, not the cluster `connect_timeout`, bounds connect). The `Scheduler` dispatches HTTP vs TCP by which checker is present; counters, ejection, and `pick()` are untouched. A new differential fixture `0074` witnesses connection-only ejection → synth-503 byte-exact via the reused `http1_after_settle` driver.

**Tech Stack:** Rust (edition 2024), `tokio` (add `net` feature to `envoy-health`), `serde`/`serde_yaml` (config), `base64 = "0.22"` (new direct dep of `envoy-config`; hex is hand-rolled), the existing `testcontainers` differential harness.

## Global Constraints

- **Reference pin (D-3.7):** all measured behavior is against `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`). Assert ONLY what is measured (D-3.3).
- **`#![forbid(unsafe_code)]`** stays at the head of every crate root (`lib.rs`) — do not remove it (D-3.8).
- **Fail-loud config posture (ADR-0049):** every invalid `tcp_health_check` config surfaces a typed `ConfigError` at parse/validate time (STDOUT via envoy-bin), never a silent default.
- **Config-load error messages are NATIVE** (ADR-0137 PV-1) — byte-parity with Envoy's `invalid hex string '…'` is explicitly WAIVED (config-load errors are not a differential wire surface).
- **The HC `timeout` bounds the whole probe** including connect (ADR-0137 PV-6) — one `tokio::time::timeout(hc.timeout, ...)`, mirroring the HTTP `probe_once`. The cluster `connect_timeout` is NOT consulted by the checker.
- **The `receive` scan is a contiguous-substring search**; only SINGLE-block is differentially/parity-pinned (ADR-0137 PV-3). Multi-block is implemented (sequential in-order search) but NOT asserted for Envoy parity.
- **Reuse, do not re-implement:** the `envoy-health` `Scheduler`, `EndpointHealth`, ejection sweeper, `pick()` exclusion, the `cluster.<n>.health_check.{attempt,success,failure}` counters, `membership_healthy`/`membership_total`, and the `http1_after_settle` differential driver + `set_equal_modulo_allow_list` headers are all reused unchanged. The TCP checker witnesses the IDENTICAL stat names (no new stat names).
- **Do NOT revert landed 67/12 work; do NOT touch the CidrRange surface (M-1); never weaken a fixture; never trim `known-failures.txt`.** Any ROADMAP row edit preserves all 6 cells + escapes literal `|` as `\|`; rows `36`/`38`/`39`/`52`/`54` are already malformed — do NOT "fix" them.
- **`cargo build -p envoy-bin` before ANY local differential** (the harness runs `target/debug/envoy-bin`). CI (FULL 40-char SHA) is authoritative for the documented host-flake set.

## File Structure

- `crates/envoy-config/Cargo.toml` — add `base64 = "0.22"` to `[dependencies]`.
- `crates/envoy-config/src/bootstrap.rs` — add `TcpHealthCheck` + `HealthCheckPayload` structs + `HealthCheckPayload::decode()` + `HealthCheck.tcp_health_check` field; restructure `validate_health_checks` (line 4681); update the pinning test at line 14942.
- `crates/envoy-config/src/lib.rs` — new `ConfigError` variants (`BothHttpAndTcpHealthCheck`, `InvalidHealthCheckPayloadHex`, `InvalidHealthCheckPayloadBase64`, `EmptyHealthCheckPayload`); update the `UnsupportedHealthCheckType` message.
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/` — new un-ignored seed carrying a `tcp_health_check`.
- `crates/envoy-health/Cargo.toml` — add `net` to `tokio` features.
- `crates/envoy-health/src/probe.rs` — new `tcp_probe_once`, `tcp_probe_loop`, `receive_matches` (pure matcher), `TcpProbeError`.
- `crates/envoy-health/src/scheduler.rs` — checker-type dispatch in `Scheduler::spawn`.
- `tests/differential/src/lib.rs` — new `DEAD_BACKEND_PORT` marker (reserve_port, no listener) + BACKEND_HOST gate extension.
- `tests/fixtures/0074-upstream-tcp-health-check/` — `envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml`, `README.md`.
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — a `tcp_health_check` subsection.

---

### Task 1: `HealthCheckPayload` schema + hex/base64 decode + `ConfigError` variants

**Files:**
- Modify: `crates/envoy-config/Cargo.toml` (add `base64 = "0.22"`)
- Modify: `crates/envoy-config/src/bootstrap.rs` (add `HealthCheckPayload` + `decode()`, near `HttpHealthCheck` ~line 2450)
- Modify: `crates/envoy-config/src/lib.rs` (add 3 `ConfigError` variants)
- Test: inline `#[cfg(test)] mod tests` in `bootstrap.rs`

**Interfaces:**
- Produces: `pub struct HealthCheckPayload { pub text: Option<String>, pub binary: Option<String> }`; `impl HealthCheckPayload { pub fn decode(&self) -> Result<Vec<u8>, PayloadDecodeError>; }`; `pub enum PayloadDecodeError { InvalidHex(String), InvalidBase64(String), Empty }`. Consumed by Task 3 (validator) and Task 4 (probe).

- [ ] **Step 1: Add the `base64` dependency**

In `crates/envoy-config/Cargo.toml`, under `[dependencies]` (alphabetically near the top), add:

```toml
base64 = "0.22"
```

- [ ] **Step 2: Write the failing decode tests**

Add to the `#[cfg(test)] mod tests` block in `crates/envoy-config/src/bootstrap.rs`:

```rust
#[test]
fn payload_decode_hex_text_ok() {
    let p = HealthCheckPayload { text: Some("50494e47".to_string()), binary: None };
    assert_eq!(p.decode().unwrap(), b"PING");
}

#[test]
fn payload_decode_odd_length_hex_is_err() {
    let p = HealthCheckPayload { text: Some("0".to_string()), binary: None };
    assert!(matches!(p.decode(), Err(PayloadDecodeError::InvalidHex(ref s)) if s == "0"));
}

#[test]
fn payload_decode_non_hex_is_err() {
    let p = HealthCheckPayload { text: Some("zzzz".to_string()), binary: None };
    assert!(matches!(p.decode(), Err(PayloadDecodeError::InvalidHex(ref s)) if s == "zzzz"));
}

#[test]
fn payload_decode_base64_binary_ok() {
    let p = HealthCheckPayload { text: None, binary: Some("AAECAw==".to_string()) };
    assert_eq!(p.decode().unwrap(), vec![0u8, 1, 2, 3]);
}

#[test]
fn payload_decode_bad_base64_is_err() {
    let p = HealthCheckPayload { text: None, binary: Some("!!!!".to_string()) };
    assert!(matches!(p.decode(), Err(PayloadDecodeError::InvalidBase64(_))));
}

#[test]
fn payload_decode_empty_is_err() {
    let p = HealthCheckPayload { text: None, binary: None };
    assert!(matches!(p.decode(), Err(PayloadDecodeError::Empty)));
    let both = HealthCheckPayload { text: Some("00".to_string()), binary: Some("AA==".to_string()) };
    assert!(matches!(both.decode(), Err(PayloadDecodeError::Empty)));
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p envoy-config payload_decode 2>&1 | tail -20`
Expected: FAIL to COMPILE — `HealthCheckPayload` / `PayloadDecodeError` not defined.

- [ ] **Step 4: Implement `HealthCheckPayload`, `PayloadDecodeError`, and `decode()`**

Add near `HttpHealthCheck` in `bootstrap.rs`:

```rust
/// 68 (ADR-0136/0137): a `tcp_health_check` `send`/`receive` payload — an
/// `envoy.config.core.v3.HealthCheck.Payload` oneof `{ text: <hex> | binary:
/// <base64> }`. Modeled as two serde Options (the bootstrap oneof-as-two-Options
/// precedent); `decode()` yields the raw bytes fail-loud (ADR-0137 PV-1).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HealthCheckPayload {
    /// Hex-encoded bytes (upstream `Payload.text`). Odd-length / non-hex → fatal.
    #[serde(default)]
    pub text: Option<String>,
    /// Base64-encoded bytes (upstream `Payload.binary`).
    #[serde(default)]
    pub binary: Option<String>,
}

/// 68: `HealthCheckPayload::decode` failure — mapped to a `ConfigError` by the
/// validator (Task 3) and `.expect()`ed by the probe (Task 4, defense-in-depth,
/// the `parse_duration` precedent).
#[derive(Debug, PartialEq)]
pub enum PayloadDecodeError {
    /// `text` was odd-length or contained a non-hex digit. Carries the offending string.
    InvalidHex(String),
    /// `binary` was not valid base64. Carries the offending string.
    InvalidBase64(String),
    /// Neither `text` nor `binary` set, OR both set (the `Payload` oneof requires exactly one).
    Empty,
}

impl HealthCheckPayload {
    /// 68 (ADR-0137 PV-1): decode to raw bytes. Native fail-loud (byte-parity waived).
    pub fn decode(&self) -> Result<Vec<u8>, PayloadDecodeError> {
        match (&self.text, &self.binary) {
            (Some(hex), None) => decode_hex(hex).ok_or_else(|| PayloadDecodeError::InvalidHex(hex.clone())),
            (None, Some(b64)) => {
                use base64::Engine as _;
                base64::engine::general_purpose::STANDARD
                    .decode(b64)
                    .map_err(|_| PayloadDecodeError::InvalidBase64(b64.clone()))
            }
            _ => Err(PayloadDecodeError::Empty),
        }
    }
}

/// 68: hand-rolled hex decode (the `from_str_radix` precedent at
/// `crates/envoy-http1/src/client.rs:631`). Returns `None` on odd length or a
/// non-hex digit. Kept private; `HealthCheckPayload::decode` is the entry point.
fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(s.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = (bytes[i] as char).to_digit(16)?;
        let lo = (bytes[i + 1] as char).to_digit(16)?;
        out.push((hi * 16 + lo) as u8);
        i += 2;
    }
    Some(out)
}
```

Add to `ConfigError` in `lib.rs` (near the other health-check variants, ~line 736):

```rust
    /// 68 (ADR-0137 PV-1): a `tcp_health_check` `send`/`receive` payload `text`
    /// was odd-length or non-hex. Native fail-loud (byte-parity with Envoy's
    /// `invalid hex string` waived — config-load errors are not a wire surface).
    #[error("cluster '{cluster}' tcp_health_check payload text '{value}' is not a valid hex string")]
    InvalidHealthCheckPayloadHex { cluster: String, value: String },

    /// 68 (ADR-0137 PV-1): a `tcp_health_check` payload `binary` was not valid base64.
    #[error("cluster '{cluster}' tcp_health_check payload binary '{value}' is not valid base64")]
    InvalidHealthCheckPayloadBase64 { cluster: String, value: String },

    /// 68 (ADR-0137 PV-1): a `tcp_health_check` payload set neither `text` nor
    /// `binary` (or both). The `Payload` oneof requires exactly one.
    #[error("cluster '{cluster}' tcp_health_check payload must set exactly one of text or binary")]
    EmptyHealthCheckPayload { cluster: String },
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p envoy-config payload_decode 2>&1 | tail -20`
Expected: PASS (6 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-config/Cargo.toml crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs Cargo.lock
git commit -m "phase 68: HealthCheckPayload schema + hex/base64 decode + ConfigError variants"
```

---

### Task 2: `TcpHealthCheck` struct + `HealthCheck.tcp_health_check` field; update the pinning test

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (add `TcpHealthCheck`; add the `HealthCheck.tcp_health_check` field at line 2447; update pinning test at line 14942)
- Test: inline `#[cfg(test)] mod tests` in `bootstrap.rs`

**Interfaces:**
- Consumes: `HealthCheckPayload` (Task 1).
- Produces: `pub struct TcpHealthCheck { pub send: Option<HealthCheckPayload>, pub receive: Vec<HealthCheckPayload> }`; `HealthCheck.tcp_health_check: Option<TcpHealthCheck>`. Consumed by Tasks 3, 4, 5.

- [ ] **Step 1: Write the failing parse tests**

Add to `bootstrap.rs` tests:

```rust
#[test]
fn parses_empty_tcp_health_check_connection_only() {
    let yaml = hc_yaml(
        "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          tcp_health_check: {}",
        "",
    );
    let bs = crate::parse_bootstrap(&yaml).expect("empty tcp_health_check parses");
    let hc = &bs.static_resources.clusters[0].health_checks[0];
    let tcp = hc.tcp_health_check.as_ref().expect("tcp checker present");
    assert!(tcp.send.is_none());
    assert!(tcp.receive.is_empty());
    assert!(hc.http_health_check.is_none());
}

#[test]
fn parses_tcp_health_check_send_receive() {
    let yaml = hc_yaml(
        "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          tcp_health_check:\n            send: { text: \"000102\" }\n            receive:\n              - { text: \"0304\" }",
        "",
    );
    let bs = crate::parse_bootstrap(&yaml).expect("send/receive tcp_health_check parses");
    let tcp = bs.static_resources.clusters[0].health_checks[0].tcp_health_check.as_ref().unwrap();
    assert_eq!(tcp.send.as_ref().unwrap().text.as_deref(), Some("000102"));
    assert_eq!(tcp.receive.len(), 1);
    assert_eq!(tcp.receive[0].text.as_deref(), Some("0304"));
}

#[test]
fn tcp_health_check_rejects_unknown_field() {
    let yaml = hc_yaml(
        "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          tcp_health_check: { bogus: 1 }",
        "",
    );
    assert!(crate::parse_bootstrap(&yaml).is_err(), "deny_unknown_fields rejects unknown tcp_health_check key");
}
```

(`hc_yaml` is the existing helper at `bootstrap.rs:14975`.)

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-config tcp_health_check 2>&1 | tail -20`
Expected: FAIL — `tcp_health_check` is an unknown field (`deny_unknown_fields`); `HealthCheck` has no `tcp_health_check` field.

- [ ] **Step 3: Implement `TcpHealthCheck` + wire the field**

Add near `HttpHealthCheck` in `bootstrap.rs`:

```rust
/// 68 (ADR-0136/0137): the active TCP health-check probe shape
/// (`envoy.config.core.v3.HealthCheck.TcpHealthCheck`). Empty ⇒ connection-only.
/// `send` (optional) is written once after connect; `receive` (repeated) is
/// scanned as a contiguous substring in the inbound bytes (ADR-0137 PV-3).
#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields, default)]
pub struct TcpHealthCheck {
    pub send: Option<HealthCheckPayload>,
    pub receive: Vec<HealthCheckPayload>,
}
```

In `struct HealthCheck` (after the `http_health_check` field at line 2447):

```rust
    /// 68 (ADR-0136): the TCP checker. Optional at the schema level, alongside
    /// `http_health_check`; the validator (Task 3) rejects BOTH present (the
    /// upstream oneof) and NEITHER present (`UnsupportedHealthCheckType`).
    #[serde(default)]
    pub tcp_health_check: Option<TcpHealthCheck>,
```

- [ ] **Step 4: Update the pinning test (TCP is now a known field)**

The pinning test `cluster_rejects_unknown_health_check_field` at `bootstrap.rs:14942` currently feeds `tcp_health_check: {}` and asserts a parse error — that assertion is now FALSE (TCP parses). Switch its unknown-field probe to `grpc_health_check` (still deferred, still `deny_unknown_fields`-rejected). Replace the `tcp_health_check: {}` line inside the test's YAML with:

```
          grpc_health_check: {}
```

and update the test's leading comment to `// deny_unknown_fields rejects gRPC/custom checkers + deferred upstream knobs (TCP is now supported — phase 68).`

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p envoy-config health_check 2>&1 | tail -30`
Expected: PASS — the 3 new tests + the updated pinning test + all existing `health_check` tests green.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 68: TcpHealthCheck struct + HealthCheck.tcp_health_check field; repoint pinning test to grpc_health_check"
```

---

### Task 3: Validator — both-checkers oneof rejection, TCP payload decode, updated message

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (`validate_health_checks`, line 4681)
- Modify: `crates/envoy-config/src/lib.rs` (add `BothHttpAndTcpHealthCheck`; update `UnsupportedHealthCheckType` message)
- Test: inline `#[cfg(test)] mod tests` in `bootstrap.rs`

**Interfaces:**
- Consumes: `TcpHealthCheck`, `HealthCheckPayload::decode` / `PayloadDecodeError` (Tasks 1–2).
- Produces: the restructured `validate_health_checks` accepts a TCP-only checker, rejects both-present and payload-decode failures.

- [ ] **Step 1: Write the failing validator tests**

Add to `bootstrap.rs` tests (reuse the existing `hc_yaml` helper):

```rust
#[test]
fn validate_accepts_tcp_only_checker() {
    let yaml = hc_yaml(
        "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          tcp_health_check: { receive: [ { text: \"50494e47\" } ] }",
        "",
    );
    assert!(crate::parse_bootstrap(&yaml).is_ok(), "tcp-only checker validates");
}

#[test]
fn validate_rejects_both_http_and_tcp() {
    let yaml = hc_yaml(
        "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          http_health_check: { path: /z }\n          tcp_health_check: {}",
        "",
    );
    let err = crate::parse_bootstrap(&yaml).expect_err("both checkers rejected");
    assert!(matches!(err, crate::ConfigError::BothHttpAndTcpHealthCheck { ref cluster } if cluster == "hc_backend"));
}

#[test]
fn validate_rejects_tcp_payload_bad_hex() {
    let yaml = hc_yaml(
        "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          tcp_health_check: { send: { text: \"zzzz\" } }",
        "",
    );
    let err = crate::parse_bootstrap(&yaml).expect_err("bad hex rejected");
    assert!(matches!(err, crate::ConfigError::InvalidHealthCheckPayloadHex { ref cluster, ref value } if cluster == "hc_backend" && value == "zzzz"));
}

#[test]
fn validate_rejects_tcp_payload_empty() {
    let yaml = hc_yaml(
        "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          tcp_health_check: { receive: [ {} ] }",
        "",
    );
    let err = crate::parse_bootstrap(&yaml).expect_err("empty payload rejected");
    assert!(matches!(err, crate::ConfigError::EmptyHealthCheckPayload { ref cluster } if cluster == "hc_backend"));
}

#[test]
fn validate_rejects_tcp_bad_threshold() {
    let yaml = hc_yaml(
        "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 0\n          unhealthy_threshold: 2\n          tcp_health_check: {}",
        "",
    );
    let err = crate::parse_bootstrap(&yaml).expect_err("threshold validated for tcp too");
    assert!(matches!(err, crate::ConfigError::InvalidHealthCheckThreshold { ref cluster, field } if cluster == "hc_backend" && field == "healthy_threshold"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-config validate_ 2>&1 | tail -30`
Expected: FAIL — `BothHttpAndTcpHealthCheck` not defined; a TCP-only checker currently errors `UnsupportedHealthCheckType`.

- [ ] **Step 3: Add the `BothHttpAndTcpHealthCheck` variant + update the `UnsupportedHealthCheckType` message**

In `lib.rs`, update the `UnsupportedHealthCheckType` doc + message (line 732) to:

```rust
    /// 12.1 / 68: cluster's health check sets NEITHER `http_health_check` nor
    /// `tcp_health_check` (gRPC/custom still deferred, fail-loud).
    #[error(
        "cluster '{cluster}' health check sets neither http_health_check nor tcp_health_check; only HTTP and TCP health checks are supported"
    )]
    UnsupportedHealthCheckType { cluster: String },

    /// 68 (ADR-0137 PV-4): a health check sets BOTH `http_health_check` and
    /// `tcp_health_check` — the upstream `HealthCheck.health_checker` oneof
    /// rejects this at load (MEASURED against v1.33.0).
    #[error("cluster '{cluster}' health check sets both http_health_check and tcp_health_check (mutually exclusive)")]
    BothHttpAndTcpHealthCheck { cluster: String },
```

- [ ] **Step 4: Restructure `validate_health_checks`**

Replace the body inside `if let Some(hc) = cluster.health_checks.first() { ... }` (lines 4687–4725) with:

```rust
    if let Some(hc) = cluster.health_checks.first() {
        // 68 (ADR-0137 PV-4): the upstream `health_checker` oneof — both present is fatal.
        if hc.http_health_check.is_some() && hc.tcp_health_check.is_some() {
            return Err(crate::ConfigError::BothHttpAndTcpHealthCheck {
                cluster: cluster.name.clone(),
            });
        }
        // Neither present → unsupported (gRPC/custom deferred). Precedence preserved.
        if hc.http_health_check.is_none() && hc.tcp_health_check.is_none() {
            return Err(crate::ConfigError::UnsupportedHealthCheckType {
                cluster: cluster.name.clone(),
            });
        }
        // Shared threshold + timing validation (both checker types).
        if hc.healthy_threshold < 1 {
            return Err(crate::ConfigError::InvalidHealthCheckThreshold {
                cluster: cluster.name.clone(),
                field: "healthy_threshold",
            });
        }
        if hc.unhealthy_threshold < 1 {
            return Err(crate::ConfigError::InvalidHealthCheckThreshold {
                cluster: cluster.name.clone(),
                field: "unhealthy_threshold",
            });
        }
        for (field, raw) in [("timeout", &hc.timeout), ("interval", &hc.interval)] {
            if parse_positive_duration(raw).is_none() {
                return Err(crate::ConfigError::InvalidHealthCheckTiming {
                    cluster: cluster.name.clone(),
                    field,
                });
            }
        }
        // Per-checker-type validation.
        if let Some(http) = &hc.http_health_check {
            if http.path.is_empty() {
                return Err(crate::ConfigError::EmptyHealthCheckPath {
                    cluster: cluster.name.clone(),
                });
            }
            for range in &http.expected_statuses {
                if range.start >= range.end {
                    return Err(crate::ConfigError::InvalidInt64Range {
                        start: range.start,
                        end: range.end,
                    });
                }
            }
        }
        if let Some(tcp) = &hc.tcp_health_check {
            let mut validate_payload = |p: &crate::HealthCheckPayload| match p.decode() {
                Ok(_) => Ok(()),
                Err(crate::PayloadDecodeError::InvalidHex(value)) => {
                    Err(crate::ConfigError::InvalidHealthCheckPayloadHex {
                        cluster: cluster.name.clone(),
                        value,
                    })
                }
                Err(crate::PayloadDecodeError::InvalidBase64(value)) => {
                    Err(crate::ConfigError::InvalidHealthCheckPayloadBase64 {
                        cluster: cluster.name.clone(),
                        value,
                    })
                }
                Err(crate::PayloadDecodeError::Empty) => {
                    Err(crate::ConfigError::EmptyHealthCheckPayload {
                        cluster: cluster.name.clone(),
                    })
                }
            };
            if let Some(send) = &tcp.send {
                validate_payload(send)?;
            }
            for recv in &tcp.receive {
                validate_payload(recv)?;
            }
        }
    }
```

Ensure `HealthCheckPayload` / `PayloadDecodeError` / `TcpHealthCheck` are re-exported from `lib.rs` if the tests reference them via `crate::` (add `pub use bootstrap::{HealthCheckPayload, PayloadDecodeError, TcpHealthCheck};` alongside the existing `HealthCheck`/`HttpHealthCheck` re-exports — grep `pub use bootstrap::` to find the line).

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p envoy-config 2>&1 | tail -30`
Expected: PASS — the 5 new validator tests + ALL existing `envoy-config` tests (the neither-checker `UnsupportedHealthCheckType` test still matches the variant; message change is variant-matched, not string-matched).

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
git commit -m "phase 68: validate_health_checks accepts TCP checker, rejects both-present + bad payloads"
```

---

### Task 4: TCP probe (`tcp_probe_once` / `tcp_probe_loop` / `receive_matches`)

**Files:**
- Modify: `crates/envoy-health/Cargo.toml` (add `net` to `tokio` features)
- Modify: `crates/envoy-health/src/probe.rs` (add the TCP probe + pure matcher)
- Test: inline `#[cfg(test)] mod tests` in `probe.rs`

**Interfaces:**
- Consumes: `TcpHealthCheck`, `HealthCheckPayload` (config); `EndpointHealth`, `Counter`, `CancellationToken` (as the HTTP probe does).
- Produces: `pub(crate) async fn tcp_probe_loop(addr, send: Option<Vec<u8>>, receive: Vec<Vec<u8>>, probe_timeout, interval_dur, endpoint_health, attempt, success, failure, cancel)`; `pub(crate) async fn tcp_probe_once(addr, send: &Option<Vec<u8>>, receive: &[Vec<u8>], probe_timeout) -> Result<(), TcpProbeError>`; `fn receive_matches(receive: &[Vec<u8>], buf: &[u8]) -> bool`. Consumed by Task 5 (scheduler dispatch).

- [ ] **Step 1: Add the `net` tokio feature**

In `crates/envoy-health/Cargo.toml`, extend the `tokio` features:

```toml
tokio = { version = "1", features = ["rt", "macros", "time", "sync", "net", "io-util"] }
```

- [ ] **Step 2: Write the failing pure-matcher tests**

Add to `probe.rs` tests:

```rust
#[test]
fn receive_matches_single_block_substring_anywhere() {
    // MEASURED: banner "ABPINGCD", receive [PING] → healthy (substring in the middle).
    assert!(receive_matches(&[b"PING".to_vec()], b"ABPINGCD"));
    assert!(receive_matches(&[b"PING".to_vec()], b"PING"));
    assert!(!receive_matches(&[b"PONG".to_vec()], b"ABPINGCD"));
}

#[test]
fn receive_matches_empty_receive_is_true() {
    // Connection-only: no receive payloads ⇒ connect success alone is healthy.
    assert!(receive_matches(&[], b""));
    assert!(receive_matches(&[], b"anything"));
}

#[test]
fn receive_matches_sequential_in_order() {
    // envoy-rust's OWN documented multi-block contract (NOT an Envoy-parity claim,
    // ADR-0137 PV-3): each block found at/after the previous match end.
    assert!(receive_matches(&[b"AB".to_vec(), b"CD".to_vec()], b"AB__CD"));
    assert!(!receive_matches(&[b"CD".to_vec(), b"AB".to_vec()], b"AB__CD"));
}
```

- [ ] **Step 3: Run the matcher tests to verify they fail**

Run: `cargo test -p envoy-health receive_matches 2>&1 | tail -20`
Expected: FAIL to compile — `receive_matches` not defined.

- [ ] **Step 4: Implement `receive_matches`, `tcp_probe_once`, `tcp_probe_loop`, `TcpProbeError`**

Add to `probe.rs` (add imports `use tokio::io::{AsyncReadExt, AsyncWriteExt}; use tokio::net::TcpStream;`):

```rust
/// 68 (ADR-0137 PV-3): scan `buf` for the `receive` payloads in order — each
/// found as a contiguous substring at/after the previous match's end. Empty
/// `receive` ⇒ connection-only (always true once connected). Single-block
/// reduces to "substring anywhere" (the reliably-measured Envoy behavior);
/// multi-block is envoy-rust's own sequential contract, NOT an Envoy-parity claim.
fn receive_matches(receive: &[Vec<u8>], buf: &[u8]) -> bool {
    let mut offset = 0usize;
    for payload in receive {
        if payload.is_empty() {
            continue;
        }
        match find_subslice(&buf[offset..], payload) {
            Some(pos) => offset += pos + payload.len(),
            None => return false,
        }
    }
    true
}

/// First index of `needle` in `haystack`, or `None`.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
}

/// 68: TCP-probe failure surface (diagnostic; the counters + EndpointHealth carry
/// the live signal, mirroring the HTTP `ProbeError`).
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum TcpProbeError {
    /// `tokio::time::timeout(probe_timeout, ...)` elapsed (connect hang, or
    /// `receive` never matched — the MEASURED `active_hc_timeout` path).
    Timeout,
    /// `TcpStream::connect` failed (the MEASURED connect-refuse path).
    Connect(String),
    /// Write of the `send` payload failed.
    Send(String),
    /// The connection reached EOF before `receive` matched.
    Eof,
}

/// 68 (ADR-0137 PV-6): one TCP probe — connect → optional `send` → scan for
/// `receive`, the WHOLE thing under one `timeout(probe_timeout, ...)` (the HC
/// timeout, not the cluster connect_timeout, bounds connect). Empty `receive`
/// ⇒ a successful connect is healthy. Mirrors the HTTP `probe_once` shape.
pub(crate) async fn tcp_probe_once(
    addr: SocketAddr,
    send: &Option<Vec<u8>>,
    receive: &[Vec<u8>],
    probe_timeout: Duration,
) -> Result<(), TcpProbeError> {
    let probe = async move {
        let mut stream = TcpStream::connect(addr)
            .await
            .map_err(|e| TcpProbeError::Connect(e.to_string()))?;
        if let Some(bytes) = send {
            stream
                .write_all(bytes)
                .await
                .map_err(|e| TcpProbeError::Send(e.to_string()))?;
        }
        if receive.is_empty() {
            // Connection-only: connect success ⇒ healthy.
            return Ok(());
        }
        let mut buf: Vec<u8> = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = stream
                .read(&mut chunk)
                .await
                .map_err(|e| TcpProbeError::Send(e.to_string()))?;
            if n == 0 {
                return Err(TcpProbeError::Eof);
            }
            buf.extend_from_slice(&chunk[..n]);
            if receive_matches(receive, &buf) {
                return Ok(());
            }
        }
    };
    match timeout(probe_timeout, probe).await {
        Ok(r) => r,
        Err(_) => Err(TcpProbeError::Timeout),
    }
}

/// 68: the periodic TCP-probe loop — the L4 sibling of `probe_loop`. Same
/// `interval` ticker + `tokio::select!` cancel branch + counter/EndpointHealth
/// wiring; only `probe_once` → `tcp_probe_once` differs.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn tcp_probe_loop(
    addr: SocketAddr,
    send: Option<Vec<u8>>,
    receive: Vec<Vec<u8>>,
    probe_timeout: Duration,
    interval_dur: Duration,
    endpoint_health: Arc<EndpointHealth>,
    attempt: Arc<Counter>,
    success: Arc<Counter>,
    failure: Arc<Counter>,
    cancel: CancellationToken,
) {
    let mut ticker = interval(interval_dur);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::debug!(addr=%addr, "active-HC TCP probe task shutting down");
                return;
            }
            _ = ticker.tick() => {
                attempt.inc();
                match tcp_probe_once(addr, &send, &receive, probe_timeout).await {
                    Ok(()) => {
                        success.inc();
                        endpoint_health.record_success();
                    }
                    Err(e) => {
                        tracing::debug!(addr=%addr, error=?e, "active-HC TCP probe failed");
                        failure.inc();
                        endpoint_health.record_failure();
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 5: Write + run the integration probe tests (mock TcpListener)**

Add to `probe.rs` tests (these exercise `tcp_probe_once` against a real ephemeral listener; use `#[tokio::test]`):

```rust
#[tokio::test]
async fn tcp_probe_connection_only_healthy() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { let _ = listener.accept().await; });
    assert!(tcp_probe_once(addr, &None, &[], Duration::from_secs(2)).await.is_ok());
}

#[tokio::test]
async fn tcp_probe_connect_refused_is_err() {
    // Reserve then drop a listener → the port refuses.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let r = tcp_probe_once(addr, &None, &[], Duration::from_secs(1)).await;
    assert!(matches!(r, Err(TcpProbeError::Connect(_)) | Err(TcpProbeError::Timeout)));
}

#[tokio::test]
async fn tcp_probe_receive_match_healthy() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        use tokio::io::AsyncWriteExt;
        let _ = s.write_all(b"AB").await;
        let _ = s.write_all(b"PING").await;
        let _ = s.write_all(b"CD").await;
        tokio::time::sleep(Duration::from_millis(200)).await;
    });
    let r = tcp_probe_once(addr, &None, &[b"PING".to_vec()], Duration::from_secs(2)).await;
    assert!(r.is_ok());
}

#[tokio::test]
async fn tcp_probe_receive_mismatch_times_out() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        use tokio::io::AsyncWriteExt;
        let _ = s.write_all(b"NOPE").await;
        tokio::time::sleep(Duration::from_secs(2)).await;
    });
    let r = tcp_probe_once(addr, &None, &[b"PING".to_vec()], Duration::from_millis(400)).await;
    assert!(matches!(r, Err(TcpProbeError::Timeout)));
}

#[tokio::test]
async fn tcp_probe_send_then_receive_healthy() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut b = [0u8; 16];
        let n = s.read(&mut b).await.unwrap();
        assert_eq!(&b[..n], b"hi");
        let _ = s.write_all(b"resp-OKOK-end").await;
        tokio::time::sleep(Duration::from_millis(200)).await;
    });
    let r = tcp_probe_once(addr, &Some(b"hi".to_vec()), &[b"OKOK".to_vec()], Duration::from_secs(2)).await;
    assert!(r.is_ok());
}
```

Run: `cargo test -p envoy-health 2>&1 | tail -30`
Expected: PASS — the matcher tests + the 5 probe tests + the existing `envoy-health` tests.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-health/Cargo.toml crates/envoy-health/src/probe.rs
git commit -m "phase 68: L4 TCP probe (connect/send/receive-scan) + pure receive_matches"
```

---

### Task 5: Scheduler dispatch — spawn the TCP probe when `tcp_health_check` is present

**Files:**
- Modify: `crates/envoy-health/src/scheduler.rs` (`Scheduler::spawn`, lines 47–120)
- Test: inline `#[cfg(test)] mod tests` in `scheduler.rs`

**Interfaces:**
- Consumes: `tcp_probe_loop`, `HealthCheckPayload::decode` (Tasks 1, 4).
- Produces: `Scheduler::spawn` dispatches HTTP vs TCP by checker presence; same 3 counters; ejection/`pick()` untouched.

- [ ] **Step 1: Write the failing scheduler test**

Add to `scheduler.rs` tests a TCP bootstrap const + a task-count test:

```rust
const TCP_HC_BOOTSTRAP: &str = r#"
static_resources:
  listeners: []
  clusters:
    - name: tcp_hc_backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      common_lb_config:
        healthy_panic_threshold: { value: 0 }
      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 2
          tcp_health_check: { receive: [ { text: "50494e47" } ] }
      load_assignment:
        cluster_name: tcp_hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 60011 } }
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
"#;

#[tokio::test]
async fn spawns_tcp_probe_task_and_registers_counters() {
    let bootstrap = parse_bootstrap(TCP_HC_BOOTSTRAP).expect("parse");
    let registry = Arc::new(StatsRegistry::new());
    let cluster_mgr = Arc::new(from_bootstrap(&bootstrap, Arc::clone(&registry)).await.expect("build"));
    let cancel = CancellationToken::new();
    let scheduler = Scheduler::spawn(&bootstrap, cluster_mgr, registry.clone(), cancel.clone()).expect("scheduler");
    assert_eq!(scheduler.task_count(), 1, "one TCP probe task for the single endpoint");
    let snapshot = registry.snapshot();
    for kind in ["attempt", "success", "failure"] {
        let name = format!("cluster.tcp_hc_backend.health_check.{kind}");
        assert!(snapshot.iter().any(|(n, _)| n == &name), "registry must contain {name}");
    }
    scheduler.shutdown().await;
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p envoy-health spawns_tcp_probe 2>&1 | tail -20`
Expected: FAIL — `Scheduler::spawn` panics at `.http_health_check.as_ref().expect(...)` (line 53) because the TCP checker has no HTTP checker.

- [ ] **Step 3: Restructure the dispatch in `Scheduler::spawn`**

Replace the HTTP-only extraction (lines 53–56) and the per-target spawn (lines 84–119) so the checker type is selected. Concretely, after the `let hc = match cfg.health_checks.first() { ... }` block and the counter registration + duration re-parse (which stay unchanged), replace the target loop with a dispatch:

```rust
            // Re-decode TCP payloads at spawn (defense-in-depth; the validator
            // already accepted them — the `parse_duration` precedent).
            let tcp_cfg = hc.tcp_health_check.as_ref().map(|tcp| {
                let send = tcp.send.as_ref().map(|p| p.decode().expect("validator-accepted send payload"));
                let receive: Vec<Vec<u8>> = tcp
                    .receive
                    .iter()
                    .map(|p| p.decode().expect("validator-accepted receive payload"))
                    .collect();
                (send, receive)
            });
            let http_cfg = hc.http_health_check.as_ref().map(|http| {
                (
                    http.host.clone().unwrap_or_else(|| cfg.name.clone()),
                    http.path.clone(),
                    http.expected_statuses.clone(),
                )
            });

            let handle = match cluster_mgr.get(&cfg.name) {
                Some(h) => h,
                None => continue,
            };
            let targets = handle
                .health_probe_targets()
                .expect("HC-configured cluster has health_probe_targets");
            for (addr, endpoint_health) in targets {
                let cancel = cancel.clone();
                let a = Arc::clone(&attempt);
                let s = Arc::clone(&success);
                let f = Arc::clone(&failure);
                let eh: Arc<EndpointHealth> = endpoint_health;
                let h = match (&http_cfg, &tcp_cfg) {
                    (Some((host, path, exp)), None) => {
                        let (host, path, exp) = (host.clone(), path.clone(), exp.clone());
                        tokio::spawn(async move {
                            probe_loop(addr, host, path, probe_timeout, interval_dur, exp, eh, a, s, f, cancel).await;
                        })
                    }
                    (None, Some((send, receive))) => {
                        let (send, receive) = (send.clone(), receive.clone());
                        tokio::spawn(async move {
                            crate::probe::tcp_probe_loop(addr, send, receive, probe_timeout, interval_dur, eh, a, s, f, cancel).await;
                        })
                    }
                    // Validator guarantees exactly one checker present.
                    _ => unreachable!("validator guarantees exactly one health checker"),
                };
                handles.push(h);
            }
```

Delete the now-superseded lines that unconditionally extracted `http` / `host_default` / `path` / `expected` (the old lines 53–56 and 80–82). Import the TCP loop (`use crate::probe::{probe_loop, tcp_probe_loop};` — or reference it via `crate::probe::tcp_probe_loop` as above). Make `tcp_probe_loop` visible to `scheduler.rs` (it is `pub(crate)` in `probe.rs`, so `crate::probe::tcp_probe_loop` works).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-health 2>&1 | tail -30`
Expected: PASS — the new TCP scheduler test + ALL existing scheduler tests (the HTTP path is unchanged).

- [ ] **Step 5: Confirm envoy-bin needs no change**

The scheduler is already wired into `envoy-bin` (grep `Scheduler::spawn` under `crates/envoy-bin/src/`); TCP dispatch is internal to `Scheduler::spawn`, so envoy-bin's call site is unchanged. Run: `cargo build -p envoy-bin 2>&1 | tail -5` — expect a clean build.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-health/src/scheduler.rs
git commit -m "phase 68: Scheduler dispatches HTTP vs TCP probe by checker presence"
```

---

### Task 6: Differential fixture `0074` + the `DEAD_BACKEND_PORT` harness marker

**Files:**
- Modify: `tests/differential/src/lib.rs` (add the `DEAD_BACKEND_PORT` marker: reserve a port, spawn NO backend, push into both kv maps, extend the BACKEND_HOST gate)
- Create: `tests/fixtures/0074-upstream-tcp-health-check/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`

**Interfaces:**
- Consumes: the `http1_after_settle` driver + `set_equal_modulo_allow_list` (fixture 0019 discipline) + `reserve_port()`.
- Produces: a green cross-proxy fixture witnessing connection-only TCP-HC ejection → synth-503.

- [ ] **Step 1: Add the `DEAD_BACKEND_PORT` marker to the harness**

In `tests/differential/src/lib.rs`, near the other `scan_needs_marker` gates (~line 3266) add:

```rust
    // 68 (ADR-0137 PV-2): a hermetic REFUSED port — reserve an ephemeral port
    // and spawn NO listener, so both proxies get ECONNREFUSED on the TCP HC
    // probe. `reserve_port` skips ports already handed out to the proxies, so
    // nothing binds it for the test's duration.
    let needs_dead_backend = scan_needs_marker(&backend_scan_sources, "DEAD_BACKEND_PORT");
    let dead_backend_port_str: Option<String> = if needs_dead_backend {
        Some(reserve_port().context("reserving DEAD_BACKEND_PORT")?.to_string())
    } else {
        None
    };
```

Push it into BOTH kv maps (upstream_kvs near line 3498, subject_kvs near line 3595):

```rust
        if let Some(dp) = dead_backend_port_str.as_deref() {
            v.push(("DEAD_BACKEND_PORT", dp.to_string()));
        }
```

Extend the BACKEND_HOST gate on BOTH sides (the `if backend_port_str.is_some() || ...` blocks at ~3620 upstream / ~3620 subject) to include `|| dead_backend_port_str.is_some()` so the container gets `host.docker.internal` and the subject gets `127.0.0.1`.

- [ ] **Step 2: Write the fixture configs**

`tests/fixtures/0074-upstream-tcp-health-check/envoy-rust.yaml`:

```yaml
# envoy-rust side. Connection-only TCP health check against a REFUSED port
# ({{DEAD_BACKEND_PORT}} has no listener) → after settle the sole endpoint is
# Unhealthy → pick() None → synth-503. Mirrors fixture 0019 (ADR-0137 PV-2).
node:
  cluster: phase-68-cluster
  id: phase-68-envoy-rust
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address: { address: 127.0.0.1, port_value: {{PORT}} }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                codec_type: HTTP1
                stat_prefix: ingress_http
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: local
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: tcp_hc_backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: tcp_hc_backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      dns_lookup_family: V4_ONLY
      common_lb_config:
        healthy_panic_threshold: { value: 0 }
      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 2
          tcp_health_check: {}
      load_assignment:
        cluster_name: tcp_hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: {{BACKEND_HOST}}
                      port_value: {{DEAD_BACKEND_PORT}}
```

`tests/fixtures/0074-upstream-tcp-health-check/envoy.yaml` — same as above but WITH the reference-side conventions of a sibling HC fixture (compare `0019/envoy.yaml` verbatim: it carries an `admin:` block, `generate_request_id` etc. as that file does). Copy `0019/envoy.yaml`, then: change `node.id`/`cluster` to phase-68; rename the cluster to `tcp_hc_backend`; replace the `http_health_check: { path: /healthz, expected_statuses: [...] }` block with `tcp_health_check: {}`; set `unhealthy_threshold: 2`; and change the endpoint `port_value` to `{{DEAD_BACKEND_PORT}}` (keep `{{BACKEND_HOST}}`).

- [ ] **Step 3: Write `expectations.yaml`** (copy `0019/expectations.yaml` verbatim — the observable is identical: 503 + `no healthy upstream` byte-exact after settle):

```yaml
# Phase-68 fixture-0074 expectations: post-convergence steady state on the H1
# listener after a 3.5s settle. The sole endpoint fails a connection-only
# tcp_health_check (ECONNREFUSED to {{DEAD_BACKEND_PORT}}) → Unhealthy → pick()
# None → synth-503 "no healthy upstream" (19 bytes, ADR-0037). Same observable
# as fixture 0019; settle_ms = interval 1s × unhealthy_threshold 2 + timeout 1s
# + margin = 3500ms.
driver:
  kind: http1_after_settle
  settle_ms: 3500
  method: get
  path: "/"
  host: "tcp_hc_backend"
  expected_status: 503
  expected_body:
    kind: byte_exact
    body: "no healthy upstream"
  expected_headers: set_equal_modulo_allow_list
equivalence:
  response_status: exact
  response_body:
    kind: byte_exact
```

- [ ] **Step 4: Write `README.md`** (mirror `0019/README.md`, describing the connection-only refused-port failure; note the `DEAD_BACKEND_PORT` marker and that NO backend is spawned).

- [ ] **Step 5: Build envoy-bin, then run the fixture differentially**

```bash
cargo build -p envoy-bin
cargo test -p differential -- fixture_0074 --nocapture 2>&1 | tee /tmp/f0074.log | tail -40
```
Expected: PASS. (If it false-REDs locally, cross-check against the documented host-flake set; CI is authoritative. Confirm the reserved dead port genuinely refuses on this host — the container reaches `host.docker.internal:<port>`; if the host firewall drops rather than refuses, the probe still TIMES OUT within `timeout` and ejects, so the 503 observable holds either way.)

- [ ] **Step 6: Commit**

```bash
git add tests/differential/src/lib.rs tests/fixtures/0074-upstream-tcp-health-check/
git commit -m "phase 68: fixture 0074 (connection-only TCP-HC ejection) + DEAD_BACKEND_PORT marker"
```

---

### Task 7: `BEHAVIOR_CONTRACT.md` `tcp_health_check` subsection + `§7.4` fuzz corpus seed

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (add a `tcp_health_check` subsection)
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/<seed>` (un-ignored)
- Modify: `crates/envoy-config/fuzz/.gitignore` (or the corpus `.gitignore`) to `!`-un-ignore the new seed

- [ ] **Step 1: Add the BEHAVIOR_CONTRACT subsection**

Append a `### Active TCP health check (`tcp_health_check`)` subsection recording the MEASURED facts (ADR-0136 §0 + ADR-0137): empty ⇒ connection-only healthy; `send` single `Payload` written once, `receive` repeated `Payload` scanned as a contiguous substring (single-block pinned; multi-block not parity-asserted); `Payload` = hex `text` | base64 `binary`; odd/non-hex `text` and both-checkers-present are load-fatal (native messages, byte-parity waived); the HC `timeout` bounds the whole probe incl. connect; receive-no-match/connect-refuse ⇒ `failure`+`network_failure` (`/failed_active_hc[/active_hc_timeout]`); the `cluster.<n>.health_check.*` + `membership_*` stat tree is IDENTICAL to phase-12 (no new names). Follow the section style of the existing HTTP health-check contract entry (grep `health_check` in the file).

- [ ] **Step 2: Add the fuzz corpus seed**

Create a seed file (raw YAML bytes) under `crates/envoy-config/fuzz/corpus/parse_bootstrap/` — e.g. `seed_tcp_health_check` — containing a minimal bootstrap with a cluster carrying `tcp_health_check: { send: { text: "000102" }, receive: [ { text: "0304" }, { binary: "AAECAw==" } ] }`. Then un-ignore it: grep the fuzz `.gitignore` (`crates/envoy-config/fuzz/.gitignore` and/or `crates/envoy-config/fuzz/corpus/.gitignore`) — add `!corpus/parse_bootstrap/seed_tcp_health_check` (memory `fuzz-corpus-seed-gitignored-by-default`).

- [ ] **Step 3: Verify the seed is tracked**

Run: `git add -A crates/envoy-config/fuzz/ && git status --porcelain crates/envoy-config/fuzz/ && git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/ | grep seed_tcp_health_check`
Expected: the seed appears in `git ls-files` output (NOT silently ignored). No new fuzz TARGET is added, so no `ci.yml` change is needed (ADR-0137 §7.4).

- [ ] **Step 4: Smoke-run the existing target over the new seed**

```bash
cd crates/envoy-config
cargo +nightly fuzz run parse_bootstrap -- -runs=0 2>&1 | tail -10   # loads the corpus incl. the new seed
cd ../..
```
Expected: no crash; the seed is a valid input (memory `cargo-fuzz-runs-from-crate-dir-not-repo-root`).

- [ ] **Step 5: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md crates/envoy-config/fuzz/
git commit -m "phase 68: BEHAVIOR_CONTRACT tcp_health_check subsection + parse_bootstrap fuzz seed"
```

---

## Self-Review

**Spec coverage** (SPEC §2.1 in-scope items → task):
1. Config schema (`TcpHealthCheck`/`HealthCheckPayload`, hex/base64 decode) → Tasks 1–2. ✓
2. Validation (both-checkers oneof, neither→Unsupported, shared timing/thresholds, message + pinning-test update) → Tasks 2–3. ✓
3. TCP probe task (connect/send/receive-scan/connection-only, one timeout) → Task 4. ✓
4. Dispatch wiring (checker-type selection) → Task 5. ✓
5. Fixture `0074` → Task 6. ✓
6. In-process coverage (decode + rejections + connection-only + receive-match + mismatch) → Tasks 1, 3, 4. ✓
7. `BEHAVIOR_CONTRACT.md` subsection → Task 7. ✓
8. `known-failures.txt`/conformance unchanged → no task (correctly untouched). ✓
- §2.3 fuzz disposition (parse_bootstrap seed, no new target) → Task 7. ✓

**Type consistency:** `HealthCheckPayload` / `PayloadDecodeError` / `TcpHealthCheck` defined in Task 1–2, re-exported in Task 3, consumed by Tasks 3–5 with matching signatures; `tcp_probe_once`/`tcp_probe_loop`/`receive_matches` defined in Task 4, consumed in Task 5; `DEAD_BACKEND_PORT` marker defined in Task 6, used only by fixture 0074. ✓

**§6.1 split gate (PV-5):** Re-derived against the live tree — Task 1 ~90 LoC, Task 2 ~40, Task 3 ~110, Task 4 ~200, Task 5 ~70, Task 6 ~180 (fixture+marker), Task 7 ~60 = **~750 net LoC** implementation + ~300 test LoC ≈ **~1050 total**, **7 tasks** (≤ the ~1500 LoC / ~25 task gate). **SINGLE-PHASE — ADR-0138 does NOT fire.** ✓

**Placeholder scan:** every code step carries complete code; no TBD/TODO. Task 6 Step 4 (README) and Task 7 Step 1 (contract prose) are documentation-by-template pointing at exact source files to mirror — acceptable per the "follow the established pattern" guidance. ✓

---

## Execution Handoff

Per §5.1, the state-3 implementation is a SEPARATE session (do not chain from this PLAN-write). The next session cold-starts, detects `PLAN.md` present + implementation incomplete (§5 state 3), and runs `superpowers:executing-plans` (or `superpowers:subagent-driven-development` — the tasks are largely sequential with shared config types, so inline `executing-plans` with checkpoints fits well) with `superpowers:test-driven-development` on every task.
