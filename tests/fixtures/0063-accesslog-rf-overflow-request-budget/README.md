# Fixture 0063 — access-log `%RESPONSE_FLAGS%`/`%RESPONSE_CODE_DETAILS%` overflow path, request-budget arm (byte-exact)

Closes carry-forward **M50-C** (phase 55, ADR-0112). Phase 50 (ADR-0107)
tagged BOTH the connection-pool overflow arms (`hcm.rs:508`/`:515`) AND the
request-budget (`max_requests`) overflow arm (`hcm.rs:951`-`:952`) with the
identical rcd string `upstream_reset_before_response_started{overflow}`,
feeding the same `UO` `%RESPONSE_FLAGS%` derive arm — but fixture `0058`
(phase 50) exercises ONLY the pool-overflow arm (`max_connections:1` /
`max_pending_requests:0` against a dead literal endpoint, no backend spawn).
This fixture witnesses the SECOND set-site: the request-budget arm, via the
code path `0058` cannot reach.

**This is NOT a new `%RESPONSE_FLAGS%` or `%RESPONSE_CODE_DETAILS%` value** —
`UO` and `upstream_reset_before_response_started{overflow}` are already
witnessed (phase 50, fixture `0058`). The request-budget *disposition* itself
(status 503 + body `"...reset reason: overflow"` + stats) is ALSO already
differentially proven by fixture `0025` (phase 17, ADR-0046/ADR-0047) at the
wire/stats level. This fixture's sole new contribution is the
`%RESPONSE_CODE_DETAILS%`/`%RESPONSE_FLAGS%` access-log rendering on the
request-budget arm specifically, which `0025` does not log (no `json_format`
access-log in that fixture).

## What this proves

On a request-budget (`max_requests:0`) rejection against a REACHABLE
endpoint, both proxies return a deterministic 503 and render
`%RESPONSE_CODE_DETAILS%` = `upstream_reset_before_response_started{overflow}`
+ `%RESPONSE_FLAGS%` = `UO`. envoy-rust's request-budget gate
(`try_acquire_request()`, `hcm.rs:913`-`:933`) rejects UNCONDITIONALLY before
any pool/backend contact, tagging the rcd at `hcm.rs:951`-`:952`; the SAME
derive arm (`hcm.rs:1385`) that already handles the pool-overflow arm maps it
to `"UO"`. NO source change was needed — reconfirmed at state-1 (this
project's SPEC session) and re-reconfirmed at state-2 (this session, against
a freshly-built `target/debug/envoy-bin`).

The assertion is **pure cross-proxy equality** — there is NO static expected
literal. The overflow synth-503 is deterministic on both sides.

## The `json_format` map (request-budget overflow route)

| key   | operator                  | rendered value                                       |
|-------|---------------------------|-------------------------------------------------------|
| `rc`  | `%RESPONSE_CODE%`         | `503` (json NUMBER)                                    |
| `rcd` | `%RESPONSE_CODE_DETAILS%` | `upstream_reset_before_response_started{overflow}`     |
| `rf`  | `%RESPONSE_FLAGS%`        | `UO`                                                   |

Keys sort by UTF-8 byte order (ADR-0094 §A): rc, rcd, rf; compact separators
+ ONE trailing `\n` (ADR-0092 §E). Emitted line:

```
{"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}
```

## Probe

| # | request                    | emitted JSON object (byte-identical on both sides) |
|---|----------------------------|----------------------------------------------------|
| 1 | `GET /` (no extra headers) | see below                                          |

```
{"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}
```

A single probe — the request-budget overflow path is a single pre-loop
rejection arm.

## The request-budget trigger (endpoint MUST be reachable)

`rq_zero` is `STRICT_DNS` ROUND_ROBIN (`dns_lookup_family: V4_ONLY`) with
`circuit_breakers.thresholds` set to `max_requests: 0` and ONE endpoint at
the SPAWNED, REACHABLE `Http1EchoBackend` (`{{BACKEND_HOST}}`:
`{{HTTP1_BACKEND_PORT}}` — the same marker pair as fixture `0051`). On every
`GET /`, the request-budget gate rejects the request with the overflow
synth-503 BEFORE any pool/backend dispatch on the envoy-rust side.

**Reachability is load-bearing, not incidental.** An UNREACHABLE endpoint
under `max_requests:0` instead produces `%RESPONSE_FLAGS%` = `UF` (a REAL
connect attempt) on live Envoy — this is the pre-existing, ALREADY-DOCUMENTED
`upstream_cx_total` connection-pool-prefetch divergence (ADR-0047,
`BEHAVIOR_CONTRACT.md:401`: Envoy prefetches a pool connection even on
reject; envoy-rust's `try_acquire_request()` rejects unconditionally before
any pool contact, regardless of reachability). Using the same
`{{HTTP1_BACKEND_PORT}}`-spawned reachable backend as `0051` (rather than a
dead literal address, the `0058` pattern) is what makes both proxies emit the
SAME `UO` disposition here.

## Per-side divergences

| Side       | bind address | admin block | access-log path                          |
|------------|--------------|-------------|-------------------------------------------|
| envoy      | `0.0.0.0`    | yes (port 0)| `/tmp/0063-envoy-mount/access.log`        |
| envoy-rust | `127.0.0.1`  | omitted     | `/tmp/0063-envoy-rust-mount/access.log`   |

The asserted line omits `%UPSTREAM_HOST%`, so the per-side `{{BACKEND_HOST}}`
divergence (`host.docker.internal` vs `127.0.0.1`) never appears in the
compared line — byte-identity holds regardless.

## Driver

`kind: http1_access_log_byte_exact` (same driver as fixtures
0040/0046/0051/0053/0056/0057/0058/0059/0060/0061/0062) — drives the probe,
scrapes both files, asserts the scraped line count equals `probes.len()`, and
calls `access_log::assert_access_log_lines_byte_identical`. The
`{{HTTP1_BACKEND_PORT}}` marker triggers the UNCONDITIONAL `Http1EchoBackend`
launch arm in `run_fixture` (`tests/differential/src/lib.rs:3209`) — no new
harness code, no fixture-name allowlist entry needed.

## Cross-references

- ADR: ADR-0112 (phase-55 pick + scope — witness the request-budget overflow
  arm's access-log rendering byte-exact, closing M50-C, via a NEW fixture
  reusing `0025`'s cluster shape + `0058`'s json_format shape).
- Related fixtures: `0058` (`%RESPONSE_FLAGS%` = `UO`, the phase-50 sibling
  witnessing the pool-overflow arm — the SAME rcd string/flag, a DIFFERENT
  set-site), `0025` (phase 17, the pre-existing wire/stats-level proof of
  this exact request-budget disposition — this fixture adds ONLY the
  access-log-level witness), `0051` (the `STRICT_DNS`/`{{BACKEND_HOST}}`/
  `{{HTTP1_BACKEND_PORT}}` reachable-backend + json_format template this
  fixture's cluster/backend shape is built from).
- Consumes: M50-C. Also folds the pre-existing doc-only M54-1 (a
  `BEHAVIOR_CONTRACT.md` anchor off-by-one, unrelated to this fixture's own
  content) while the same contract row is being edited.
- Deferred: the H2 request-budget overflow path (M45-1: no H2 access-log
  differential driver), the unreachable-endpoint `upstream_cx_total`
  prefetch divergence (ADR-0047/`BEHAVIOR_CONTRACT.md:401` — a PRE-EXISTING
  known divergence, not new scope for this phase).
