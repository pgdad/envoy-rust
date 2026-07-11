# envoy-rust — Test Gap Analysis

> Author: unattended test-coverage audit session, 2026-07-11.
> Scope: analyse the tests that verify Envoy-compatible behaviour, find the
> highest-value gaps, and implement the ones that fit in one pass.
> Reference for "correct": upstream Envoy `v1.33.0` (the pinned differential
> target, `docs/envoy-rust/ENVOY_TARGET.md`), not this codebase's own assumptions.

This document is the Phase-3 checkpoint required by the audit prompt. The
"Implemented this pass" section at the end is filled in as work lands.

---

## 1. Baseline inventory

### 1.1 What the project is

An in-progress, phase-driven reimplementation of Envoy in Rust. Progress is
tracked by an on-disk state machine (`docs/envoy-rust/`). At audit time the trunk
(phases 00–08: listeners, TCP proxy, TLS/SNI, HTTP/1.1, HTTP/2, access log,
stats, filter framework, admin) is **done**, and many feature-family phases have
landed: HTTP filters (header_mutation, local_ratelimit, rbac, fault, jwt_authn,
cors, csrf, buffer, cdn_loop), load balancing (ring_hash, maglev, subset),
upstream robustness (active health checks, connection pooling, outlier detection,
circuit breakers, retries), file-based xDS (CDS/LDS/RDS/EDS + RDS/EDS hot reload),
observability (access-log command operators, json_format, dynamic metadata), and
the network-filters family opener (direct_response, rbac). The active phase
`67.2-network-rbac-connection-matchers` is **reviewed NOT APPROVED** over a known
Critical (see §2.1). There is **no gRPC-ADS xDS** — dynamic config is file/mtime
watch only. There is no HTTP/3/QUIC, no WASM.

### 1.2 Test inventory (counts)

| Layer | Count | Notes |
|---|---|---|
| Unit tests (`#[test]`/`#[tokio::test]`, inline) | ~1150 across 14 crates | See per-crate table below |
| `envoy-bin` integration tests (`crates/envoy-bin/tests/`) | 38 files | Boot the real `envoy-bin` process against a config |
| Differential fixtures (`tests/fixtures/NNNN-*`) | 73 (0001–0073) | Each paired with one `#[tokio::test]` in `tests/differential/tests/` |
| Conformance suites (`tests/conformance/`) | 1 (h2spec) | Pass-rate gate 0.95, one known-failure `3.5/2` |
| Fuzz targets (`crates/*/fuzz`) | 4 | `parse_bootstrap`, `jwt_parse`, `cdn_loop_parse`, `accesslog_format_parse` |
| Benchmarks (`bench/`) | 2 shell scripts | wrk-vs-Docker, manual, not in CI; uses Envoy v1.31 (differs from the v1.33 pin) |

Per-crate unit-test density (inline `#[test]`/`#[tokio::test]`):

| crate | src LoC | tests | thin/untested surface |
|---|---|---|---|
| envoy-config | 6.8k (+18k bootstrap) | 583 | `lib.rs` (1.2k) 0 inline tests |
| envoy-filter | 2.4k | 211 | — |
| envoy-cluster | 1.6k | 160 | well covered |
| envoy-http1 | 5.2k | 162 | **`pool.rs`/`client.rs` 0 inline; codec robustness gaps** |
| envoy-accesslog | 1.3k | 98 | — |
| envoy-admin | 1.5k | 97 | — |
| envoy-http2 | 1.9k (+5.9k hcm) | 89 | **flow-control / RST-flood / HPACK limits delegated to `h2`, unasserted** |
| envoy-bin | 1.5k | 138 (mostly integration) | — |
| envoy-listener | 1.4k | 54 | — |
| envoy-stats | 0.3k | 25 | — |
| envoy-tls | 1.1k | 15 | SNI-mismatch / ALPN-failure gaps |
| envoy-jwt | 0.4k | 12 | covered + fuzzed |
| envoy-tcp | 0.9k | 11 | — |
| envoy-health | 0.3k | 8 | — |

Property-based testing: **none** (no `proptest`/`quickcheck`). All fuzzing is
libFuzzer byte-slice fuzzing.

### 1.3 CI

`.github/workflows/ci.yml`, two jobs on every push/PR:
- **build** (30 min): fmt → clippy `-D warnings` → build → install h2spec 2.6.0 →
  pre-pull `envoyproxy/envoy:v1.33.0` → `cargo test --workspace` (runs the 73
  differential fixtures **and** the h2spec gate, both Docker-backed) → `cargo deny check`.
- **fuzz** (15 min): nightly, all 4 fuzz targets `-max_total_time=30` each.

Everything runs on every push; nothing is nightly-cron or manually gated.

### 1.4 Baseline suite result

`cargo test --workspace` on a **clean checkout fails 33 differential targets** —
but this is an environment artefact, **not** a product regression: the differential
harness shells out to helper binaries (`tcp-echo-server`, `http1-echo-server`, …)
and `target/debug/envoy-bin`, which do not exist until built. The harness's own
error message says so:

```
tcp-echo-server not found at .../target/debug/tcp-echo-server;
run `cargo build -p tcp-echo-server` or `cargo test --workspace`
```

After `cargo build --workspace --all-targets`, the differential + unit suites run
against Docker/Envoy as designed. **Finding B-0 (test-ergonomics):** running a
single differential test (`cargo test -p differential --test X`) does not build the
sibling helper crates it depends on, because they are separate workspace members
with no cargo artefact-dependency edge. The full `cargo test --workspace` builds
everything first, so CI is unaffected — but the failure mode is opaque to anyone
running a subset locally. Low priority; documented, not fixed this pass.

---

## 2. Gap analysis against real Envoy (prioritised)

Priorities weigh (a) risk of silent incorrectness vs Envoy, (b) breadth of code
exercised, (c) cost.

### 2.1 [CRITICAL, known] C-1 — `CidrRange` data-plane panic on IPv4-mapped-IPv6 prefixes

Already found by the phase-67.2 code review (`phases/67.2-.../REVIEW.md`), not yet
fixed. `CidrRange::validate` (`crates/envoy-config/src/bootstrap.rs:1646`) sizes
`prefix_len` against the **pre-canonical** address family, so
`address_prefix: "::ffff:127.0.0.0"` (parsed as IPv6) with `prefix_len` in
`33..=128` **passes startup validation**. But `CidrRange::contains` canonicalises a
v4-mapped-v6 address to a 4-byte IPv4 before `prefix_match` indexes `net[..full]`
with `full = prefix_len/8 = 5..16` — a config-reachable, release-mode **panic of the
connection task** on the first matching-ish connection, across all four network-RBAC
IP arms (`destination_ip`, `direct_remote_ip`, `remote_ip`, `source_ip`). Reproduced
end-to-end in the review. **Fix is small and prescribed:** canonicalise in `validate`
so a mapped prefix is bounded at 32 and `prefix_len > 32` is rejected fail-loud.
**I-1 (coverage cause):** the `parse_bootstrap` fuzzer reaches `validate` but never
`contains` (data-plane only), so gate (d) is structurally blind to this class.
→ **Implemented this pass** (§4): family-consistent `validate` + regression test +
a `contains`-level exhaustive property test that would have caught it.

### 2.2 [HIGH] Duplicate / conflicting `Content-Length` is silently accepted (request smuggling)

`parse_content_length` (`crates/envoy-http1/src/hcm.rs:1683`) uses `find_header`,
which returns the **first** matching header. A request with two `Content-Length`
rows of different values (`Content-Length: 5` / `Content-Length: 6`) takes the first
and treats the remainder of the second body as a pipelined next request — the classic
CL/CL smuggling vector. RFC 7230 §3.3.3 rule 4 requires rejection, and Envoy rejects
such requests with 400. **No test exercises this.** This is the single highest-value
correctness/security gap in the HTTP/1 codec. Fix is small and clearly correct
(reject conflicting values). → **Implemented this pass** (§4): reject with a codec
error mapped to the existing 400 synth path + unit tests. (Identical duplicate values
are tolerated, matching RFC "may combine".)

### 2.3 [MEDIUM] HTTP/1.1 codec robustness gaps

- **obs-fold (obsolete line folding):** no test; behaviour delegated to `httparse`
  and unasserted. Envoy rejects obs-fold in request headers (400).
- **Transfer-Encoding smuggling variants:** the `chunked` detector matches only the
  exact token `"chunked"` (`hcm.rs:840`), so `Transfer-Encoding: chunked, identity`
  or a whitespace/comment-obfuscated TE would **not** be seen as chunked and the
  request would fall through to Content-Length framing. Current design rejects any
  exact-`chunked` TE with 501; the obfuscated forms are untested. (TE+CL both present
  with exact `chunked` is safe today via the 501 rejection.)
- These are cheap unit tests over `Http1Codec::parse_request` + the HCM reject path.
  → **Partly implemented this pass** (§4): obs-fold + TE-variant documenting tests.

### 2.4 [MEDIUM] Slow-client idle-read timeout implemented but untested

`IDLE_READ_TIMEOUT = 5s` (`crates/envoy-http1/src/hcm.rs:23`) guards the request-body
read, but no test exercises the deadline (no `tokio::time::pause`, no slow-client
integration test). A regression that removed or lengthened the timeout would be
invisible. Deterministic testing needs `tokio::time` virtual clock or a controllable
slow client; moderate effort. → **Deferred** (ranked in §5); called out because the
code path is real and unguarded.

### 2.5 [MEDIUM] HTTP/2 abuse-resistance delegated to `h2`, unasserted

Flow control, RST-flood / rapid-reset (CVE-2023-44487), GOAWAY, and
`max_header_list_size` / HPACK limits are all left to the `h2` crate's defaults;
`build_h2_server` does not set `max_header_list_size`, and there is no guard test.
h2spec covers baseline protocol conformance at a 0.95 gate but does not test the
rapid-reset DoS. Verifying these properly is larger (needs a raw-frame H2 client or
extending h2spec); flagged, not done this pass.

### 2.6 [LOW] TLS SNI-mismatch and ALPN-failure not asserted

`envoy-tls` drives mismatching SNIs but only asserts the *matching* case succeeds
(`tests.rs:274`); the mismatch outcome is left as "may vary". No test forces an ALPN
negotiation failure. Small unit tests; deferred behind the higher-value HTTP items.

### 2.7 [LOW] Test-quality cleanups

- `crates/envoy-listener/src/lib.rs:2794` + `:2802` — two duplicate tautological
  tests both asserting `DRAIN_BUDGET == Duration::from_secs(5)` (a constant equals its
  own literal). Zero behavioural coverage.
- `crates/envoy-http2/src/codec.rs:53` — self-admitted no-op smoke test ("we just
  verify the call compiles"); the H2 protocol-options (`initial_window_size`,
  `max_frame_size`) have no wire-effect assertion.
- ~10 integration sites use fixed `sleep`-then-assert synchronisation (notably
  `upstream_outlier_detection.rs:338,374`, whose own header claims it avoids exactly
  that). Timing-flaky on slow CI. Deferred (churn-heavy, low correctness value).

### 2.8 Conformance posture (what's well covered — not gaps)

Malformed config rejection (strong: ~30 `rejects_*` tests + the `parse_bootstrap`
fuzzer), upstream connect-refused/reset at/before response, oversized headers (8 KiB
cap) and bodies (413), TLS handshake failures (empty/malformed/untrusted cert),
graceful drain + listener shutdown, connection-pool reuse/overflow, file-based xDS
hot reload, H1 keep-alive and header order/case preservation — all covered by real
socket-driven tests. The differential harness diffs status, body (byte-exact /
Prometheus-set / JSON-shape / text-lines), headers (set-equal modulo allow-list),
access-log records (token-mapped), and stats deltas against real Envoy — a genuine
conformance suite, not self-assertion.

---

## 3. Prioritised proposals

| # | Proposal | Verifies | Risk if untested | Effort | New infra? |
|---|---|---|---|---|---|
| P1 | Fix C-1 + regression + `contains` property test | v4-mapped-v6 CIDR never panics; validate/contains agree | Client-triggerable DoS shipped as "valid config" | S | No |
| P2 | Reject conflicting duplicate `Content-Length` + tests | RFC 7230 §3.3.3 / Envoy 400 on CL/CL smuggling | Request smuggling | S | No |
| P3 | H1 codec robustness tests (obs-fold, TE variants) | Documents + pins reject behaviour vs Envoy | Silent smuggling drift | S | No |
| P4 | Slow-client idle-read-timeout test (virtual clock) | 5s deadline still fires | Silent hang / resource leak | M | No |
| P5 | H2 rapid-reset / HPACK-limit guard + test | CVE-2023-44487 class resistance | DoS | L | Maybe (raw H2 client) |
| P6 | TLS SNI-mismatch / ALPN-failure asserts | Deterministic reject | Wrong cert served | S | No |
| P7 | Delete/​strengthen tautological + no-op tests | Test-suite signal quality | Wasted maintenance | S | No |

**This pass implements P1, P2, P3, P7** (all no-new-infra, highest value-per-cost).
P4/P5/P6 are ranked remaining work (§5). This respects the audit prompt's "at most
one new piece of test infrastructure" — in fact none is needed for the top items.

---

## 4. Implemented this pass

_(filled in as work lands — see final report at the end of this file)_

---

## 5. Ranked remaining recommended work

1. **P4 — slow-client idle-read-timeout test** using `tokio::time::pause`/`advance`
   over `read_request_body`. Deterministic, no sleeps. Guards a real hang path.
2. **P5 — HTTP/2 rapid-reset (CVE-2023-44487) + `max_header_list_size`**: set an
   explicit HPACK/header-list bound in `build_h2_server` and add a raw-frame test (or
   extend h2spec) that opens-and-resets N streams and asserts a bounded GOAWAY. Larger.
3. **P6 — TLS SNI-mismatch + ALPN-failure deterministic asserts** in `envoy-tls`.
4. **Upstream-reset-mid-body** differential/unit case (reset after partial DATA).
5. **Convert fixed `sleep`-then-assert integration sites to poll-with-deadline**,
   starting with `upstream_outlier_detection.rs:338,374`.
6. **Property/​fuzz coverage for other data-plane-only paths** now that P1 shows the
   `validate`-vs-runtime divergence pattern (e.g. header-mutation, route matching).
7. **Bench pin drift**: `bench/` uses Envoy v1.31 while the differential target is
   v1.33; align or document.
