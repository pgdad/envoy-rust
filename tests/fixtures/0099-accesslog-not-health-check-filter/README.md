# Fixture 0099 — the `not_health_check_filter` access-log FILTER arm

Phase 116. The cross-proxy witness for `not_health_check_filter`, the **EIGHTH**
of upstream Envoy's twelve `envoy.config.accesslog.v3.AccessLogFilter` `oneof`
arms, after `status_code_filter` (70), `response_flag_filter` (71),
`header_filter` (72), `and_filter`/`or_filter` (73), `metadata_filter` (74) and
`grpc_status_filter` (114).

This is a **log-emission predicate**, not a formatter. It drops a record iff
the request was **answered by the downstream `envoy.filters.http.health_check`
filter** (phase 115).

- **Driver:** the EXISTING `Driver::Http1AccessLogByteExact` (`kind:
  http1_access_log_byte_exact`). No new driver, no harness change —
  `tests/differential/src/lib.rs` is untouched by this phase.
- **Shape:** one H1 HCM listener with the chain `[rbac, health_check, router]`;
  ONE `FileAccessLog` sink carrying `filter: { not_health_check_filter: {} }`;
  one `prefix: "/"` `direct_response` route answering `200 MAIN`;
  `clusters: []`, **no backend spawns**.
  - `rbac` is `action: DENY` with one policy whose principal is
    `header: { name: x-deny, present_match: true }`.
  - `health_check` is `pass_through_mode: false` with one matcher,
    `":path"` `string_match: { exact: "/healthz" }`.
- **Cluster-free and backend-free**, therefore verifiable on a development host.
- Six probes; **four are kept**.

## The format string (identical on both sides)

```
NHC %REQ(:METHOD)% %REQ(:PATH)% %RESPONSE_CODE% UA=%REQ(USER-AGENT)%\n
```

⚠ It does **not** render `%RESPONSE_CODE_DETAILS%`. Probe 5's RBAC 403 is a KEPT
row, and envoy-rust's RBAC filter sets no details where upstream renders
`rbac_access_denied_matched_policy[deny-flagged]`. That gap is pre-existing,
unrelated to this arm, and banked as `CF-116-3`. `%REQ(USER-AGENT)%` is rendered
so probe 6's kept line shows the health-checker user-agent really was sent.

## The MEASURED rule

Measured against `envoyproxy/envoy:v1.33.0`
(`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`) on
six sinks over two listeners at the phase-116 pick, and this exact fixture was
dry-run on both proxies at the PLAN-write.

1. **The arm drops EXACTLY the requests the health_check filter answered.**
   The drop is method-agnostic (probe 2), and a `/healthz?x=1` that falls
   through is kept (probe 3).
2. **The marking comes from the filter's DECISION, not the request.** A request
   carrying upstream's active-health-checker user-agent `Envoy/HC` is kept
   (probe 6).
3. **The filter must actually RUN.** A `/healthz` that RBAC answers first is kept
   (probe 5), although its path matches the health_check matcher.

The pick also measured that the same `GET /healthz` is **kept** on a listener
with no health_check filter. One fixture holds one listener's chain, so that
cell is pinned in-process instead, on both codecs:
`h1_not_health_check_filter_drops_exactly_the_intercepts`
(`crates/envoy-http1/src/hcm.rs`) and
`h2_not_health_check_filter_drops_exactly_the_intercepts`
(`crates/envoy-http2/src/hcm.rs`).

## Why probes 5 and 6 make this fixture non-vacuous

Probes 1 and 2 defeat an always-true arm; probes 3, 4 and 6 defeat an
always-false arm. Probes 5 and 6 defeat the plausible WRONG implementations —
an arm keyed on the request instead of on the filter's decision. Proved by
MUTATION at the PLAN-write, each from the same tree with `envoy-bin` rebuilt:

| the H1 bit is … | envoy-rust emits | verdict |
|---|---|---|
| forced `true` | 0 lines (4 expected) | RED |
| forced `false` | 6 lines (4 expected) — the two intercepts leak | RED |
| `req.path.starts_with("/healthz")` | 2 lines — probes 3 and 5 lost | RED |
| unmutated | 4 lines, byte-identical to upstream | GREEN |

## Authoring constraints (all load-bearing)

1. **The format must not render `%RESPONSE_CODE_DETAILS%`** (`CF-116-3`).
2. **RBAC must precede health_check** in `http_filters`, or probe 5 stops
   testing rule 3.
3. **Every probe is a distinct (method, path, header) combination**, so each
   kept line is attributable.
4. **The LAST probe is KEPT**, so the driver's ordering-aware suppression settle
   charges the cheap short wait rather than the long one.
5. **`{{PORT}}` is the only token.** `Http1AccessLogByteExact` does not receive
   `{{ADMIN_PORT}}`, so the upstream `admin:` block uses a literal
   `port_value: 0`.

## The two YAMLs

`envoy.yaml` is `envoy-rust.yaml` plus exactly **four harness hunks**, none of
them semantic, the same four every landed access-log byte-exact fixture carries:

```
1a2    > admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
5c6    <       address: { socket_address: { address: 127.0.0.1, port_value: {{PORT}} } }
       >       address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
12a14  >                 generate_request_id: false
19c21  <                       path: /tmp/0099-envoy-rust-mount/access.log
       >                       path: /tmp/0099-envoy-mount/access.log
```

**The `filter:` block is byte-identical on both sides** (verified by md5 over the
block). The two log paths live in different parent directories because the
driver bind-mounts only the upstream side's parent.

## The assertion

Pure **cross-proxy equality**: whole-line `==` between upstream Envoy v1.33.0 and
envoy-rust, plus an exact per-side line count. Four lines per side,
byte-identical:

```
NHC GET /healthz?x=1 200 UA=-
NHC GET /other 200 UA=-
NHC GET /healthz 403 UA=-
NHC GET /ua 200 UA=Envoy/HC
```
