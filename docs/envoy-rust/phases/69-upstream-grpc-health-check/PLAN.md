# Phase 69 — Active gRPC Health Checking (`grpc_health_check`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every task is TDD (D-3.1): RED → GREEN → commit, no exceptions.

**Goal:** Land active **gRPC** health checking (`envoy.config.core.v3.HealthCheck.grpc_health_check`) as the upstream-robustness family's THIRD checker type (after phase-12 HTTP + phase-68 TCP), behaviorally equivalent to `envoyproxy/envoy:v1.33.0` under the differential contract.

**Architecture:** Reuse the ENTIRE phase-12/68 health machinery (the `envoy-health` `Scheduler`, the `EndpointHealth` consecutive-success/failure state machine, `pick()` exclusion, ejection, the `cluster.<n>.health_check.*` + `membership_*` stat tree) AND the upstream-H2 client (`crates/envoy-http2`). Add only: a `grpc_health_check` config schema + H2-requirement/oneof validation; a hand-rolled `HealthCheckRequest`/`HealthCheckResponse` protobuf codec; a **trailers-aware** unary `grpc.health.v1.Health/Check`-over-H2 call (the single new primitive — `grpc-status` lives in a trailer the existing client never reads); a `grpc_probe_once`/`grpc_probe_loop` mirroring `tcp_probe_loop`; a scheduler 3-tuple dispatch; a `Driver::Http2AfterSettle` differential driver; and fixture `0075`.

**Tech Stack:** Rust (pinned toolchain), `tokio`, `h2` 0.4.13 (`RecvStream::trailers()`), `bytes`. NO `prost`/`tonic` (hand-rolled codec — zero proto toolchain in-tree). `serde`/`serde_yaml` for config.

## Global Constraints

- `#![forbid(unsafe_code)]` at every crate root — never weaken (D-3.8).
- Config-load errors are NATIVE-worded, not byte-parity with Envoy (ADR-0049); they go to STDOUT and are not a differential wire surface.
- The gRPC checker REQUIRES an H2-upstream cluster; a `grpc_health_check` on a non-H2 cluster is load-fatal (parity with Envoy "cluster must support HTTP/2 for gRPC healthchecking"). H2-ness predicate: `cluster.typed_extension_protocol_options.as_ref().is_some_and(|teo| teo.http_protocol_options.explicit_http_config.http2_protocol_options.is_some())` (the idiom already at `bootstrap.rs:3795`).
- The probe `timeout` bounds the WHOLE probe (H2 connect + handshake + request + response + trailers) via one `tokio::time::timeout(hc.timeout, …)`; the cluster `connect_timeout` is NOT consulted (ADR-0137/0139 PV-6).
- **`network_failure` is NOT modeled** in envoy-rust for ANY checker type (grep-confirmed 0 hits). The gRPC probe ticks the SAME 3 counters as HTTP/TCP: `attempt`/`success`/`failure`. Do NOT add a 4th counter or a transport-vs-app classification branch (CF-69-2 — deferred).
- Verdict = (`grpc-status` trailer `== 0` (OK)) AND (`HealthCheckResponse.status == SERVING(1)`) ⇒ `Ok(())` (Healthy); anything else ⇒ `Err` (a `failure`).
- ADR-0028 is NOT lifted: fixture `0075` uses an **H2 listener** (`codec_type: HTTP2`) because the H2-upstream cluster forbids an H1 listener.
- Never trim `known-failures.txt`; never weaken an existing fixture. ROADMAP row edits preserve 6 cells + escape `\|`; rows 36/38/39/52/54 are malformed — do NOT "fix" them (append-only).
- `cargo build -p envoy-bin` before ANY local differential (the harness runs `target/debug/envoy-bin`). CI is authoritative for the documented host-flake set.
- New fuzz target ⇒ wire into `ci.yml` by hand; new corpus seed ⇒ `!`-un-ignore it (verify via `git ls-files`).

## File Structure

**`crates/envoy-config/src/bootstrap.rs`** — add the `GrpcHealthCheck` struct + the `grpc_health_check` field on `HealthCheck`; restructure `validate_health_checks` (H2-requirement + at-most-one-of-three); update the pinning test + add new validation tests.
**`crates/envoy-config/src/lib.rs`** — replace `BothHttpAndTcpHealthCheck` → `MultipleHealthCheckers`; add `GrpcHealthCheckRequiresHttp2`; widen the `UnsupportedHealthCheckType` message.
**`crates/envoy-http2/src/grpc.rs`** (NEW) — the hand-rolled gRPC health codec (`HealthCheckRequest` encode, `HealthCheckResponse`/`ServingStatus` decode, gRPC 5-byte framing, varint) + the trailers-aware unary `Health/Check` call.
**`crates/envoy-http2/src/lib.rs`** — `pub mod grpc;`.
**`crates/envoy-http2/src/error.rs`** — a decode-error type surface if needed (or keep decode errors inside `grpc.rs`).
**`crates/envoy-health/src/probe.rs`** — `GrpcProbeError` + `grpc_probe_once` + `grpc_probe_loop` (mirroring `tcp_probe_*`); fold M68-2.
**`crates/envoy-health/src/scheduler.rs`** — 3-tuple checker dispatch + `grpc_cfg` extraction.
**`tests/differential/src/lib.rs`** — `Driver::Http2AfterSettle` variant + `run_http2_after_settle_arm` + a `run_fixture` dispatch arm.
**`tests/differential/tests/upstream_grpc_health_check.rs`** (NEW) — the per-fixture `#[tokio::test]`.
**`tests/fixtures/0075-upstream-grpc-health-check/`** (NEW) — `envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml`, `README.md`.
**`docs/envoy-rust/BEHAVIOR_CONTRACT.md`** — a `## Active gRPC health check (grpc_health_check)` section after the TCP-HC section.
**`crates/envoy-config/fuzz/corpus/parse_bootstrap/`** (NEW seed) + `.gitignore` un-ignore.
**`crates/envoy-http2/fuzz/`** (NEW subcrate) — `Cargo.toml`, `fuzz_targets/grpc_health_decode.rs`, `corpus/grpc_health_decode/seed`, `.gitignore`.
**`.github/workflows/ci.yml`** — a rust-cache line + a fuzz step for `grpc_health_decode`.

---

## Task 1: `GrpcHealthCheck` config schema + `grpc_health_check` field

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (near the `HealthCheck`/`TcpHealthCheck` structs, ~`2430`-`2477`)
- Test: inline `#[cfg(test)]` in `crates/envoy-config/src/bootstrap.rs`

**Interfaces:**
- Produces: `pub struct GrpcHealthCheck { pub service_name: String, pub authority: String, pub initial_metadata: Vec<HeaderValueOption> }`; `HealthCheck.grpc_health_check: Option<GrpcHealthCheck>`.
- Consumes: `HeaderValueOption` (already at `bootstrap.rs:1886`).

- [ ] **Step 1: Write the failing test** (add near the existing `parses_empty_tcp_health_check_connection_only` test):

```rust
#[test]
fn parses_grpc_health_check_with_fields() {
    // gRPC checker on an H2-upstream cluster; all three fields set.
    let yaml = cluster_yaml_with_h2_and_health_check(
        "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          grpc_health_check:\n            service_name: my.svc\n            authority: hc.example.com\n            initial_metadata:\n              - header: { key: x-hc, value: \"1\" }",
    );
    let bs = crate::parse_bootstrap(&yaml).expect("grpc_health_check parses");
    let hc = bs.static_resources.clusters[0].health_checks.first().unwrap();
    let grpc = hc.grpc_health_check.as_ref().expect("grpc checker present");
    assert_eq!(grpc.service_name, "my.svc");
    assert_eq!(grpc.authority, "hc.example.com");
    assert_eq!(grpc.initial_metadata.len(), 1);
    assert!(hc.http_health_check.is_none());
    assert!(hc.tcp_health_check.is_none());
}

#[test]
fn parses_empty_grpc_health_check() {
    let yaml = cluster_yaml_with_h2_and_health_check(
        "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          grpc_health_check: {}",
    );
    let bs = crate::parse_bootstrap(&yaml).expect("empty grpc_health_check parses");
    let grpc = bs.static_resources.clusters[0].health_checks[0].grpc_health_check.as_ref().unwrap();
    assert_eq!(grpc.service_name, ""); // empty ⇒ overall server
}

#[test]
fn grpc_health_check_rejects_unknown_field() {
    let yaml = cluster_yaml_with_h2_and_health_check(
        "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          grpc_health_check: { bogus: 1 }",
    );
    assert!(crate::parse_bootstrap(&yaml).is_err(), "deny_unknown_fields rejects unknown grpc key");
}
```

> **Note:** Add a small test helper `cluster_yaml_with_h2_and_health_check(hc_block: &str) -> String` (in the test module) that emits a STATIC cluster carrying `typed_extension_protocol_options` with `explicit_http_config.http2_protocol_options: {}` plus the `hc_block`. Grep the existing tests near `bootstrap.rs:15132` for the exact bootstrap wrapper (`node`/`admin`/`static_resources`) they use and mirror it; the H2 block is `typed_extension_protocol_options:\n        envoy.extensions.upstreams.http.v3.HttpProtocolOptions:\n          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions\n          explicit_http_config:\n            http2_protocol_options: {}`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-config parses_grpc_health_check_with_fields parses_empty_grpc_health_check grpc_health_check_rejects_unknown_field 2>&1 | tail -20`
Expected: FAIL — `no field grpc_health_check` / `GrpcHealthCheck` unknown.

- [ ] **Step 3: Add the struct + field**

In `bootstrap.rs`, add `pub grpc_health_check: Option<GrpcHealthCheck>` to `HealthCheck` (with `#[serde(default)]`, mirroring `tcp_health_check` at `:2451`), and add the struct:

```rust
/// 69 (ADR-0138/0139): the gRPC checker sub-message
/// (`envoy.config.core.v3.HealthCheck.GrpcHealthCheck`). All fields optional:
/// `service_name` empty ⇒ probe the OVERALL server (gRPC service name "").
/// `initial_metadata` accepted for schema completeness; the probe does not
/// thread it (MINIMAL support per SPEC §2.2 — unobservable in fixture 0075).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields, default)]
pub struct GrpcHealthCheck {
    pub service_name: String,
    pub authority: String,
    pub initial_metadata: Vec<HeaderValueOption>,
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-config parses_grpc_health_check_with_fields parses_empty_grpc_health_check grpc_health_check_rejects_unknown_field 2>&1 | tail -20`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 69: add GrpcHealthCheck config schema + grpc_health_check field"
```

---

## Task 2: Validator — `MultipleHealthCheckers` + `GrpcHealthCheckRequiresHttp2` + neither-message + pinning test

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (the `ConfigError` HC variants, ~`727`-`746`)
- Modify: `crates/envoy-config/src/bootstrap.rs` (`validate_health_checks`, ~`4762`-`4780`; the pinning test at `:15174`; the both-checkers test at `:15324`)
- Test: inline `#[cfg(test)]`

**Interfaces:**
- Consumes: `GrpcHealthCheck` field (Task 1); H2-predicate on `Cluster`.
- Produces: `ConfigError::MultipleHealthCheckers { cluster: String }`, `ConfigError::GrpcHealthCheckRequiresHttp2 { cluster: String }`; the removal of `BothHttpAndTcpHealthCheck`.

- [ ] **Step 1: Write the failing tests** (add near `validate_rejects_both_http_and_tcp` at `bootstrap.rs:15324`):

```rust
#[test]
fn validate_rejects_grpc_on_non_h2_cluster() {
    // STATIC (non-H2) cluster + grpc_health_check ⇒ GrpcHealthCheckRequiresHttp2.
    let yaml = cluster_yaml_non_h2_with_health_check(
        "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          grpc_health_check: {}",
    );
    let err = crate::parse_bootstrap(&yaml).expect_err("grpc on non-H2 is fatal");
    assert!(matches!(err, crate::ConfigError::GrpcHealthCheckRequiresHttp2 { ref cluster } if cluster == "hc_backend"), "got {err:?}");
}

#[test]
fn validate_accepts_grpc_on_h2_cluster() {
    let yaml = cluster_yaml_with_h2_and_health_check(
        "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          grpc_health_check: {}",
    );
    assert!(crate::parse_bootstrap(&yaml).is_ok(), "grpc on H2 cluster validates OK");
}

#[test]
fn validate_rejects_multiple_health_checkers() {
    let yaml = cluster_yaml_with_h2_and_health_check(
        "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          http_health_check: { path: /z }\n          grpc_health_check: {}",
    );
    let err = crate::parse_bootstrap(&yaml).expect_err("two checkers is fatal");
    assert!(matches!(err, crate::ConfigError::MultipleHealthCheckers { ref cluster } if cluster == "hc_backend"), "got {err:?}");
}
```

Also add the non-H2 helper `cluster_yaml_non_h2_with_health_check(hc_block: &str) -> String` (STATIC cluster named `hc_backend`, NO `typed_extension_protocol_options`), and update the existing `validate_rejects_both_http_and_tcp` test's assertion from `BothHttpAndTcpHealthCheck` to `MultipleHealthCheckers`, and re-point `cluster_rejects_unknown_health_check_field` (`bootstrap.rs:15188`) from `grpc_health_check: {}` to `custom_health_check: {}` (still `deny_unknown_fields`-rejected; update its `:15175` comment to note gRPC is now supported/H2-gated).

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p envoy-config validate_rejects_grpc_on_non_h2_cluster validate_accepts_grpc_on_h2_cluster validate_rejects_multiple_health_checkers 2>&1 | tail -20`
Expected: FAIL — variants `GrpcHealthCheckRequiresHttp2`/`MultipleHealthCheckers` do not exist.

- [ ] **Step 3: Implement.**

In `lib.rs`, REPLACE the `BothHttpAndTcpHealthCheck` variant with `MultipleHealthCheckers` and ADD `GrpcHealthCheckRequiresHttp2`; widen the `UnsupportedHealthCheckType` message:

```rust
    /// 69 (ADR-0139): a health check sets MORE THAN ONE of
    /// http_health_check / tcp_health_check / grpc_health_check — the upstream
    /// `HealthCheck.health_checker` oneof rejects this at load. (Generalizes the
    /// phase-68 `BothHttpAndTcpHealthCheck`.)
    #[error("cluster '{cluster}' health check sets more than one of http_health_check/tcp_health_check/grpc_health_check (mutually exclusive)")]
    MultipleHealthCheckers { cluster: String },

    /// 69 (ADR-0139): grpc_health_check on a cluster whose upstream is not HTTP/2.
    /// Real Envoy makes this load-fatal (MEASURED v1.33.0: "cluster must support
    /// HTTP/2 for gRPC healthchecking").
    #[error("cluster '{cluster}' uses grpc_health_check but the cluster does not support HTTP/2 (set typed_extension_protocol_options HttpProtocolOptions.explicit_http_config.http2_protocol_options)")]
    GrpcHealthCheckRequiresHttp2 { cluster: String },
```
(Also widen the `UnsupportedHealthCheckType` `#[error]` message to "…sets none of http_health_check/tcp_health_check/grpc_health_check; custom_health_check is not supported".)

In `validate_health_checks` (`bootstrap.rs:4768`-`4780`), replace the both/neither block:

```rust
    if let Some(hc) = cluster.health_checks.first() {
        let n_set = [
            hc.http_health_check.is_some(),
            hc.tcp_health_check.is_some(),
            hc.grpc_health_check.is_some(),
        ]
        .iter()
        .filter(|b| **b)
        .count();
        if n_set > 1 {
            return Err(crate::ConfigError::MultipleHealthCheckers { cluster: cluster.name.clone() });
        }
        if n_set == 0 {
            return Err(crate::ConfigError::UnsupportedHealthCheckType { cluster: cluster.name.clone() });
        }
        if hc.grpc_health_check.is_some() {
            let is_h2 = cluster.typed_extension_protocol_options.as_ref().is_some_and(|teo| {
                teo.http_protocol_options.explicit_http_config.http2_protocol_options.is_some()
            });
            if !is_h2 {
                return Err(crate::ConfigError::GrpcHealthCheckRequiresHttp2 { cluster: cluster.name.clone() });
            }
        }
        // ... existing shared threshold/timing + per-type (http/tcp) validation unchanged ...
```

> Before editing `lib.rs`, `grep -rn "BothHttpAndTcpHealthCheck" crates/` to confirm the only non-test references are the validator + the one test; update all sites.

- [ ] **Step 4: Run to verify GREEN**

Run: `cargo test -p envoy-config health 2>&1 | tail -30` (runs all HC validation tests, incl. the re-pointed pinning test + the updated both→multiple test).
Expected: PASS (all HC tests green).

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs
git commit -m "phase 69: validator — MultipleHealthCheckers + GrpcHealthCheckRequiresHttp2; re-point pinning test"
```

---

## Task 3: Hand-rolled gRPC health codec (`envoy-http2::grpc`)

**Files:**
- Create: `crates/envoy-http2/src/grpc.rs`
- Modify: `crates/envoy-http2/src/lib.rs` (add `pub mod grpc;`)
- Test: inline `#[cfg(test)]` in `grpc.rs`

**Interfaces:**
- Produces:
  - `pub enum ServingStatus { Unknown, Serving, NotServing, ServiceUnknown }` (from `u64`: 0/1/2/3, else `Unknown`).
  - `pub fn encode_health_check_request(service: &str) -> Vec<u8>` — returns the FULL gRPC-framed body (5-byte prefix + message).
  - `pub fn decode_health_check_response(frame: &[u8]) -> Result<ServingStatus, GrpcDecodeError>` — strips the 5-byte prefix, decodes.
  - `pub enum GrpcDecodeError { ShortFrame, Compressed, LengthMismatch, BadVarint, BadWireType }`.

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_request_empty_service() {
        // service="" ⇒ empty message ⇒ frame = flag(0) + len(0)
        assert_eq!(encode_health_check_request(""), vec![0x00, 0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn encode_request_named_service() {
        // service="svc.up" ⇒ 00 00 00 00 08 0A 06 73 76 63 2E 75 70
        assert_eq!(
            encode_health_check_request("svc.up"),
            vec![0x00, 0x00, 0x00, 0x00, 0x08, 0x0A, 0x06, 0x73, 0x76, 0x63, 0x2E, 0x75, 0x70]
        );
    }

    #[test]
    fn decode_serving() {
        // frame 00 00 00 00 02 08 01 ⇒ SERVING
        assert_eq!(decode_health_check_response(&[0, 0, 0, 0, 2, 0x08, 0x01]).unwrap(), ServingStatus::Serving);
    }

    #[test]
    fn decode_not_serving() {
        assert_eq!(decode_health_check_response(&[0, 0, 0, 0, 2, 0x08, 0x02]).unwrap(), ServingStatus::NotServing);
    }

    #[test]
    fn decode_empty_message_is_unknown() {
        // absent field ⇒ protobuf default 0 ⇒ UNKNOWN (NOT healthy)
        assert_eq!(decode_health_check_response(&[0, 0, 0, 0, 0]).unwrap(), ServingStatus::Unknown);
    }

    #[test]
    fn decode_skips_unknown_field() {
        // an unknown field 2 (wire 2, len 1) before status: 12 01 FF 08 01 ⇒ still SERVING
        assert_eq!(decode_health_check_response(&[0, 0, 0, 0, 5, 0x12, 0x01, 0xFF, 0x08, 0x01]).unwrap(), ServingStatus::Serving);
    }

    #[test]
    fn decode_rejects_short_frame() {
        assert!(matches!(decode_health_check_response(&[0, 0, 0]), Err(GrpcDecodeError::ShortFrame)));
    }

    #[test]
    fn decode_rejects_compressed() {
        assert!(matches!(decode_health_check_response(&[1, 0, 0, 0, 2, 0x08, 0x01]), Err(GrpcDecodeError::Compressed)));
    }

    #[test]
    fn decode_rejects_length_mismatch() {
        // declared len 9 but only 2 message bytes present
        assert!(matches!(decode_health_check_response(&[0, 0, 0, 0, 9, 0x08, 0x01]), Err(GrpcDecodeError::LengthMismatch)));
    }
}
```

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p envoy-http2 grpc::tests 2>&1 | tail -20`
Expected: FAIL — module `grpc` / functions do not exist.

- [ ] **Step 3: Implement `grpc.rs`** (codec portion; the call fn is Task 4)

```rust
//! Hand-rolled gRPC health-checking codec + a trailers-aware unary call.
//! `envoy-http2` is the sole user of `h2` (client.rs:2); this module keeps the
//! gRPC-over-H2 logic co-located. NO prost/tonic — the two health messages are
//! one field each (ADR-0139 PV-3).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServingStatus {
    Unknown,
    Serving,
    NotServing,
    ServiceUnknown,
}

impl ServingStatus {
    fn from_u64(v: u64) -> ServingStatus {
        match v {
            1 => ServingStatus::Serving,
            2 => ServingStatus::NotServing,
            3 => ServingStatus::ServiceUnknown,
            _ => ServingStatus::Unknown, // 0 and any unknown enum value
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrpcDecodeError {
    ShortFrame,
    Compressed,
    LengthMismatch,
    BadVarint,
    BadWireType,
}

/// Encode `HealthCheckRequest { string service = 1 }` and wrap it in a gRPC
/// length-prefixed frame (1 flag byte 0x00 + 4-byte big-endian length).
pub fn encode_health_check_request(service: &str) -> Vec<u8> {
    // message body: field 1, wire type 2 (length-delimited). Empty service ⇒
    // omit the field (protobuf default) ⇒ empty message.
    let mut msg = Vec::new();
    if !service.is_empty() {
        msg.push(0x0A); // (1 << 3) | 2
        write_varint(&mut msg, service.len() as u64);
        msg.extend_from_slice(service.as_bytes());
    }
    let mut frame = Vec::with_capacity(5 + msg.len());
    frame.push(0x00); // uncompressed
    frame.extend_from_slice(&(msg.len() as u32).to_be_bytes());
    frame.extend_from_slice(&msg);
    frame
}

/// Decode a gRPC-framed `HealthCheckResponse { ServingStatus status = 1 }`.
pub fn decode_health_check_response(frame: &[u8]) -> Result<ServingStatus, GrpcDecodeError> {
    if frame.len() < 5 {
        return Err(GrpcDecodeError::ShortFrame);
    }
    if frame[0] != 0x00 {
        return Err(GrpcDecodeError::Compressed);
    }
    let len = u32::from_be_bytes([frame[1], frame[2], frame[3], frame[4]]) as usize;
    let body = &frame[5..];
    if body.len() != len {
        return Err(GrpcDecodeError::LengthMismatch);
    }
    let mut status = 0u64; // UNKNOWN default (absent field)
    let mut i = 0usize;
    while i < body.len() {
        let (tag, n) = read_varint(&body[i..]).ok_or(GrpcDecodeError::BadVarint)?;
        i += n;
        let field = tag >> 3;
        let wire = tag & 0x07;
        match wire {
            0 => {
                let (v, n) = read_varint(&body[i..]).ok_or(GrpcDecodeError::BadVarint)?;
                i += n;
                if field == 1 {
                    status = v;
                }
            }
            2 => {
                let (l, n) = read_varint(&body[i..]).ok_or(GrpcDecodeError::BadVarint)?;
                i += n;
                let l = l as usize;
                if i + l > body.len() {
                    return Err(GrpcDecodeError::LengthMismatch);
                }
                i += l;
            }
            1 => { if i + 8 > body.len() { return Err(GrpcDecodeError::LengthMismatch); } i += 8; }
            5 => { if i + 4 > body.len() { return Err(GrpcDecodeError::LengthMismatch); } i += 4; }
            _ => return Err(GrpcDecodeError::BadWireType), // 3/4 groups
        }
    }
    Ok(ServingStatus::from_u64(status))
}

fn write_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let mut b = (v & 0x7F) as u8;
        v >>= 7;
        if v != 0 {
            b |= 0x80;
        }
        out.push(b);
        if v == 0 {
            break;
        }
    }
}

/// Returns (value, bytes_consumed), or None on truncation / >10-byte overrun.
fn read_varint(buf: &[u8]) -> Option<(u64, usize)> {
    let mut result = 0u64;
    let mut shift = 0u32;
    for (i, &b) in buf.iter().enumerate() {
        if i >= 10 {
            return None; // varint too long
        }
        result |= ((b & 0x7F) as u64) << shift;
        if b & 0x80 == 0 {
            return Some((result, i + 1));
        }
        shift += 7;
    }
    None // truncated
}
```

- [ ] **Step 4: Run to verify GREEN**

Run: `cargo test -p envoy-http2 grpc::tests 2>&1 | tail -20`
Expected: PASS (9 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-http2/src/grpc.rs crates/envoy-http2/src/lib.rs
git commit -m "phase 69: hand-rolled gRPC health codec (encode/decode + framing)"
```

---

## Task 4: Trailers-aware unary `Health/Check` call

**Files:**
- Modify: `crates/envoy-http2/src/grpc.rs` (add the call fn + an in-process `h2::server` test)
- Modify: `crates/envoy-http2/src/error.rs` if a shared error is preferred (else keep in `grpc.rs`)
- Test: inline `#[cfg(test)]` with a loopback `h2::server`

**Interfaces:**
- Consumes: `encode_health_check_request`, `decode_health_check_response`, `ServingStatus` (Task 3); `envoy_http2::client::Client::connect(addr, host)` → `ClientStream` (its `send_request: h2::client::SendRequest<Bytes>` at `client.rs:82`).
- Produces: `pub async fn grpc_health_check_call(stream: &mut ClientStream, authority: &str, service: &str) -> Result<ServingStatus, GrpcCallError>` where the returned `Ok(status)` is ONLY produced when the `grpc-status` trailer is `0` (OK); a non-zero `grpc-status`, a missing trailer, a decode error, or an H2 error ⇒ `Err`.
- Produces: `pub enum GrpcCallError { Http2(String), GrpcStatus(i64), MissingTrailer, Decode(GrpcDecodeError), BadResponse }`.

> **Design:** the existing `ClientStream::send_request` (`client.rs:125`) drops `recv_stream` before reading trailers. The new call must build the request, send the framed body, drain DATA, THEN call `recv_stream.trailers().await`. Implement it as a method/fn that borrows `stream.send_request` directly (`pub(crate)`) so it can keep `recv_stream` alive. Header construction follows the `client.rs:140`-`164` idiom: `:method POST`, `:path /grpc.health.v1.Health/Check`, absolute URI `http://{authority}/grpc.health.v1.Health/Check`, `version HTTP_2`, headers `content-type: application/grpc` + `te: trailers`. Send HEADERS with `end_of_stream=false`, then `send_stream.send_data(Bytes::from(frame), true)`. Read the response; require `:status 200` else `Err(BadResponse)`; concatenate DATA chunks (releasing flow-control capacity as in `client.rs:196`-`199`); after `data()` yields `None`, `let trailers = recv_stream.trailers().await.map_err(...)?`; extract `grpc-status` (default `0` if the RPC put it in HEADERS for a trailers-only response — for the health service it is a trailer); if `grpc-status != 0` ⇒ `Err(GrpcStatus)`; else `decode_health_check_response(&body)` ⇒ `Ok(status)`.

- [ ] **Step 1: Write the failing test** (an in-process `h2::server` on a loopback that answers one `Health/Check`):

```rust
#[tokio::test]
async fn call_serving_verdict() {
    use tokio::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    // Server: accept one H2 conn, read the request, reply SERVING + grpc-status:0 trailer.
    let srv = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut conn = h2::server::handshake(tcp).await.unwrap();
        if let Some(req) = conn.accept().await {
            let (_req, mut respond) = req.unwrap();
            let resp = http::Response::builder()
                .status(200)
                .header("content-type", "application/grpc")
                .body(())
                .unwrap();
            let mut send = respond.send_response(resp, false).unwrap();
            // SERVING frame: 00 00 00 00 02 08 01
            send.send_data(bytes::Bytes::from_static(&[0, 0, 0, 0, 2, 0x08, 0x01]), false).unwrap();
            let mut trailers = http::HeaderMap::new();
            trailers.insert("grpc-status", http::HeaderValue::from_static("0"));
            send.send_trailers(trailers).unwrap();
        }
        // drive the connection to completion
        while conn.accept().await.is_some() {}
    });
    let mut stream = crate::client::Client::connect(addr, "hc.local").await.unwrap();
    let status = grpc_health_check_call(&mut stream, "hc.local", "").await.unwrap();
    assert_eq!(status, ServingStatus::Serving);
    srv.abort();
}
```

> Add a sibling `call_not_serving_still_ok_grpc_status` (server replies `08 02` + `grpc-status:0`) asserting `Ok(ServingStatus::NotServing)`, and `call_nonzero_grpc_status_is_err` (server replies `grpc-status:5`) asserting `Err(GrpcCallError::GrpcStatus(5))`. The PROBE (Task 5) maps `NotServing`→failure; the CALL only reports the decoded status + surfaces transport/grpc-status errors.

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p envoy-http2 grpc::tests::call_ 2>&1 | tail -20`
Expected: FAIL — `grpc_health_check_call` undefined.

- [ ] **Step 3: Implement `grpc_health_check_call`** per the Design note above.

- [ ] **Step 4: Run to verify GREEN**

Run: `cargo test -p envoy-http2 grpc:: 2>&1 | tail -20`
Expected: PASS (all `grpc::` tests).

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-http2/src/grpc.rs crates/envoy-http2/src/error.rs
git commit -m "phase 69: trailers-aware unary gRPC Health/Check-over-H2 call"
```

---

## Task 5: `GrpcProbeError` + `grpc_probe_once`/`grpc_probe_loop` (+ M68-2 fold)

**Files:**
- Modify: `crates/envoy-health/src/probe.rs`
- Modify: `crates/envoy-health/Cargo.toml` (ensure `envoy-http2` is a path dep; add if absent)
- Test: inline `#[cfg(test)]`

**Interfaces:**
- Consumes: `envoy_http2::client::Client::connect`, `envoy_http2::grpc::{grpc_health_check_call, ServingStatus}` (Task 4); `EndpointHealth`, `Counter`, `CancellationToken` (as `tcp_probe_loop` uses).
- Produces:
  - `pub(crate) enum GrpcProbeError { Timeout, Connect(String), Rpc(String), NotServing, GrpcStatus(i64), Decode(String) }` (plain `#[derive(Debug)] #[allow(dead_code)]`, the `TcpProbeError` style).
  - `pub(crate) async fn grpc_probe_once(addr: SocketAddr, authority: &str, service: &str, probe_timeout: Duration) -> Result<(), GrpcProbeError>`.
  - `pub(crate) async fn grpc_probe_loop(addr, authority, service, probe_timeout, interval_dur, endpoint_health, attempt, success, failure, cancel)` — the SAME arg shape as `tcp_probe_loop` (`probe.rs:229`) minus send/receive plus `authority`/`service`.

> `grpc_probe_once` builds the whole future — `Client::connect(addr, authority)` then `grpc_health_check_call(&mut stream, authority, service)` — and wraps it in ONE `timeout(probe_timeout, …)`; `Err(_)` timeout ⇒ `GrpcProbeError::Timeout`. A `connect` error ⇒ `Connect`; a `GrpcCallError::GrpcStatus(n)` ⇒ `GrpcStatus(n)`; `ServingStatus::Serving` ⇒ `Ok(())`; any other status ⇒ `NotServing`. `grpc_probe_loop` mirrors `tcp_probe_loop` EXACTLY: `interval` ticker + cancel-select; `attempt.inc()`; `Ok`→`success.inc()`+`endpoint_health.record_success()`; `Err`→`failure.inc()`+`endpoint_health.record_failure()`. NO `network_failure`.

- [ ] **Step 1: Write the failing test** (connect-refuse ⇒ `Err(Connect)`; the SERVING/NOT_SERVING verdicts are covered by Task 4's call tests + a probe test against the same in-process server):

```rust
#[tokio::test]
async fn grpc_probe_connect_refused_is_err() {
    // Reserve a port then drop the listener ⇒ ECONNREFUSED.
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    drop(l);
    let r = grpc_probe_once(addr, "hc.local", "", std::time::Duration::from_secs(1)).await;
    assert!(matches!(r, Err(GrpcProbeError::Connect(_)) | Err(GrpcProbeError::Timeout)), "got {r:?}");
}

#[tokio::test]
async fn grpc_probe_serving_is_ok() {
    // Reuse the Task-4 in-process SERVING server (extract a test helper or inline it).
    // ... spin the loopback h2 server returning 08 01 + grpc-status:0 ...
    // let r = grpc_probe_once(addr, "hc.local", "", Duration::from_secs(2)).await;
    // assert!(r.is_ok());
}
```

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p envoy-health grpc_probe 2>&1 | tail -20`
Expected: FAIL — `grpc_probe_once` undefined.

- [ ] **Step 3: Implement** `GrpcProbeError` + `grpc_probe_once` + `grpc_probe_loop`. **Fold M68-2:** at `probe.rs:209`, change the READ-error mapping `TcpProbeError::Send(...)` → a correctly-named variant (add `TcpProbeError::Read(String)` or reuse an `Eof`/`Recv` variant) so a read error is no longer mislabeled `Send`.

- [ ] **Step 4: Run to verify GREEN**

Run: `cargo test -p envoy-health 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-health/src/probe.rs crates/envoy-health/Cargo.toml
git commit -m "phase 69: grpc_probe_once/grpc_probe_loop + GrpcProbeError; fold M68-2 read-label fix"
```

---

## Task 6: Scheduler 3-tuple dispatch + `grpc_cfg` extraction

**Files:**
- Modify: `crates/envoy-health/src/scheduler.rs`
- Test: inline `#[cfg(test)]`

**Interfaces:**
- Consumes: `grpc_probe_loop` (Task 5); `HealthCheck.grpc_health_check` (Task 1).
- Produces: a `(None, None, Some(...))` arm that spawns `grpc_probe_loop`.

- [ ] **Step 1: Write the failing test** — a STATIC H2 cluster with `grpc_health_check` whose endpoint is a dead port; assert `cluster.<n>.health_check.attempt` is registered and ticks (mirror the existing `scheduler.rs:289` TCP test that iterates `["attempt","success","failure"]`).

- [ ] **Step 2: Run to verify RED** — `cargo test -p envoy-health scheduler 2>&1 | tail -20` — the grpc cluster hits the `unreachable!()` catch-all (panic) or `grpc_cfg` is unextracted.

- [ ] **Step 3: Implement.** Extract `grpc_cfg` mirroring `tcp_cfg` (`scheduler.rs:80`):

```rust
let grpc_cfg = hc.grpc_health_check.as_ref().map(|g| {
    let authority = if g.authority.is_empty() { host_default.clone() } else { g.authority.clone() };
    (authority, g.service_name.clone())
});
```
Widen the dispatch (`scheduler.rs:115`) to `match (&http_cfg, &tcp_cfg, &grpc_cfg)`, re-tag the existing arms `(Some(...), None, None)` / `(None, Some(...), None)`, add:
```rust
(None, None, Some((authority, service))) => {
    let (authority, service) = (authority.clone(), service.clone());
    tokio::spawn(async move {
        grpc_probe_loop(addr, authority, service, probe_timeout, interval_dur, eh, a, s, f, cancel).await;
    })
}
```
Keep `_ => unreachable!("validator guarantees exactly one health checker")`. Add `grpc_probe_loop` to the `use crate::probe::{...}` at `scheduler.rs:22`.

- [ ] **Step 4: Run to verify GREEN** — `cargo test -p envoy-health 2>&1 | tail -20` — PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-health/src/scheduler.rs
git commit -m "phase 69: scheduler 3-tuple checker dispatch + grpc_cfg extraction"
```

---

## Task 7: `Driver::Http2AfterSettle` + `run_http2_after_settle_arm`

**Files:**
- Modify: `tests/differential/src/lib.rs`

**Interfaces:**
- Consumes: `drive_http2` (`lib.rs:2259`, GET/OPTIONS-only), `diff_headers`, `HEADER_ALLOW_LIST`, `assert_body_rule`, `FixtureCtx`.
- Produces: `Driver::Http2AfterSettle { settle_ms, method, path, host, expected_status, expected_body, expected_headers }` (serde `kind: http2_after_settle`) + `run_http2_after_settle_arm(...)`.

- [ ] **Step 1: Write the failing test** — a `#[test]` that deserializes an `expectations.yaml`-shaped snippet with `driver: { kind: http2_after_settle, settle_ms: 100, method: get, path: "/", host: h, expected_status: 503, expected_body: { kind: byte_exact, body: "no healthy upstream" } }` into the `Driver` enum and asserts the `Http2AfterSettle` variant.

- [ ] **Step 2: Run to verify RED** — `cargo test -p differential http2_after_settle 2>&1 | tail -20` — FAIL, unknown variant.

- [ ] **Step 3: Implement.** Add the `Driver::Http2AfterSettle` enum variant (mirror `Http1AfterSettle` at `lib.rs:211`), a `run_fixture` dispatch arm (mirror `lib.rs:4095`), and `run_http2_after_settle_arm` — a verbatim clone of `run_http1_after_settle_arm` (`lib.rs:4917`-`5009`) with the two `drive_http1(addr, method, path, host, &[], None)` calls replaced by `drive_http2(addr, method, path, host, &[])`. The `expected_headers` field stays `Option<...>` — when the fixture omits it, the header axis is skipped (this is what `0075` relies on).

- [ ] **Step 4: Run to verify GREEN** — `cargo test -p differential http2_after_settle 2>&1 | tail -20` — PASS. Also `cargo build -p differential --tests` clean.

- [ ] **Step 5: Commit**

```bash
git add tests/differential/src/lib.rs
git commit -m "phase 69: differential Driver::Http2AfterSettle + run_http2_after_settle_arm"
```

---

## Task 8: Fixture `0075` + per-fixture differential test

**Files:**
- Create: `tests/fixtures/0075-upstream-grpc-health-check/{envoy.yaml, envoy-rust.yaml, expectations.yaml, README.md}`
- Create: `tests/differential/tests/upstream_grpc_health_check.rs`

**Interfaces:**
- Consumes: `Driver::Http2AfterSettle` (Task 7); `run_fixture`; the harness markers `{{PORT}}`, `{{BACKEND_HOST}}`, `{{DEAD_BACKEND_PORT}}`.

- [ ] **Step 1: Write the fixture + failing test.** Clone `0074`'s two YAMLs, changing on BOTH: `codec_type: HTTP2` on the listener; add to the cluster `typed_extension_protocol_options` with `explicit_http_config.http2_protocol_options: {}`; replace `tcp_health_check: {}` with `grpc_health_check: {}`. `envoy.yaml` keeps `admin` + binds `0.0.0.0`; `envoy-rust.yaml` binds `127.0.0.1`, no admin. `expectations.yaml`:

```yaml
driver:
  kind: http2_after_settle
  settle_ms: 3500
  method: get
  path: "/"
  host: "grpc_hc_backend"
  expected_status: 503
  expected_body:
    kind: byte_exact
    body: "no healthy upstream"
  # expected_headers OMITTED — envoy-rust's H2 no-healthy synth-503 emits a
  # narrower header set than Envoy (server + content-type only); the header
  # axis is a pre-existing H2-503 gap (CF-69-1), not asserted here.
equivalence:
  response_status: exact
  response_body:
    kind: byte_exact
```

`tests/differential/tests/upstream_grpc_health_check.rs` mirrors `upstream_tcp_health_check.rs`: one `#[tokio::test]` that joins the fixture dir and calls `differential::run_fixture(&dir)`. Write a `README.md` documenting the topology (H2 listener → H2-upstream cluster → gRPC-HC against a dead port → ejection → synth-503) and the header-axis omission (CF-69-1).

- [ ] **Step 2: Build the subject binary + run RED**

Run: `cargo build -p envoy-bin && cargo test -p differential --test upstream_grpc_health_check 2>&1 | tail -40`
Expected: this is the FULL differential (needs Docker + the pinned image). It should PASS once the product code (Tasks 1-6) is in. If it REDs on the documented host-flake set (bridge-IP / parallel-load), re-run in isolation and cross-check CI (state-4). If it REDs on a genuine header/body mismatch, debug via `superpowers:systematic-debugging` — the likely culprit is a config the subject rejects (rebuild `envoy-bin`) or the H2 no-healthy path.

- [ ] **Step 3: (fix as needed to green — this task's GREEN is the differential passing)**

- [ ] **Step 4: Confirm GREEN** — the fixture passes cross-proxy (status 503 + byte-exact `no healthy upstream`).

- [ ] **Step 5: Commit**

```bash
git add tests/fixtures/0075-upstream-grpc-health-check/ tests/differential/tests/upstream_grpc_health_check.rs
git commit -m "phase 69: differential fixture 0075 (gRPC-HC ejection → synth-503, H2 listener)"
```

---

## Task 9: `BEHAVIOR_CONTRACT.md` gRPC health-check section

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (insert after the TCP-HC section, before the `---` at line ~569)

- [ ] **Step 1:** Add a `## Active gRPC health check (grpc_health_check)` section recording the MEASURED facts (mirror the TCP-HC section's structure): REQUIRES an H2-upstream cluster (else load-fatal `GrpcHealthCheckRequiresHttp2`); `{}` ⇒ overall server (service `""`), else `service_name`; the probe is a unary `grpc.health.v1.Health/Check` over H2; verdict = `grpc-status == 0` AND `status == SERVING`; NOT_SERVING ⇒ `failure` (app-level), connect/transport failure ⇒ `failure` (envoy-rust does NOT model `network_failure` for ANY checker — CF-69-2); the http/tcp/grpc oneof ⇒ `MultipleHealthCheckers`; the shared `cluster.<n>.health_check.{attempt,success,failure}` + `membership_*` stat tree (unchanged names); the `0075` differential asserts status + byte-exact body only (the H2 no-healthy synth-503 header set is narrower than Envoy — CF-69-1); `grpc-timeout` request header not emitted (deferred-unobservable).

- [ ] **Step 2:** No test (docs). Verify markdown renders + the section is self-contained (D-3.4).

- [ ] **Step 3: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 69: BEHAVIOR_CONTRACT — grpc_health_check section"
```

---

## Task 10: `parse_bootstrap` corpus seed (config-surface fuzz)

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/grpc_health_check_seed` (a YAML bootstrap with a `grpc_health_check` on an H2 cluster)
- Modify: `crates/envoy-config/fuzz/.gitignore` (or the corpus `.gitignore`) — add a `!`-un-ignore line for the seed

- [ ] **Step 1:** Copy a minimal valid bootstrap (H2 cluster + `grpc_health_check: { service_name: svc }`) into the corpus dir. `!`-un-ignore it.

- [ ] **Step 2: Verify it is tracked**

Run: `git add -f` is NOT needed if the `!` line works: `git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/ | grep grpc_health_check_seed`
Expected: the seed path is listed (memory `fuzz-corpus-seed-gitignored-by-default`).

- [ ] **Step 3: Smoke-run the existing target over the seed** (optional, if nightly available): `cd crates/envoy-config && cargo +nightly fuzz run parse_bootstrap -- -runs=0` (memory `cargo-fuzz-runs-from-crate-dir-not-repo-root`).

- [ ] **Step 4: Commit**

```bash
git add crates/envoy-config/fuzz/
git commit -m "phase 69: parse_bootstrap corpus seed carrying grpc_health_check"
```

---

## Task 11: `grpc_health_decode` fuzz target (new `envoy-http2/fuzz` subcrate) + `ci.yml`

**Files:**
- Create: `crates/envoy-http2/fuzz/Cargo.toml`, `crates/envoy-http2/fuzz/fuzz_targets/grpc_health_decode.rs`, `crates/envoy-http2/fuzz/corpus/grpc_health_decode/serving_seed`, `crates/envoy-http2/fuzz/.gitignore`
- Modify: `.github/workflows/ci.yml` (the `fuzz` job — a rust-cache path line + a fuzz step)

**Interfaces:**
- Consumes: `envoy_http2::grpc::decode_health_check_response` (Task 3).

- [ ] **Step 1:** Create the fuzz subcrate mirroring `crates/envoy-config/fuzz/` (a `[package]` with `cargo-fuzz = true`, `libfuzzer-sys`, `envoy-http2` as a path dep; `[[bin]] name = "grpc_health_decode"`, `[workspace]` empty to exclude from the root workspace). The target:

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let _ = envoy_http2::grpc::decode_health_check_response(data);
});
```
Add a seed `serving_seed` = the bytes `00 00 00 00 02 08 01`. `.gitignore` un-ignores the seed (memory `fuzz-corpus-seed-gitignored-by-default`).

- [ ] **Step 2:** Wire `ci.yml` (the `fuzz` job, `ci.yml:77`): add `crates/envoy-http2/fuzz -> target` to the rust-cache paths (`ci.yml:93`-`96`) and a step:

```yaml
      - name: fuzz grpc_health_decode
        working-directory: crates/envoy-http2/fuzz
        run: cargo +nightly fuzz run grpc_health_decode -- -max_total_time=30
```
Update the job `name:` to include `grpc_health_decode` (memory `new-fuzz-target-needs-a-ci-yml-step`).

- [ ] **Step 3: Verify locally** (nightly): `cd crates/envoy-http2/fuzz && cargo +nightly fuzz run grpc_health_decode -- -max_total_time=15` — clean, no crash. Confirm the seed is tracked: `git ls-files crates/envoy-http2/fuzz/corpus/`.

- [ ] **Step 4: Commit**

```bash
git add crates/envoy-http2/fuzz/ .github/workflows/ci.yml
git commit -m "phase 69: grpc_health_decode fuzz target + ci.yml wiring"
```

---

## Task 12: Full §7.5 gate dry-run (pre-verification sanity)

> This is NOT the state-4 verification (that is a separate session). This task
> is a fast local sanity sweep so state-3 lands green-by-construction.

- [ ] **Step 1:** `cargo build --workspace --all-targets 2>&1 | tail -5` — clean.
- [ ] **Step 2:** `cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -20` — clean (watch for `dead_code` on the removed `BothHttpAndTcpHealthCheck`, and `is_multiple_of`/manual-lint nits).
- [ ] **Step 3:** `cargo fmt --all -- --check` — clean.
- [ ] **Step 4:** `cargo test --workspace --no-fail-fast 2>&1 > /tmp/t.txt; tail -30 /tmp/t.txt` — redirect full output (memory `never-pipe-verification-runs-through-tail`); adjudicate any RED against the documented host-flake set (memory `local-red-set-varies-run-to-run`).
- [ ] **Step 5: Commit** any fmt/clippy fixes:

```bash
git add -A
git commit -m "phase 69: clippy/fmt cleanups"
```

---

## Self-Review (run before handing to state-3)

**Spec coverage** (SPEC §2.1): (1) config schema → Task 1 ✓; (2) validation (oneof-of-three + H2-requirement + neither + pinning test) → Task 2 ✓; (3) gRPC-unary-over-H2 primitive → Task 4 ✓; (4) hand-rolled codec → Task 3 ✓; (5) probe task + `GrpcProbeError` → Task 5 ✓; (6) scheduler dispatch → Task 6 ✓; (7) fixture `0075` + `http2_after_settle` → Tasks 7-8 ✓; (8) in-process coverage (SERVING/NOT_SERVING/refuse + framing + decode + rejections) → Tasks 3/4/5/2 ✓; (9) BEHAVIOR_CONTRACT → Task 9 ✓; (10) fuzz → Tasks 10-11 ✓.

**Deviations from SPEC (per ADR-0139, all documented):** `network_failure` NOT implemented (CF-69-2); `0075` header axis OMITTED (CF-69-1); `grpc-timeout` request header deferred; a dedicated `grpc_health_decode` fuzz target ADDED (the SPEC left it "confirm at PLAN-write").

**Type consistency:** `ServingStatus`/`GrpcDecodeError`/`GrpcCallError`/`GrpcProbeError` are used consistently across Tasks 3→4→5. `grpc_probe_loop`'s arg shape matches `tcp_probe_loop`. The `Driver::Http2AfterSettle` field set mirrors `Http1AfterSettle`.

**Ordering / DAG:** Tasks 1→2 (config), 3→4 (codec→call), 4+5 (call→probe), 5→6 (probe→scheduler), 7→8 (driver→fixture) are serial. Task 1 unblocks 6's `grpc_cfg`. Tasks 3-4 (envoy-http2) are independent of Tasks 1-2 (envoy-config) and MAY run in parallel worktrees; Tasks 9-11 (docs/fuzz) are leaf. Task 8 (differential) depends on ALL product code (1-7) + `cargo build -p envoy-bin`.
