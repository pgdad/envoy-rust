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

Seven commits, each a logical unit (fmt + clippy `-D warnings` clean, tests green):

| Area | Change | Tests added | Bug? |
|---|---|---|---|
| **C-1** `CidrRange` (envoy-config) | `validate` now sizes `prefix_len` against the **canonical** address family via a shared `canonical_ip` helper, so an IPv4-mapped-IPv6 `address_prefix` is bounded at /32 and an over-wide one is rejected fail-loud at config load; `prefix_match` gains a defensive non-panicking bounds bail | regression test (mapped /33–/128 rejected, `contains` never panics), a `contains`-level property sweep over every validate-passing prefix × a v4/v6/v4-mapped address matrix (I-1), and an end-to-end config-load rejection through `validate(&mut bootstrap)` | **Yes — known Critical, now fixed** |
| **P2** Content-Length (envoy-http1) | `parse_content_length` scans **all** `Content-Length` rows; conflicting values are rejected as `MalformedHeader` (no new response shape), identical repeats tolerated (RFC 7230 §3.3.3) | 5 unit tests + 2 end-to-end `drive` tests (conflicting → no response; identical-dup → 200) | **Yes — new, CL/CL smuggling** |
| **P3** Transfer-Encoding (envoy-http1) | chunked detection now matches a `chunked` token in **any** comma-separated position or across multiple TE rows, not only the exact value `"chunked"` | `has_chunked_transfer_encoding` unit test + end-to-end `chunked, gzip` → 501 (was silently CL-framed 200) + codec characterization test (obs-fold, space-in-name, NUL-in-value all rejected) | **Yes — new, TE/CL smuggling** |
| **P4** idle timeout (envoy-http1) | — (test only) | deterministic slow-client idle-read-timeout test using tokio's paused clock (no fixed sleep) — proves a stalled partial request is cleanly closed with no response | No (untested path now covered) |
| **P6** TLS SNI (envoy-tls) | — (test only) | end-to-end handshake-abort test: unknown SNI with no catch-all fails both client connect and server accept | No (untested path now covered) |

Each of the three bug fixes ships a test that was verified to **fail against the
pre-fix code** (the C-1 property test panics at the exact `prefix_match` index;
the CL and TE end-to-end tests return the wrong success status), so they are
genuine regression guards, not tautologies.

### Test-suite results before / after

- **Unit + `envoy-bin` integration (non-Docker):** before ~1150 unit tests green;
  after **1713 passed, 0 failed, 7 ignored** (`cargo test --workspace --exclude
  differential --exclude h2spec-conformance`). +19 new tests from this pass; the
  rest of the delta is the full integration set now counted.
- **Differential (Docker):** unchanged — the only failure is the pre-existing,
  host-dependent environmental RED where a backend resolves to an unroutable IPv6
  address (Envoy logs `UF`/`immediate_connect_error: Network is unreachable`,
  envoy-rust logs `UC`); documented in `STATE.md` as adjudicated. No fixture
  exercises duplicate-CL, TE-variant, or v4-mapped-CIDR inputs, so all fixtures
  stay green. Running the suite at full parallelism additionally surfaces ~8
  transient `accept-ready within 10s` timeouts (container/port contention, not
  product failures); at `--test-threads=4` only the one IPv6 RED remains.
- **fmt / clippy:** `cargo fmt --all -- --check` and `cargo clippy --workspace
  --all-targets --all-features -- -D warnings` both clean.

### Implementation bugs discovered

1. **C-1 (Critical, known, fixed):** config-reachable release-mode data-plane panic
   on an IPv4-mapped-IPv6 `CidrRange` prefix with `prefix_len > 32`.
2. **CL/CL request smuggling (new, fixed):** two conflicting `Content-Length` rows
   were silently framed on the first — the classic smuggling desync.
3. **TE/CL request smuggling (new, fixed):** a `Transfer-Encoding` value carrying a
   `chunked` token in a non-exact position (`chunked, gzip`) was not recognised as
   chunked and fell through to Content-Length framing instead of the 501 rejection.

All three are small, clearly-correct fixes that break no differential fixture; the
smuggling fixes reject strictly more malformed input than before and introduce no
new response shape (they reuse the existing `MalformedHeader` reject disposition).

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

---

# Second pass — 2026-07-17

> Author: unattended test-coverage audit session, 2026-07-17 (second pass).
> Baseline: `60a5272` (origin/main), 79 commits after the first pass's
> `96a1fd7`. Branch: `test-review-20260717`.

## 6. Baseline and new-surface inventory

**Baseline suite state at `60a5272`:** `cargo build --workspace --all-targets`
clean; fast suite (`cargo test --workspace --exclude differential --exclude
h2spec-conformance`) **1785 passed, 0 failed, 7 ignored** — green on a clean
checkout. (First pass ended at 1713; the delta is phases 68–70's own tests.)

**New surface since the first pass** (phases 67.3, 68, 69, 70):

| Phase | Surface | Test posture found |
|---|---|---|
| 67.3 | `FirstByteGate` / establishment-then-gate TCP RBAC (`envoy-tcp`) | Well covered: +309 lines of `network_filter_rbac.rs` integration tests incl. the C-1 regression witness (send-nothing client vs `direct_response`), FIN matrix, counter parity. |
| 68 | Active TCP health checking (`tcp_probe_once`, `receive_matches`, payload hex/base64 decode) | Well covered: pure-matcher units + socket-level probe tests + fixture 0074 + fuzz seed. Multi-block `receive` is documented as envoy-rust's own contract (not Envoy parity) — acceptable. |
| 69 | gRPC health checking (hand-rolled codec `envoy-http2/src/grpc.rs`, `grpc_probe_once`) | Well covered: encode/decode units incl. a **fuzz-found usize-overflow regression pin** (`decode_rejects_huge_length_delimited_field_without_overflow_panic`), loopback H2 verdict servers for Serving/NotServing/timeout/non-zero-grpc-status, fixture 0075, `grpc_health_decode` fuzz target wired into CI. |
| 70 (in-progress) | Access-log `status_code_filter` (`ComparisonFilter` EQ/GE/LE) | Well covered: `LogFilter` boundary units, H1+H2 per-sink emit gates with `access_logs_total` non-tick pins, config→runtime op-mapping table test, YAML-token→`ComparisonOp` serde pin (landed in `60a5272` — not redone here), fixture 0076 (byte-exact, no backend). |

Verdict on the new surface: the phase machine's TDD/review loop is producing
genuinely strong test coverage — no smuggling-class, panic-class, or
tautology-class gap was found in phases 67.3–70. The second pass therefore
went to the first pass's §5 ranked remaining work.

## 7. Implemented this pass

| Commit | Change | Bug? |
|---|---|---|
| `db633bd` | **P5 (top-ranked): H2 inbound header-list bound + abuse-resistance guards.** `build_h2_server` now pins `max_header_list_size` to a new `DEFAULT_MAX_HEADER_LIST_SIZE = 60 KiB` (Envoy's HCM `max_request_headers_kb` default). The `h2` crate's receive-side default is **16 MiB** — a ~273× wider per-stream memory-amplification window than Envoy grants, asymmetric with envoy-http1's 8 KiB cap. Guard tests: (a) a ~100 KiB header list draws h2's synthesized **431** — the same status upstream Envoy returns on this path — and the listener stays live (fail-first verified: without the bound the request is served 200); (b) 8 KiB is accepted (pins the bound's position); (c) **rapid-reset (CVE-2023-44487) flood guard** — 512 open+RST cycles must leave the listener serving a fresh connection; surviving OR GOAWAY-ing the flooding connection (h2 0.4.x's `DEFAULT_REMOTE_RESET_STREAM_MAX = 20` mitigation) are both acceptable bounded outcomes. No new infrastructure needed — the h2 client crate can drive both attacks. | **Yes — unbounded (16 MiB/stream) inbound H2 header lists, fixed** |
| `446627f` | **Item 4: upstream-reset-mid-body pins (H1 + H2).** Upstream sends 200 + partial CL body then FIN (H1) / 200 HEADERS + partial DATA then RST_STREAM (H2). Both classify as Reset in the buffered proxy → clean synth-503 downstream, no truncated-body leak. Tests **document the deliberate divergence** from streaming Envoy (which would forward the 200 and truncate) so a future move to streaming proxying revisits it consciously. | No (untested path pinned) |
| `45ec1bf` | **Item 7: bench pin drift.** `bench/*.sh` compared against `envoyproxy/envoy:v1.31-latest`; now `v1.33.0`, matching the conformance pin. | No |
| (docs commit) | `.gitignore` `/tools/` (the h2spec runner's documented local-binary drop location) + this section. | No |

**Item 5 (sleep→poll) re-adjudicated, not implemented:** the
`upstream_outlier_detection.rs:338,374` sites flagged in §2.7/§5 are in fact
already poll-with-deadline loops (the sleeps are poll *intervals* inside
`Instant::now() < deadline` loops — fine practice). The remaining fixed
sleeps in the integration tests (`network_filter_rbac.rs` 300–400 ms settles,
`SETTLE_MS` sites) mostly precede **assert-zero** checks (proving a counter
did NOT tick), which polling cannot replace — a fixed settle is inherent to
negative assertions. Withdrawn from the ranked list.

**P6 (ALPN) re-adjudicated:** `envoy-tls` contains no ALPN support at all —
an ALPN-failure test has nothing to exercise. This is a feature gap for the
phase machine, not a test gap; withdrawn (the SNI half was done in pass 1).

### Test-suite results (after)

- **Fast suite:** 1790 passed, 0 failed, 7 ignored (`cargo test --workspace
  --exclude differential --exclude h2spec-conformance`); +5 tests.
  fmt + clippy `-D warnings` clean.
  **Flake observed (pre-existing, F-2):** `network_filter_rbac::
  rules_omitted_is_inert_neither_counter_ticks` failed with
  `listener up: ConnectionRefused` in 2 of 3 full parallel runs on this
  loaded host (passes 5/5 in isolation). Root cause is
  `tests/common/mod.rs::reserve_port`'s bind-then-release TOCTOU: under
  parallel load the freed port can be re-taken before `envoy-bin` binds it,
  the data listener dies while admin survives, and `wait_ready` refuses for
  its whole 10 s budget. Every `spawn_envoy_bin`-based test shares the
  pattern; a proper fix needs `envoy-bin` to accept port 0 and advertise the
  bound address (e.g. via admin), which is phase-machine work — ranked below.
- **Differential (Docker, v1.33.0, `--no-fail-fast --test-threads=4`,
  against the rebuilt post-change binary):** **230 passed, 5 failed** — all
  5 are the pre-adjudicated environmental REDs enumerated in
  `docs/envoy-rust/phases/70-accesslog-status-code-filter/PROGRESS.md`
  (triage table): 4× the IPv6-unreachable close-backend divergence
  (`access_log_{rcd,rf}_upstream_reset`, `access_log_h2_{rcd,uc}_upstream_reset`;
  Envoy `UF`/`Network is unreachable` vs envoy-rust `UC`) and 1× the Docker
  bridge-IP `/clusters` divergence (`admin_config_dump_server_info`,
  `host.docker.internal` → `192.168.65.2`). Zero failures attributable to
  this pass; the new fixture 0076 and both new-phase fixtures 0074/0075 are
  green against the post-change binary. (Operational note: the first attempt
  aborted on a wedged Docker Desktop daemon — `docker run` hanging at
  container create — cleared by `systemctl --user restart docker-desktop`.)
- **h2spec (2.6.0, local binary in `tools/`):** `passed=145 failed=0
  total=145 pass_rate=1.0000` — but the gate itself reports **RED on this
  host** because the sole `known-failures.txt` entry `3.5/2` now PASSES and
  the gate enforces lockstep trimming. Adjudication: this is
  **host-dependent and pre-existing** — a clean worktree build of the
  pre-change baseline `60a5272` produces the identical stale-entry RED with
  the same 145/145. Not caused by this pass, and `known-failures.txt` is
  out of scope for the audit (trimming it is the phase machine's call —
  the entry's own comment documents the RST-vs-GOAWAY handshake timing
  that evidently resolves differently on this host/h2 version). Recorded,
  not "fixed".
- The `max_header_list_size` change is wire-visible (SETTINGS advertisement),
  so both Docker/e2e conformance suites were re-run against the rebuilt
  binary specifically to clear it: h2spec is unaffected (145/145 before and
  after), and the differential fixtures below ran against the post-change
  `envoy-bin`.

### Implementation bugs discovered this pass

1. **Unbounded inbound H2 header lists (fixed, `db633bd`):** the H2 listener
   accepted up to 16 MiB of decoded headers per stream (h2 crate default;
   nothing in envoy-rust set a bound), vs Envoy's 60 KiB
   `max_request_headers_kb` default and envoy-http1's own 8 KiB cap. A
   config-independent memory-amplification DoS window; now bounded at 60 KiB
   with Envoy's 431 reject observable.

## 8. Re-ranked remaining recommended work

1. **Streaming vs buffered proxying decision record**: the mid-body-reset pins
   (`446627f`) make the divergence explicit; if streaming ever lands, those
   tests plus the H1/H2 502/503 synth arms are the contract to revisit. Until
   then, response-size limits are the buffered proxy's real exposure — there
   is **no cap on upstream response body size** (an upstream can make
   envoy-rust buffer an arbitrarily large CL/chunked body per request).
   Worth a bound + test, same shape as the header-list fix.
2. **`max_request_headers_kb` as config**: the 60 KiB H2 bound is a constant;
   Envoy exposes it on the HCM. When the phase machine adds the knob, the
   constant and `h2_oversized_request_header_list_is_rejected` are the seam.
3. **F-2 — de-flake `reserve_port` (bind-then-release TOCTOU)**: observed
   failing 2/3 full parallel runs on a loaded host (details in §"Test-suite
   results" above). The clean fix is `envoy-bin` accepting port 0 and
   advertising bound addresses (admin `/listeners` parity — real Envoy
   exposes exactly this), then tests stop guessing ports entirely.
4. **Property/fuzz coverage for other validate-vs-runtime pairs** (first pass
   §5 item 6, still open): header-mutation and route matching remain
   data-plane-only paths with no property sweep.
5. **h2spec in-repo ergonomics**: the runner's `tools/h2spec` drop location
   is now gitignored; consider a script target that fetches the pinned 2.6.0
   binary so local runs stop silently skipping. Separately, the stale
   `known-failures.txt` entry `3.5/2` (now passing on this host, see above)
   needs a phase-machine decision: trim it in lockstep or make the gate
   tolerate host-dependent entries.
6. **Differential fixture for oversized-header 431 parity** (H2): pin the 431
   against real Envoy (needs a raw-ish client in the harness driver — medium).
