# Phase 25.2 — `envoy.filters.http.buffer` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (the project default per `feedback_execution_style`; dispatch implementers SERIALLY per `feedback_serial_subagent_dispatch` — parallel implementers race on shared `main` and this harness garbles large parallel tool batches). Every task is TDD (test first). Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `envoy.filters.http.buffer` (the NINTH `HttpFilterInstance` variant) — a decode-side-only filter that rejects an over-limit request body with a 413 `Payload Too Large` local reply (`body.len() > effective_max_request_bytes`, strict `>`), configurable per-route via `BufferPerRoute` (disable / lowered limit), and prove it green via fixture `0033-http-filter-buffer` on an H1 listener proxying to a real `http1-echo-server`. Flips parent phase `25` to `done` at state-6.

**Architecture:** Additive plug-in on already-landed infrastructure. The full request body is available as `FilterRequest.body` at `decode_headers` time (on H1 via the now-closed `25.1`; on H2 via the codec). `BufferFilter` length-checks it and short-circuits with a 413 decorated by the existing H1/H2 filter-synth helpers — the rbac/csrf precedent. The per-route override threads through the existing phase-23 `apply_route_config` hook (the cors/csrf precedent) — **NO HCM change**. Config is a `Buffer { max_request_bytes: u32 }` chain entry + a `BufferPerRoute` oneof `{ disabled, buffer }` third `PerFilterConfig` variant; the generic `PerRouteConfigForAbsentFilter` validator covers buffer for free. **NO stats** (ADR-0063 dropped them). This phase also lands the two `25.1` `REVIEW.md` carry-forwards (M25.1-1 allocation bound + M25.1-2 cross-segment forwarding test).

**Tech Stack:** Rust; `bytes` (`Bytes`/`BytesMut`); `serde`/`serde_yaml` (config); the existing `envoy-config` bootstrap parser, `envoy-filter` pipeline, `envoy-http1` HCM, and the `tests/differential` harness.

**Scope locked by:** ADR-0062 (parent scope), ADR-0063 (the §6.2 wire contract — 413 `Payload Too Large` 17B / strict `>` / NO stats / `Buffer`+`BufferPerRoute` shapes / reuse `PerRouteConfigForAbsentFilter`), ADR-0064 (the split). SPEC: `docs/envoy-rust/phases/25.2-http-filter-buffer/SPEC.md`; parent SPEC: `docs/envoy-rust/phases/25-http-filter-buffer/SPEC.md`.

**Split gate (BOOTSTRAP §6.1):** 7 tasks / ~800 LoC net — UNDER the `> ~25 tasks` / `> ~1500 LoC` gate (ADR-0064 refined the estimate downward: NO stats, the 413 reuses the decorate helpers verbatim, the absent-filter validator is reused verbatim). **No further split.**

---

## The `max_request_bytes` residual disposition — RESOLVED in this PLAN (ADR-0063 Residual; NO new ADR)

ADR-0063 left ONE residual to the `25.2` state-2 PLAN-write: the disposition of an **absent / `0` / malformed** `max_request_bytes` (the live §6.2 probes used only valid limits `10` and `4`). **Decision — all-fatal via a required serde field (the ADR-0049 posture; the projection HOLDS):**

- `max_request_bytes` is a proto `UInt32Value` accepted on the wire as a **plain integer** (ADR-0063 finding 2: `max_request_bytes: 10` `--mode validate`-accepted). envoy-rust models it as a **required, non-`Option` `u32`** field with `#[serde(deny_unknown_fields)]` — the `LocalRateLimitConfig { stat_prefix: String, token_bucket: TokenBucket, … }` required-scalar precedent (`crates/envoy-config/src/bootstrap.rs:887`).
- **Absent** → `serde_yaml` emits a `missing field \`max_request_bytes\`` deserialize error → **fatal config rejection at startup** (the project's all-fatal posture, ADR-0049). Envoy's PGV marks the field required (`UInt32Value … {required: true}`), so Envoy ALSO rejects absent → **differentially faithful** (both fatal).
- **Malformed** (non-integer / negative / `> u32::MAX`) → `serde_yaml` u32 parse error → **fatal**. Matches Envoy's proto-parse rejection.
- **`0`** → a valid `u32`; **ACCEPTED**. Semantics: reject iff `body.len() > 0` (the strict `>` of ADR-0063 finding 6). A `UInt32Value{value: 0}` is "set" so it satisfies Envoy's `required` → **both accept** → differentially faithful.
- **Robustness note:** even if Envoy's exact disposition differed, envoy-rust being all-fatal-stricter is the ALREADY-ACCEPTED project posture (ADR-0063 finding 7: envoy-rust's generic `PerRouteConfigForAbsentFilter` is stricter than Envoy's accept-inert, kept consistent with cors/csrf). Fixture `0033` always supplies a valid limit, so the absent/malformed disposition is never differentially exercised.
- **Consequence:** **NO new `ConfigError` variant** (serde handles absent/malformed; `0` is accepted) and **NO ADR-0065** — the projected all-fatal posture is confirmed, so the PLAN-write surfaces no NEW decision. ADR ledger head stays **ADR-0064**; next available stays **ADR-0065** (unfired).

---

## The two `25.1` `REVIEW.md` carry-forwards (adjudicated non-blocking for `25.1`; this phase's `max_request_bytes` layer owns them)

- **M25.1-1 — bound the H1 request-body read/allocation.** The `25.1` body read does `BytesMut::with_capacity(body_len)` (`crates/envoy-http1/src/hcm.rs:641`), reserving the untrusted client `Content-Length` up front (a client sending only `Content-Length: 4000000000` triggers a ~4 GB reservation). `BufferFilter`'s post-read `body.len() > max` check runs AFTER the body is buffered (SPEC §5.3 — full-body-before-pipeline) and does NOT by itself cap the read. **Task 4** softens the up-front reservation to `body_len.min(INITIAL_BODY_BUF_CAP)` (grow-on-demand via `extend_from_slice`) — the cheap, behavior-preserving fix the reviewer recommended. *(A true per-request cap tied to the effective limit would require threading the per-route limit to the pre-pipeline read site or a global ceiling — a deferred non-goal under SPEC §4's streaming deferral; the effective limit is not known at body-read time because the route/filter config is resolved later in the pipeline.)*
- **M25.1-2 — cross-TCP-segment body-reassembly forwarding test.** The four `25.1` tests write head+body in one `write_all`, so the multi-read `while remaining > 0` reassembly loop (`hcm.rs:645-662`) ran zero times. **Task 4** adds a test that writes the head, flushes, sleeps, then writes the body across TCP segments and asserts the upstream received the full forwarded body.

---

## Code anchors (code-HEAD `dc6d9cca6`; **verify line numbers before editing** — `hcm.rs` is ~5600 lines and drifts)

- **`crates/envoy-config/src/bootstrap.rs`** — `HttpFilterTypedConfig` `@type`-tagged enum `:741-771` (Cors `:767`, Csrf `:769`); `PerFilterConfig` `@type`-tagged enum `:791-796` (Cors `:792`, Csrf `:794`); `Route.typed_per_filter_config` field `:1247` + hand-rolled deserializer `:1413-1515`; the per-filter name↔typed_config validation match `:2860-2913` (Cors arm `:2896-2904`, Csrf arm `:2905-2912`); the generic `PerRouteConfigForAbsentFilter` validator `:2728-2746` (iterates `typed_per_filter_config.keys()` against `present_http_filter_names` — buffer covered for free); `LocalRateLimitConfig` required-scalar precedent `:887-894`.
- **`crates/envoy-config/src/lib.rs`** — the `ConfigError` enum (`UnsupportedHttpFilter { name }` is the name-mismatch variant reused by every filter arm; csrf variants `:539,:546`). **No new variant this phase.**
- **`crates/envoy-filter/src/instance.rs`** — `HttpFilterInstance` enum `:33-77` (Csrf `:67`, the 8th; **Buffer is the 9th**); `build` dispatch `:108-135` (Csrf arm `:132-134`); `decode_headers` dispatch `:138-155` (Csrf `:147`); `encode_headers` dispatch `:157-174` (Csrf `:166`); `apply_route_config` dispatch `:180-190` (Cors `:182`, Csrf `:183`).
- **`crates/envoy-filter/src/csrf.rs`** — the closest precedent for the WHOLE buffer module (struct + `build_from_config` + `apply_route_config` + `decode_headers` + the trivial `encode_headers` `Continue` arm + `failure_response()` via `Bytes::from_static` + the `#[cfg(test)] mod tests` shape).
- **`crates/envoy-filter/src/types.rs`** — `FilterRequest { method, path, headers, body: Option<Bytes> }` `:28-35`; `FilterResponse { status: u16, reason: Option<&'static str>, headers: Vec<(String,String)>, body: Bytes }` `:43-48`. **`crates/envoy-filter/src/pipeline.rs`** — `Decision { Continue, StopAndSend(FilterResponse) }` `:12-15`; `FilterPipeline::apply_route_config` `:66-70`.
- **`crates/envoy-filter/src/lib.rs`** — the `mod <filter>;` + `pub use` re-export list (find the `csrf` line and mirror it for `buffer`).
- **`crates/envoy-http1/src/hcm.rs`** — the `25.1` body-read block `:630-667` (the `BytesMut::with_capacity(body_len)` at `:641` is the M25.1-1 site; the `while remaining > 0` reassembly loop `:645-662` is the M25.1-2 site); `parse_content_length(&req.headers)?` call `:600`; chunked-501 rejection `:694`; the boundary conversion `filter_req { … body: req.body.take() }` `:677`; the write-back `req.body = filter_req.body;` `:684`; `out_req.body = req.body.clone()` in `run_attempt` (`:359` region); the H1 synth decorator `decorate_filter_synth_response(resp, close)` `:1472-1517`. Test helpers in `#[cfg(test)] mod tests`: `spawn_capturing_upstream(response) -> (u16, Arc<Mutex<Vec<u8>>>)`, `cluster_mgr_with_endpoint(name, port).await`, `hcm_config_with_cluster(prefix, RouteAction, cluster_mgr)`, `drive(cfg, req_bytes).await -> Vec<u8>`.
- **`tests/differential/src/lib.rs`** — `Http1Probe` struct `:817-833` (`name`/`method`/`path`/`host`/`extra_headers`/`expected_status`/`expected_body`/`expected_headers`; **NO `body` field today**); `drive_http1(addr, method, path, host, extra_headers)` `:1526-1648` (request assembly `:1537-1547`); `Http1BodyRule` (`{ kind: byte_exact, body: "…" }`); the `http1_probe_list` driver-kind match arm that calls `drive_http1` per probe (grep `http1_probe_list` and `drive_http1(`).
- **Fixture template:** `tests/fixtures/0032-http-filter-csrf/` (`envoy.yaml` + `envoy-rust.yaml` + `expectations.yaml` + `inputs/.gitkeep` + `README.md`); the differential acceptance test `tests/differential/tests/http_filter_csrf.rs` (a thin `differential::run_fixture(&dir)` runner).
- **Echo server:** `tests/helpers/http1-echo-server/src/main.rs:149-244` — reads `content-length` bytes and echoes `method:`/`path:`/`headers:` (lowercase-name-sorted)/`body: <BODY>` as a `200 text/plain` response.
- **Fuzz corpus:** `crates/envoy-config/fuzz/corpus/parse_bootstrap/` (drop a new hand-authored seed YAML; NO new fuzz target — buffer reuses the bootstrap parser).
- **BEHAVIOR_CONTRACT:** `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — the CSRF local-reply section ends ~`:565`; add the buffer 413 row after it (before `## Admin endpoint body shapes` `:629`).

---

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `crates/envoy-config/src/bootstrap.rs` | config schema | MODIFY — add `Buffer`/`BufferPerRoute` structs; add `Buffer` to `HttpFilterTypedConfig` + `PerFilterConfig`; add the name↔typed_config validation arm; add serde tests. |
| `crates/envoy-filter/src/buffer.rs` | the `BufferFilter` runtime | CREATE — the filter + its unit tests (the decode-side backstop). |
| `crates/envoy-filter/src/lib.rs` | crate module list | MODIFY — `mod buffer;` + `pub use`. |
| `crates/envoy-filter/src/instance.rs` | filter dispatch | MODIFY — add the `Buffer` variant + 4 dispatch arms (build/decode/encode/apply_route_config) + a `FilterPipeline` in-process backstop test. |
| `crates/envoy-http1/src/hcm.rs` | H1 body read | MODIFY — M25.1-1 allocation bound + M25.1-2 cross-segment forwarding test. |
| `tests/differential/src/lib.rs` | the differential harness | MODIFY — add `Http1Probe.body` + a `body` param to `drive_http1` + a harness unit test; fix all call sites. |
| `tests/fixtures/0033-http-filter-buffer/` | the differential fixture | CREATE — `envoy.yaml` + `envoy-rust.yaml` + `expectations.yaml` + `inputs/.gitkeep` + `README.md`. |
| `tests/differential/tests/http_filter_buffer.rs` | fixture acceptance test | CREATE — the `differential::run_fixture` runner. |
| `crates/envoy-config/fuzz/corpus/parse_bootstrap/seed-buffer.yaml` | fuzz seed | CREATE — a bootstrap YAML exercising `Buffer`/`BufferPerRoute`. |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | differential contract | MODIFY — add the 413 `Payload Too Large` local-reply row. |

---

## Task 1: Config schema — `Buffer` + `BufferPerRoute` + the two enum variants + the validation arm

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (the `HttpFilterTypedConfig` enum `:741`, the `PerFilterConfig` enum `:791`, the validation match `:2905`, and a new test).

- [ ] **Step 1: Write the failing serde tests.**

Add to the `#[cfg(test)] mod tests` in `bootstrap.rs` (model the existing cors/csrf parse tests near `:13067`). These pin the residual disposition (absent → fatal; `0` → accepted; malformed → fatal) and the two wire shapes.

```rust
#[test]
fn buffer_chain_config_parses_plain_integer() {
    // ADR-0063 finding 2: max_request_bytes is a UInt32Value accepted as a
    // plain integer.
    let yaml = r#"
"@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.Buffer
max_request_bytes: 10
"#;
    let cfg: crate::HttpFilterTypedConfig = serde_yaml::from_str(yaml).unwrap();
    match cfg {
        crate::HttpFilterTypedConfig::Buffer(b) => assert_eq!(b.max_request_bytes, 10),
        _ => panic!("expected Buffer"),
    }
}

#[test]
fn buffer_chain_config_zero_is_accepted() {
    // Residual disposition: 0 is a valid u32 limit (reject iff body.len() > 0).
    let yaml = r#"
"@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.Buffer
max_request_bytes: 0
"#;
    let cfg: crate::HttpFilterTypedConfig = serde_yaml::from_str(yaml).unwrap();
    assert!(matches!(cfg, crate::HttpFilterTypedConfig::Buffer(b) if b.max_request_bytes == 0));
}

#[test]
fn buffer_chain_config_absent_max_request_bytes_is_fatal() {
    // Residual disposition: a required non-Option u32 → serde missing-field
    // error → fatal at startup (ADR-0049 all-fatal posture).
    let yaml = r#"
"@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.Buffer
"#;
    let r: Result<crate::HttpFilterTypedConfig, _> = serde_yaml::from_str(yaml);
    assert!(r.is_err(), "absent max_request_bytes must be a fatal parse error");
}

#[test]
fn buffer_chain_config_negative_max_request_bytes_is_fatal() {
    let yaml = r#"
"@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.Buffer
max_request_bytes: -1
"#;
    let r: Result<crate::HttpFilterTypedConfig, _> = serde_yaml::from_str(yaml);
    assert!(r.is_err(), "negative max_request_bytes must be a fatal parse error");
}

#[test]
fn buffer_per_route_disabled_parses() {
    // ADR-0063 finding 3: BufferPerRoute oneof { disabled, buffer }.
    let yaml = r#"
"@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.BufferPerRoute
disabled: true
"#;
    let pfc: crate::PerFilterConfig = serde_yaml::from_str(yaml).unwrap();
    match pfc {
        crate::PerFilterConfig::Buffer(b) => {
            assert!(b.disabled);
            assert!(b.buffer.is_none());
        }
        _ => panic!("expected Buffer per-route"),
    }
}

#[test]
fn buffer_per_route_lowered_limit_parses() {
    let yaml = r#"
"@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.BufferPerRoute
buffer:
  max_request_bytes: 4
"#;
    let pfc: crate::PerFilterConfig = serde_yaml::from_str(yaml).unwrap();
    match pfc {
        crate::PerFilterConfig::Buffer(b) => {
            assert!(!b.disabled);
            assert_eq!(b.buffer.unwrap().max_request_bytes, 4);
        }
        _ => panic!("expected Buffer per-route"),
    }
}

#[test]
fn buffer_chain_config_rejects_unknown_field() {
    let yaml = r#"
"@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.Buffer
max_request_bytes: 10
bogus: 1
"#;
    let r: Result<crate::HttpFilterTypedConfig, _> = serde_yaml::from_str(yaml);
    assert!(r.is_err(), "deny_unknown_fields must reject unknown field");
}
```

- [ ] **Step 2: Run the tests to verify they FAIL (no `Buffer` type yet).**

Run: `cargo test -p envoy-config buffer_ -- --nocapture`
Expected: FAIL to compile — `HttpFilterTypedConfig::Buffer` / `PerFilterConfig::Buffer` / `Buffer` / `BufferPerRoute` do not exist.

- [ ] **Step 3: Add the `Buffer` + `BufferPerRoute` structs.**

Place them near the other per-filter config structs (after `CsrfPolicy`; search for `pub struct CsrfPolicy`). `max_request_bytes` is a **required, non-`Option` `u32`** (the residual disposition — absent → serde missing-field → fatal; `0` accepted; malformed → fatal):

```rust
/// `envoy.extensions.filters.http.buffer.v3.Buffer` — the chain-level buffer
/// config. `max_request_bytes` is a proto `UInt32Value` accepted on the wire as
/// a plain integer (ADR-0063 finding 2); modeled as a REQUIRED non-Option u32
/// (absent → serde missing-field error → fatal at startup, the ADR-0049
/// all-fatal posture; `0` is a valid limit → reject iff body.len() > 0).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Buffer {
    pub max_request_bytes: u32,
}

/// `envoy.extensions.filters.http.buffer.v3.BufferPerRoute` — the per-route
/// override. Envoy's `override` oneof is `{ disabled: bool; buffer: Buffer }`
/// (ADR-0063 finding 3); modeled as two fields where `disabled: true` bypasses
/// the filter for the route and `buffer` lowers/overrides the limit. An empty
/// `{}` (neither set) falls back to the chain-level base at apply time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BufferPerRoute {
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub buffer: Option<Buffer>,
}
```

- [ ] **Step 4: Add the `Buffer` variant to `HttpFilterTypedConfig` (`:741-771`, after the `Csrf` arm).**

```rust
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.buffer.v3.Buffer")]
    Buffer(Buffer),
```

- [ ] **Step 5: Add the `Buffer` variant to `PerFilterConfig` (`:791-796`, after the `Csrf` arm — the THIRD variant).**

```rust
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.buffer.v3.BufferPerRoute")]
    Buffer(BufferPerRoute),
```

- [ ] **Step 6: Add the validation arm (`:2905`, after the `Csrf` arm).**

The match over `HttpFilterTypedConfig` is exhaustive, so the new variant forces a new arm (compile error otherwise). Buffer needs no semantic validation beyond serde (the cors empty-config precedent `:2896`):

```rust
            crate::HttpFilterTypedConfig::Buffer(_cfg) => {
                if f.name != "envoy.filters.http.buffer" {
                    return Err(crate::ConfigError::UnsupportedHttpFilter {
                        name: f.name.clone(),
                    });
                }
                // Buffer.max_request_bytes is a required u32 (serde-enforced;
                // absent/malformed → fatal parse error); `0` is a valid limit.
                // No further validation (ADR-0063 — NO stats, NO new ConfigError).
            }
```

- [ ] **Step 7: Run the tests to verify they PASS.**

Run: `cargo test -p envoy-config buffer_ -- --nocapture`
Expected: PASS (all 7).

- [ ] **Step 8: Run the isolated-crate + full envoy-config build/test (per `project_isolated_crate_build_blindspot`).**

Run: `cargo build -p envoy-config && cargo test -p envoy-config`
Expected: clean + all green (the exhaustive-match guarantee means any missed arm fails to compile).

- [ ] **Step 9: Commit.**

```bash
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 25.2 Task 1: Buffer + BufferPerRoute config schema + validation arm [ADR-0063]"
```

---

## Task 2: `BufferFilter` runtime + unit tests (the decode-side backstop)

**Files:**
- Create: `crates/envoy-filter/src/buffer.rs`
- Modify: `crates/envoy-filter/src/lib.rs` (add `mod buffer;` + `pub use`).

- [ ] **Step 1: Write the module with failing-by-absence unit tests.**

Create `crates/envoy-filter/src/buffer.rs`. The filter mirrors `csrf.rs` structurally (a chain-level base + a per-request `effective` resolved from the route) but takes NO stats (ADR-0063). `build` is infallible. The unit tests ARE the decode-side backstop (heeds the phase-10 M1 lesson — the filter logic is proven independent of the fixture).

```rust
//! `envoy.filters.http.buffer` — decode-side request-body length guard.
//!
//! §6.2-verified against envoyproxy/envoy:v1.33.0 (phase-25 PLAN-write; ADR-0063).
//!
//! ## Behaviour summary
//! - The full request body is available as `FilterRequest.body` at
//!   `decode_headers` time (H1 via phase 25.1; H2 via the codec). The filter
//!   rejects iff `body.len() > effective_max_request_bytes` (strict `>`,
//!   ADR-0063 finding 6) with a 413 `Payload Too Large` local reply (17 bytes,
//!   no trailing newline; `content-type: text/plain` stamped by the H1/H2 synth
//!   decorators — the rbac/csrf precedent). Else `Continue` (the body flows
//!   upstream via phase 25.1 on H1 / the codec on H2).
//! - The chain-level `Buffer.max_request_bytes` is the BASE limit; a per-route
//!   `BufferPerRoute` (threaded via `apply_route_config`) either DISABLES the
//!   filter for the route or OVERRIDES the limit (ADR-0063 finding 3). A route
//!   with no buffer override keeps the chain base.
//! - Decode-side only; `encode_headers` is the trivial `Continue` arm. NO stats
//!   (ADR-0063 finding 4 — Envoy emits no buffer-scoped counters).
use bytes::Bytes;

use crate::pipeline::Decision;
use crate::types::{FilterRequest, FilterResponse};

const BUFFER_FILTER_NAME: &str = "envoy.filters.http.buffer";
/// ADR-0063 finding 1: the over-limit local-reply body, 17 bytes, NO newline.
const OVER_LIMIT_BODY: &[u8] = b"Payload Too Large";

/// The per-request effective policy resolved from the route (ADR-0063 finding 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Effective {
    /// `BufferPerRoute { disabled: true }` — the filter is bypassed for the route.
    Disabled,
    /// The effective byte limit (chain base, or a `BufferPerRoute` override).
    Limit(u32),
}

/// The `envoy.filters.http.buffer` runtime filter. Built once per filter-chain
/// from the chain-level `Buffer` (the base limit); `apply_route_config` selects
/// the per-request effective policy each request.
#[derive(Debug, Clone)]
pub struct BufferFilter {
    /// Chain-level base limit (`Buffer.max_request_bytes`).
    base_max: u32,
    /// Effective policy for the current request (route override if present, else
    /// the chain base).
    effective: Effective,
}

impl BufferFilter {
    /// Build from the chain-level `Buffer` config. Infallible — no stats to
    /// register (ADR-0063), no validation beyond the serde-enforced required u32.
    pub(crate) fn new(cfg: &envoy_config::Buffer) -> Self {
        Self {
            base_max: cfg.max_request_bytes,
            effective: Effective::Limit(cfg.max_request_bytes),
        }
    }

    /// Select the per-request effective policy: the route's `BufferPerRoute`
    /// override if present (disable / lowered limit), else the chain base.
    pub(crate) fn apply_route_config(&mut self, route: Option<&envoy_config::Route>) {
        self.effective = match route
            .and_then(|r| r.typed_per_filter_config.get(BUFFER_FILTER_NAME))
        {
            Some(envoy_config::PerFilterConfig::Buffer(bpr)) => {
                if bpr.disabled {
                    Effective::Disabled
                } else if let Some(b) = &bpr.buffer {
                    Effective::Limit(b.max_request_bytes)
                } else {
                    // Empty `{}` per-route → fall back to the chain base.
                    Effective::Limit(self.base_max)
                }
            }
            // No buffer per-route override (absent, or a different filter's
            // config) → the chain base still guards the route.
            _ => Effective::Limit(self.base_max),
        };
    }

    /// Decode-side entry point: reject an over-limit body with a 413.
    pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision {
        let limit = match self.effective {
            Effective::Disabled => return Decision::Continue,
            Effective::Limit(n) => n,
        };
        let body_len = req.body.as_ref().map_or(0, |b| b.len());
        // strict `>` (ADR-0063 finding 6); compare in u64 to avoid usize/u32
        // truncation on a > 4 GiB body.
        if body_len as u64 > u64::from(limit) {
            Decision::StopAndSend(over_limit_response())
        } else {
            Decision::Continue
        }
    }

    /// Buffer is decode-side only; encode is the trivial `Continue` arm (the
    /// exhaustive-match arm for the `HttpFilterInstance` wiring).
    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        Decision::Continue
    }
}

/// The 413 over-limit local reply (ADR-0063 finding 1). `content-type`,
/// `content-length`, `server`, `date`(, `connection`) are stamped by the H1/H2
/// synth decorators downstream of the pipeline (the rbac/csrf precedent).
fn over_limit_response() -> FilterResponse {
    FilterResponse {
        status: 413,
        reason: Some("Payload Too Large"),
        headers: Vec::new(),
        body: Bytes::from_static(OVER_LIMIT_BODY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn req_with_body(method: &str, path: &str, body: &[u8]) -> FilterRequest {
        FilterRequest {
            method: method.into(),
            path: path.into(),
            headers: vec![],
            body: if body.is_empty() {
                None
            } else {
                Some(Bytes::copy_from_slice(body))
            },
        }
    }

    fn route_with_buffer(pr: envoy_config::BufferPerRoute) -> envoy_config::Route {
        let mut pfc = BTreeMap::new();
        pfc.insert(
            BUFFER_FILTER_NAME.to_string(),
            envoy_config::PerFilterConfig::Buffer(pr),
        );
        envoy_config::Route {
            r#match: envoy_config::RouteMatch {
                prefix: Some("/".to_string()),
                path: None,
                headers: vec![],
            },
            action: envoy_config::RouteAction::DirectResponse(envoy_config::DirectResponse {
                status: 200,
                body: envoy_config::DataSource {
                    filename: None,
                    inline_string: None,
                },
            }),
            typed_per_filter_config: pfc,
        }
    }

    fn filter(max: u32) -> BufferFilter {
        BufferFilter::new(&envoy_config::Buffer {
            max_request_bytes: max,
        })
    }

    #[test]
    fn within_limit_continues() {
        let mut f = filter(10);
        f.apply_route_config(None);
        assert!(matches!(
            f.decode_headers(&mut req_with_body("POST", "/", b"hello")),
            Decision::Continue
        ));
    }

    #[test]
    fn at_limit_continues_strict_gt() {
        // ADR-0063 finding 6: reject is strictly `>`; exactly-limit → Continue.
        let mut f = filter(5);
        f.apply_route_config(None);
        assert!(matches!(
            f.decode_headers(&mut req_with_body("POST", "/", b"hello")), // 5 == 5
            Decision::Continue
        ));
    }

    #[test]
    fn over_limit_rejects_413_payload_too_large() {
        let mut f = filter(10);
        f.apply_route_config(None);
        match f.decode_headers(&mut req_with_body("POST", "/", b"hello world!!")) {
            Decision::StopAndSend(resp) => {
                assert_eq!(resp.status, 413);
                assert_eq!(resp.reason, Some("Payload Too Large"));
                assert_eq!(&resp.body[..], b"Payload Too Large");
                assert_eq!(resp.body.len(), 17);
            }
            _ => panic!("expected 413"),
        }
    }

    #[test]
    fn per_route_disabled_bypasses() {
        let mut f = filter(10);
        let route = route_with_buffer(envoy_config::BufferPerRoute {
            disabled: true,
            buffer: None,
        });
        f.apply_route_config(Some(&route));
        assert!(matches!(
            f.decode_headers(&mut req_with_body("POST", "/", b"way over the limit")),
            Decision::Continue
        ));
    }

    #[test]
    fn per_route_lowered_limit_rejects() {
        let mut f = filter(100);
        let route = route_with_buffer(envoy_config::BufferPerRoute {
            disabled: false,
            buffer: Some(envoy_config::Buffer {
                max_request_bytes: 4,
            }),
        });
        f.apply_route_config(Some(&route));
        // 5 > 4 → reject even though the chain base (100) would allow it.
        assert!(matches!(
            f.decode_headers(&mut req_with_body("POST", "/", b"hello")),
            Decision::StopAndSend(_)
        ));
    }

    #[test]
    fn per_route_empty_falls_back_to_chain_base() {
        let mut f = filter(10);
        let route = route_with_buffer(envoy_config::BufferPerRoute {
            disabled: false,
            buffer: None,
        });
        f.apply_route_config(Some(&route));
        assert!(matches!(
            f.decode_headers(&mut req_with_body("POST", "/", b"hello world!!")), // 13 > 10
            Decision::StopAndSend(_)
        ));
    }

    #[test]
    fn get_no_body_passes() {
        let mut f = filter(10);
        f.apply_route_config(None);
        assert!(matches!(
            f.decode_headers(&mut req_with_body("GET", "/", b"")),
            Decision::Continue
        ));
    }

    #[test]
    fn zero_limit_rejects_any_nonempty_body() {
        // Residual disposition: max_request_bytes: 0 → reject iff body.len() > 0.
        let mut f = filter(0);
        f.apply_route_config(None);
        assert!(matches!(
            f.decode_headers(&mut req_with_body("POST", "/", b"x")),
            Decision::StopAndSend(_)
        ));
        assert!(matches!(
            f.decode_headers(&mut req_with_body("GET", "/", b"")),
            Decision::Continue
        ));
    }
}
```

- [ ] **Step 2: Register the module in `crates/envoy-filter/src/lib.rs`.**

Find the `mod csrf;` and its `pub use csrf::CsrfFilter;` (or equivalent re-export) and add the buffer counterparts alongside:

```rust
mod buffer;
pub use buffer::BufferFilter;
```

(If `csrf` is exported via a grouped `pub use {...}` or a `pub(crate)` path, mirror that exact shape — match the existing convention rather than inventing a new one.)

- [ ] **Step 3: Run the buffer unit tests.**

Run: `cargo test -p envoy-filter buffer:: -- --nocapture`
Expected: PASS (all 8 tests).

- [ ] **Step 4: Commit.**

```bash
git add crates/envoy-filter/src/buffer.rs crates/envoy-filter/src/lib.rs
git commit -m "phase 25.2 Task 2: BufferFilter runtime + decode-side backstop unit tests [ADR-0063]"
```

---

## Task 3: Wire `HttpFilterInstance::Buffer` + a `FilterPipeline` in-process backstop

**Files:**
- Modify: `crates/envoy-filter/src/instance.rs` (the enum `:33`, build `:108`, decode `:138`, encode `:157`, apply_route_config `:180`, + a test).

- [ ] **Step 1: Add the `Buffer` variant to the `HttpFilterInstance` enum (`:67`, after `Csrf`).**

```rust
    /// Phase-25.2: the `envoy.filters.http.buffer` filter (decode-side request-
    /// body length guard; the chain-level `Buffer.max_request_bytes` is the base
    /// limit, optionally DISABLED or OVERRIDDEN per-route via `BufferPerRoute`
    /// through `apply_route_config`; over-limit → 413 `Payload Too Large`. NO
    /// stats — ADR-0063 finding 4).
    Buffer(BufferFilter),
```

Add `BufferFilter` to the `use` imports at the top of `instance.rs` (find the line importing `CsrfFilter` and add `BufferFilter` — likely `use crate::{… csrf::CsrfFilter, …};` or a `use crate::buffer::BufferFilter;`).

- [ ] **Step 2: Add the build dispatch arm (`:132`, after the `Csrf` arm). `Buffer::new` is infallible — no `?`.**

```rust
            envoy_config::HttpFilterTypedConfig::Buffer(cfg) => {
                Ok(HttpFilterInstance::Buffer(BufferFilter::new(cfg)))
            }
```

- [ ] **Step 3: Add the decode + encode dispatch arms (`:147` / `:166`, after each `Csrf` arm).**

In `decode_headers`:
```rust
            HttpFilterInstance::Buffer(f) => f.decode_headers(req),
```
In `encode_headers`:
```rust
            HttpFilterInstance::Buffer(f) => f.encode_headers(resp_arg),
```

- [ ] **Step 4: Add the `apply_route_config` arm (`:183`, after the `Csrf` arm).**

```rust
            HttpFilterInstance::Buffer(f) => f.apply_route_config(route),
```

- [ ] **Step 5: Write the in-process backstop test (the full config→pipeline→decision path, no Docker).**

Add to the `instance.rs` `#[cfg(test)] mod tests` (model the existing cors/csrf instance/pipeline tests — search for `FilterPipeline::build_from_config` usage, or the existing instance-level build tests). It builds a pipeline from a chain config that includes a `buffer` http_filter + a terminal `router`, then drives within-limit / over-limit / per-route-disabled / per-route-lowered FilterRequests through the pipeline's decode side, asserting `Continue` vs `StopAndSend(413 "Payload Too Large")`. This exercises `build` → `apply_route_config` → `decode_headers` end-to-end in-process (SPEC D6 / parent SPEC §6.4 backstop; covers ALL four dispositions).

```rust
#[test]
fn buffer_pipeline_backstop_all_dispositions() {
    use envoy_config::{Buffer, BufferPerRoute, PerFilterConfig, Route, RouteMatch,
        RouteAction, DirectResponse, DataSource};
    use std::collections::BTreeMap;

    // Build a [buffer(max=10), router] pipeline. Use the SAME constructor the
    // production HCM uses — FilterPipeline::build_from_config over an
    // http_filters Vec — so this proves the real build path, not a hand-rolled
    // instance. (If the test module already has a `pipeline_from_filters` /
    // `build_test_pipeline` helper for cors/csrf, reuse it; otherwise construct
    // the http_filters Vec [Buffer, Router] inline as the cors/csrf tests do.)
    let buffer_hf = envoy_config::HttpFilter {
        name: "envoy.filters.http.buffer".to_string(),
        typed_config: envoy_config::HttpFilterTypedConfig::Buffer(Buffer {
            max_request_bytes: 10,
        }),
    };
    let router_hf = envoy_config::HttpFilter {
        name: "envoy.filters.http.router".to_string(),
        typed_config: envoy_config::HttpFilterTypedConfig::Router(
            envoy_config::RouterConfig::default(),
        ),
    };
    let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
    let mut pipe = crate::FilterPipeline::build_from_config(
        &[buffer_hf, router_hf],
        &registry,
        "ingress_http",
    )
    .expect("pipeline builds");

    let mk_req = |body: &[u8]| envoy_filter_request("POST", "/", body);

    // (1) within-limit (no route override) → Continue (reaches the router).
    pipe.apply_route_config(None);
    assert!(matches!(
        pipe.decode_headers(&mut mk_req(b"hello")),
        Decision::Continue
    ));

    // (2) over-limit → StopAndSend 413 "Payload Too Large".
    pipe.apply_route_config(None);
    match pipe.decode_headers(&mut mk_req(b"hello world!!")) {
        Decision::StopAndSend(resp) => {
            assert_eq!(resp.status, 413);
            assert_eq!(&resp.body[..], b"Payload Too Large");
        }
        _ => panic!("expected 413"),
    }

    // (3) per-route disabled → Continue even when over the chain limit.
    let disabled_route = route_with_buffer_pr(BufferPerRoute { disabled: true, buffer: None });
    pipe.apply_route_config(Some(&disabled_route));
    assert!(matches!(
        pipe.decode_headers(&mut mk_req(b"way over the limit")),
        Decision::Continue
    ));

    // (4) per-route lowered (max=4) → 413 for a 5-byte body.
    let lowered_route = route_with_buffer_pr(BufferPerRoute {
        disabled: false,
        buffer: Some(Buffer { max_request_bytes: 4 }),
    });
    pipe.apply_route_config(Some(&lowered_route));
    assert!(matches!(
        pipe.decode_headers(&mut mk_req(b"hello")),
        Decision::StopAndSend(_)
    ));

    // Helper: a Route carrying a BufferPerRoute under the buffer filter name.
    fn route_with_buffer_pr(pr: envoy_config::BufferPerRoute) -> Route {
        let mut pfc = BTreeMap::new();
        pfc.insert("envoy.filters.http.buffer".to_string(), PerFilterConfig::Buffer(pr));
        Route {
            r#match: RouteMatch { prefix: Some("/".to_string()), path: None, headers: vec![] },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 200,
                body: DataSource { filename: None, inline_string: None },
            }),
            typed_per_filter_config: pfc,
        }
    }
}
```

> **Implementer note:** the exact `FilterPipeline::build_from_config` signature, the `HttpFilter` struct fields (`name` + `typed_config`), the `RouterConfig` constructor, and the `envoy_filter_request(...)` / `mk_req` request-builder MUST match what the cors/csrf instance tests already use — find one cors/csrf pipeline test in `instance.rs` (or `pipeline.rs`) and copy its construction idiom verbatim rather than guessing. If `FilterPipeline::decode_headers` is not the public driver the existing tests use, use whatever decode-driver they use (e.g. iterating instances). Do NOT invent a new pipeline entry point. The four assertions above are the contract; adapt the scaffolding to the existing test helpers.

- [ ] **Step 6: Run the envoy-filter suite.**

Run: `cargo test -p envoy-filter`
Expected: PASS (the new backstop + all pre-existing filter tests; the exhaustive dispatch matches guarantee no arm was missed).

- [ ] **Step 7: Commit.**

```bash
git add crates/envoy-filter/src/instance.rs
git commit -m "phase 25.2 Task 3: wire HttpFilterInstance::Buffer + in-process pipeline backstop"
```

---

## Task 4: M25.1-1 (bound the H1 body allocation) + M25.1-2 (cross-TCP-segment forwarding test)

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (the `25.1` body-read block `:630-667`; the test module).

- [ ] **Step 1: Write the M25.1-2 cross-segment forwarding test (it must PASS already — the production loop is correct — but it currently has ZERO coverage; this pins it).**

Add to the `hcm.rs` test module (next to `h1_forwards_request_body_upstream`). It writes the request head, flushes, sleeps so the head and body land in SEPARATE reads (`from_buf < body_len` → the `while remaining > 0` loop runs), then writes the body, and asserts the upstream received the full forwarded body. Use the existing `spawn_capturing_upstream`, `cluster_mgr_with_endpoint`, `hcm_config_with_cluster`; drive a raw `TcpStream` to the HCM-served listener (model the connection-driving idiom `drive`/`drive_keep_alive` already use — find the helper that opens a `TcpStream` to the listener and write to it directly so the head/body can be split across `write_all` calls).

```rust
#[tokio::test(flavor = "multi_thread")]
async fn h1_forwards_body_split_across_tcp_segments() {
    // 25.1 M25.1-2: head and body arrive in SEPARATE reads, so the body-read
    // reassembly loop (`while remaining > 0`) actually runs. Assert the upstream
    // still receives the full forwarded body.
    let (upstream_port, captured) =
        spawn_capturing_upstream(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;
    let cluster_mgr = cluster_mgr_with_endpoint("backend", upstream_port).await;
    let cfg = hcm_config_with_cluster(
        "/",
        RouteAction::Route(RouteAction_Route { cluster: "backend".into(), retry_policy: None }),
        cluster_mgr,
    );
    // Serve the HCM on a listener and drive a split-write client. Reuse the test
    // module's listener-serving idiom (the one `drive`/`drive_keep_alive` use):
    //   let (addr, _h) = serve_hcm(cfg).await;     // whatever the module calls it
    //   let mut s = TcpStream::connect(addr).await.unwrap();
    //   s.write_all(b"POST /seg HTTP/1.1\r\nHost: x\r\nContent-Length: 11\r\nConnection: close\r\n\r\n").await.unwrap();
    //   s.flush().await.unwrap();
    //   tokio::time::sleep(Duration::from_millis(50)).await; // force a segment boundary
    //   s.write_all(b"hello world").await.unwrap();
    //   read the response to EOF...
    let resp = drive_split(
        cfg,
        b"POST /seg HTTP/1.1\r\nHost: x\r\nContent-Length: 11\r\nConnection: close\r\n\r\n",
        b"hello world",
    )
    .await;
    assert!(
        String::from_utf8_lossy(&resp).starts_with("HTTP/1.1 200 OK\r\n"),
        "downstream got 200"
    );
    let got = String::from_utf8_lossy(&captured.lock().unwrap()).to_string();
    assert!(got.starts_with("POST /seg HTTP/1.1\r\n"), "upstream got the request: {got}");
    assert!(got.ends_with("hello world"), "upstream got the reassembled body: {got}");
}
```

> **Implementer note:** if the test module has no `drive_split` (head, body) two-write helper, add a minimal one next to `drive`: open one `TcpStream` to the same HCM-served listener `drive` uses, `write_all(head)`, `flush`, `sleep(50ms)`, `write_all(body)`, then read the response to EOF. Reuse `drive`'s exact listener-serving idiom (the `serve_hcm`/equivalent it calls); do NOT invent a new HCM entry point. The 50 ms sleep forces the kernel to deliver head and body in distinct reads on loopback.

- [ ] **Step 2: Run the new test to verify it PASSES (the production loop is already correct).**

Run: `cargo test -p envoy-http1 h1_forwards_body_split_across_tcp_segments -- --nocapture`
Expected: PASS — proves the reassembly path forwards the full body. (If it FAILS, the split is not actually crossing a read boundary — increase the sleep or the body size past one MSS; do not change production code to satisfy it.)

- [ ] **Step 3: Apply the M25.1-1 allocation bound.**

In the `25.1` body-read block, add the cap constant near the other `hcm.rs` consts (e.g. next to `IDLE_READ_TIMEOUT`):

```rust
/// 25.2 M25.1-1: cap the UP-FRONT body-buffer reservation so an untrusted,
/// uncapped client `Content-Length` cannot trigger a proportional allocation
/// before any body byte arrives (a client sending only `Content-Length:
/// 4000000000` and no body would otherwise reserve ~4 GB). The buffer still
/// grows on demand via `extend_from_slice`, so the bytes actually buffered are
/// unchanged — this bounds the RESERVATION, not the read. A true per-request
/// cap tied to the buffer filter's effective limit is a deferred non-goal (the
/// effective limit is resolved later in the pipeline, not at this read site).
const INITIAL_BODY_BUF_CAP: usize = 64 * 1024;
```

Then change the reservation at `:641` from:
```rust
            let mut body_buf = BytesMut::with_capacity(body_len);
```
to:
```rust
            let mut body_buf = BytesMut::with_capacity(body_len.min(INITIAL_BODY_BUF_CAP));
```

This is behavior-preserving: for any real (small) body `body_len <= INITIAL_BODY_BUF_CAP`, so `.min` is a no-op and the allocation is byte-identical to before; for a large declared `Content-Length` it reserves at most 64 KiB up front and grows as bytes arrive. Also update the stale comment at `hcm.rs:599` ("Compute body length (for drain)") to "(for the body read + the M25.1-1 reservation bound)" (M25.1-7, cosmetic — closes the `25.1` REVIEW note).

- [ ] **Step 4: Add a regression test that a multi-KB body (exceeding the 64 KiB reservation is impractical to send in a unit test; use a body larger than a single 4 KiB read chunk) is still forwarded byte-exact — proving grow-on-demand.**

```rust
#[tokio::test(flavor = "multi_thread")]
async fn h1_forwards_large_body_grows_on_demand() {
    // 25.2 M25.1-1: a body larger than one 4 KiB read chunk is forwarded
    // byte-exact even though the up-front reservation is now bounded — proves
    // `extend_from_slice` grows the buffer correctly.
    let (upstream_port, captured) =
        spawn_capturing_upstream(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;
    let cluster_mgr = cluster_mgr_with_endpoint("backend", upstream_port).await;
    let cfg = hcm_config_with_cluster(
        "/",
        RouteAction::Route(RouteAction_Route { cluster: "backend".into(), retry_policy: None }),
        cluster_mgr,
    );
    let body = vec![b'z'; 10_000]; // > one 4 KiB read chunk
    let mut req = format!(
        "POST /big HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    req.extend_from_slice(&body);
    let resp = drive(cfg, &req).await;
    assert!(String::from_utf8_lossy(&resp).starts_with("HTTP/1.1 200 OK\r\n"));
    let got = captured.lock().unwrap().clone();
    assert!(got.ends_with(&body), "upstream received the full 10 KB body verbatim");
}
```

> **Implementer note:** `drive` takes `&[u8]`; if its signature is `&'static [u8]`, add a sibling `drive_owned(cfg, &[u8])` next to it (same body, owned slice) rather than leaking a `Vec`. Match the existing `drive` connection idiom exactly.

- [ ] **Step 5: Run the envoy-http1 suite.**

Run: `cargo test -p envoy-http1`
Expected: PASS (the two new tests + all `25.1` tests still green — the allocation bound is behavior-preserving).

- [ ] **Step 6: Commit.**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 25.2 Task 4: M25.1-1 bound H1 body allocation + M25.1-2 cross-segment forwarding test"
```

---

## Task 5: Differential harness — `Http1Probe.body` + `drive_http1` body support

**Files:**
- Modify: `tests/differential/src/lib.rs` (the `Http1Probe` struct `:817`, `drive_http1` `:1526`, the `http1_probe_list` driver arm, and all `drive_http1` call sites).

- [ ] **Step 1: Write a failing harness unit test that drives a POST WITH a body to an in-process echo and asserts the body was sent.**

Add to the `tests/differential/src/lib.rs` `#[cfg(test)] mod tests` (or a sibling test file if the harness keeps tests out-of-line — match the existing convention). It spins a tiny in-process TCP server that records the bytes it received, calls `drive_http1` with a body, and asserts the recorded request ends with the body.

```rust
#[tokio::test]
async fn drive_http1_sends_request_body() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let recorded = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    let rec = recorded.clone();
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        // read until a short idle (the small request has fully arrived)
        loop {
            match tokio::time::timeout(Duration::from_millis(200), sock.read(&mut buf)).await {
                Ok(Ok(0)) | Ok(Err(_)) | Err(_) => break,
                Ok(Ok(n)) => rec.lock().unwrap().extend_from_slice(&buf[..n]),
            }
        }
        let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;
        let _ = sock.shutdown().await;
    });
    let _ = drive_http1(addr, &Http1Method::Post, "/", "x.test", &[], Some(b"hello")).await;
    let got = String::from_utf8_lossy(&recorded.lock().unwrap()).to_string();
    assert!(got.contains("Content-Length: 5"), "driver set content-length: {got}");
    assert!(got.ends_with("hello"), "driver appended the body: {got}");
}
```

- [ ] **Step 2: Run it to verify it FAILS to compile (`drive_http1` has no `body` param).**

Run: `cargo test -p differential drive_http1_sends_request_body`
Expected: FAIL — arity mismatch.

- [ ] **Step 3: Add the `body` param to `drive_http1` (`:1526`).**

Change the signature to add `body: Option<&[u8]>` (last param), and assemble it after the headers — set `Content-Length` from the body length (only when a body is present), then append the body bytes after the `\r\n\r\n` terminator:

```rust
pub async fn drive_http1(
    addr: SocketAddr,
    method: &Http1Method,
    path: &str,
    host: &str,
    extra_headers: &[(String, String)],
    body: Option<&[u8]>,
) -> Result<DriveHttp1Result> {
    use tokio::net::TcpStream;
    let mut stream = TcpStream::connect(addr)
        .await
        .with_context(|| format!("connecting to {addr}"))?;
    let mut head = format!("{} {} HTTP/1.1\r\nHost: {}\r\n", method.as_str(), path, host);
    for (n, v) in extra_headers {
        head.push_str(&format!("{n}: {v}\r\n"));
    }
    if let Some(b) = body {
        head.push_str(&format!("Content-Length: {}\r\n", b.len()));
    }
    head.push_str("Connection: close\r\n\r\n");
    let mut wire = head.into_bytes();
    if let Some(b) = body {
        wire.extend_from_slice(b);
    }
    stream.write_all(&wire).await?;
    // ... (the response-read loop below is unchanged) ...
```

(Leave the entire response-read body from `let mut buf …` onward unchanged.)

- [ ] **Step 4: Add the `body` field to `Http1Probe` (`:817`).**

```rust
    /// Optional request body (sent after the headers; the driver auto-adds
    /// `Content-Length`). A probe with a body MUST NOT also list `content-length`
    /// in `extra_headers` (the driver sets it). `None` → bodyless request.
    #[serde(default)]
    pub body: Option<String>,
```

- [ ] **Step 5: Thread `probe.body` through the `http1_probe_list` driver arm + fix all other call sites.**

In the `http1_probe_list` match arm (grep `http1_probe_list`), at the `drive_http1(...)` call, pass `probe.body.as_deref().map(str::as_bytes)` as the new last arg. Then `grep -n 'drive_http1(' tests/differential/src/lib.rs` and update EVERY other call site (admin scrapes, other driver arms) to pass `None` as the new last arg (mechanical compile fix).

- [ ] **Step 6: Run the harness test + build the differential crate (`--no-run` keeps it cheap).**

Run: `cargo test -p differential drive_http1_sends_request_body`
Run: `cargo test -p differential --no-run`
Expected: the unit test PASSES; the crate builds (all `drive_http1` call sites fixed).

- [ ] **Step 7: Commit.**

```bash
git add tests/differential/src/lib.rs
git commit -m "phase 25.2 Task 5: harness — Http1Probe.body + drive_http1 request-body support"
```

---

## Task 6: Fixture `0033-http-filter-buffer` + the differential acceptance test

**Files:**
- Create: `tests/fixtures/0033-http-filter-buffer/{envoy.yaml, envoy-rust.yaml, expectations.yaml, inputs/.gitkeep, README.md}`
- Create: `tests/differential/tests/http_filter_buffer.rs`

The fixture is modeled on `0032-http-filter-csrf` (same per-side YAML asymmetry, same real `http1-echo-server` cluster, same `request_headers_to_remove` on the Envoy side so the within-limit echo bodies are byte-equal cross-proxy). The HCM `http_filters` chain is `[envoy.filters.http.buffer (max_request_bytes: 10), envoy.filters.http.router]`. Three routes (first-match order — specific prefixes BEFORE the catch-all): `/disabled` (`BufferPerRoute { disabled: true }`), `/small` (`BufferPerRoute { buffer: { max_request_bytes: 4 } }`), `/` (chain base 10). Five probes:

1. `POST /` body `hello` (5 ≤ 10) → **200** + echoed body (PROVES the `25.1` H1 body forwarding differentially).
2. `POST /` body `hello world!!` (13 > 10) → **413** `Payload Too Large`.
3. `POST /disabled` body `hello world!!` (13, route disables the filter) → **200** + echoed body.
4. `POST /small` body `hello` (5 > the route's lowered limit 4) → **413**.
5. `GET /` (no body) → **200** passthrough echo.

- [ ] **Step 1: Create `tests/fixtures/0033-http-filter-buffer/envoy-rust.yaml`.**

```yaml
# Phase-25.2 envoy-rust counterpart for fixture 0033-http-filter-buffer.
# Identical HCM shape to envoy.yaml modulo bind address (127.0.0.1 per the
# 0017/0031/0032 precedent), the absent admin block, and the absent
# request_headers_to_remove (envoy-rust does not inject the Envoy header suite,
# so stripping is a no-op; the parser rejects unknown fields). The
# http_filters chain [buffer(max_request_bytes: 10), router] and the per-route
# typed_per_filter_config (BufferPerRoute disable on /disabled, lowered limit 4
# on /small) are byte-identical to envoy.yaml, so the 200/413 split is
# deterministic cross-proxy. References: ADR-0062 (SPEC), ADR-0063 (PLAN-write).
node:
  cluster: phase-25-cluster
  id: phase-25-envoy-rust
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {{PORT}}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: default
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/disabled" }
                          route: { cluster: backend }
                          typed_per_filter_config:
                            envoy.filters.http.buffer:
                              "@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.BufferPerRoute
                              disabled: true
                        - match: { prefix: "/small" }
                          route: { cluster: backend }
                          typed_per_filter_config:
                            envoy.filters.http.buffer:
                              "@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.BufferPerRoute
                              buffer:
                                max_request_bytes: 4
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.buffer
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.Buffer
                      max_request_bytes: 10
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: {{BACKEND_HOST}}, port_value: {{HTTP1_BACKEND_PORT}} } }
```

- [ ] **Step 2: Create `tests/fixtures/0033-http-filter-buffer/envoy.yaml` (the upstream-Envoy side).**

Mirror `0032`'s `envoy.yaml` asymmetry verbatim: add the `admin` block (port 0), `generate_request_id: false`, `request_headers_to_remove` (the 6-header Envoy-injected list), bind `0.0.0.0:{{PORT}}`, cluster `dns_lookup_family: V4_ONLY`, and the same routes + filter chain as `envoy-rust.yaml`.

```yaml
# Phase-25.2 differential acceptance fixture: drive 5 sequential HTTP/1.1
# requests through an HCM whose http_filters chain is
#   [envoy.filters.http.buffer, envoy.filters.http.router]
# with a chain-level Buffer { max_request_bytes: 10 } and per-route
# BufferPerRoute overrides (disable on /disabled, lowered limit 4 on /small),
# proxying to a real http1-echo-server upstream (ADR-0063 finding 8: a
# within-limit request must reach a real upstream to yield a body-echoing 200;
# direct_response engages neither per-route filter config nor body forwarding).
#   1. post-within-limit  — POST / body 5B (<=10) → 200, echo body
#   2. post-over-limit     — POST / body 13B (>10) → 413 "Payload Too Large"
#   3. post-route-disabled — POST /disabled body 13B (filter disabled) → 200, echo
#   4. post-route-lowered  — POST /small body 5B (>route limit 4) → 413
#   5. get-no-body         — GET / (no body) → 200 passthrough echo
# Per-side YAML asymmetry follows the 0031/0032 precedent (admin; 0.0.0.0 bind;
# generate_request_id: false; request_headers_to_remove stripping the Envoy-
# injected upstream headers so the echo-server body is byte-equal cross-proxy;
# cluster dns_lookup_family: V4_ONLY; {{BACKEND_HOST}} = host.docker.internal).
# References: ADR-0062 (SPEC), ADR-0063 (PLAN-write — wire shapes + 413 body).
admin:
  address:
    socket_address:
      address: 0.0.0.0
      port_value: 0
node:
  cluster: phase-25-cluster
  id: phase-25-envoy
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address:
          address: 0.0.0.0
          port_value: {{PORT}}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                generate_request_id: false
                route_config:
                  name: default
                  request_headers_to_remove:
                    - x-forwarded-for
                    - x-forwarded-proto
                    - x-request-id
                    - x-envoy-expected-rq-timeout-ms
                    - x-envoy-internal
                    - x-envoy-external-address
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/disabled" }
                          route: { cluster: backend }
                          typed_per_filter_config:
                            envoy.filters.http.buffer:
                              "@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.BufferPerRoute
                              disabled: true
                        - match: { prefix: "/small" }
                          route: { cluster: backend }
                          typed_per_filter_config:
                            envoy.filters.http.buffer:
                              "@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.BufferPerRoute
                              buffer:
                                max_request_bytes: 4
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.buffer
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.Buffer
                      max_request_bytes: 10
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STRICT_DNS
      dns_lookup_family: V4_ONLY
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: {{BACKEND_HOST}}, port_value: {{HTTP1_BACKEND_PORT}} } }
```

- [ ] **Step 3: Create `tests/fixtures/0033-http-filter-buffer/expectations.yaml`.**

The 413 bodies are asserted byte-exact per-probe; the 200 echo bodies are compared cross-proxy via the top-level `equivalence.response_body` (byte_exact) — both proxies forward the identical upstream request (Envoy strips its injected headers; envoy-rust does not inject them) → identical echo.

```yaml
# Phase-25.2: 5-probe sequential burst against the HCM filter chain
#   [envoy.filters.http.buffer, envoy.filters.http.router]
# Buffer chain limit 10; per-route disable on /disabled; per-route limit 4 on
# /small. Over-limit (body.len() > effective_max, strict >) → 413 with body
# "Payload Too Large" (17 bytes, no newline; content-type: text/plain
# auto-added by the H1 filter-synth helper). Within-limit → 200 + echoed body.
# Status sequence → [200, 413, 200, 413, 200].
driver:
  kind: http1_probe_list
  probes:
    - name: probe-1-post-within-limit-200
      method: post
      path: "/"
      host: "buffer.test"
      body: "hello"            # 5 bytes <= chain limit 10
      expected_status: 200
      expected_headers: set_equal_modulo_allow_list
    - name: probe-2-post-over-limit-413
      method: post
      path: "/"
      host: "buffer.test"
      body: "hello world!!"    # 13 bytes > chain limit 10
      expected_status: 413
      expected_body: { kind: byte_exact, body: "Payload Too Large" }
      expected_headers: set_equal_modulo_allow_list
    - name: probe-3-post-route-disabled-200
      method: post
      path: "/disabled"
      host: "buffer.test"
      body: "hello world!!"    # 13 bytes, but the route disables the filter
      expected_status: 200
      expected_headers: set_equal_modulo_allow_list
    - name: probe-4-post-route-lowered-413
      method: post
      path: "/small"
      host: "buffer.test"
      body: "hello"            # 5 bytes > the route's lowered limit 4
      expected_status: 413
      expected_body: { kind: byte_exact, body: "Payload Too Large" }
      expected_headers: set_equal_modulo_allow_list
    - name: probe-5-get-no-body-200
      method: get
      path: "/"
      host: "buffer.test"
      expected_status: 200
      expected_headers: set_equal_modulo_allow_list
equivalence:
  response_status: exact
  response_body: { kind: byte_exact }
```

- [ ] **Step 4: Create `tests/fixtures/0033-http-filter-buffer/inputs/.gitkeep` (empty) and `README.md`.**

The `README.md` mirrors `0032`'s: a 1-paragraph description of the chain, the 5 probes, the per-side YAML asymmetry rationale, and the ADR cross-references (ADR-0062 / ADR-0063). (Copy `0032/README.md` and adapt the filter name, the limits, the probe table, and the 413 body.)

- [ ] **Step 5: Create the differential acceptance test `tests/differential/tests/http_filter_buffer.rs` (mirror `http_filter_csrf.rs`).**

```rust
//! Phase 25.2 differential acceptance test for fixture 0033-http-filter-buffer.
//! Drives 5 sequential HTTP/1.1 requests (`Host: buffer.test`) over an HTTP/1.1
//! listener through an HCM whose `http_filters` chain is
//! `[envoy.filters.http.buffer, envoy.filters.http.router]` with a chain-level
//! `Buffer { max_request_bytes: 10 }` and per-route `BufferPerRoute` overrides
//! (disable on `/disabled`; lowered limit 4 on `/small`), proxying to the real
//! `http1-echo-server` upstream (ADR-0063 finding 8). Both proxies must produce
//! the deterministic status sequence `[200, 413, 200, 413, 200]`:
//!   1. post-within-limit  — POST / 5B  (<=10) → 200, echo body
//!   2. post-over-limit     — POST / 13B (>10) → 413 "Payload Too Large"
//!   3. post-route-disabled — POST /disabled 13B (disabled) → 200, echo body
//!   4. post-route-lowered  — POST /small 5B (>4) → 413 "Payload Too Large"
//!   5. get-no-body         — GET / (no body) → 200 passthrough echo
//! The 413 body (`Payload Too Large`, 17 bytes, no newline) is byte-exact
//! cross-proxy (asserted per-probe); the 200 echo bodies are compared via the
//! top-level `equivalence.response_body` (byte_exact). Docker-gated by the
//! harness (skips when DOCKER_HOST is unavailable).
use std::path::PathBuf;

#[tokio::test]
async fn http_filter_buffer_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0033-http-filter-buffer");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
```

- [ ] **Step 6: Build the differential crate (do NOT run the Docker suite — that is the state-4 gate).**

Run: `cargo test -p differential --no-run`
Expected: builds (the new test compiles; the fixture files are data). The actual Docker differential run is the state-4 `superpowers:verification-before-completion` gate next session.

- [ ] **Step 7: Commit.**

```bash
git add tests/fixtures/0033-http-filter-buffer/ tests/differential/tests/http_filter_buffer.rs
git commit -m "phase 25.2 Task 6: fixture 0033-http-filter-buffer + differential acceptance test"
```

---

## Task 7: BEHAVIOR_CONTRACT 413 row + `parse_bootstrap` fuzz seed + state-3 workspace gates

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (add the buffer local-reply row after the CSRF section, ~`:565`).
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/seed-buffer.yaml`.

- [ ] **Step 1: Add the buffer 413 local-reply row to BEHAVIOR_CONTRACT.md.**

Insert after the CSRF local-reply wire-shape block (after ~`:565`, before `## Admin endpoint body shapes` `:629`):

```markdown
**25 entries (Buffer filter).**

> The HTTP-filter-family seventh phase (ADR-0062 SPEC / ADR-0063 PLAN-write).
> `envoy.filters.http.buffer` is a decode-side request-body length guard. With
> the full request body available as `FilterRequest.body` (H1 via phase 25.1;
> H2 via the codec), the filter rejects iff `body.len() > effective_max_request_bytes`
> (strict `>`, ADR-0063 finding 6) with a 413 local reply; else the body flows
> upstream. The effective limit is the chain-level `Buffer.max_request_bytes`,
> optionally DISABLED or OVERRIDDEN per-route via `BufferPerRoute`
> (`apply_route_config` — the third per-route `typed_per_filter_config` consumer
> after cors + csrf). **NO buffer-scoped stats** (ADR-0063 finding 4 — Envoy
> v1.33 emits none; the over-limit 413 is reflected only in the generic HCM
> `downstream_rq_too_large`, not asserted by the fixture).

**Buffer over-limit local-reply wire shape (ADR-0063 finding 1).**

- Status: **413** (`Payload Too Large`).
- Body: **`Payload Too Large`** — exactly **17 bytes**, NO trailing newline
  (hex `50 61 79 6c 6f 61 64 20 54 6f 6f 20 4c 61 72 67 65`). Set verbatim by
  `BufferFilter` via `Bytes::from_static`.
- `content-type: text/plain` + `content-length: 17` are stamped by the H1/H2
  synth decorators (`decorate_filter_synth_response{,_h2}`) — the rbac/csrf
  precedent (non-empty filter local reply → `content-type` added only-if-missing).
- Verified byte-exact at BOTH the chain level AND a `BufferPerRoute`-lowered
  per-route limit against `envoyproxy/envoy:v1.33.0` (ADR-0063 finding 1).
```

- [ ] **Step 2: Create the `parse_bootstrap` fuzz seed.**

Write `crates/envoy-config/fuzz/corpus/parse_bootstrap/seed-buffer.yaml` — a valid bootstrap exercising `Buffer` (chain) + `BufferPerRoute` (both the `disabled` and `buffer` oneof arms) so the fuzzer's mutation surface covers the new parse paths:

```yaml
node:
  id: buffer-fuzz-seed
  cluster: buffer-fuzz
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address: { address: 127.0.0.1, port_value: 8080 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: default
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/disabled" }
                          route: { cluster: backend }
                          typed_per_filter_config:
                            envoy.filters.http.buffer:
                              "@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.BufferPerRoute
                              disabled: true
                        - match: { prefix: "/small" }
                          route: { cluster: backend }
                          typed_per_filter_config:
                            envoy.filters.http.buffer:
                              "@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.BufferPerRoute
                              buffer:
                                max_request_bytes: 4
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.buffer
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.Buffer
                      max_request_bytes: 10
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 9001 } }
```

- [ ] **Step 3: Sanity-check the seed parses (it must be a valid bootstrap, not just fuzz fodder).**

Run: `cargo run -p envoy-bin -- -c crates/envoy-config/fuzz/corpus/parse_bootstrap/seed-buffer.yaml --mode validate 2>&1 | head` *(if envoy-bin has a `--mode validate`; otherwise add a one-off `#[test]` in `bootstrap.rs` that `parse_bootstrap`-loads the seed file and asserts Ok, then delete it after confirming, OR rely on Task 1's parse tests which already cover the same shapes).*
Expected: the bootstrap validates (config OK).

- [ ] **Step 4: Run the state-3 workspace gates (the full §7.5 differential + fuzz short-run + `cargo deny` are the NEXT state).**

Run: `cargo build -p envoy-config -p envoy-filter -p envoy-http1` (isolated-crate, per `project_isolated_crate_build_blindspot`)
Run: `cargo build --workspace --all-targets`
Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Run: `cargo fmt --all -- --check`
Run: `cargo test --workspace` (run `-p envoy-bin` helpers standalone if the nested-cargo backstop flakes — `project_workspace_test_nested_cargo_backstop_flake`)
Expected: all clean. (`cargo deny check`, `cargo fuzz run parse_bootstrap` short-budget, and the Docker 33-fixture differential are the state-4 `superpowers:verification-before-completion` gate — NOT this state.)

- [ ] **Step 5: Commit.**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md crates/envoy-config/fuzz/corpus/parse_bootstrap/seed-buffer.yaml
git commit -m "phase 25.2 Task 7: BEHAVIOR_CONTRACT 413 row + parse_bootstrap buffer fuzz seed + state-3 gates"
```

---

## Self-Review (run after the plan is written; checklist, not a dispatch)

1. **Spec coverage (SPEC §3 D2–D6):**
   - **D2** (`BufferFilter` runtime, ninth variant, decode-side-only `body.len() > effective_max → 413` else Continue, NO stats) = **Task 2** (runtime + backstop unit tests) + **Task 3** (the `HttpFilterInstance::Buffer` variant + dispatch).
   - **D3** (`Buffer { max_request_bytes: u32 }` + `BufferPerRoute` oneof `{ disabled, buffer }` third `PerFilterConfig` variant + reuse `PerRouteConfigForAbsentFilter` verbatim + the absent/`0`/malformed disposition) = **Task 1** (schema + validation arm + the residual disposition pinned in the PLAN preamble; the generic absent-filter validator is reused with ZERO new code — confirmed at `bootstrap.rs:2728-2746`).
   - **D4** (`build` + `apply_route_config` dispatch; NO HCM change) = **Task 3**.
   - **D5** (BEHAVIOR_CONTRACT §2.2 the 413 row; §2.1 stats DROPPED) = **Task 7** (the 413 row; no stats row by ADR-0063).
   - **D6** (fixture `0033` + `http1_probe_list` body harness extension + fuzz seed + in-process backstop) = **Task 5** (harness body support) + **Task 6** (fixture + acceptance test) + **Task 7** (fuzz seed) + the in-process backstop split across **Task 2** (decode-side, isolated) and **Task 3** (config→pipeline→decision, all four dispositions).
   - **M25.1-1 + M25.1-2** carry-forwards = **Task 4**.
   - SPEC §1 acceptance "all 33 Docker-gated fixtures green simultaneously" = the state-4 gate (next session), set up by Task 6 (fixture) + Task 7 (workspace gates). No SPEC requirement is unmapped.
2. **Placeholder scan:** every code step carries complete code. The three "implementer note" soft spots (Task 3 pipeline-test scaffolding; Task 4 `drive_split`/`drive_owned` helpers; Task 5 call-site fixups) each give a concrete fallback and name the exact existing idiom to copy — none is a "TODO". No `expected_body` is left to "verify later" (the 413 body is byte-locked by ADR-0063 finding 1).
3. **Type consistency:** `Buffer { max_request_bytes: u32 }` and `BufferPerRoute { disabled: bool, buffer: Option<Buffer> }` are used identically in Task 1 (schema), Task 2 (`BufferFilter::new` + tests), Task 3 (pipeline backstop), Task 6 (fixture YAML). `BufferFilter::new(&Buffer)` (infallible, no `?`) matches the Task-3 build arm. `over_limit_response()` → `FilterResponse { status: 413, reason: Some("Payload Too Large"), body: Bytes::from_static(b"Payload Too Large") }` is consistent between Task 2's definition and Task 2/3's assertions (17 bytes). `drive_http1(..., body: Option<&[u8]>)` (Task 5) is the signature the harness test (Task 5 Step 1) and the `http1_probe_list` arm call. `Http1Probe.body: Option<String>` → `probe.body.as_deref().map(str::as_bytes)` at the call site.

---

## Execution notes

- **State-3 is subagent-driven** (`feedback_execution_style`); dispatch implementers **SERIALLY** (`feedback_serial_subagent_dispatch` — parallel implementers race on shared `main` and this harness garbles large parallel tool batches). One task → one subagent → review → next.
- **No HCM change** (SPEC §3 D4): the per-route override threads through the EXISTING phase-23 `apply_route_config` hook. If a task tempts an HCM edit, stop — it is out of scope (the only `hcm.rs` change is the Task-4 M25.1-1/M25.1-2 hardening, which does NOT touch the filter dispatch).
- **The exhaustive match-arm discipline is your friend:** adding the `Buffer` variant to `HttpFilterTypedConfig` (Task 1) and `HttpFilterInstance` (Task 3) makes the compiler enumerate every dispatch/validation site that needs an arm — a missed arm is a compile error, not a silent gap.
- **Pre-build the helpers** before any Docker work at the state-4 gate (`project_flaky_access_log_fixture_0012`); not needed for this state's pure-Rust tests.
- After Task 7, phase 25.2 is at state-3-complete → the NEXT session runs **state-4** `superpowers:verification-before-completion` (the full §7.5 gate: workspace + clippy + fmt + test + `cargo deny` + `cargo fuzz run parse_bootstrap` short-budget + the Docker **33**-fixture differential LOCALLY per `feedback_state4_runs_docker_differential`, with the AUTHORITATIVE Linux CI anchor per ADR-0049 + the fixture flake family `project_flaky_access_log_fixture_0012`). Then state-5 review, then the state-6 close-out flips ROADMAP rows **`25.2` AND parent `25`** to `done`.
