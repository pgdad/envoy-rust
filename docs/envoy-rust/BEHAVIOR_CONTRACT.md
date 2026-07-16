# envoy-rust Behavior Contract

> This document is the canonical definition of what "behaviorally equivalent
> to upstream Envoy" means for the differential test harness. Every fixture's
> `expectations.yaml` is derived from the rules here. Divergences from the
> contract are resolved by either (a) fixing the implementation, or (b) landing
> an ADR that updates the contract — never both silently (doctrine D-3.3).

---

## Equivalence matrix

| Dimension | Required equivalence |
|---|---|
| Response status | Exact |
| Response body | Byte-exact for deterministic handlers; semantically equal for filter-modified bodies |
| Response headers | Set-equal modulo documented allow-list (`server`, `date`, timing/identity headers explicitly listed) |
| Response trailers | Set-equal under the same allow-list discipline |
| HTTP/2 & HTTP/3 framing | Structurally equivalent (same frame types/order on equivalent events); not byte-equal |
| Access log records | Semantically equal after field-mapping |
| Stats | Names match Envoy's documented stat tree; presence required; values exact on deterministic flows |
| xDS wire behavior | ADS message sequences match the protocol state machine; effective-config diff on identical snapshots |
| Timing | Not compared by default; a phase may opt in to latency bounds |

---

## Response body — no-healthy-upstream synth-503

> Authored per phase 12.2 SPEC §2.2 + ADR-0037. The H1 HCM per-request
> dispatch path returns a synthetic 503 when `Cluster::pick()` yields
> `None` — both proxies emit it with identical wire shape on the same
> active-HC eviction.

| Reachability path | Equivalence disposition |
|---|---|
| `pick() -> None` (HCM H1 `hcm.rs:582` arm; cluster has `health_checks` configured AND all endpoints unhealthy AND panic not engaged) | Status 503; body byte-exact `no healthy upstream` (19 bytes, hex `6e 6f 20 68 65 61 6c 74 68 79 20 75 70 73 74 72 65 61 6d`, NO trailing newline); 5 standard HTTP/1.1 response headers `{server, date, content-length: 19, content-type, connection}`. Emitted via the dedicated `synth_no_healthy_upstream` helper adjacent to `synth_status` — the helper is used ONLY on this path. The connect-fail 503 + send-fail/reset **503** paths keep `synth_status`'s empty body (phase-04.3 wire shape). |
| `max_connections` cap overflow OR `max_pending_requests: 0` reject (HCM H1 `hcm.rs:508` (`PoolError::Overflow`/`max_connections` → `AcquireOutcome::Overflow`) / `hcm.rs:515` (`PoolError::PendingOverflow`/`max_pending_requests` → `AcquireOutcome::Overflow`) arms; 15 D5 / ADR-0043 §6.2 finding 3) | Status 503; body byte-exact `upstream connect error or disconnect/reset before headers. reset reason: overflow` (81 bytes, NO trailing newline); header `x-envoy-overloaded: true` — the wire surfacing of Envoy's `UO` response flag, which is otherwise **access-log-only** (no `%RESPONSE_FLAGS%` wire surface). **Equivalence = byte-exact body + status.** Emitted via the dedicated `synth_overflow` helper adjacent to `synth_no_healthy_upstream` (used ONLY on these two overflow arms; H2 sibling `synth_h2_overflow`, Task 5). envoy-rust emits 6 headers `{server, date, content-length: 81, content-type, connection, x-envoy-overloaded}` — Envoy's set is `{x-envoy-overloaded, content-length: 81, content-type, date, server}` (no `connection`); the extra `connection` header is **allow-listed** by the harness (the 0019/0022 synth-503 precedent). The Envoy-only `circuit_breakers.*` sibling gauges (`{default,high}.{cx_pool_open, rq_open, rq_pending_open, rq_retry_open}` + `high.cx_open`) are NOT emitted at phase-15 scope (deferred). The overflow `%RESPONSE_CODE_DETAILS%` (`upstream_reset_before_response_started{overflow}`) + `%RESPONSE_FLAGS%` (`UO`) are now witnessed byte-exact in the access log (fixture 0058, phase 50, ADR-0107) — distinct from the still-non-deterministic connect-failure rcd. |

---

## Request body forwarding (HTTP/1.1)

As of phase 25.1, the HTTP/1.1 router forwards the **Content-Length-delimited**
downstream request body to the upstream verbatim (it is read into a `Bytes`
before the filter pipeline runs, exposed to the pipeline as `FilterRequest.body`,
and cloned per upstream attempt — replay-safe across retries). This closes the
pre-existing phase-04.3 gap where H1 forwarded an always-empty body. The body is
compared cross-proxy byte-exact under the existing `response_body` / echo-server
fixtures (differentially proven by fixture `0033-http-filter-buffer` in phase
25.2). **Chunked / streaming request bodies remain a non-goal** — a
`Transfer-Encoding: chunked` request is 501-rejected before any body read.
HTTP/2 already buffers and forwards request bodies (unchanged).

---

## LB selection

> Authored per phase 28 SPEC + **ADR-0070** (the §6.2-verified, 36/36-validated
> ring algorithm). The cluster's `lb_policy` decides which upstream endpoint a
> request is dispatched to from the eligible (healthy / non-ejected) set. The
> selection is observable on the wire (the chosen backend's response-body marker),
> so it is a first-class differential dimension.

**`ROUND_ROBIN` (the default since phase 02 — unchanged this phase).** Cursor-based
rotation over the eligible endpoints (the `pick()` fast path in
`crates/envoy-cluster/src/cluster.rs`). The per-request hash key (below) is **inert**
for round-robin — `pick_endpoint(Some(hash))` behaves identically to the cursor path,
the load-bearing regression-equivalence proof that all 35 pre-phase-28 fixtures stay
green (the policy-dispatch + the `pick()`/`pick_endpoint()` hash-key signature change
are behavior-preserving for the round-robin arm).

**`RING_HASH` (NEW, phase 28).** A ketama-style consistent-hash ring. The selection is
**deterministic** and **byte-identical to upstream Envoy v1.33.0** (the STRONG
differential target — cross-proxy identical selection per key), reproduced exactly by
the ADR-0070 algorithm:

- **Hash = xxHash64, seed 0**, written from scratch (D-3.2 — no hashing crate in-tree
  or permitted; `crates/envoy-cluster/src/xxhash.rs`). Canonical vectors:
  `xxh64("") == 0xEF46DB3751D8E999`, `xxh64("abc") == 0x44BC2CF5AD770999`.
- **Ring build:** each host contributes `replicas = minimum_ring_size / num_hosts`
  entries (equal weight; e.g. `1024 / 2 = 512`). Entry `i` (decimal `0..replicas-1`)
  has ring hash `xxh64("{ip:port}_{i}")` where `{ip:port}` is the host address in IPv4
  `SocketAddr` Display form (e.g. `172.22.0.2:5678`, matching Envoy's
  `address()->asString()`). **The `_` separator is load-bearing** — a one-character
  change breaks the differential. IPv6 ring hosts (bracketed `[::1]:5678` Display) are
  an **untested non-goal** (the fixture is IPv4).
- **Ring** = the `(hash, host_index)` pairs sorted ascending.
- **Request hash** = `xxh64(hash_policy header value bytes)` (the same xxHash64 path as
  the ring keys).
- **Lookup** = the first ring entry with `entry.hash >= request_hash`; if none, **wrap
  to index 0** (`bisect_left` / first-clockwise).

**Keying.** A route-level `hash_policy` (`{ header: { header_name } }`) supplies the
request key, extracted from the named request header in the HCM request path
(H1 `crates/envoy-http1/src/hcm.rs`, H2 `crates/envoy-http2/src/hcm.rs`). The MVP is a
**single header source** (cookie / connection-source-IP / query-parameter / filter-state
sources, `terminal`, multi-policy combination, and `regex_rewrite` are deferred — SPEC §2.2).

**Empty-vs-absent (the load-bearing ADR-0070 refinement).** A header that is **present
but empty** (`x-hash-key:` empty) is **HASHED** — `xxh64("")`, deterministic — NOT the
fallback. Only an **ABSENT key** (no `hash_policy` match, or the named header missing)
falls back. The fallback is **Envoy's random host**, which is **non-deterministic and
therefore NOT differentially asserted** (cross-proxy identity cannot be required of a
random pick); it is covered by the in-process backstop only. The fixture always supplies
the header, so the differential never exercises the fallback.

**The XX_HASH-only narrowing (a documented intentional divergence).**
`hash_function: MURMUR_HASH_2` is a valid upstream Envoy enum but is **rejected** by
envoy-rust this phase (an all-fatal config error per ADR-0049 — `UnsupportedHashFunction`).
A bogus `hash_function` enum → parse-reject; `minimum_ring_size > maximum_ring_size` →
validation-reject (`RingSizeInversion`). All three are startup-fatal (no reload path this
phase). `ring_hash_lb_config` defaults: `minimum_ring_size` 1024, `maximum_ring_size`
8388608, `hash_function` XX_HASH.

**M28-1 — `maximum_ring_size` is parse-validation-only (a documented bound vs Envoy).**
The ring build is governed solely by `minimum_ring_size` (`minimum_ring_size / num_hosts`
replicas per host); envoy-rust does **NOT** scale replicas up toward `maximum_ring_size`
for small host counts (Envoy's ketama would). `maximum_ring_size` stays parse-validated
(`RingSizeInversion`) and stored but does not affect the ring — a documented bound,
validated against the 2-host/1024 oracle.

**Differential witness.** Fixture **`0036-lb-ring-hash`** — one cluster with
`lb_policy: RING_HASH` and two distinguishable backends; the driver sweeps distinct
`x-hash-key` values and asserts cross-proxy **identical** RING_HASH selection per key
(by response-body marker), same-key→same-backend stability, and spread across both
backends. This is a **normal request/response, observable LOCALLY** (no file-watch/reload
trigger, unlike phases 26/27) — fixture 0036 runs + is authoritative on this dev host.

**`MAGLEV` (NEW, phase 29).** A deterministic consistent-hash LB. Like `RING_HASH`, the
selection is **byte-identical to upstream Envoy v1.33.0** (the STRONG cross-proxy
differential — same `x-hash-key` header value → same backend on both proxies), reproduced
exactly by the §6.2-LOCKED algorithm (**ADR-0072**):

- **Hash = xxHash64** (the same from-scratch `crates/envoy-cluster/src/xxhash.rs`), but
  with a **seeded** variant for the per-host permutation. The host key is `ip:port` with
  **NO `_i` suffix** (the contrast with the ring's `{ip:port}_{i}` key shape is
  load-bearing). For table size `M`: `offset = xxh64(key, seed 0) % M`;
  `skip = xxh64(key, seed 1) % (M - 1) + 1` (**seed 1 is load-bearing**);
  `permutation[j] = (offset + j*skip) % M`.
- **Table build:** config-order **round-robin populate** — each host claims its next
  unclaimed permutation slot in turn; on a collision the host advances its own cursor;
  the **earlier host in config order wins** a contested slot. Populate continues until
  every one of the `M` slots is filled (`crates/envoy-cluster/src/maglev.rs`).
- **Lookup** = `table[xxh64(header_value, seed 0) % M]` — a single O(1) array index (no
  binary search / wrap, unlike the ring).
- **`table_size`** default **65537** (Envoy proto default); must be **prime**; max
  **5000011**.

**MAGLEV dispositions.** A **non-prime** `table_size` → startup-fatal
(`MaglevTableSizeNotPrime`); **over-max** (`> 5000011`) → startup-fatal
(`MaglevTableSizeTooLarge`) — both per the ADR-0049 all-fatal posture, no reload path this
phase. A `maglev_lb_config` carried on a **non-MAGLEV** cluster is **accept-and-ignore**
(Envoy parity — the block is only consulted when `lb_policy == MAGLEV`). There is **no
portable LB stat** to assert (selection is observed on the wire, not via a counter).
Header-absent → **cursor fallback** (M28-2 — not differentially asserted, mirroring the
RING_HASH fallback rationale). An **empty-but-present** header (`x-hash-key:`) is **hashed**
(`xxh64("", seed 0)`, deterministic), NOT the fallback — same empty-vs-absent refinement as
the ring.

**MAGLEV differential witness.** Fixture **`0037-lb-maglev`** — one `lb_policy: MAGLEV`
cluster with two distinguishable backends; the driver sweeps distinct `x-hash-key` values
and asserts cross-proxy **identical** MAGLEV selection per key (by response-body marker),
same-key→same-backend stability, and spread across both backends. A normal
request/response observable LOCALLY (no file-watch/reload trigger) — fixture 0037 runs + is
authoritative on this dev host. Per the consistent-hash differential discipline, BOTH
proxies build the table from one **shared** host LAN IP (identical endpoint strings), since
the algorithm hashes the `ip:port` string.

**Deferred non-goal — HC/OD + RING_HASH composition (RECORDED here per doctrine D-3.3).**
The ring skip-and-retry over **ineligible** (unhealthy / ejected) hosts — Envoy's
documented behavior of advancing to the next ring entry when the selected host is not
eligible — is a **SPEC §2.2 deferred non-goal** (Task 5 decision). The phase-28 fixture
cluster is **PLAIN** (no active health checking, no outlier detection), so the ring
returns a host directly and the eligibility-skip path is exercised by the **backstop
only** — the differential does **NOT** validate it. Wiring it would couple `HashRing` to
the cluster's health/ejection state (a `lookup_eligible(pred)` forward-with-wrap walk) for
a path no phase-28 fixture exercises; `RING_HASH` over an HC/OD cluster is therefore **not
yet differentially validated**. Also deferred (brief — all in SPEC §2.2): the **weighted
ring** (`load_balancing_weight` → unequal replicas — applies to MAGLEV too),
**non-header hash sources**, and the non-deterministic **`least_request` / `random`**
policies (which
need a contract-relaxation ADR before they can be differential). **`RING_HASH` +
EDS-hot-reload composition** (re-ringing on a hot endpoint-set swap) is also deferred —
the fixture uses a static (non-EDS) RING_HASH cluster.

**`subset LB` (NEW, phase 30).** An **orthogonal pre-dispatch layer** (NOT an `LbPolicy`
value) that **narrows** the candidate endpoint set BEFORE the inner `lb_policy` (MVP
**ROUND_ROBIN**) picks within the subset. It is configured by a cluster's
`lb_subset_config` (selectors + fallback) plus per-endpoint `metadata` and a route's
`metadata_match`; when no `lb_subset_config` is present the layer is **inert** (see the
no-op clause below).

- **Match semantics (ADR-0074).** An endpoint is a candidate iff its `envoy.lb` metadata is
  a **superset** of the route `metadata_match`. The `subset_selectors` entry **used** is the
  one whose `keys` **set equals** the `metadata_match` key set; the index build groups
  endpoints per selector by the **value-tuple** of that selector's keys, and an endpoint
  **missing** a selector key is **excluded** from that selector's groups.
- **Fallback (ADR-0074, §6.2-VERIFIED).** On no-match: **`NO_FALLBACK`** → `503 no healthy
  upstream`; **`ANY_ENDPOINT`** → round-robin over **all** endpoints; **`DEFAULT_SUBSET`** →
  the subset named by `default_subset` (an empty/absent `default_subset` → matches all →
  round-robin all).
- **Wire shapes.** Endpoint `metadata` and route `metadata_match` are `core.v3.Metadata`
  (nested `filter_metadata."envoy.lb"`); `lb_subset_config.default_subset` is a **flat**
  `google.protobuf.Struct` (`{ stage: prod }`, NOT nested) per **ADR-0075**.
- **Config validity (ADR-0074 correction #1).** Subset config is **NOT startup-fatal** —
  upstream Envoy boots for empty `subset_selectors`, empty `keys`, an uncovered selector, or
  a missing `default_subset` (the consequences are request-time), so envoy-rust **accepts**
  all of these (no fatal validator), per **ADR-0049** (fatal only where Envoy itself
  rejects).
- **Stats (ADR-0074 correction #2).** Envoy's `lb_subsets_active`/`lb_subsets_created` read
  an **opaque, non-portable** value (an observed **66** for a minimal 2-endpoint / single-
  `[stage]`-selector cluster — NOT the naive "1 per distinct value"; the derivation is
  opaque) → envoy-rust emits **no `lb_subsets_*` stat** this phase (deferred non-goal);
  fixture 0038 **ignore-lists** them.
- **No-op regression.** A cluster with **no** `lb_subset_config` is **byte-identical** to
  before (the subset layer is inert) — all pre-existing fixtures plus the
  RING_HASH/MAGLEV/round-robin selection paths are unchanged.
- **Differential witness.** Fixture **`0038-lb-subset`** — two metadata'd backends
  (`{stage:prod}` / `{stage:canary}`) and three `metadata_match` routes; a **new
  route-select driver** asserts cross-proxy **identical** selection (`/prod`→prod,
  `/canary`→canary) plus the `NO_FALLBACK` `/nope`→**503** probe. Observable **LOCALLY** (no
  file-watch/reload trigger). **H1-only** (an H2 pick-none → 502 case is **not** asserted).
- **Inner LB within a subset = ROUND_ROBIN** (MVP). Deferred §2.2 non-goals:
  subset + consistent-hash inner-LB, subset + HC/OD differential, multiple-overlapping
  selectors, per-selector fallback, and `single_host_per_subset`.

---

## Network filters

> Opened by phase 66 (the Network-filters family's first row). Scope today:
> `echo`, `tcp_proxy`, `http_connection_manager`, `direct_response`, `rbac`.
>
> **Do not conflate** `envoy.filters.network.direct_response` (this network filter, which
> writes a payload on connection accept) with the HCM **route-level** `direct_response`
> action (phase 04, which returns an HTTP response for a matched route). They are
> different features with the same name; every `direct_response` row elsewhere in this
> document refers to the route-level action.
>
> **Do not conflate** `envoy.filters.network.rbac` (phase 67.1 — an L4 filter that permits or
> denies a whole **connection**) with `envoy.filters.http.rbac` (phase 10,
> `crates/envoy-filter/src/rbac.rs` — an HTTP filter that permits or denies a **request**).
> They are different features with the same name. They share the `Rules` / `Policy` /
> `Permission` / `Principal` config trees and nothing else. Every `rbac` row elsewhere in this
> document refers to the HTTP filter unless it says "network".
>
> **Terminal vs non-terminal.** `echo`, `tcp_proxy`, `http_connection_manager` and
> `direct_response` are TERMINAL; `rbac` is the family's first NON-TERMINAL filter.

### `envoy.filters.network.direct_response` (phase 66, ADR-0123 / ADR-0124)

1. **Response semantics.** On each accepted downstream connection the filter writes the configured
   `response` payload immediately — without reading or waiting for any client bytes — then closes
   the connection with a clean EOF (no RST). A missing or empty `response` yields a zero-byte write
   followed by a clean close. Output is byte-identical across connections and independent of client
   input and of client read timing. *(Witnessed against `envoyproxy/envoy:v1.33.0`; SPEC §0 R-0.5, R-0.7.)*
   Differentially witnessed byte-exact by fixture **`0071-network-filter-direct-response`** via
   `Driver::TcpDirectResponse` (the harness's first read-to-EOF raw-TCP driver).

2. **Read-half drain (ADR-0124).** After sending FIN, both proxies continue to drain (read and
   discard) the downstream read half until the client closes. A client write issued AFTER it
   observes EOF is therefore **accepted, not reset** — measured on upstream Envoy at 0, 21, and
   200 000 unread bytes (`post_write=writes_ok`). envoy-rust matches. A server that closed without
   draining would RST the client, which upstream Envoy does not do. **This clause has no
   differential observable** (fixture `0071`'s driver never writes after EOF); it is pinned
   in-process by `post_eof_client_write_is_accepted_not_reset`
   (`crates/envoy-bin/src/direct_response.rs`), whose doc comment carries a mutation-check
   instruction: delete the drain loop and that test must fail.

3. **Network-filter terminal rule (bilateral).** All four network filters envoy-rust supports —
   `echo`, `tcp_proxy`, `http_connection_manager`, `direct_response` — are TERMINAL: each must be
   the last filter in its chain, and upstream Envoy rejects a config that places any of them before
   another network filter (`terminal filter named <X> ... must be the last filter in a network
   filter chain`). envoy-rust enforces the identical rule via
   `ConfigError::NetworkFilterNotTerminal`, where previously it silently ignored every filter after
   the first. *(SPEC §0 R-0.6.)* Implemented as a per-name `is_terminal_network_filter` predicate,
   not a `chain.filters.len() <= 1` check, so the first non-terminal network filter (`sni_cluster`,
   network `rbac`) drops in without re-litigating the rule.

4. **Recorded divergence — `DataSource` arms (CF-66-1).** Upstream Envoy accepts
   `response.inline_bytes` and `response.filename`; envoy-rust accepts only `response.inline_string`
   and rejects the other arms loudly at config load (serde `deny_unknown_fields`). Deliberate, per
   the ADR-0049 decision-2 (b) fail-loud posture. No differential observable — fixture `0071` uses
   `inline_string`.

5. **Scope note — `echo` `typed_config` asymmetry (pre-existing, unchanged).** Upstream Envoy
   REQUIRES `typed_config` on `envoy.filters.network.echo`; envoy-rust forbids it
   (`UnexpectedTypedConfig`). Fixture `0001`'s two sides differ accordingly (ADR-0014 YAML shim).
   `direct_response` introduces no such asymmetry — both sides of fixture `0071` are identical.

### `envoy.filters.network.rbac` (phase 67.1, ADR-0128 / ADR-0129 / ADR-0130 / ADR-0131 / ADR-0132; connection-level matcher arms phase 67.2, ADR-0133)

1. **Decision timing — ONE_TIME_ON_FIRST_BYTE. This is a property of the RBAC *verdict*, NOT of the
   chain's hand-off to the terminal filter (ADR-0132).** The policy is evaluated exactly ONCE per
   connection, **when the first downstream byte arrives** — not at connection establishment.
   Measured against `envoyproxy/envoy:v1.33.0` (**ADR-0131**, which corrects phase-67 SPEC R-2's
   "before any downstream byte is read" reading), on a `[rbac, echo]` chain:

   | client behavior | both proxies |
   |---|---|
   | connect, send nothing | connection stays **open**; no counter ticks |
   | connect, half-close (FIN) without sending | **clean EOF**; no counter ticks |
   | first byte, immediately or after a delay | decision taken; a counter ticks |

   The wait is unbounded — a client idling 2 s before its first byte is still evaluated then.

   **Upstream runs EVERY filter's `onNewConnection` at connection establishment — the TERMINAL
   filter's included — and defers only the verdict to the first byte.** envoy-rust's
   `envoy_listener::ChainHandler` instead peeks (without consuming) the first byte before delegating
   to the terminal handler at all, which gates the *whole chain*. Those two models are
   observationally identical **only** for a terminal filter with no establishment-time work. See
   item **13** for the per-terminal consequences, which is where envoy-rust diverges.

   **No differential observable** for the two byte-less rows (no fixture drives them); they are
   pinned in-process by `chain_handler_skips_filters_when_client_closes_without_sending` and
   `connection_that_sends_nothing_is_never_evaluated`.

   The data-less-FIN row is **per-terminal, not a chain property** (ADR-0132, measured): upstream
   ticks no counter for `echo` / `http_connection_manager`, but **does** evaluate for `tcp_proxy`
   (downstream half-close propagation). **Reproduced in envoy-rust from phase `67.3`** (ADR-0135):
   `echo`/`hcm` inherit the default `handle_gated` (a non-consuming `peek` → `Ok(0)` → skip), while
   `tcp_proxy`'s `handle_gated` override consumes one read on the split downstream half, so a
   data-less FIN (`Ok(0)`) still evaluates the chain. Pinned by
   `dataless_fin_ticks_allowed_for_tcp_proxy_but_not_echo` (envoy-bin FIN matrix).

2. **DENY semantics.** Zero bytes written; clean EOF, **never an RST**; the client's already-sent
   bytes are discarded; a post-EOF client write is **accepted**. The terminal filter never runs.
   Differentially witnessed by fixture **`0072-network-filter-rbac-deny`** (body byte-exact **AND**
   `rbac_deny.rbac.denied` delta `== 1`). The post-EOF-write clause has **no differential
   observable** and is pinned in-process by `deny_post_eof_client_write_is_accepted_not_reset`.

3. **ALLOW semantics.** The connection proceeds to the terminal filter and the payload round-trips.
   Differentially witnessed by fixture **`0073-network-filter-rbac-allow`** — the family's first
   differential proof that a **non-terminal filter runs and then yields**, i.e. of the chain
   iteration protocol itself.

4. **Stats.** `<stat_prefix>.rbac.{allowed,denied,shadow_allowed,shadow_denied}`. `stat_prefix` is
   **required and non-empty** (upstream proto constraint `RBACValidationError.StatPrefix`);
   `rules` is **optional**. *(SPEC R-3.)*

5. **`rules` omitted ⇒ the filter is INERT.** The connection is allowed and **NEITHER counter
   increments** — `allowed` stays `0`, not `1`. All four counters are still registered at `0`, so
   the stat tree matches. *(SPEC R-4, measured.)* A default `Rules { action: ALLOW }` that ticked
   `allowed` would be a **stat divergence with no body divergence**, invisible to a body-only
   fixture. Pinned by `rules_omitted_is_inert_neither_counter_ticks`.

6. **Bilateral chain-termination rule.** Upstream Envoy rejects a chain whose **last** filter is
   non-terminal (`non-terminal filter named <X> ... is the last filter in a network filter chain`),
   the dual of phase 66's "a terminal filter must be last". envoy-rust enforces the identical rule
   via `ConfigError::NetworkFilterChainNotTerminated`, on static **and** LDS-loaded listeners.
   *(SPEC R-1.)*

7. **Error precedence.** A chain violating **both** rules (`[echo, rbac]`) reports the
   **terminal-not-last** error on both proxies. *(SPEC R-5, measured.)*

8. **Empty chain — measured parity (closes M66-5), with a recorded runtime divergence.**
   `filters: []` is **accepted** by upstream Envoy (`configuration OK`) and by envoy-rust; the
   phase-66 review's intuition that Envoy rejects it was wrong, which is exactly why that review
   recorded envoy-rust's behavior and declined to assert Envoy's (D-3.3). **Runtime divergence
   (ADR-0130 §2):** envoy-rust binds **no data listener** for an empty chain and logs a warning;
   upstream Envoy binds one. envoy-rust previously *panicked* here. What upstream does with a
   *connection* to such a listener was never probed — carried forward as **CF-67-5**. **No
   differential observable**: no fixture configures an empty chain.

9. **Recorded divergence — L4 matcher leaves (CF-67-4).** envoy-rust rejects `header` in **parity**
   with upstream Envoy, which rejects it at config load (`Found header(name: ":path"...`).
   envoy-rust **also** rejects `url_path` and `metadata`, which upstream **accepts** even though
   they can never match at L4 — a deliberate **fail-loud** divergence per the ADR-0049 decision-2
   (b) posture. **No differential observable** — neither fixture uses them. *(SPEC R-6, measured.)*

9b. **Recorded divergence — `rules: { policies: {} }` (M-5, UNMEASURED for the network filter).**
    envoy-rust rejects an empty `policies` map at config load (`ConfigError::EmptyRbacPolicies`,
    the phase-10 check reused per 67.1 D1, ADR-0049 fail-loud posture). **Upstream Envoy's behavior
    for the NETWORK filter on this input was never measured** — SPEC R-3 measured only `rules`
    *omitted*, which is item 5 above and is genuine parity. Recorded rather than smoothed over: the
    reuse is sanctioned, but the parity claim is not established. **No differential observable** —
    no fixture configures an empty `policies` map. A phase that next probes upstream on this filter
    should measure it.

10. **Recorded divergence — `shadow_rules` (CF-67-1).** Upstream accepts `shadow_rules` /
    `shadow_rules_stat_prefix`; envoy-rust rejects them loudly at config load (serde
    `deny_unknown_fields`) and emits `shadow_allowed` / `shadow_denied` as constant `0` so the stat
    tree matches. **No differential observable.**

11. **Scope — matcher arms.** Phase 67.1 shipped `any` plus the `and`/`or`/`not` combinators only.
    The connection-level arms (`direct_remote_ip`, `remote_ip`, `source_ip`, `destination_port`,
    `destination_ip`) landed in phase **67.2** (ADR-0133) and now exist — see item **14**.
    `Action::LOG` is deferred (**CF-67-2**); payload-visible (`on_data`-time) filter iteration is
    deferred (**CF-67-3**).

12. **Scope — per-listener stats (ADR-0130).** `echo` and `direct_response` listeners now emit
    `listener.<name>.downstream_cx_{total,active,accept_failed}` and count in
    `listener_manager.total_listeners_active`, because phase 67.1 routed them through the shared
    `envoy_listener::Listener` accept loop. This is **toward** upstream parity (Envoy counts every
    listener). No fixture asserts set-equality over those names on a raw-TCP listener.

13. **COMPOSITION with each terminal filter (ADR-0132, measured).** Upstream Envoy runs every
    filter's `onNewConnection` at connection establishment, **including the terminal filter's** —
    that is where `direct_response` writes its payload and where `tcp_proxy` connects upstream — and
    defers only the RBAC verdict to the first downstream byte. Measured on `[rbac(any), <terminal>]`
    against `envoyproxy/envoy:v1.33.0`, with `/stats` scraped **mid-flight** (client connection still
    open) so each counter's *trigger* is disambiguated rather than inferred:

    | terminal | connect, send nothing, stay open | connect + FIN, no data | connect + first byte | establishment-time work |
    |---|---|---|---|---|
    | `echo` | no tick; stays open | no tick; clean EOF | tick | **none** |
    | `http_connection_manager` | no tick; stays open | no tick; clean EOF | tick | **none** |
    | `direct_response` | **payload written, clean EOF, NO tick** | same | same | **writes payload, closes** |
    | `tcp_proxy` | no tick; **banner delivered; `upstream_cx_total: 1`** | **TICKS** | tick | **connects upstream** |

    envoy-rust's status, per terminal:

    - **`echo`, `http_connection_manager` — full parity.** No establishment-time work, so
      `ChainHandler`'s first-byte gate is observationally identical to upstream's model. Witnessed by
      fixtures `0072`/`0073` and by `rbac_before_hcm_evaluates_on_the_first_request`.
    - **`direct_response` — full parity, by BYPASSING the chain.** `envoy-bin` hands the connection
      straight to `DirectResponseHandler` and never builds a `ChainHandler`. The `NetworkRbacFilter`
      is still constructed, so all four `<stat_prefix>.rbac.*` counters **register at `0`** and the
      stat tree matches; **no counter ever ticks**, and the payload is delivered and the connection
      closed **including under `action: DENY`** — a DENY policy does *not* suppress the payload,
      because the terminal filter writes and closes before any `onData` fires. Pinned by
      `direct_response_delivers_payload_to_a_client_that_sends_nothing` and
      `deny_does_not_suppress_the_direct_response_payload`. **No differential observable** — no
      fixture composes `rbac` with `direct_response`.
    - **`tcp_proxy` — SPLIT OUTCOME (phase `67.3`, ADR-0135).** The establishment/data-phase split of
      `envoy_listener::ConnectionHandler` (`handle_gated` + the filter-owned `FirstByteGate`) makes
      the composition behave for **plaintext** listeners; the **TLS-downstream** form stays a recorded
      fail-loud divergence.
      - **PLAINTEXT `[rbac, tcp_proxy]` = FULL PARITY.** `tcp_proxy` connects upstream at
        ESTABLISHMENT (`cluster.<name>.upstream_cx_total` ticks before any downstream byte), so a
        server-first banner reaches a byte-less client; the RBAC verdict lands on the first downstream
        byte **or** a data-less FIN; on **DENY the first byte is withheld from the upstream**. Pinned
        by the in-process witnesses (`banner_reaches_a_client_that_sends_nothing_through_rbac_allow`,
        `deny_delivers_banner_then_closes_without_forwarding_the_byte`,
        `dataless_fin_through_rbac_allow_reaches_backend_as_eof` in `envoy-tcp`) and the envoy-bin
        backstops (`plaintext_rbac_before_tcp_proxy_delivers_banner_to_a_byteless_client`,
        `deny_before_tcp_proxy_delivers_banner_then_withholds_the_byte`,
        `dataless_fin_ticks_allowed_for_tcp_proxy_but_not_echo`). **No differential observable** — a
        server-first backend is not host-deterministic under the Docker harness (the `67.2`
        precedent), so the witnesses are in-process. ADR-0135.
      - **TLS-DOWNSTREAM `[rbac, tcp_proxy]` = RECORDED FAIL-LOUD DIVERGENCE, owner CF-67-7.** MEASURED
        (ADR-0135, the D6 probe against `envoyproxy/envoy:v1.33.0`): upstream Envoy establishes the
        `tcp_proxy` upstream at **raw-TCP accept (BEFORE the handshake)** and takes the RBAC verdict on
        the first **DECRYPTED** byte — an ordering envoy-rust's TLS handler does not yet reproduce.
        Upstream **accepts** the config; envoy-rust rejects it at config load with
        `ConfigError::UnsupportedNetworkFilterChainComposition`, whose message names **CF-67-7**. A
        deliberate fail-loud divergence (ADR-0049 decision-2 (b)); **never silent**. Pinned by
        `tls_rbac_before_tcp_proxy_is_still_rejected` (envoy-config) and
        `tls_rbac_before_tcp_proxy_is_rejected_at_config_load` (envoy-bin); the over-rejection guards
        are `lone_tcp_proxy_chain_is_still_accepted` / `tcp_proxy_alone_is_still_accepted` and the
        plaintext-accept `plaintext_rbac_before_tcp_proxy_is_now_accepted`.

14. **Connection-level matcher arms (phase 67.2, ADR-0133; wire shapes MEASURED against
    `envoyproxy/envoy:v1.33.0` with `--mode validate`).** The network `rbac` filter evaluates five
    connection-level arms in addition to `any` + the `and`/`or`/`not` combinators:

    | arm | kind | evaluates against |
    |---|---|---|
    | `direct_remote_ip` | Principal | `peer_addr.ip()` (downstream connection source) |
    | `remote_ip` | Principal | `peer_addr.ip()` |
    | `source_ip` | Principal | `peer_addr.ip()` |
    | `destination_ip` | Permission | `local_addr.ip()` (listener/local address) |
    | `destination_port` | Permission | `local_addr.port()` |

    - **`remote_ip` ≡ `direct_remote_ip` ≡ `source_ip` today.** Upstream distinguishes
      `direct_remote_ip` (the immediate peer) from `remote_ip` (after a PROXY-protocol / XFF
      **listener filter** rewrites the remote address); envoy-rust has **no listener filters**, so
      all three evaluate `peer_addr.ip()` — modeled as three enum variants sharing ONE evaluation
      expression, not silently aliased. `source_ip` is a **deprecated** upstream alias of
      `direct_remote_ip` that upstream accepts with a deprecation warning; envoy-rust does **not**
      replicate the warning (**no differential observable**).

    - **`CidrRange` wire shape (X-1).** `address_prefix` is a bare IP string parsed to `IpAddr`;
      `prefix_len` is Envoy's `UInt32Value`, which upstream accepts as EITHER a bare integer
      (`prefix_len: 24`) OR the wrapper (`prefix_len: {value: 24}`). **envoy-rust models it as a bare
      `u8` and REJECTS the wrapper**, fail-loud — matching the codebase's `Buffer::max_request_bytes`
      UInt32Value posture (ADR-0063) and the ADR-0049 stance. An IPv4-mapped-IPv6 peer
      (`::ffff:127.0.0.1`) is **canonicalised to IPv4** before matching, so it matches an IPv4
      range — upstream's behavior. An absent `prefix_len` is a fatal serde missing-field error
      (upstream defaults it to 0). **No differential observable** for any of these — 67.2 ships no
      new fixture.

    - **RECORDED DIVERGENCE — mapped-prefix width rejected at load (the C-1 repair, ADR-0134).**
      An IPv4-mapped-IPv6 `address_prefix` (e.g. `"::ffff:127.0.0.0"`) is canonicalised to IPv4 for
      BOTH validation and matching (the shared `canonical_ip` rule), so `prefix_len > 32` on a
      mapped prefix is rejected fail-loud at config load (`ConfigError::InvalidCidrRange`,
      `prefix_len N exceeds 32 for IPv4`, with policy name + path — nested combinators and LDS
      listeners included). **Upstream Envoy v1.33.0 ACCEPTS the same config** (measured,
      `--mode validate` → `configuration OK`); its RUNTIME matching semantics for a mapped prefix
      were NOT measured (the IP arms are host-dependent under the Docker harness — parent V-4 — so
      no fixture can witness them), and only acceptance is asserted. Deliberate **fail-loud
      divergence** per ADR-0049 decision-2 (b): pre-repair, this config was accepted and then
      PANICKED the connection task on first evaluation (phase-67.2 REVIEW C-1) — rejection at load
      is strictly safer than either accept-then-panic or silently-dead 16-byte matching. A mapped
      prefix within the canonical width (`prefix_len ≤ 32`) validates and matches identically to
      its plain-IPv4 spelling (witnessed end-to-end at the state-5 re-review). **No differential
      observable** — no fixture uses a mapped prefix.

    - **`destination_port` is a `u16`.** Upstream models it as a plain `uint32` with PGV `lte: 65535`
      that itself rejects the `{value:N}` wrapper AND values > 65535 (both measured), so a bare `u16`
      is exactly faithful (serde rejects the wrapper and > 65535 for free).

    - **RECORDED DIVERGENCE — the HTTP RBAC filter REJECTS these L4 arms, fail-loud.** `Permission`
      and `Principal` are shared with `envoy.filters.http.rbac`. Upstream Envoy **ACCEPTS**
      `destination_port` / `direct_remote_ip` etc. in an HTTP rbac filter (measured `configuration
      OK`); envoy-rust rejects them at filter construction (`FilterError::InvalidConfig`,
      startup-fatal). This is a deliberate **fail-loud divergence** (ADR-0049 decision-2 (b)), **not
      symmetric parity** — it corrects the `67.2/SPEC.md` §D3 "parity" framing. **No differential
      observable** — no HTTP rbac fixture uses an L4 arm.

    - **Why no differential fixture (parent PLAN-VERIFY V-4, measured).** The IP arms would see this
      host's Docker bridge address and `destination_port` a per-proxy `{{PORT}}` that differs between
      the two proxies, so the arms are not host-deterministic under the Docker harness. They are
      witnessed **in-process** bound to `127.0.0.1` with a known port (engine unit tests +
      `direct_remote_ip_loopback_allows_end_to_end` / `direct_remote_ip_non_loopback_denies_end_to_end`
      / `destination_port_end_to_end`), where peer and local addresses are exact — the phase-25.1
      foundation-slice precedent. The differential surface for 67.2 is **regression-only** (`0001`–`0073`
      stay green).

---

## Active TCP health check (`tcp_health_check`)

Phase 68 (ADR-0136 / ADR-0137) adds active **TCP** health checking — the
upstream-robustness family's second checker type after phase-12 HTTP. Every
wire/behavior fact below was MEASURED against `envoyproxy/envoy:v1.33.0` during
the state-0 recon (SPEC §0) and the state-2 §6.2 re-verification (ADR-0137); the
implementation asserts only what is measured (D-3.3).

- **Checker shape.** `HealthCheck.tcp_health_check` is a sub-message
  `{ send?: Payload, receive?: [Payload] }`. **Empty** (`tcp_health_check: {}`)
  ⇒ a **connection-only** check: a successful TCP connect ⇒ Healthy. `send` (a
  single `Payload`) is written once after connect; `receive` (repeated `Payload`)
  is then scanned in the inbound bytes.
- **`Payload` oneof.** `Payload` is `{ text: <hex> | binary: <base64> }` —
  exactly one of the two. `text` is a hex string, `binary` is base64. Decoded to
  raw bytes at **validate time**, fail-loud: odd-length / non-hex `text`, invalid
  base64, and neither-or-both-set are all **load-fatal** typed `ConfigError`s
  (native messages — byte-parity with Envoy's `invalid hex string '…'` is WAIVED
  per ADR-0049 / ADR-0137 PV-1; config-load errors are not a differential wire
  surface).
- **`health_checker` oneof.** A `HealthCheck` setting **more than one** of
  `http_health_check` / `tcp_health_check` / `grpc_health_check` is **load-fatal**
  (`ConfigError::MultipleHealthCheckers`) — the upstream `health_checker` oneof
  (MEASURED, R-0.4 / ADR-0137 PV-4; generalized from the phase-68 two-checker
  rejection to at-most-one-of-three at phase 69, ADR-0139). Setting **neither**
  stays `UnsupportedHealthCheckType` (`custom_health_check` still deferred; gRPC
  is supported as of phase 69 — see [Active gRPC health check](#active-grpc-health-check-grpc_health_check)).
- **`receive` matching.** The `receive` scan is a **contiguous-substring** search:
  a payload found anywhere in the accumulated inbound bytes matches (MEASURED for
  a single block — a banner `ABPINGCD` matches `receive: [PING]`). **Only
  single-block is Envoy-parity-pinned** (ADR-0137 PV-3); multi-block is
  implemented as envoy-rust's own sequential-in-order contract (each block at/after
  the previous match end) and is NOT asserted for Envoy parity.
- **Timeout.** ONE `timeout(hc.timeout, …)` bounds the WHOLE probe — connect,
  `send`, and the `receive` scan (MEASURED: a blackhole endpoint is ejected at
  ~`timeout`, well before any cluster `connect_timeout`; ADR-0137 PV-6). The
  cluster `connect_timeout` is NOT consulted by the checker (mirrors the HTTP
  `probe_once`).
- **Outcomes & stats.** A matched `receive` or a successful connection-only probe
  ⇒ `.success` + (after `healthy_threshold`) Healthy. A **connect refusal** ⇒ an
  immediate `.failure` (Envoy `health_flags` `/failed_active_hc`). A `receive`
  **no-match within `timeout`** ⇒ a `.failure` on timeout elapse (Envoy
  `/failed_active_hc/active_hc_timeout`). Ejection after `unhealthy_threshold`
  consecutive failures drives `membership_healthy` → 0 and, on a cluster fronted
  by an HCM/router with panic disabled, `pick() → None` → synth-503
  `no healthy upstream` (the fixture-0074 differential observable, identical to
  fixture 0019). The `cluster.<name>.health_check.*` + `membership_*` stat tree is
  **identical** to phase 12 — no new stat names (see the Stat-name mapping "68
  entries" note).

---

## Active gRPC health check (`grpc_health_check`)

Phase 69 (ADR-0138 / ADR-0139) adds active **gRPC** health checking — the
upstream-robustness family's third checker type after phase-12 HTTP and phase-68
TCP. Every wire/behavior fact below was MEASURED against `envoyproxy/envoy:v1.33.0`
during the state-0 recon (SPEC §0) and the state-2 §6.2 re-verification (ADR-0139);
the implementation asserts only what is measured (D-3.3).

- **Checker shape.** `HealthCheck.grpc_health_check` is a sub-message
  `{ service_name?: String, authority?: String, initial_metadata?: [HeaderValueOption] }`.
  **Empty** (`grpc_health_check: {}`) ⇒ probe the **overall server** (gRPC service
  name `""`). `service_name` names a specific gRPC health service; `authority`
  overrides the probe's `:authority`. `initial_metadata` is accepted for schema
  completeness but the probe does **not** thread it (MINIMAL support per SPEC §2.2 —
  unobservable in fixture 0075).
- **HTTP/2-upstream requirement.** `grpc_health_check` **requires** the cluster to
  be H2-upstream (`typed_extension_protocol_options` →
  `HttpProtocolOptions.explicit_http_config.http2_protocol_options`). On a non-H2
  cluster it is **load-fatal** (`ConfigError::GrpcHealthCheckRequiresHttp2`),
  mirroring Envoy's MEASURED "cluster must support HTTP/2 for gRPC healthchecking"
  (R-0.3; native message per ADR-0049). Because the cluster must be H2-upstream and
  the H1-listener × H2-cluster dispatch stays deferred (ADR-0028, NOT lifted),
  fixture 0075 uses an **H2 listener** (`codec_type: HTTP2`).
- **`health_checker` oneof.** Covered above in the TCP section — setting more than
  one of `http`/`tcp`/`grpc` ⇒ `ConfigError::MultipleHealthCheckers`.
- **Probe protocol.** The probe is a unary `grpc.health.v1.Health/Check` RPC over
  the cluster's upstream H2: `POST /grpc.health.v1.Health/Check`,
  `content-type: application/grpc`, `te: trailers`; request body = a length-prefixed
  gRPC frame (1 compression byte `0x00` + 4-byte big-endian length +
  `HealthCheckRequest { string service = 1 }`). Response = `:status 200` +
  `content-type: application/grpc` + a gRPC frame
  (`HealthCheckResponse { ServingStatus status = 1 }`, enum
  `UNKNOWN=0/SERVING=1/NOT_SERVING=2/SERVICE_UNKNOWN=3`) + the **`grpc-status`
  trailer** (`0` = OK). The two messages are hand-rolled (no `prost`/`tonic`
  in-tree, ADR-0139 PV-3).
- **Verdict.** (`grpc-status` trailer `== 0` (OK)) **AND**
  (`HealthCheckResponse.status == SERVING`) ⇒ Healthy (`.success` + after
  `healthy_threshold`). **Any other** `ServingStatus` (`UNKNOWN`/`NOT_SERVING`/
  `SERVICE_UNKNOWN`), a non-zero `grpc-status`, a decode error, a connect/transport
  failure, or a per-probe timeout ⇒ `.failure`.
- **No `network_failure` distinction.** A `NOT_SERVING` response is an
  **application-level** failure (the gRPC call completed) while a connect refusal is
  a **transport** failure — but envoy-rust does **NOT** model `health_check.network_failure`
  for **any** checker type (CF-69-2). Both fold into the same `.failure` counter,
  exactly as HTTP/TCP do; the transport-vs-app distinction is neither emitted nor
  differentially asserted.
- **Timeout.** ONE `timeout(hc.timeout, …)` bounds the **whole** probe — H2 connect,
  handshake, request, response, and trailers (ADR-0139 PV-6, mirroring HTTP/TCP).
  The cluster `connect_timeout` is NOT consulted by the checker. No `grpc-timeout`
  request header is emitted (deferred-unobservable).
- **Outcomes & stats.** Ejection after `unhealthy_threshold` consecutive failures
  drives `membership_healthy` → 0 and, on a cluster fronted by an HCM/router with
  panic disabled, `pick() → None` → synth-503 `no healthy upstream`. The
  `cluster.<name>.health_check.{attempt,success,failure}` + `membership_*` stat tree
  is **identical** to phases 12/68 — no new stat names (the gRPC checker witnesses
  the same names via the shared scheduler).
- **Differential surface.** Fixture 0075 (`0075-upstream-grpc-health-check`)
  witnesses ejection via the **connect-refuse** observable (`grpc_health_check: {}`
  pointing at a dead port → after settle Unhealthy → `pick() → None` → synth-503),
  driven over an H2 listener by the `http2_after_settle` differential driver. It
  asserts **status + byte-exact body ONLY**; the header axis is **OMITTED** because
  envoy-rust's H2 no-healthy synth-503 emits a narrower header set (`server` +
  `content-type` only, no `content-length`/`date`) than Envoy — a pre-existing H2-503
  gap (CF-69-1), out of scope for this checker phase. The SERVING-healthy,
  NOT_SERVING-failure, gRPC framing, trailers, and message-decode paths are covered
  **in-process** (the connect-refuse fixture fully witnesses the ejection→503 path;
  the response-status verdict is exhaustively unit-tested).

---

## Header allow-list

> **To be filled per-phase as needed.**
>
> The header allow-list enumerates response headers whose values may differ
> between upstream Envoy and envoy-rust without the fixture being red.
> Membership on this list must be justified (e.g. `server` carries an
> implementation-identifying string, `date` is wall-clock non-determinism).
> Timing and identity headers must be listed explicitly — no wildcards.
>
> Every phase that introduces a new header surface (HTTP/1.1, HTTP/2, HTTP/3,
> access-log header filter, router header manipulations, etc.) updates this
> section or produces an ADR explaining why the defaults suffice.

| Header | Equivalence | Rationale |
|---|---|---|
| `server` | name-required, value-may-differ | Implementation-identifying. Both proxies emit `server: <name>`; envoy-rust's HCM default is `server: envoy-rust`, Envoy's default is `server: envoy`. When HCM `server_name` config field is set (deferred to phase 05+ per parent SPEC §4), value tightens to exact-match on both sides. |
| `date` | name-required, value-may-differ | Wall-clock non-determinism (RFC 7231 §7.1.1.2 IMF-fixdate format). Both proxies stamp the response with the wall-clock at response-write time; values diverge because the two proxies write at slightly different instants. |
| `x-envoy-upstream-service-time` | name-required, value-may-differ | Per-request upstream-side latency in milliseconds. envoy-rust measures from `Client::connect` start to last-response-byte-read end (computed in the router proxy arm before the response is written downstream). Envoy emits the same header (its semantics are documented at `https://www.envoyproxy.io/docs/envoy/v1.33.0/configuration/http/http_filters/router_filter#x-envoy-upstream-service-time`). Only present on responses that proxied through to an upstream cluster (NOT on `direct_response` paths — that's 04.1's surface where this header is never emitted). Both proxies emit on every router-proxy response; values diverge by measurement. Lands in 04.3 per phase-04 parent SPEC §2 + 04.3 SPEC §2. |
| `x-envoy-attempt-count` | value-exact (total upstream attempts; `2` after one retry) | Present on the downstream response **only** when the matched VirtualHost sets `include_attempt_count_in_response: true` (ADR-0045 finding L5/L6 — NOT automatic; absent without the flag regardless of whether a `retry_policy` is configured). Injection reuses the `x-envoy-upstream-service-time` machinery at the retry-loop exit: H1 at `crates/envoy-http1/src/hcm.rs` (constant defined in `crates/envoy-http1/src/router.rs`); H2 at `crates/envoy-http2/src/hcm.rs`. Exercised by fixture 0024 with `include_attempt_count_in_response: true` on both proxy configs; both probes assert `x-envoy-attempt-count: 2` (one retry each). **Phase-17 L11 extension:** the header IS also emitted on synthesized overflow local replies (value `1` — one admitted attempt even though no upstream request was sent) when the vhost flag is set — verified empirically vs Envoy v1.33 at the phase-17 §6.2 verification (closes the phase-16 review's M16-3); fixture 0025 asserts it on all three probes (values 1/2/1) including the overflow local reply. |

**Phase 08.1 D1 dedupe note:** With phase 08.1's case-insensitive dedupe in
`crates/envoy-admin/src/handler.rs::serialize_response`, a future endpoint may
legitimately set its own `cache-control` (or any of the other 3 standard
headers). The dedupe guarantees no duplicate header lands on the wire; only one
instance of the header name appears in the response, and the caller-supplied
value wins.

---

## Stat-name mapping

> **To be filled per-phase as needed.**
>
> Upstream Envoy emits stats under a documented, hierarchical name tree.
> envoy-rust must emit the same tree. Mapping entries are recorded here only
> when envoy-rust must produce a stat under a different internal label that
> needs to be projected back to the Envoy-canonical name at the stats sink.
> The default assumption is that stat names match one-to-one.
>
> Every phase that introduces a new stat family (connection counters, HTTP
> response-code counters, cluster health counters, filter-local stats, admin
> stats, etc.) updates this section or produces an ADR explaining why the
> defaults suffice.

**06.1 initial entries:**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `listener.<name>.downstream_cx_total` | value-exact | Counter; one increment per accepted TCP connection on the listener. envoy-rust internal label matches Envoy's documented name one-to-one. Both proxies emit on every accept; under deterministic harness load (a fixed connection count) the values are byte-equal. |
| `cluster.<name>.upstream_cx_total` | value-exact (H1 + H2 clusters under the harness's single-downstream-keep-alive-conn driver); name-required, value-may-differ (TCP-proxy clusters — TCP pool defers to a follow-up phase per parent-13 SPEC §4) | Counter; one increment per established upstream TCP connection at pool-create time. Under H1/H2 pooling (phase 13), both proxies emit the same small N under deterministic load: 1 if the workload fits in one pooled connection (the fixture 0020 + 0021 baseline shape); more if the harness exceeds `max_concurrent_streams` or `max_connections`, in which case both proxies still emit identical N because the cap is bilaterally configured. The increment site lives in the H1/H2 pool's `acquire()` connect-on-miss branch (one source of truth per protocol; H1 at `crates/envoy-http1/src/pool.rs::H1Pool::acquire` per 13.1; H2 at `crates/envoy-http2/src/pool.rs::H2Pool::acquire` per 13.2). The TCP-proxy increment at `crates/envoy-tcp/src/lib.rs:108` remains per-call until TCP pooling lands; existing TCP fixtures (`0001/0003/0004/0005/0006`) carry the pre-13.2 name-required, value-may-differ disposition under the carve-out (their `expectations.yaml` assertions are presence-only — the tightened value-exact disposition is satisfied trivially on the H1/H2 side, the TCP side remains presence-only via the carve-out). The value-exact disposition is **conditional on the harness driver issuing multiple requests over a single downstream keep-alive conn** (per parent-13 SPEC §6.2 item-iv; else N upstream conns per N downstream conns regardless of pool — the harness's `Driver::Http1KeepAlive` from 13.1 D10 makes this configurable per-fixture). **This row tightening fully closes 06.3 REVIEW I2 (b)** — combined with the 13.1 fixture-0020-driven I2 (a) closure (per-class HCM `downstream_rq_{2,3,4,5}xx` + cluster `upstream_rq_5xx` bilateral assertions), **the full 06.3 REVIEW I2 carryforward is CLOSED at the phase-13 close.** |
| `http.<stat_prefix>.downstream_rq_total` | value-exact | Counter; one increment per HCM-handled request (any response code; any method). Both proxies emit on every request; under deterministic harness load (a fixed request count) the values are byte-equal. The `<stat_prefix>` segment is sourced from `HttpConnectionManagerConfig.stat_prefix`. |

**06.3 entries:**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `http.<stat_prefix>.downstream_rq_2xx` | value-exact | Counter; one increment per 2xx HCM-handled response. Fires at the factored access-log dispatch site AFTER all 5 writer arms have populated `response_status_for_log`. Status-class bucketing via integer division `status / 100`. Both proxies emit on every 2xx response; under deterministic harness load the values are byte-equal. Sibling: `downstream_rq_3xx/4xx/5xx`. |
| `http.<stat_prefix>.downstream_rq_3xx` | value-exact | Counter; one increment per 3xx HCM-handled response. See `downstream_rq_2xx`. |
| `http.<stat_prefix>.downstream_rq_4xx` | value-exact | Counter; one increment per 4xx HCM-handled response. See `downstream_rq_2xx`. |
| `http.<stat_prefix>.downstream_rq_5xx` | value-exact | Counter; one increment per 5xx HCM-handled response. See `downstream_rq_2xx`. Fires on direct_response 5xx, proxy synth-503 (no-endpoint, connect-fail, send-fail/reset, overflow), AND upstream-emitted 5xx responses — the per-class counter is symmetric on `response_status_for_log`, agnostic to synth-vs-proxy origin (both 502 and 503 are 5xx, so the COUNT is unaffected by the connect-fail / reset 502→503 corrections). |
| `http.<stat_prefix>.access_logs_total` | value-exact | Counter; incremented at queue-enter time via `Counter::add(N)` where N is the configured sink count. Fires BEFORE the per-sink `sink.emit(...).await` per parent-06 SPEC §6 Rule 4 (fire-and-forget emission). Both proxies emit one increment-by-N per request when access_log is configured; 0 when no access_log is configured. |
| `http.<stat_prefix>.access_logs_failed` | value-exact (0-failures case) | Counter; incremented inside the per-sink error arm before `tracing::warn!`. Both proxies emit 0 under the deterministic-success harness; non-zero values are only seen under sink-emission failure (file-path permission issues, disk full, etc.). 06.3 verifies the 0-case; future fixtures could exercise emission failure deterministically. |
| `listener.<name>.downstream_cx_active` | value-exact (deterministic close) | Gauge; incremented on every accepted TCP connection, decremented at the per-connection task's epilogue (Drop on success and error paths uniformly). Scope: data-path listeners only — admin listener excluded via code-path (envoy-bin's admin listener uses `tokio::net::TcpListener` + `envoy_admin::serve` directly, not `envoy_listener::Listener::bind`). Terminal-zero gauge: returns to 0 after all per-connection tasks complete and Drop fires. The harness's post-request settle window (50-100ms) gives the gauge time to return to 0 before the scrape captures the value. |
| `listener.<name>.downstream_cx_accept_failed` | value-exact (0-failures case) | Counter; incremented inside the listener accept loop's `Err(_)` arm BEFORE `tracing::warn!`. Signpost 6: all accept errors count (no carve-outs). Both proxies emit 0 under harness conditions (the harness produces well-formed connections; OS accept errors are extremely rare in lab settings). |
| `cluster.<name>.upstream_cx_active` | value-exact (deterministic close) | Gauge; incremented at the HCM proxy-arm and TCP-proxy dial sites via the `ConnGaugeGuard` RAII (architecture decision 13). Decrement fires via `Drop` at scope exit, covering both success and error close paths uniformly. Terminal-zero gauge; same settle-window considerations as the listener gauge. |
| `cluster.<name>.upstream_rq_total` | value-exact | Counter; one increment per upstream response received (NOT per upstream connect attempt). H1: fires at `write_proxied_response` function prologue; H2: fires inline at the post-dispatch success site in `finalize_h2_stream`. Synth-503 paths (envoy-rust-side 503 on connect-fail) do NOT increment — these are not upstream responses. Both proxies emit one increment per `upstream_resp` received. |
| `cluster.<name>.upstream_rq_5xx` | value-exact | Counter; conditional sibling of `upstream_rq_total`, increments when `upstream_resp.status / 100 == 5`. Synth local-reply paths (connect-fail 503, send-fail/reset **503**) bypass for the same reason as `upstream_rq_total`. |

**08.2 entries (drain machinery):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `server.live` | value-exact | Gauge; `1` when `DrainState::current() == Live`; `0` otherwise (HealthcheckFailing and Draining both emit `0`). Updated inline at the `DrainState::{fail_healthcheck, ok_healthcheck, drain}` CAS-success sites (one source of truth — NOT polled). Initial value `1` at process start. Both proxies emit on every snapshot. |
| `server.state` | value-exact (Live=0 baseline; Draining=2 post-drain) | Gauge; discriminant of `DrainStage` (`Live=0`, `HealthcheckFailing=1`, `Draining=2`). The `#[repr(u8)]` on `DrainStage` makes the discriminant load-bearing for the gauge value. Updated inline at the same CAS-success sites as `server.live` (one source of truth). Initial value `0` at process start. Fixture 0015 asserts the post-drain value `2`. |
| `listener_manager.total_listeners_active` | value-exact | Gauge; count of currently-active data-plane listeners (HCM + tcp_proxy paths going through `envoy_listener::Listener::bind`/`serve`). Echo path (fixture 0002 only) + admin path use `tokio::net::TcpListener` directly and are naturally excluded. RAII-guarded at `Listener::serve` entry (inc) / exit (dec); decrement fires AFTER drain completes and AFTER stragglers join. Mirrors the 06.3 `listener.<name>.downstream_cx_active` gauge pattern but is global (not per-listener-named); registered idempotently inside `Listener::bind`. |

**09 entries (LocalRateLimit filter):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `http_local_rate_limit.<stat_prefix>.enabled` | value-exact | Counter; one increment per decode-side filter invocation when the filter is enabled. At phase-09 scope `filter_enabled` defaults to always-on (100%); per upstream Envoy parity `enabled` increments unconditionally on every `decode_headers` call. Both proxies emit one increment per request reaching the filter. |
| `http_local_rate_limit.<stat_prefix>.ok` | value-exact | Counter; one increment per `try_acquire` success (token consumed; request allowed to continue). Both proxies emit one increment per under-limit request. |
| `http_local_rate_limit.<stat_prefix>.rate_limited` | value-exact | Counter; one increment per `try_acquire` failure (no tokens available; request would-be-rate-limited). At phase-09 scope `filter_enforced` defaults to always-on (100%) so `rate_limited` counts coincide with `enforced` — but the upstream-Envoy semantic distinguishes "would-be-rate-limited" (`rate_limited`) from "actually-rate-limited" (`enforced`). Both proxies emit one increment per over-limit request. |
| `http_local_rate_limit.<stat_prefix>.enforced` | value-exact | Counter; one increment per request actually rate-limited (429 response emitted via `Decision::StopAndSend`). At phase-09 scope `enforced == rate_limited` because `filter_enforced` defaults to always-on; the two stat names track for upstream-Envoy parity. When a future phase lands runtime-fractional-percent `filter_enforced` overrides, the two counters diverge. Both proxies emit one increment per 429 emission. |

**10 entries (RBAC filter):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `http.<hcm_stat_prefix>.rbac.allowed` | value-exact | Counter; one increment per request allowed under the primary rules — either by explicit Allow-action policy match OR by Deny-action no-match (per phase-10 SPEC §5.6 decision matrix). Both proxies emit one increment per allowed request at the decision site in `RbacFilter::decode_headers` (synchronously, before `Decision::Continue`). Upstream Envoy v1.33 emits the same name at the same `http.<hcm_stat_prefix>.rbac.*` namespace per the §6.2 empirical verification at PLAN-write. |
| `http.<hcm_stat_prefix>.rbac.denied` | value-exact | Counter; one increment per request denied under the primary rules — either by explicit Deny-action policy match OR by Allow-action no-match. Both proxies emit one increment per denied request at the decision site in `RbacFilter::decode_headers` (synchronously, before constructing the `Decision::StopAndSend(FilterResponse)` 403). The `allowed + denied == total_requests_to_filter` invariant holds per SPEC §2.1 (each counter incremented at its own fire site; no double-counting). |

**11 entries (Fault filter):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `http.<hcm_stat_prefix>.fault.aborts_injected` | value-exact | Counter; one increment per request the filter aborts (the header gate matches AND the deterministic percentage selects at 100%). Both proxies emit one increment per aborted request at the abort decision site in `FaultFilter::decode_headers` (synchronously, before constructing the `Decision::StopAndSend(FilterResponse)` abort). Never increments on pass-through (gate miss OR 0% percentage). Upstream Envoy v1.33 emits the same name at the `http.<hcm_stat_prefix>.fault.*` namespace per the §6.2 empirical verification at phase-11 state-2 PLAN-write (`http.ingress_http.fault.aborts_injected: 4` after 4 aborts). The `<hcm_stat_prefix>` is sourced from the parent HCM's `stat_prefix` (the fault filter has no `stat_prefix` field of its own — same threading as RBAC at phase 10). |

**12.1 entries (active health checking):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster.<name>.membership_healthy` | value-exact (12.2 steady state; reads 0 at 12.1) | Gauge; the count of currently-healthy endpoints in the cluster. Registered at `from_bootstrap` time only when the cluster configures `health_checks`; updated inline at each `EndpointHealth` Healthy/Unhealthy flip (one source of truth, NOT polled — the 08.2 `server.live` pattern). At 12.1, with no probe task, a configured-HC cluster's gauge reads its initial value 0 (all endpoints start Unhealthy per §6.2 item-1); 12.2's probe task drives it to the converged steady state. Inert when `health_checks` is unconfigured (no such gauge registered). The 3 `cluster.<name>.health_check.{attempt,success,failure}` counters defer to 12.2 where the probe task increments them (12.1 D6 lock-in). |

**12.2 entries (active health checking — counters):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster.<name>.health_check.attempt` | name-required, value-may-differ | Counter; one increment per health-check probe issued by the `envoy-health` scheduler. The count is **timing-dependent** — both proxies tick on their own independent `tokio::time::interval` schedules from independent process-start instants, so the elapsed-probe count over a fixed test window differs across proxies. Both proxies emit the name; the equivalence dimension is name-required only (value-exact is not feasible without timing-tolerance opt-in per §Timing tolerances, which phase 12 does NOT take). Registered at `Scheduler::spawn` time only when the cluster configures `health_checks`. |
| `cluster.<name>.health_check.success` | name-required, value-may-differ | Counter; one increment per probe whose response status ∈ `expected_statuses` (default exactly 200, half-open `Int64Range`). Same timing-dependence rationale as `.attempt`. |
| `cluster.<name>.health_check.failure` | name-required, value-may-differ | Counter; one increment per probe whose response status is NOT in `expected_statuses`, OR connect failure, OR per-probe `tokio::time::timeout` elapsed, OR malformed response (the network-failure-class results fold into `failure` at phase-12 scope; the dedicated `network_failure` sub-counter defers per parent SPEC §4). Same timing-dependence rationale as `.attempt`. |

**68 entries (active TCP health checking — IDENTICAL stat tree):** the active
**TCP** checker (`tcp_health_check`, phase 68, ADR-0136 / ADR-0137) witnesses the
**same** `cluster.<name>.membership_healthy` gauge and `cluster.<name>.health_check.{attempt,success,failure}`
counters listed in the 12.1 / 12.2 rows above — **no new stat names**. A TCP
probe increments `.success` on a matched/connection-only probe and `.failure` on
a connect refusal or a `receive` no-match within `timeout`; ejection drives
`membership_healthy` exactly as the HTTP checker does. See the behavior section
[Active TCP health check (`tcp_health_check`)](#active-tcp-health-check-tcp_health_check).

**13.1 entries (H1 connection pool):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster.<name>.upstream_cx_destroy` | value-exact (0-failures case) | Counter; incremented at every pool eviction. Three eviction paths: (a) idle-sweeper past-deadline (the second periodic-background primitive — sweeps every `idle_timeout / 4`); (b) `PoolGuard::invalidate()` flag on protocol error (Drop's None-arm); (c) connect-failure rollback (the `established` count decrement does NOT fire `cx_destroy` per 13.1 D3 — only successful-acquire-then-destroy paths count). Under the deterministic harness load with no forced-close + the hardcoded 60 s idle timeout (well past the ~5 s fixture settle window per 13.1 §5.4 lock-in), no idle eviction fires during fixture lifetime → both proxies emit 0 within the fixture window. Future fixtures exercising forced-close or longer settle would harden the disposition. Registered at `H1PoolManager::for_bootstrap` time only for clusters whose `upstream_protocol()` is `Http1`. |
| `cluster.<name>.upstream_cx_http1_total` | value-exact | Counter; one increment per H1 pool connect-on-miss (fires at the same site as the existing `cluster.<name>.upstream_cx_total` for H1 clusters — the H1 pool's `acquire()` connect-on-miss branch per 13.1 D3 + D4). Under the fixture 0020 single-downstream-keep-alive-conn driver issuing 10 sequential requests → both proxies emit 1 (full pool reuse). Registered at `H1PoolManager::for_bootstrap` time only for clusters whose `upstream_protocol()` is `Http1`. The existing `cluster.<name>.upstream_cx_total` BEHAVIOR_CONTRACT row at line `:89` (06.1 initial entry) STAYS `name-required, value-may-differ` AT 13.1 — the row tightening to `value-exact` is the **13.2 D7.1 deliverable** (the 06.3 REVIEW I2 (b) full-closure site; fires only when both H1 + H2 pools uniformly, since the row mentions no protocol carve-out and tightening at 13.1 would falsify the H2 surface that still increments per-call). |

**13.2 entries (H2 connection pool):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster.<name>.upstream_cx_http2_total` | value-exact | Counter; one increment per H2 pool connect-on-miss (fires at the same site as the existing `cluster.<name>.upstream_cx_total` for H2 clusters — the H2 pool's `acquire()` connect-on-miss branch per 13.2 D5 + D6, at `crates/envoy-http2/src/pool.rs::H2Pool::acquire`). Under the fixture 0021 single-downstream-keep-alive-conn driver issuing 5 sequential requests over an H2-upstream cluster → both proxies emit 1 (single upstream H2 connection multiplexing 5 concurrent stream slots; per the H2 pool's per-entry `active_streams` claim loop). Under hypothetical workloads beyond `DEFAULT_MAX_CONCURRENT_STREAMS = 100` (the RFC 7540 §6.5.2 default when peer SETTINGS is unobserved) the H2 pool would establish additional connections and the counter would tick again — fixture 0021's 5-request workload stays well under the cap so the bilateral value is deterministic 1. Registered at `H2PoolManager::for_bootstrap` time only for clusters whose `upstream_protocol()` is `Http2`. Sibling of `cluster.<name>.upstream_cx_http1_total` (13.1 entry); together they enumerate the per-protocol breakdown of `cluster.<name>.upstream_cx_total`. |

**14.1 entries (outlier detection):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster.<name>.outlier_detection.ejections_active` | value-exact (14.2 steady state; reads 0 at 14.1) | Gauge; count of currently-ejected endpoints in the cluster. Registered at `from_bootstrap` time only when `outlier_detection` is configured; updated inline at each `EndpointEjection::eject` / `try_un_eject` edge (one source of truth, NOT polled — the 12.1 `membership_healthy` pattern). At 14.1 the gauge reads its initial value 0 (all endpoints start never-ejected per §6.2 item-3); 14.2's response-receipt hook + sweeper drive it to the converged steady state. Inert when `outlier_detection` unconfigured (no such gauge registered). **The only gauge in the namespace** — the 6 sibling stats are counters. |
| `cluster.<name>.outlier_detection.ejections_enforced_total` | value-exact (14.2 steady state; reads 0 at 14.1) | Counter; one increment per actual ejection enforced (after the `max_ejection_percent` cap check at the cluster level). Sum across detector types modulo overflow. Per-detector siblings `ejections_enforced_consecutive_5xx` + `ejections_enforced_consecutive_gateway_failure` break it down. At 14.1 the value is 0 (no caller drives ejection until 14.2 D4). |
| `cluster.<name>.outlier_detection.ejections_overflow` | value-exact (0-case at fixture 0022's `max_ejection_percent: 100`; reads 0 at 14.1) | Counter; **per the §6.2 item-4 finding**, increments per detection-tick on cap-blocked enforcement (NOT once-per-host — overflow is a re-fire counter). Cluster-level (lives on `OutlierDetectionState`, not per-endpoint). Fixture 0022's `max_ejection_percent: 100` keeps this at 0 in steady state. At 14.1 the value is 0 (no caller drives the cap check until 14.2 D4). |
| `cluster.<name>.outlier_detection.ejections_detected_consecutive_5xx` | value-exact (14.2 steady state; reads 0 at 14.1) | Counter; per-detector-type tick fired at every threshold-crossing on the consecutive_5xx detector, **regardless of whether the cap permits enforcement** (per ADR-0041 §6.2 item-2). Sibling of `ejections_enforced_consecutive_5xx`. Incremented inline by `EndpointEjection::record_response` at the threshold-crossing site. At 14.1 the value is 0 (no caller). |
| `cluster.<name>.outlier_detection.ejections_enforced_consecutive_5xx` | value-exact (14.2 steady state; reads 0 at 14.1) | Counter; per-detector-type tick fired only when the threshold-crossing actually drives an ejection (cap honored). Equal to `ejections_detected_consecutive_5xx` minus the per-detector overflow share. At `enforcing_consecutive_5xx: 100` (the fixture-0022 setting and envoy-rust's only supported value at phase-14 scope per parent SPEC §4 deferral of `enforcing_*` knobs), `enforced == detected` modulo the cap. At 14.1 the value is 0 (no caller). |
| `cluster.<name>.outlier_detection.ejections_detected_consecutive_gateway_failure` | value-exact (0-case at fixture 0022; reads 0 at 14.1) | Counter; same shape as the `_consecutive_5xx` sibling. The fixture-0022 backend serves status 500 (NOT 502/503/504), so the gateway-failure detector never fires during fixture lifetime; both proxies emit 0. At 14.1 the value is 0 (no caller). |
| `cluster.<name>.outlier_detection.ejections_enforced_consecutive_gateway_failure` | value-exact (0-case at fixture 0022; reads 0 at 14.1) | Counter; sibling of `_detected_consecutive_gateway_failure`. 0-case at fixture-0022. At 14.1 the value is 0 (no caller). |

The remaining 13 Envoy-side names under `cluster.<name>.outlier_detection.*` (the `_detected_/_enforced_` pairs for `consecutive_local_origin_failure`, `success_rate`, `local_origin_success_rate`, `failure_percentage`, `local_origin_failure_percentage` = 10; the legacy aliases `ejections_total` + `ejections_consecutive_5xx` + `ejections_success_rate` = 3) are NOT emitted by envoy-rust at phase-14 minimum-viable scope (out per parent §4 deferral; ratified by ADR-0041 §6.2 item-2). **14.2 M8 reconciliation:** the count is **13** (5 detector pairs + 3 legacy aliases), correcting the prior "14" claim to match the enumeration. Fixture 0022's `expectations.yaml` does NOT need an `allowlist_envoy_only` for these: its `Driver::Http1KeepAlive` stat path asserts only the named `expected_stats` (no full set-diff), so unasserted Envoy-only names are ignored (unlike the 0011 prometheus-set-diff path, whose `allowlist_envoy_only` key does not exist on the keep-alive driver).

**15 entries (circuit breakers):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster.<name>.upstream_rq_pending_overflow` | value-exact bilaterally (fixture 0023: 1 bilaterally) | Counter; `+1` per request rejected on the connect-on-miss path when `max_pending_requests == 0` (the reject-on-establish gate, ADR-0043 §6.2 finding 1). One source of truth at the pool pending-gate (one site per protocol, BEFORE the cap-check). Registered only when `circuit_breakers` is configured (inert-when-unconfigured per lock-in #4); an unconfigured cluster defaults `max_pending_requests` to 1024 so the gate never fires and no such stat is registered. |
| `cluster.<name>.upstream_cx_overflow` | value-exact-at-0 bilaterally (fixtures 0020/0023 never trip the cap); the NON-ZERO cross-proxy value DIVERGES — validated non-zero IN-PROCESS only (the Task-8 backstop) | Counter; `+1` per upstream-connection demand rejected because the pool is AT `max_connections` (cap-hit; ADR-0043 §6.2 finding 2). One source of truth at the pool cap-check branch. On a cap-hit Envoy queues→counts the cap-hit but (with default pending) serves 200; envoy-rust has no pending queue at phase-15 scope and 503s — so the counter name+semantics match and the value matches at 0, but the non-zero value (and the downstream status multiset) is a **known divergence pending the deferred pending-queue phase** (§0.C finding 2 / ADR-0043). Registered only when `circuit_breakers` is configured. |
| `cluster.<name>.circuit_breakers.default.cx_open` | value-exact-at-0 bilaterally; non-zero in-process only (the Task-8 backstop) | Gauge 0/1; `1` while `upstream_cx_active == max_connections` (at-cap inclusive, ADR-0043 §6.2 finding 4), `0` otherwise; **edge-driven** (set at the `established`-count mutation edges, NOT polled), terminal-0 (returns to 0 after drain). `default` = the only supported `RoutingPriority` at phase-15 scope. Envoy always emits the full `circuit_breakers.{default,high}.{cx_open, cx_pool_open, rq_open, rq_pending_open, rq_retry_open}` 10-gauge set regardless of config; envoy-rust emits ONLY `default.cx_open` at phase-15 scope (the other 9 are Envoy-only, deferred). Fixture 0023's `Driver::Http1KeepAlive` scrapes only NAMED stats (no full set-diff), so no `allowlist_envoy_only` enumeration is needed for the Envoy-only siblings. Registered only when `circuit_breakers` is configured. |

**Overflow-model divergence note (ADR-0043 §6.2).** Under `max_pending_requests: 0`, Envoy rejects ALL establish-on-miss requests via `upstream_rq_pending_overflow` (NOT `upstream_cx_overflow`); the pool never warms (`upstream_cx_total: 0`, backend never contacted), and `upstream_cx_overflow`/`cx_open` stay inert-0 because no connection demand reaches the cap. The `{200,503}` cx-overflow multiset asserted in-process (Task-8 backstop) is **in-process-only**: on a `max_connections` cap-hit with a default (non-zero) pending budget, Envoy queues the cap-overflow request and serves it 200, yielding a bilateral `{200,200}` queue-and-serve shape; envoy-rust 503s the overflow. The bilateral `{200,200}` queue-and-serve fixture therefore **defers to the future pending-queue phase** (the `max_pending_requests > 0` queue, deferred per ADR-0042 §4 / ADR-0043 option d). See ADR-0043. The overflow-503 wire shape (status + 81-byte `…reset reason: overflow` body + `x-envoy-overloaded: true`) is captured in the Equivalence matrix row above (Task 4) and is NOT duplicated here.

**16 entries (HTTP retries):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster.<name>.upstream_rq_retry` | value-exact | Counter; `+1` per retry attempted (per re-dispatch beyond the first attempt). One source of truth at the retry-loop classification site — H1 at `crates/envoy-http1/src/hcm.rs`, H2 at `crates/envoy-http2/src/hcm.rs` (one site per protocol). Fixture 0024: 2 cumulative over both probes (one retry per probe). Registered unconditionally for every cluster at `from_bootstrap` time (`crates/envoy-cluster/src/cluster.rs`); inert at 0 when no route configures `retry_policy`. |
| `cluster.<name>.upstream_rq_retry_success` | value-exact | Counter; `+1` when a retried request ultimately produces a non-retriable outcome (i.e., the final attempt is not itself retriable — the request "succeeds out of retry"). Registered and incremented at the same H1/H2 retry-loop classification site as `upstream_rq_retry`. Fixture 0024: 1 (probe 1 only — 503→200 path). |
| `cluster.<name>.upstream_rq_retry_limit_exceeded` | value-exact | Counter; `+1` when `num_retries` is exhausted and the final attempt is still retriable (limit-exceeded path; the final upstream response is surfaced verbatim downstream). Registered and incremented at the same H1/H2 retry-loop classification site. Fixture 0024: 1 (probe 2 only — both attempts 503; see wire-shape note below). |

**Per-attempt counting reconciliation (ADR-0045 finding L5).** `cluster.<name>.upstream_rq_total` counts per upstream **attempt** — a request with one retry ticks it twice. Fixture 0024 asserts 4 over 2 probes (2 attempts × 2 probes). `cluster.<name>.upstream_rq_5xx` reflects the **completing** (downstream-returned) response only; the retried-away 5xx does **not** tick the main `upstream_rq_5xx` counter — it surfaces in the Envoy-only `cluster.<name>.retry.upstream_rq_{503,5xx,completed}` sub-scope which envoy-rust does NOT emit (allow-listed per ADR-0045 option (b)). Fixture 0024 asserts `upstream_rq_5xx: 1` (probe 2's completing 503 only). The completing-response tick fires only when the completing attempt received a real upstream response — synthetic local replies (the no-healthy-upstream synth-503, connect-failure synth-503, reset synth-503, and overflow synth-503 paths) do not tick `upstream_rq_5xx`, preserving the pre-phase-16 baseline where these paths never ticked it (state-5 review fix). The Envoy-only `upstream_rq_retry_overflow` / `upstream_rq_retry_backoff_*` / `retry_or_shadow_abandoned` / `circuit_breakers.*.rq_retry_open` names are similarly NOT emitted (allow-listed; per ADR-0045). **This paragraph supersedes the 06.3 `cluster.<name>.upstream_rq_total` row's "one increment per upstream response received" wording for retried requests.** For non-retried requests, per-attempt == per-response-received — the 06.3 row's wording remains accurate for all pre-phase-16 fixtures. The per-attempt semantic applies from phase 16 forward, per ADR-0045.

**Retry-limit-exceeded wire shape (ADR-0045 finding L9).** When `num_retries` is exhausted and the final attempt is still retriable, the downstream response is the **last upstream response verbatim** (status + body + headers) — NOT a synthetic local reply. This is distinct from the no-healthy-upstream and overflow synth-503 paths (which produce local replies with fixed bodies). Envoy's `%RESPONSE_FLAGS%` shows `URX` on this path, which is **access-log-only** and never surfaces as a response header. Phase 51 (ADR-0108) fixture **0059** now witnesses `%RESPONSE_FLAGS% = URX` byte-exact cross-proxy here; envoy-rust derives it from the `retry_limit_exceeded_for_log` boolean set at the limit-exceeded loop-exit (the same gate as `upstream_rq_retry_limit_exceeded`), NOT from `%RESPONSE_CODE_DETAILS%` (which stays the shared `via_upstream`).

**17 entries (circuit-breaker budgets):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster.<name>.upstream_rq_retry_overflow` | value-exact | Counter; `+1` per retry abandoned because the retry budget (`max_retries`) is exhausted; ticks inside the failed `try_acquire_retry` (single source of truth, one site per budget — NOT per protocol; the budget lives in `envoy-cluster::BudgetState` per ADR-0046 §5.4). Registered UNCONDITIONALLY for every cluster (inert at 0 — the phase-16 retry-counter posture). Fixture 0025: 1 (budget_zero) / 0 (budget_default) / 0 (rq_zero). |
| `cluster.<name>.circuit_breakers.default.rq_retry_open` | value-exact-at-0 bilaterally (fixture 0025); NON-ZERO edge in-process only (the Task-9 backstop's >0-cap concurrency path) | Gauge 0/1; MOMENTARY semantic per ADR-0047 L4: `1` iff `active_retries > 0 AND active_retries >= max_retries`; never latched; `0` in every sequential-regime scrape. Registered only when `circuit_breakers` is configured. |
| `cluster.<name>.circuit_breakers.default.rq_open` | value-exact-at-0 bilaterally (fixture 0025); NON-ZERO edge in-process only (the Task-9 backstop's >0-cap concurrency path) | Gauge 0/1; same shape as `rq_retry_open` but for the request budget (`active_requests` vs `max_requests`). Registered only when `circuit_breakers` is configured. |
| `cluster.<name>.circuit_breakers.default.remaining_retries` | value-exact, registered ONLY when `track_remaining: true` (ADR-0047 L8: absent — not present-at-0 — otherwise) | Gauge; `= max_retries − active_retries`, floored at 0. Fixture 0025: 0 (budget_zero, cap 0) / 3 (budget_default, the Envoy default read back bilaterally). |
| `cluster.<name>.circuit_breakers.default.remaining_rq` | value-exact, registered ONLY when `track_remaining: true` (same conditionality as `remaining_retries`) | Gauge; `= max_requests − active_requests`, floored at 0. Fixture 0025: 1024 (budget_default — the Envoy default). |

**The L3 overflow co-firing paragraph (ADR-0047).** The `max_requests`-overflow local reply ticks `cluster.<name>.upstream_rq_pending_overflow` (inside the failed `try_acquire_request` — the same counter name phase 15 wired for `max_pending_requests`, idempotently shared) AND `cluster.<name>.upstream_rq_5xx` (at the HCM caller site) — **the ONLY synthetic local reply that ticks `upstream_rq_5xx`**; this narrowly supersedes the phase-16 "synthetic local replies do not tick `upstream_rq_5xx`" sentence for exactly this path (per ADR-0047; all other synth paths keep the phase-16 posture). `upstream_rq_total` stays 0 (matches Envoy). `upstream_cx_total` on the overflow cluster is a KNOWN DIVERGENCE left unasserted: Envoy 1 (connection-pool prefetch) vs envoy-rust 0 (no pool contact). Envoy additionally co-fires `upstream_rq_503`/`upstream_rq_completed`/`external.upstream_rq_503` (Envoy-only, not emitted by envoy-rust, unasserted).

**The §5.4 registration-seam paragraph (ADR-0046).** The `circuit_breakers.default.*` namespace now has TWO registration sites — the per-protocol POOLS register `cx_open` (phase 15: connection-lifecycle concept) while the CLUSTER registers `rq_open`/`rq_retry_open`/`remaining_*` (phase 17: cluster-wide budget concepts spanning both protocol pools). The `upstream_rq_pending_overflow` counter handle is idempotently shared between the phase-15 pool gate and the phase-17 request-budget gate.

**The L12 Envoy-only enumeration paragraph.** Per cluster with `circuit_breakers`, Envoy always emits the 10-gauge family `circuit_breakers.{default,high}.{cx_open, cx_pool_open, rq_open, rq_pending_open, rq_retry_open}`; with `track_remaining: true` it adds 5 `circuit_breakers.default.remaining_*` gauges (`remaining_cx`, `remaining_pending`, `remaining_rq`, `remaining_retries`, `remaining_cx_pools`). envoy-rust at phase-17 scope emits: `default.cx_open` (pools, phase 15) + `default.rq_open` + `default.rq_retry_open` (cluster, conditional on `circuit_breakers`) + `default.remaining_retries`/`default.remaining_rq` (conditional on `track_remaining`). The rest are Envoy-only unasserted names (ignored by the named-stat scrape).

**18 entries (file-based CDS):**

> The xDS-family opener. These are the project's first **top-level-scope**
> (non-resource-prefixed) `cluster_manager.*` stats, all derived from the
> §6.2 empirical lock-in **L3** (verified against `envoyproxy/envoy:v1.33.0`,
> digest `sha256:56da5afd…`, 2026-06-02). Envoy emits 18 names under
> `cluster_manager.*` after a successful CDS load; envoy-rust's minimum-viable
> subset is **6 names**. Registered at `ClusterManager::from_bootstrap` time
> (`crates/envoy-cluster/src/cluster.rs`), **conditionally** — ONLY when
> `dynamic_resources.cds_config` is configured.

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster_manager.cds.update_attempt` | value-exact (fixture 0026: 1) | Counter; one increment per CDS load attempt. envoy-rust's synchronous `load_dynamic_resources` ticks it once at the file-read+parse step. Both proxies emit 1 on the single initial load (no hot-reload at phase-18 scope — L11 inconclusive/deferred). |
| `cluster_manager.cds.update_success` | value-exact (fixture 0026: 1) | Counter; one increment per CDS load that produced an installed cluster set. The load-bearing differential proof of the phase: `update_success: 1` can only pass if real Envoy genuinely loaded the CDS file (fixture 0026 asserts it bilaterally). |
| `cluster_manager.cds.update_failure` | value-exact (fixture 0026: 0) | Counter; in Envoy, `+1` per CDS load that hit a **parse error** (malformed envelope) — Envoy then warns-and-serves with `active_clusters: 0`. **In envoy-rust this is structurally 0:** all CDS load errors are FATAL pre-construction (the L4 all-fatal posture, ADR-0049 — see the xDS-wire-state-machine §(c)), so the process exits rather than reach a non-zero `update_failure` state. Registered at 0 and unreachable non-zero. Bilaterally satisfiable at 0 on fixture 0026 (a successful load). |
| `cluster_manager.cds.update_rejected` | value-exact (fixture 0026: 0) | Counter; in Envoy, `+1` per CDS load whose resource was **semantically invalid** (PGV violation / cluster-build failure) — distinct from the parse-error `update_failure` bucket; Envoy warns-and-serves. **In envoy-rust this is structurally 0** for the same reason as `update_failure` (the all-fatal posture; the process exits instead). Registered at 0 and unreachable non-zero. Bilaterally satisfiable at 0 on fixture 0026. |
| `cluster_manager.cluster_added` | value-exact (fixture 0026: 1) | Counter; `+1` per cluster ADDED to the manager. The count includes **static clusters** — Envoy counts ALL clusters added to the manager, not just CDS-supplied ones; envoy-rust mirrors (`= all_clusters().count()`, the merged static+dynamic size). Bilateral on fixture 0026 because it has **zero static clusters** (the single dynamic cluster yields 1 on both sides); a fixture mixing static + dynamic clusters would assert the combined count. |
| `cluster_manager.active_clusters` | value-exact (fixture 0026: 1) | Gauge; the count of currently-active clusters in the manager — the same merged static+dynamic size as `cluster_added`, and the same static-inclusion caveat applies (bilateral on fixture 0026 only because it has zero static clusters). The lone gauge of the 6 names (the other 5 are counters). |

**The §5.2 conditional-registration narrowing (recorded divergence, L10/ADR-0049).** All 6 names register ONLY when `dynamic_resources.cds_config` is configured. This is a **deliberate divergence** from Envoy's tree: Envoy emits the `cluster_manager.cds.*` subtree conditionally (the cds subtree exists only with CDS configured — both proxies agree here), but Envoy emits the **base** `cluster_manager.*` names (`active_clusters`, `cluster_added`, …) **unconditionally** on every bootstrap. envoy-rust narrows the base names to the same CDS-configured condition (registers nothing on non-CDS fixtures), so on all non-CDS fixtures the base `cluster_manager.*` names stay **Envoy-only-unasserted** (fixture 0011's Prometheus set-diff posture is unchanged; zero existing-fixture edits). Recorded explicitly per doctrine D-3.3.

**The L3 Envoy-only enumeration paragraph.** After a successful load Envoy emits **18** `cluster_manager.*` names; envoy-rust emits the **6** above. The 12 Envoy-only unasserted names (ignored by fixture 0026's named-stat scrape — no set-diff on the `Http1KeepAlive` driver) are: `cds.update_time`, `cds.version`, `cds.version_text`, `cds.update_duration`, `cds.init_fetch_timeout`, `cluster_modified`, `cluster_removed`, `cluster_updated`, `cluster_updated_via_merge`, `update_merge_cancelled`, `update_out_of_merge_window`, `warming_clusters`. (Envoy additionally carries a `cds.control_plane.*` family, irrelevant to the filesystem transport.) None of the 6 emitted `cluster_manager.*` values change pre- vs post-GET — request counters live under `cluster.<name>.*`.

**19 entries (file-based LDS):**

> The xDS-family continuation (ADR-0050 SPEC / PLAN). file-based LDS loads
> listeners from `dynamic_resources.lds_config.path_config_source.path` at
> startup; these are the first top-level-scope `listener_manager.*` stats. All
> derived from the §6.2 empirical lock-in **L3** (verified against
> `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd…`, 2026-06-02). Envoy
> emits **21** names under `listener_manager.*` after a successful LDS load;
> envoy-rust's minimum-viable subset is **6 names**. The 4 `lds.*` names +
> `listener_added` register **conditionally** — ONLY when
> `dynamic_resources.lds_config` is configured (`register_lds_stats`, Task 4);
> `total_listeners_active` keeps its pre-existing 08.2 **unconditional**
> registration, here tightened to a bilateral assertion on fixture 0027.

| Stat name | Equivalence | Rationale |
|---|---|---|
| `listener_manager.lds.update_attempt` | value-exact (fixture 0027: 1) | Counter; one increment per LDS load attempt. envoy-rust's synchronous `load_dynamic_resources` ticks it once at the file-read+parse step. Both proxies emit 1 on the single initial load (no hot-reload at phase-19 scope). |
| `listener_manager.lds.update_success` | value-exact (fixture 0027: 1) | Counter; one increment per LDS load that produced an installed listener set. The load-bearing differential proof of the phase: `update_success: 1` can only pass if real Envoy genuinely loaded the LDS file (fixture 0027 asserts it bilaterally). |
| `listener_manager.lds.update_failure` | value-exact (fixture 0027: 0) | Counter; in Envoy, `+1` per LDS load that hit a **parse error** (malformed envelope / missing `@type`) — Envoy then warns-and-serves. **In envoy-rust this is structurally 0:** all LDS load errors are FATAL pre-construction (the L4 all-fatal posture, ADR-0049 extended to LDS by ADR-0050 — see the xDS-wire-state-machine LDS §(c)), so the process exits rather than reach a non-zero `update_failure` state. Registered at 0 and **structurally unreachable non-zero**. Bilaterally satisfiable at 0 on fixture 0027 (a successful load). |
| `listener_manager.lds.update_rejected` | value-exact (fixture 0027: 0) | Counter; in Envoy, `+1` per LDS load whose resource was **semantically invalid** (PGV violation / listener-build failure) — distinct from the parse-error `update_failure` bucket; Envoy warns-and-serves. **In envoy-rust this is structurally 0** for the same reason as `update_failure` (the all-fatal posture; the process exits instead). Registered at 0 and **structurally unreachable non-zero**. Bilaterally satisfiable at 0 on fixture 0027. |
| `listener_manager.listener_added` | value-exact (fixture 0027: 1) | Counter; `+1` per listener ADDED to the manager. The count includes **static listeners** — Envoy counts ALL listeners added, not just LDS-supplied ones; envoy-rust mirrors. Bilateral on fixture 0027 because it has **zero static listeners** (the single dynamic listener yields 1 on both sides); the L7 collision backstop (a static listener defined under the same name as the LDS entry) asserts 1 (the static listener only — the collision-skipped LDS entry does not re-tick). **Conditional registration narrowing** — see the §5.2 paragraph below. |
| `listener_manager.total_listeners_active` | value-exact (fixture 0027: 1) | Gauge; the count of currently-active listeners in the manager. **Distinct from `listener_added` in registration:** this gauge keeps its pre-existing 08.2 **unconditional** registration (it predates LDS); phase 19 only tightens it to a bilateral assertion on fixture 0027. The lone gauge of the 6 names (the other 5 are counters). |

**The §5.2 conditional-registration narrowing (recorded divergence, L10/ADR-0050).** The 4 `lds.*` names **and** `listener_added` register ONLY when `dynamic_resources.lds_config` is configured. This is a **deliberate divergence** from Envoy's tree: Envoy emits the `listener_manager.lds.*` subtree conditionally (the lds subtree exists only with LDS configured — both proxies agree here), but Envoy emits the **base** `listener_manager.*` names (`listener_added`, `listener_create_success`, `total_listeners_active`, `workers_started`, …) **unconditionally** on every bootstrap. envoy-rust narrows the base name `listener_added` to the same LDS-configured condition (registers nothing on non-LDS fixtures — verified by the backstop's inertness path (vi), which asserts `listener_added` is ABSENT and no `lds.*` names appear when no `lds_config` is present), so on the fixture-0026 topology (CDS configured, NO lds_config) the `listener_manager.lds.*` subtree + the base `listener_added` name stay **Envoy-only-unasserted**. `total_listeners_active` is the **exception** — it keeps its unconditional 08.2 registration on both LDS and non-LDS fixtures. Recorded explicitly per doctrine D-3.3.

**The L3 Envoy-only enumeration paragraph (LDS).** After a successful load Envoy emits **21** `listener_manager.*` names; envoy-rust emits the **6** above. The 15 Envoy-only unasserted names (ignored by fixture 0027's named-stat scrape — no set-diff on the `Http1KeepAlive` driver) are: `listener_create_success`, `listener_create_failure`, `listener_modified`, `listener_removed`, `listener_stopped`, `listener_in_place_updated`, `total_listeners_warming`, `total_listeners_draining`, `total_filter_chains_draining`, `workers_started`, `lds.update_time`, `lds.update_duration`, `lds.version`, `lds.version_text`, `lds.init_fetch_timeout`. **✧ `listener_create_success` is PER-WORKER** — observed at **12 on a 12-core host** (one tick per worker thread per listener); it is host-core-count-dependent, **NEVER asserted bilaterally**, and is NOT in the 6-name subset. (Envoy additionally carries an `lds.control_plane.*` family, irrelevant to the filesystem transport.) Fixture 0027 also carries the phase-18 `cluster_manager.*` 6-name subset, here with **`cluster_added: 2` / `active_clusters: 2`** (TWO clusters: the static `static_backend` + the CDS-supplied `dynamic_backend`) and `cds.update_attempt/success/failure/rejected` = 1/1/0/0.

**20 entries (file-based RDS):**

> The xDS-family continuation (ADR-0051 SPEC / ADR-0052 PLAN). file-based RDS
> loads route tables from `rds.config_source.path_config_source.path` on each
> HCM at startup; these are the project's first **per-HCM-scoped** xDS stats
> (every name is prefixed `http.<stat_prefix>.rds.<route_config_name>.`, NOT a
> top-level-scope `*_manager.*` name). All derived from the §6.2 empirical
> lock-ins (verified against `envoyproxy/envoy:v1.33.0`, digest
> `sha256:56da5afd…`, 2026-06-02). Envoy emits a fuller `http.<prefix>.rds.<name>.*`
> family after a successful RDS update; envoy-rust's minimum-viable subset is
> **5 names**. Registered **conditionally** — ONLY when the owning HCM's `rds`
> is configured (an inline-route HCM emits no `rds.*` names — L10).

| Stat name | Equivalence | Rationale |
|---|---|---|
| `http.<stat_prefix>.rds.<route_config_name>.update_attempt` | value-exact (fixture 0028: 1) | Counter; one increment per RDS update attempt. envoy-rust's synchronous initial load ticks it once at the file-read+parse step. Initial-load-only at phase-20 scope (no hot-reload) → exactly `1` after startup on both proxies. |
| `http.<stat_prefix>.rds.<route_config_name>.update_success` | value-exact (fixture 0028: 1) | Counter; one increment per successful RDS update (an installed route table). The load-bearing differential proof of the phase: `update_success: 1` can only pass if real Envoy genuinely loaded the RDS file (fixture 0028 asserts it bilaterally). |
| `http.<stat_prefix>.rds.<route_config_name>.update_failure` | value-exact (fixture 0028: 0) | Counter; in Envoy, `+1` per RDS update that hit a **parse error** (malformed envelope / missing `@type`) — Envoy then warns-and-serves. **In envoy-rust this is structurally 0:** all RDS load errors are FATAL pre-construction (the all-fatal posture, ADR-0049 decision 2 extended to RDS by ADR-0052 L4 — see the xDS-wire-state-machine RDS §(c)), so the process exits rather than reach a non-zero `update_failure` state. Registered at 0 and **structurally unreachable non-zero**. Bilaterally satisfiable at 0 on fixture 0028 (a successful load). |
| `http.<stat_prefix>.rds.<route_config_name>.update_rejected` | value-exact (fixture 0028: 0) | Counter; in Envoy, `+1` per RDS update whose resource was **semantically invalid** (PGV violation / route-build failure) — distinct from the parse-error `update_failure` bucket; Envoy warns-and-serves. **In envoy-rust this is structurally 0** for the same reason as `update_failure` (the all-fatal posture; the process exits instead). Registered at 0 and **structurally unreachable non-zero**. Bilaterally satisfiable at 0 on fixture 0028. |
| `http.<stat_prefix>.rds.<route_config_name>.config_reload` | value-exact (fixture 0028: 1) | Counter; `+1` per route-config version applied. **Ticks at initial load** (§6.2 L3 verified — the initial route table counts as the first reload), so the single synchronous load drives it to `1` on both proxies. Subsequent hot-reloads (deferred at phase-20 scope) would tick it again. |

**The per-HCM scoping paragraph (L1).** Every name in the 5-name subset is prefixed `http.<stat_prefix>.rds.<route_config_name>.` — both the `<stat_prefix>` (from the owning HCM's `stat_prefix`) AND the `<route_config_name>` (from the `rds.route_config_name`) are dynamic segments. Fixture 0028's concrete prefix is `http.ingress_http1.rds.local_route.`. This is the project's first xDS stat family scoped to a per-HCM, per-route-config name (vs the phase-18 `cluster_manager.*` / phase-19 `listener_manager.*` top-level-scope names).

**The conditional-registration narrowing (recorded divergence, L5/ADR-0052).** The `rds.*` names register ONLY when the owning HCM's `rds` is configured — a deliberate, recorded narrowing vs Envoy. An **inline-route HCM emits no `rds.*` names** (the route table comes from the static `route_config` on the HCM, with no RDS update lifecycle to count). All **27 pre-existing fixtures** (inline-route HCMs, or CDS/LDS-only topologies) therefore see **zero new envoy-rust names** under `http.<prefix>.rds.*`; only fixture 0028 (the first `rds`-on-HCM fixture) exercises the family. Recorded explicitly per doctrine D-3.3.

**The Envoy-only enumeration paragraph.** After a successful RDS update Envoy emits a fuller `http.<prefix>.rds.<name>.*` family; envoy-rust emits the **5** above. The Envoy-only unasserted names (ignored by fixture 0028's named-stat scrape — no set-diff on the `Http1KeepAlive` driver) are: `version`, `version_text`, `update_time`, `config_reload_time_ms`, `update_empty`, `init_fetch_timeout`, `update_duration`. (Envoy additionally carries an `rds.<name>.control_plane.*` family, irrelevant to the filesystem transport.)

**26 entries (RDS hot-reload — the phase-20 `rds.*` rows gain per-reload semantics; §6.2-verified by ADR-0066 on Linux against `envoyproxy/envoy:v1.33.0`):**

The 5 phase-20 `http.<stat_prefix>.rds.<route_config_name>.*` rows above were locked at their **initial-load-only** values (`update_attempt/update_success/update_failure/update_rejected/config_reload` = `1/1/0/0/1`). Phase 26 makes the route table hot-reloadable, so these counters now **advance per reload** — no NEW stat names, only a per-reload increment semantic (§6.2 P4 verified):

| Reload event | Increment semantics |
|---|---|
| **Successful reload** (valid file change → new route table atomically applied) | `update_attempt` **+1**, `update_success` **+1**, `config_reload` **+1** per successful reload. Verified bilaterally: `1/1/0/0/1` (boot) → `2/2/0/0/2` (after one reload) → `3/3/0/0/3` (after two). Fixture 0034 asserts `2/2/…/2` after its one reload. |
| **Bad reload — malformed YAML / IO / parse error** | `update_attempt` **+1**, `update_failure` **+1**; the last-good table is KEPT (warm-reject, §5.5). Both proxies agree. Fixture 0034's bad-reload probe exercises this case (`update_failure` ticks; `/probe` still routes to the last-good cluster). |
| **Bad reload — `route_config_name` absent from the reloaded envelope** | `update_attempt` **+1**, `update_rejected` **+1**; last-good KEPT. Both proxies agree. Backstop-only (the differential fixture does not drive it). |
| **Bad reload — route references an UNKNOWN cluster** | envoy-rust: `update_attempt` **+1**, `update_rejected` **+1**; last-good KEPT (re-validation against the immutable live cluster set rejects it). **Recorded divergence (ADR-0066):** Envoy instead ACCEPTS the update (`update_success`+`config_reload` tick) and serves `503`/`no_cluster` on that route. envoy-rust diverges because its request path `.expect()`s cluster existence (`crates/envoy-http1/src/hcm.rs:818`); matching Envoy would require a request-time missing-cluster→503 synth path (out of minimum-viable scope). Unobservable in fixture 0034 (which exercises only the malformed bad-reload, where both agree); surfaces only in the in-process backstop. |

The §5.2 conditional-registration invariant is unchanged (names register only for `rds`-configured HCMs; the 33 pre-existing fixtures including the idle-watcher 0028 see no new names and no value change). The Envoy-only `rds.<name>.{version,version_text,update_time,config_reload_time_ms,update_duration,update_empty,init_fetch_timeout}` names stay unasserted (they advance on Envoy's side per reload but remain outside the 5-name subset).

**21 entries (file-based EDS):**

> The xDS-family continuation (ADR-0053 SPEC / ADR-0054 PLAN). file-based EDS
> loads a cluster's `ClusterLoadAssignment` (endpoints) from
> `eds_cluster_config.eds_config.path_config_source.path` at startup, for a
> cluster declared `type: EDS`. Unlike the manager-level CDS/LDS singletons
> (`cluster_manager.*` / `listener_manager.*`) and the per-HCM RDS family
> (`http.<prefix>.rds.<name>.*`), these stats live under the **per-cluster**
> `cluster.<name>.*` namespace — the EDS subscription is scoped to the cluster
> it feeds, so the `update_*` family extends the existing per-cluster
> namespace rather than introducing a new top-level scope. All derived from the
> §6.2 empirical lock-in **L3** (verified against `envoyproxy/envoy:v1.33.0`,
> digest `sha256:56da5afd…`, 2026-06-05; ran LOCALLY). Envoy emits a fuller
> `cluster.<name>.` EDS-related family after a successful initial load; envoy-rust's
> minimum-viable subset is **4 names**. Registered at the per-cluster stat site
> (`crates/envoy-cluster/src/cluster.rs`), **conditionally** — ONLY for clusters
> whose `cluster_type == Eds`.

| Stat name | Equivalence | Rationale |
|---|---|---|
| `cluster.<name>.update_attempt` | value-exact (fixture 0029: 1) | Counter; one increment per EDS subscription update attempt. envoy-rust's synchronous `load_dynamic_resources` ticks it once at the file-read+parse step. Both proxies emit 1 on the single initial load (no hot-reload at phase-21 scope). |
| `cluster.<name>.update_success` | value-exact (fixture 0029: 1) | Counter; one increment per EDS update that produced an installed endpoint set. The load-bearing differential proof of the phase: `update_success: 1` can only pass if real Envoy genuinely loaded the EDS file's `ClusterLoadAssignment` (fixture 0029 asserts it bilaterally). |
| `cluster.<name>.update_failure` | value-exact (fixture 0029: 0) | Counter; in Envoy, `+1` per EDS update that hit a **parse error** (malformed envelope / missing `@type`) — Envoy then warms-and-503s. **In envoy-rust this is structurally 0:** all EDS load errors are FATAL pre-construction (the L4 all-fatal posture, ADR-0049 decision 2 extended to EDS by ADR-0054 — see the xDS-wire-state-machine EDS §(c)), so the process exits rather than reach a non-zero `update_failure` state. Registered at 0 and **structurally unreachable non-zero**. Bilaterally satisfiable at 0 on fixture 0029 (a successful load). |
| `cluster.<name>.update_empty` | value-exact (fixture 0029: 0) | Counter; in Envoy, `+1` per EDS update whose `resources:` list was **empty** (`update_empty: 1` co-fires with `update_success: 1`, route 503). **In envoy-rust this is structurally 0:** an empty endpoint set is FATAL pre-construction (the existing `EmptyClusterEndpoints` validator under the all-fatal posture; the process exits instead). Registered at 0 and **structurally unreachable non-zero**. Bilaterally satisfiable at 0 on fixture 0029. |

> Plus the **data-plane witness** `cluster.<name>.upstream_rq_total` (value-exact
> 1 on fixture 0029) — registered **unconditionally** (it predates EDS; it is not
> part of the conditional EDS family) and asserted to prove the EDS-supplied
> endpoint actually served the single GET.

**The per-cluster scoping paragraph (L3).** The 4-name `update_*` subset is prefixed `cluster.<name>.` — the same per-cluster namespace that owns `upstream_rq_total`, `upstream_cx_total`, the `circuit_breakers.*` family, etc. (fixture 0029's concrete prefix is `cluster.eds_backend.`). These EDS-subscription counters extend that existing per-cluster namespace; they are NOT a new top-level-scope family (contrast the phase-18 `cluster_manager.*` / phase-19 `listener_manager.*` manager singletons and the phase-20 per-HCM `http.<prefix>.rds.<name>.*` family). `update_rejected` is the only `update_*` name that is EDS-exclusive in Envoy (L10); it is **not** in the asserted subset (structurally 0 in envoy-rust — the all-fatal posture).

**The §5.2 conditional-registration narrowing (recorded divergence, L10/ADR-0054).** envoy-rust registers the `update_*` family ONLY for clusters whose `cluster_type == Eds` — a deliberate, recorded narrowing vs Envoy. Envoy emits `cluster.<name>.update_{attempt,success,failure,empty,no_rebuild}` (all at 0) for **every** cluster regardless of type (STATIC/STRICT_DNS included — PRE-EXISTING Envoy behavior, true since phase 06.1); only `cluster.<name>.update_rejected` is EDS-exclusive on Envoy. envoy-rust's per-cluster `cluster_type == Eds` gate adds **ZERO** new names to the existing 28 fixtures (whose clusters are non-EDS — the names were already on the envoy-only allow-list / excluded by fixture 0011's set-diff), preserving the regression baseline with zero edits. The inertness backstop verifies envoy-rust emits no `cluster.<name>.update_*` for a STATIC-only bootstrap. Recorded explicitly per doctrine D-3.3.

**The membership-gauge narrowing (recorded divergence, L3/ADR-0054).** For a non-health-checked EDS cluster, envoy-rust does **NOT** emit `cluster.<name>.membership_healthy` or `cluster.<name>.membership_total`. `membership_healthy` registers only when `health_checks` is configured (`crates/envoy-cluster/src/cluster.rs:926`; the explicit "no membership_healthy gauge for a plain cluster" inertness test at `:2227`), and `membership_total` does NOT exist in envoy-rust at all; fixture 0029 has no health checks. Envoy emits **both** (`membership_healthy: 1`, `membership_total: 1`) for every cluster → these are **allow-listed envoy-only, NOT broadened** — broadening would change the `:2227` inertness test + existing-fixture stat output, out of the minimum-viable scope. The endpoint set is differentially proven instead by `upstream_rq_total: 1` (the data-plane witness) + `update_success: 1`.

**The L3 Envoy-only enumeration paragraph (EDS).** After a successful initial load Envoy emits a fuller `cluster.<name>.` EDS-related family; envoy-rust emits the **4** `update_*` names above (+ the unconditional `upstream_rq_total` witness). The Envoy-only unasserted names (ignored by fixture 0029's named-stat scrape — no set-diff on the `Http1KeepAlive` driver) are: `update_no_rebuild`, `update_rejected` (structurally unreachable in envoy-rust — the all-fatal posture, L4), `update_time`, `update_duration` (histogram), `membership_change`, `membership_healthy`, `membership_total`, `membership_degraded` (`degraded`), `membership_excluded` (`excluded`), `assignment_stale`/`assignment_timeout_received`/`assignment_use_cached` (`assignment_*`), `version`, `version_text`, `warming_state`. (None of the 4 emitted `update_*` values change pre- vs post-GET — request counters live under the other `cluster.<name>.*` names.)

**27 entries (EDS hot-reload — the phase-21 `cluster.<name>.update_*` rows gain per-reload semantics; §6.2-verified at the state-2 PLAN-write by ADR-0068 on Linux against `envoyproxy/envoy:v1.33.0`):**

The 4 phase-21 `cluster.<name>.update_{attempt,success,failure,empty}` rows above were locked at their **initial-load-only** values (`update_attempt/update_success/update_failure/update_empty` = `1/1/0/0`), with `update_rejected` declared structurally-unreachable / Envoy-only (the all-fatal STARTUP posture). Phase 27 makes the cluster's endpoint set hot-reloadable, so these counters now **advance per reload** — and **`update_rejected` is PROMOTED from Envoy-only: it is now REGISTERED (at 0) and EMITTED by envoy-rust on the reload reject paths** (ADR-0068 §Decision-3). So the post-boot EDS counter state is `update_attempt/update_success/update_failure/update_rejected/update_empty` = `1/1/0/0/0` — no NEW stat names beyond promoting `update_rejected` into the asserted family, only the per-reload increment semantic. **The EDS reload taxonomy MIRRORS Envoy in ALL FIVE classes** (no recorded behavioral divergence — only the inotify-vs-mtime mechanism divergence of §2.2 remains; the SPEC's projected apply-empty warm-reject was CORRECTED to a MATCH by ADR-0068):

| Reload event | Increment semantics |
|---|---|
| **Successful reload** (valid file change → new endpoint set atomically applied) | `update_attempt` **+1**, `update_success` **+1** per successful reload. Verified bilaterally: `1/1/0/0/0` (boot) → `2/2/0/0/0` (after one reload) → `3/3/0/0/0` (after two). Fixture 0035 asserts `2/2` after its one reload (the bilateral DATA-PLANE flip backend_1→backend_2 is the differential proof; the counter / config_dump / bad-reload proofs are BACKSTOP-only — the phase-26 fixture-0034 / M26-4 precedent). |
| **Bad reload — malformed YAML / IO / parse error** | `update_attempt` **+1**, `update_failure` **+1**; the last-good endpoint set is KEPT (warm-reject). Both proxies agree. Fixture 0035's bad-reload probe drives this class (`update_failure` ticks; `/probe` still routes to the last-good endpoint). |
| **Bad reload — no CLA matches the cluster's selection name (`service_name` or cluster name; envelope non-empty)** | `update_attempt` **+1**, `update_rejected` **+1**; last-good KEPT. Both proxies agree. Backstop-only. |
| **Bad reload — matched CLA has an unparseable / non-numeric endpoint address** | `update_attempt` **+1**, `update_rejected` **+1**; last-good KEPT. Both proxies agree. Backstop-only. |
| **Apply-empty — matched CLA has `endpoints: []`** | `update_attempt` **+1**, **`update_success` +1**, and the **empty set is APPLIED** (0 hosts → `pick()` returns None → `synth_no_healthy_upstream` **503 "no healthy upstream"**, 19 bytes); last-good NOT kept. **This MIRRORS Envoy** (ADR-0068 — the SPEC's warm-reject-empty projection was CORRECTED to a MATCH; safe because `pick()` already returns None on empty and the 503 path exists). **Deliberate startup-vs-reload asymmetry:** `from_bootstrap` STILL rejects an empty cluster at STARTUP (the all-fatal posture, §2.2 phase-21 §(c)); only the RELOAD path applies-empty to mirror Envoy. Backstop-only (asserts the 503 + the byte-exact body). |
| **Empty envelope — `resources: []` (zero CLAs)** | `update_attempt` **+1**, **`update_empty` +1**; last-good KEPT (no-op). Envoy DISTINGUISHES this from apply-empty; envoy-rust mirrors. Backstop-only. |

The §5.2 conditional-registration narrowing is unchanged (the `update_*` family registers only for `cluster_type == Eds`; the pre-existing non-EDS fixtures see no new names and no value change — and the `update_rejected` promotion adds it to the conditional EDS family, NOT to every cluster, preserving the existing-fixture baseline). The membership-gauge narrowing (no `membership_healthy`/`membership_total` for a plain EDS cluster) and the Envoy-only `cluster.<name>.{update_no_rebuild,update_time,update_duration,membership_*,assignment_*,version,version_text,warming_state}` enumeration stay as locked at phase 21 (they advance on Envoy's side per reload but remain outside the asserted subset).

**22 entries (jwt_authn filter):**

> The HTTP-filter-family continuation (ADR-0055 SPEC / ADR-0056 PLAN). The
> `envoy.filters.http.jwt_authn` filter is a decode-side authentication gate:
> it selects the first `rules[]` entry whose `RouteMatch` matches the request,
> extracts the JWT from `Authorization: Bearer`, verifies RS256 against the
> rule's provider JWKS, and validates `iss`/`aud`/`exp`/`nbf` (the `envoy-jwt`
> crate). On success → `Decision::Continue` + `allowed` increment + (when the
> provider's `forward` is `false`, the Envoy default) the `Authorization`
> header is stripped upstream (§6.2 L6). On failure → `denied` increment + a
> `Decision::StopAndSend` 401/403 local reply (the Envoy-faithful body + a
> `www-authenticate` header). A request matching **no** rule is allowed and
> ticks `allowed` (§6.2 L4). The 2 stats live under the HCM-embedded
> `http.<hcm_stat_prefix>.jwt_authn.*` namespace — the same threading as RBAC
> (phase 10) and Fault (phase 11), sourced from the parent HCM's `stat_prefix`
> (the jwt_authn filter has no `stat_prefix` field of its own). All derived
> from the §6.2 empirical lock-in (verified against `envoyproxy/envoy:v1.33.0`;
> differential fixture 0030). Envoy emits a fuller `http.<prefix>.jwt_authn.*`
> family; envoy-rust's minimum-viable subset is **2 names**.

| Stat name | Equivalence | Rationale |
|---|---|---|
| `http.<hcm_stat_prefix>.jwt_authn.allowed` | value-exact | Counter; one increment per request the filter lets through. Two pass paths both tick it: (a) a rule matched AND the JWT verified+validated successfully; (b) **no rule matched** the request (the §6.2 L4 pass-through — an unmatched request is not subject to any JWT requirement at minimum scope). Incremented synchronously in `JwtAuthnFilter::decode_headers` before returning `Decision::Continue`. Never ticks on a denied (401/403) request. Upstream Envoy v1.33 emits the same name at the `http.<prefix>.jwt_authn.*` namespace; the no-rule pass-through folds into Envoy's allowed count identically. |
| `http.<hcm_stat_prefix>.jwt_authn.denied` | value-exact | Counter; one increment per request the filter rejects. Ticks on **every** failure class: each 401 (missing/non-Bearer token, NotInForm, BadHeaderJson, BadPayloadJson, NoMatchingKey, VerificationFails, IssuerMismatch, Expired, NotYetValid) **and** the single 403 (AudienceNotAllowed). Incremented synchronously before constructing the `Decision::StopAndSend(FilterResponse)` local reply. Both proxies emit one increment per denied request. |

> **Envoy-only sibling stats (unasserted, NOT broadened).** Upstream Envoy
> emits 5 additional `http.<prefix>.jwt_authn.*` siblings that envoy-rust does
> **NOT** emit at minimum scope (per §6.2 L5): `cors_preflight_bypassed`,
> `jwks_fetch_success`, `jwks_fetch_failed`, `jwt_cache_hit`, `jwt_cache_miss`.
> The last four belong to the deferred remote-JWKS + JWT-cache features (local
> inline JWKS only at phase-22 scope); `cors_preflight_bypassed` belongs to the
> deferred CORS-preflight bypass. These are **Envoy-only-unasserted** — fixture
> 0030's named-stat scrape lists only the 2 emitted names, so there is **no
> set-diff** on the scrape (the harness asserts the named values, not set
> equality of the `jwt_authn.*` sub-scope), and the 5 siblings need no
> allow-list entry.

**Response body — jwt_authn 401/403 local replies**

> The §6.2 L2 failure-class → HTTP-wire mapping. Each `envoy-jwt` `JwtError`
> class (plus the filter-local missing/non-Bearer case) maps to a fixed HTTP
> status + a byte-exact response body + a `www-authenticate` header. Bodies are
> the upstream Envoy v1.33 source-hardcoded strings (§6.2 L2 verified against
> `envoyproxy/envoy:v1.33.0`); they carry **NO trailing newline** and the local
> reply sets `content-type: text/plain`. **Equivalence = byte-exact body +
> status + `www-authenticate` value.** Emitted by `error_reply` / `missing_reply`
> in `crates/envoy-filter/src/jwt_authn.rs`.

| Failure class | Status | Body (byte-exact) | Body length | `www-authenticate` |
|---|---|---|---|---|
| Missing token / non-Bearer Authorization | 401 | `Jwt is missing` | 14 | `Bearer realm="{realm}"` (NO `error=`) |
| NotInForm | 401 | `Jwt is not in the form of Header.Payload.Signature with two dots and 3 sections` | 79 | `Bearer realm="{realm}", error="invalid_token"` |
| BadHeaderJson | 401 | `Jwt header is an invalid JSON` | 29 | `…, error="invalid_token"` |
| BadPayloadJson | 401 | `Jwt payload is an invalid JSON` | 30 | `…, error="invalid_token"` |
| NoMatchingKey | 401 | `Jwks doesn't have key to match kid or alg from Jwt` | 50 | `…, error="invalid_token"` |
| VerificationFails | 401 | `Jwt verification fails` | 22 | `…, error="invalid_token"` |
| IssuerMismatch | 401 | `Jwt issuer is not configured` | 28 | `…, error="invalid_token"` |
| Expired | 401 | `Jwt is expired` | 14 | `…, error="invalid_token"` |
| NotYetValid | 401 | `Jwt not yet valid` | 17 | `…, error="invalid_token"` |
| AudienceNotAllowed | **403** | `Audiences in Jwt are not allowed` | 32 | `…, error="invalid_token"` |

> Only the missing/non-Bearer case omits `error="invalid_token"` (it is a
> credentials-absent, not a credentials-invalid, condition per RFC 6750
> §3.1); every other class carries `www-authenticate: Bearer realm="{realm}",
> error="invalid_token"`. The `JwtError::InvalidJwks` class is config-load-time
> fatal (ADR-0049 all-fatal posture — the process never reaches a request with
> an unparseable JWKS), so it has no request-time wire mapping; the filter folds
> it defensively into the `Jwt verification fails` 401 row, but it is
> structurally unreachable at request time. The 401 reply sets reason
> `Unauthorized`; the single 403 reply sets reason `Forbidden`.

**The `www-authenticate` value-exact paragraph (L3).** The `www-authenticate`
header is **value-exact across proxies** — it is NOT on the response-header
allow-list (it is compared byte-for-byte by `set_equal_modulo_allow_list`). The
realm is reproduced byte-for-byte as `http://<Host-header><path>`, and the
differential fixture 0030 drives a **fixed** `Host: envoy.test` (§6.2 L3) so the
realm is deterministic across proxies (`http://envoy.test<path>`). The
`Authorization`-stripping on the success path (§6.2 L6, `forward: false` default)
is observed upstream as the absence of the request header — the differential
fixture's echo-backend reflects request headers so the strip is bilaterally
witnessed.

**23 entries (CORS filter):**

> The HTTP-filter-family fifth phase (ADR-0057 SPEC / ADR-0058 PLAN). The
> `envoy.filters.http.cors` filter performs origin allow-matching on every
> request that reaches it (when the route carries a `typed_per_filter_config`
> CORS policy). It provides a decode-side preflight short-circuit (OPTIONS +
> origin + `access-control-request-method`) and an encode-side actual-request
> decoration (push `access-control-allow-origin` + conditional siblings onto
> the upstream response). The 2 stats live under the HCM-embedded
> `http.<hcm_stat_prefix>.cors.*` namespace — the same threading as RBAC
> (phase 10), Fault (phase 11), and jwt_authn (phase 22), sourced from the
> parent HCM's `stat_prefix`. All derived from the §6.2 empirical lock-in
> (verified against `envoyproxy/envoy:v1.33.0`; differential fixture 0031).
> envoy-rust's minimum-viable subset is **2 names**.

| Stat name | Equivalence | Rationale |
|---|---|---|
| `http.<hcm_stat_prefix>.cors.origin_valid` | value-exact | Counter; one increment per request that carries an `origin` header whose value is **matched** by the route's `allow_origin_string_match` list (exact / prefix / suffix / regex / contains). Ticks on BOTH the preflight short-circuit path AND the actual-request decoration path — any present+allowed origin ticks it exactly once per `decode_headers` call. A **no-origin** request ticks neither counter (L5). Never ticks when `active_policy` is `None` (the filter is inert — either no filter-chain entry or the matched route carries no `typed_per_filter_config` CORS key). Upstream Envoy v1.33 emits the same name at the `http.<prefix>.cors.*` namespace. |
| `http.<hcm_stat_prefix>.cors.origin_invalid` | value-exact | Counter; one increment per request that carries an `origin` header whose value is **not matched** by the allow-list. Ticks on the disallowed-origin path (both preflight pass-through and actual-request pass-through). A **no-origin** request ticks neither counter; an absent or inert filter ticks neither counter. Both proxies emit one increment per present-but-unmatched origin at the decision site in `CorsFilter::decode_headers`. |

> **Envoy-only sibling stats (unasserted, NOT broadened).** Upstream Envoy
> emits additional `http.<prefix>.cors.*` siblings that envoy-rust does
> **NOT** emit at minimum scope: `downstream_rq_total`, `downstream_rq_success`,
> `downstream_rq_failed`, `downstream_rq_cors_preflight` (various request-level
> subtotals). These are **Envoy-only-unasserted** — fixture 0031's named-stat
> scrape lists only the 2 emitted names, and the siblings need no allow-list
> entry.

**Response headers — cors `access-control-*`**

> The §6.2 L2/L3 wire mapping for the CORS access-control header family.
> These headers are NOT on the response-header allow-list; they are
> compared value-exact by `set_equal_modulo_allow_list`. All headers
> below are **absent** when the request carries no origin, or when
> `active_policy` is `None`, or when the origin is disallowed (L4).

**Preflight short-circuit set (L2)** — emitted ONLY on
`OPTIONS + origin (matched) + access-control-request-method`; status 200,
empty body:

| Header name | Condition | Value |
|---|---|---|
| `access-control-allow-origin` | always (when origin matched) | Echoes the request `Origin` header verbatim (NOT `*`) |
| `access-control-allow-credentials` | only if `allow_credentials: true` | `true` (literal string) |
| `access-control-allow-methods` | only if `allow_methods` configured | configured value verbatim |
| `access-control-allow-headers` | only if `allow_headers` configured | configured value verbatim |
| `access-control-max-age` | only if `max_age` configured | configured value verbatim |
| `access-control-expose-headers` | only if `expose_headers` configured | configured value verbatim |

**Actual-request decoration set (L3)** — pushed onto the upstream response
on the encode side for any non-preflight allowed-origin request (including
`OPTIONS` without `access-control-request-method`):

| Header name | Condition | Value |
|---|---|---|
| `access-control-allow-origin` | always (when origin matched) | Echoes the request `Origin` header verbatim |
| `access-control-allow-credentials` | only if `allow_credentials: true` | `true` (literal string) |
| `access-control-expose-headers` | only if `expose_headers` configured | configured value verbatim |

> `access-control-allow-methods`, `access-control-allow-headers`, and
> `access-control-max-age` are **preflight-only** — they do NOT appear on
> actual-request decoration (L3). No `vary` header is emitted (verified
> empirically vs Envoy v1.33.0; contrary to RFC 6454 §7.2 advice but
> matches upstream Envoy behaviour). The `access-control-allow-origin` value
> **always echoes the request Origin verbatim** — envoy-rust does NOT emit
> the wildcard `*` under any configured-policy path.

**Preflight local-reply wire shape (L2).**

- Status: **200** (NOT 204; verified §6.2 L2 against Envoy v1.33.0).
- Body: **empty** (zero bytes); `content-length: 0` is stamped by the
  H1/H2 synth decorators downstream of the filter pipeline (same pattern
  as `jwt_authn` / `rbac` local replies — `CorsFilter` does NOT set
  `content-length` itself).
- **No `content-type` header** (ADR-0059). Upstream Envoy v1.33 does NOT
  emit `content-type` on an **empty-body** local reply. The H1
  `decorate_filter_synth_response` / H2 `decorate_filter_synth_response_h2`
  decorators add `content-type: text/plain` only-if-missing AND only when
  the body is non-empty — so the (empty-body) CORS preflight 200 carries no
  `content-type`, while every non-empty filter local reply (rbac 403 / fault
  / jwt_authn 401 / local_ratelimit 429 / overflow 503) is byte-unchanged.
- A **disallowed-origin** `OPTIONS + origin + ACRM` request is NOT
  short-circuited — it proxies through to the upstream and gets no CORS
  decoration (L4; `origin_invalid` ticks instead).
- An `OPTIONS + origin` request **without** `access-control-request-method`
  is treated as an **actual request** (not a preflight) — it continues
  through the pipeline and receives the actual-request decoration set on
  the encode side (L2 preflight detection requires all three signals).

**Cross-reference:** ADR-0058 (phase-23 PLAN-write lock-in); ADR-0059 (the
Task-7 empty-body `content-type` omission + the H1-pool `Connection: close`
correction below).

**24 entries (CSRF filter):**

> The HTTP-filter-family sixth phase (ADR-0060 SPEC / ADR-0061 PLAN). The
> `envoy.filters.http.csrf` filter is a decode-side cross-site-request-forgery
> guard. For the modify-method set `{POST, PUT, DELETE, PATCH}` (L2) it compares
> the request's **source origin** (`Origin`, falling back to `Referer`) against
> the **target** (`Host` / `:authority`); the request is valid iff the source
> matches the target OR an `additional_origins` `StringMatcher` matches it (L3).
> All comparison is on the **scheme-stripped `host[:port]` authority** (Envoy
> `Url::hostAndPort()` semantics, ADR-0061 L3 — no scheme, no path/query/fragment).
> Invalid or missing-source modify requests get a 403 `Invalid origin` local
> reply (L4); safe methods and a deterministic-0% policy pass through untouched.
> The 3 stats live under the HCM-embedded `http.<hcm_stat_prefix>.csrf.*`
> namespace — the same threading as RBAC (phase 10), Fault (phase 11), jwt_authn
> (phase 22), and cors (phase 23), sourced from the parent HCM's `stat_prefix`.
> All derived from the §6.2 empirical lock-in (verified against
> `envoyproxy/envoy:v1.33.0`). envoy-rust's minimum-viable subset is **3 names**,
> exactly **one** of which ticks per evaluated modify request (mutually
> exclusive, L5); safe methods and a disabled policy tick nothing.

| Stat name | Equivalence | Rationale |
|---|---|---|
| `http.<hcm_stat_prefix>.csrf.request_valid` | value-exact | Counter; one increment per evaluated **modify-method** request whose source origin is valid (source `host[:port]` == target `host[:port]`, OR an `additional_origins` matcher matches the source). Incremented synchronously in `CsrfFilter::decode_headers` before `Decision::Continue`. Mutually exclusive with `request_invalid` / `missing_source_origin` — exactly one of the three ticks per evaluated modify request. Never ticks for a safe method (`GET/HEAD/OPTIONS/TRACE/…`, L2) or a deterministic-0% policy (L6). Upstream Envoy v1.33 emits the same name at the `http.<prefix>.csrf.*` namespace. |
| `http.<hcm_stat_prefix>.csrf.request_invalid` | value-exact | Counter; one increment per evaluated modify-method request that carries a source origin (`Origin`, fallback `Referer`) which does NOT match the target and is NOT in `additional_origins` → the 403 `Invalid origin` local reply (L4). Incremented synchronously before constructing the `Decision::StopAndSend(FilterResponse)`. Mutually exclusive with the sibling counters. Both proxies emit one increment per invalid-origin modify request at the decision site in `CsrfFilter::decode_headers`. |
| `http.<hcm_stat_prefix>.csrf.missing_source_origin` | value-exact | Counter; one increment per evaluated modify-method request with **no usable source origin** (neither `Origin` nor `Referer` present, or both reduce to an empty authority) → the 403 `Invalid origin` local reply (L4). Incremented synchronously before the `Decision::StopAndSend`. Mutually exclusive with the sibling counters. A missing source is treated as invalid (fail-closed) but is counted separately from `request_invalid` for diagnosability. |

> **Envoy-only sibling stats (unasserted, NOT broadened).** Upstream Envoy
> emits additional `http.<prefix>.csrf.*` siblings that envoy-rust does **NOT**
> emit at minimum scope (e.g. `request_invalid_origin_with_shadow` and related
> shadow-mode subtotals — `shadow_enabled` is deferred). These are
> **Envoy-only-unasserted** and need no allow-list entry.

**CSRF local-reply wire shape (L4).**

- Status: **403** (`Forbidden`).
- Body: **`Invalid origin`** — exactly **14 bytes**, NO trailing newline.
  Set verbatim by `CsrfFilter` via `Bytes::from_static`.
- `content-type: text/plain` is stamped by the H1/H2 synth decorators
  downstream of the filter pipeline (the non-empty-body branch — same pattern
  as the rbac 403 / jwt_authn 401 / local_ratelimit 429 local replies);
  `content-length` (`14`) is likewise stamped by the decorators. `CsrfFilter`
  itself sets neither.
- Fires for BOTH the invalid-origin path (`request_invalid` ticks) AND the
  missing-source path (`missing_source_origin` ticks) — the same 403 body on
  both.

**CSRF chain-base / route-replace semantics (L6/L7).**

> Unlike `cors` (which goes inert without a route `typed_per_filter_config`
> entry), the chain-level `CsrfPolicy` is an **always-applied base**: every
> request reaching the filter is guarded by it. A per-route `CsrfPolicy`
> (attached via `typed_per_filter_config["envoy.filters.http.csrf"]`)
> **REPLACES the base wholesale** for the matched route (ADR-0061 L6) — a route
> override at 0% disables enforcement on a 100% chain, and an override at 100%
> enables enforcement on a 0% chain. A route that carries NO csrf override is
> still guarded by the chain base. The effective policy's `filter_enabled`
> (validated 0%/100% — deterministic) gates enforcement. The
> `PerRouteConfigForAbsentFilter` divergence applies to csrf (ADR-0061 L7): a
> route may carry a csrf `typed_per_filter_config` even when the filter chain
> has no csrf entry — Envoy validates the route config regardless; envoy-rust's
> per-route override is only consulted when the chain DOES carry the filter.

**Cross-reference:** ADR-0060 (phase-24 SPEC lock-in); ADR-0061 (PLAN-write
lock-in — chain-base/route-replace, scheme-stripped origin, 403 body).

**25 entries (Buffer filter):**

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

**cdn_loop filter (ADR-0076 SPEC / ADR-0077 §6.2-LOCKED).**

> The HTTP-filter-family ninth phase. `envoy.filters.http.cdn_loop` is Envoy's
> RFC 8586 `CDN-Loop` request-header filter (decode-side, header-only, fully
> self-contained — no upstream state, no per-route config, no stat). For each
> request the filter coalesces all `CDN-Loop` request headers into one
> comma-joined RFC-8586 `cdn-info` list, parses it (strict RFC-7230 token
> grammar), counts how many entries equal the configured `cdn_id` (CASE-SENSITIVE,
> parameters IGNORED), and: malformed → 400 reject; `count > max_allowed_occurrences`
> (default 0) → 502 loop reject; else appends `cdn_id` (COMMA-ONLY) and forwards.
> Inert when not in the chain (no existing filter reads `CDN-Loop` → all 38
> pre-existing fixtures 0001-0038 stay green — the 07.1 foundation-slice
> regression-equivalence property, proven in-process by the Task-5 no-op witness).
> Differentially proven by fixture `0039-http-filter-cdn-loop` (5 probes, STRONG
> cross-proxy byte-exact) against `envoyproxy/envoy:v1.33.0`.

**cdn_loop reject local-reply wire shapes (ADR-0077 §6.2-LOCKED).**

- **Loop** (`count(cdn_id) > max_allowed_occurrences`): status **502** (`Bad Gateway`);
  body **`The server has detected a loop between CDNs.`** — exactly **44 bytes**, NO
  trailing newline. Envoy emits `%RESPONSE_CODE_DETAILS% = cdn_loop_detected`; envoy-rust
  does NOT model response-code-details (the csrf precedent) and the fixture does not assert it.
- **Malformed** (`parse_cdn_loop` → `Err`): status **400** (`Bad Request`); body
  **`Invalid CDN-Loop header in request.`** — exactly **35 bytes**, NO trailing newline.
  Envoy emits `%RESPONSE_CODE_DETAILS% = invalid_cdn_loop_header` (not modeled / not asserted).
- Both bodies are set verbatim via `Bytes::from_static`; `content-type: text/plain` +
  `content-length` + `server` + `connection` are stamped by the H1/phase-11-H2 synth
  decorators (`decorate_filter_synth_response{,_h2}`). Under the differential close-driver
  BOTH reject probes carry `connection: close` on BOTH proxies (value-compared — `connection`
  is not in the header allow-list — the 0032-csrf-403 reject precedent).

**cdn_loop append byte-shape (ADR-0077 §6.2-LOCKED, validated by fixture 0039).**

- **COMMA-ONLY, no space.** No `CDN-Loop` present → the forwarded request carries the bare
  `CDN-Loop: {cdn_id}`. A foreign entry present → `{existing},{cdn_id}` (e.g.
  `othercdn.example` → `othercdn.example,mycdn.example`). (⚠ the SPEC §1.3 example wrote the
  comma-SPACE form; ADR-0077 corrects it to comma-only.)
- **Empty list entries are PRESERVED verbatim** on append (the append concatenates the RAW
  coalesced header bytes, not a parsed reserialization): `othercdn.example,` →
  `othercdn.example,,mycdn.example`. Empty entries (`a,,b`, leading/trailing comma, `,,,`)
  are NOT malformed; OWS around entries is trimmed for COUNTING but the raw bytes flow on append.
- The egress header-NAME casing is NOT differentially pinned by fixture 0039 (the
  `http1-echo-server` lowercases reflected header names → only the appended VALUE byte-shape is
  cross-proxy-proven). envoy-rust preserves the first existing entry's key casing on append and
  adds `cdn-loop` (lowercase) when absent.

**cdn_loop parser grammar (strict RFC-7230 + RFC-8586; the §A oracle, ADR-0077).**

- A `cdn-id` MUST be a bare RFC-7230 `token` (tchars). A quoted-string id (even a well-formed
  `"mycdn.example"`) or a non-tchar id (space/`/`/`@`/tab) → MALFORMED (400). A `parameter` is
  `name=value`; a bare param without `=value` → 400; an unterminated quoted-string → 400.
- Multiple `CDN-Loop` request headers are coalesced into one comma-joined list before counting;
  matching is CASE-SENSITIVE and IGNORES parameters (`mycdn.example; foo=bar` counts as a
  `mycdn.example` match → 502 at default `max_allowed_occurrences: 0`).

**cdn_loop config validity (ALL BOOT-FATAL — ADR-0049 / ADR-0077).**

- A valid `cdn_id` is a non-empty bare RFC-7230 token. Empty `cdn_id` → `ConfigError::CdnLoopEmptyCdnId`;
  a comma-containing or otherwise non-tchar `cdn_id` → `ConfigError::CdnLoopInvalidCdnId`. Both fail
  config-load (the container never serves). `@type` =
  `type.googleapis.com/envoy.extensions.filters.http.cdn_loop.v3.CdnLoopConfig`; fields `cdn_id`
  (string) + `max_allowed_occurrences` (uint32, default 0).

**cdn_loop stats — NONE** (the phase-21/24/28/29/30 no-stat discipline; effects surface only in the
generic HCM `downstream_rq_{2xx,4xx,5xx}`). **Deferred non-goals (ADR-0076):** per-route
`typed_per_filter_config` for cdn_loop; RFC 8586 `cdn-info` parameter semantics beyond counting;
encode-side behavior (cdn_loop is request-only).

**H1 upstream connection-pool `Connection: close` single-use (ADR-0059).**

> When an upstream H1 response carries `Connection: close`, the H1 connection
> pool **invalidates** that connection (destroys it on `Drop` rather than
> returning it to the idle list) — matching Envoy's single-use treatment of an
> upstream `Connection: close`. A pooled connection to a `Connection: close`
> backend (e.g. the `http1-echo-server`) is therefore never reused; reuse for
> keep-alive upstreams (no `Connection: close` — the pooling fixtures 0020/0021
> backends) is unaffected (the invalidate branch never fires). Surfaced by
> fixture 0031's four sequential probes over one downstream connection (the
> first multi-request fixture over a `Connection: close` upstream).

**06.1 Prometheus exposition shape divergence (06.1 fixture 0011):**

> Upstream Envoy's Prometheus emitter projects dynamic name segments
> (the `<name>` in `listener.<name>.downstream_cx_total`, the
> `<stat_prefix>` in `http.<stat_prefix>.downstream_rq_total`, etc.) into
> Prometheus *labels*: the wire shape is
> `envoy_listener_downstream_cx_total{envoy_listener_address="0.0.0.0_10000"} 0`.
> envoy-rust's emitter (`crates/envoy-stats/src/prometheus.rs`) instead
> projects the dynamic segment directly into the metric name:
> `envoy_listener_ingress_http_downstream_cx_total 0`.
>
> Both projections carry the same counter; only the Prometheus
> name-vs-label shape differs. Fixture 0011 bridges this via paired
> `allowlist_envoy_only` / `allowlist_envoy_rust_only` entries (see
> `tests/fixtures/0011-admin-stats-prometheus/expectations.yaml`).
> The dot-tree contract above (`http.<stat_prefix>.downstream_rq_total`
> as value-exact) remains the authoritative semantic — the emitter-side
> shape divergence does not loosen the equivalence dimension.
>
> This divergence is documented for transparency; resolution defers to
> a later phase that adds a `StatsTagExtractor`-equivalent which
> extracts the dynamic segments back into Prometheus labels at scrape
> time. When that lands, the paired allow-list entries drop together
> and this paragraph is removed (no contract loosening).

---

## Admin endpoint body shapes

> **To be filled per-phase as needed.**
>
> Authored per phase 08.1 SPEC §2.1. One row per admin endpoint with the body
> kind + per-endpoint equivalence disposition. Tasks 6/7/8/9 of phase 08.1
> populate `/config_dump`, `/server_info`, `/clusters`, `/listeners`
> respectively. Future POST-bearing admin surfaces (08.2 family) and any
> later admin endpoints append rows here with the same columns.

| Endpoint | Method | Body kind | Equivalence disposition |
|---|---|---|---|
| `/config_dump` | GET | JSON object | Top-level shape `{ "configs": [...] }`. envoy-rust emits the `BootstrapConfigDump` entry at `configs[0]`: `{ "@type": "type.googleapis.com/envoy.admin.v3.BootstrapConfigDump", "bootstrap": <static-bootstrap-as-JSON>, "last_updated": <ISO-8601 timestamp> }`. Envoy may emit additional entries for xDS-derived configs; those land on `allowlist_envoy_only`. `bootstrap.static_resources` content value-exact-after-roundtrip (modulo serde renamings; the harness's `JsonShape::required_subtree` covers this). `last_updated` name-required-value-may-differ (wall-clock non-determinism). The `BootstrapConfigDump` shows the bootstrap **as parsed from disk** — dynamic (CDS) clusters do NOT appear here (SPEC §5.5 config_dump separation); they surface in the `ClustersConfigDump` entry below. |
| `/config_dump` `ClustersConfigDump` (phase 18, L5/ADR-0049) | GET | JSON object (a `configs[]` entry) | **Conditional emission:** envoy-rust emits this entry ONLY when `dynamic_resources.cds_config` is configured; on non-CDS fixtures it is absent (fixture 0014's single-`BootstrapConfigDump`-entry shape preserved). When present, it lands at `configs[1]` on **both** proxies (Envoy's order: `BootstrapConfigDump`[0], `ClustersConfigDump`[1], …). Shape: `{ "@type": "type.googleapis.com/envoy.admin.v3.ClustersConfigDump", "dynamic_active_clusters": [ { "cluster": { "@type": "type.googleapis.com/envoy.config.cluster.v3.Cluster", <full cluster config> }, "last_updated": <ISO-8601> } ], "static_clusters": [ … when non-empty ] }`. The inner `cluster` object carries its own `@type` plus the full flattened cluster config. **Empty-key omission (proto3-JSON style):** `static_clusters` and `dynamic_active_clusters` are each `skip_serializing_if = Vec::is_empty` on both sides — a static-only Envoy emits the entry with ONLY a `static_clusters` key (no `dynamic_active_clusters`); there is NO `version_info` key (the CDS file had none — proto3 JSON omits empty fields). `last_updated` name-required-value-may-differ (wall-clock; reuses the BootstrapConfigDump ISO-8601 emitter). **Bilateral anchor (fixture 0026):** `configs.1.dynamic_active_clusters.0.cluster.name == dynamic_backend` (`JsonShape::required_subtree`; both sides equal the expected value AND each other). The surrounding `configs` array content otherwise differs substantially per side (envoy emits its full protobuf-canonical projection; envoy-rust the narrower parsed-bootstrap projection) — `value_may_differ_keys: ["configs"]`, mirroring fixture 0014. Note: envoy-rust's cluster JSON uses snake_case field names while Envoy's proto3-JSON defaults to camelCase for multi-word fields — irrelevant for the `name` anchor (single-word, identical) but binding if a future fixture asserts deeper nested cluster fields. |
| `/config_dump` `ListenersConfigDump` (phase 19, L5/ADR-0050) | GET | JSON object (a `configs[]` entry) | **Conditional emission:** envoy-rust emits this entry ONLY when `dynamic_resources.lds_config` is configured; on non-LDS fixtures it is absent (the backstop inertness path (vi) verifies `/config_dump` does NOT contain `"ListenersConfigDump"` on a CDS-only bootstrap). When present with **both** LDS+CDS configured, it lands at `configs[2]` on **both** proxies — **AFTER** the `ClustersConfigDump` at `configs[1]` (Envoy's verified order: `BootstrapConfigDump`[0], `ClustersConfigDump`[1], `ListenersConfigDump`[2], …; fixture 0026's `configs[1]` Clusters assertion needs NO amendment). Shape: `{ "@type": "type.googleapis.com/envoy.admin.v3.ListenersConfigDump", "dynamic_listeners": [ { "name": "dynamic_listener", "active_state": { "listener": { "@type": "type.googleapis.com/envoy.config.listener.v3.Listener", <full listener config> }, "last_updated": <ISO-8601> } } ], "static_listeners": [ … when non-empty ] }`. **Note the DIFFERENT nesting from the CDS dump:** the listener is nested under `dynamic_listeners[].active_state.listener` (vs the CDS dump's flatter `dynamic_active_clusters[].cluster`), and each entry carries a top-level `name` key. **No `version_info` key** — `active_state` has NO `version_info` (file-based LDS; the LDS file had none — proto3 JSON omits empty fields). **Empty-key omission:** `static_listeners` and `dynamic_listeners` are each `skip_serializing_if = Vec::is_empty` — a static-only Envoy emits the entry with ONLY `static_listeners`. `last_updated` name-required-value-may-differ (wall-clock; reuses the BootstrapConfigDump ISO-8601 emitter). **Bilateral anchor (fixture 0027):** `configs.2.dynamic_listeners.0.name == dynamic_listener` (`JsonShape::required_subtree`; both sides equal the expected value AND each other). The surrounding `configs` array otherwise differs per side — `value_may_differ_keys: ["configs"]`. **Known narrowing (LDS-only bootstrap):** on an LDS-only (no-CDS) bootstrap, envoy-rust's Listeners entry would land at `configs[1]` vs Envoy's `configs[2]` (Envoy emits a `ClustersConfigDump` for static clusters unconditionally, occupying `[1]`; envoy-rust's `ClustersConfigDump` is CDS-conditional per phase-18 L10, so it is absent and Listeners shifts up). Fixture 0027 configures BOTH LDS+CDS so the indices align at `[2]`; the divergence is recorded for any future LDS-only fixture (none exercises it today). |
| `/config_dump` `RoutesConfigDump` (phase 20, L5/ADR-0052) | GET | JSON object (a `configs[]` entry) | **Conditional emission:** envoy-rust emits this entry ONLY when **some HCM uses `rds`**; on non-RDS fixtures it is absent (vs Envoy's **always-emitted** `RoutesConfigDump`, which carries `static_route_configs` even without any RDS). Shape: `{ "@type": "type.googleapis.com/envoy.admin.v3.RoutesConfigDump", "dynamic_route_configs": [ { "route_config": { "@type": "type.googleapis.com/envoy.config.route.v3.RouteConfiguration", "name": "local_route", "virtual_hosts": [ … ] }, "last_updated": <ISO-8601> } ] }`. **No `version_info` key** — the RDS file had none (proto3 JSON omits empty fields; same posture as the CDS/LDS dumps). `last_updated` name-required-value-may-differ (wall-clock; reuses the BootstrapConfigDump ISO-8601 emitter). **Index divergence + per-side reconciliation:** the entry lands at **`configs[4]`** on Envoy (Bootstrap[0]/Clusters[1]/Listeners[2]/ScopedRoutes[3]/Routes[4]/Secrets[5]) but **`configs[2]`** on envoy-rust on fixture 0028 (Bootstrap[0]/Clusters[1]/Routes[2] — Listeners gated off, no `lds_config` on 0028) — bridged by a **per-side `JsonSubtreeRule` path override** in the harness (Envoy `configs.4.…` vs envoy-rust `configs.2.…`). **Bilateral anchor (fixture 0028):** the `route_config.name == local_route` subtree (`JsonShape::required_subtree`; both sides equal the expected value AND each other). The surrounding `configs` array otherwise differs per side — `value_may_differ_keys: ["configs"]`. Fixtures 0026/0027 hold (the RoutesConfigDump entry is RDS-conditional and absent there; their Clusters[1]/Listeners[2] assertions are NOT displaced). |
| `/config_dump` `EndpointsConfigDump` (phase 21, L5/ADR-0054) | GET | JSON object (a `configs[]` entry) | **Conditional emission:** envoy-rust emits this entry ONLY when **some cluster is `type: EDS`**; on non-EDS fixtures it is absent. **`?include_eds` divergence:** Envoy surfaces `EndpointsConfigDump` ONLY under `/config_dump?include_eds` (omitted from the default `/config_dump`); envoy-rust emits it on **every** `/config_dump` for an EDS bootstrap, **unconditional of `?include_eds`** (a recorded narrowing — envoy-rust's admin path dispatch STRIPS the query string, routing `/config_dump?include_eds` to the `ConfigDump` endpoint; Envoy does the same; no existing fixture uses query strings, so it is inert there). **Static, not dynamic:** file-based EDS endpoints land under `static_endpoint_configs[]`, NOT `dynamic_endpoint_configs[]` (Envoy classifies file/path-based EDS as "static"). Shape: `{ "@type": "type.googleapis.com/envoy.admin.v3.EndpointsConfigDump", "static_endpoint_configs": [ { "endpoint_config": { "@type": "type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment", "cluster_name": "eds_backend", "endpoints": [ … ], "policy": { … } } } ] }`. **No `version_info`/`last_updated` keys** — file-based (proto3 JSON omits empty fields; same posture as the CDS/LDS/RDS dumps). **Index divergence + per-side reconciliation:** the entry lands at **`configs[2]`** on Envoy (under `?include_eds`: Bootstrap[0]/Clusters[1]/Endpoints[2]/Listeners[3]/ScopedRoutes[4]/Routes[5]/Secrets[6]) but **`configs[1]`** on envoy-rust on fixture 0029 (Bootstrap[0]/Endpoints[1] — no `cds_config` on 0029 → no ClustersConfigDump) — bridged by a **per-side `JsonSubtreeRule` path override** in the harness (Envoy `configs.2.…` vs envoy-rust `configs.1.…`), REUSING the ADR-0052 path-override mechanism (no new harness JSON machinery). **Bilateral anchor (fixture 0029):** `static_endpoint_configs[0].endpoint_config.cluster_name == eds_backend` (`JsonShape::required_subtree`; both sides equal the expected value AND each other; the probe scrapes `/config_dump?include_eds`). The surrounding `configs` array otherwise differs per side — `value_may_differ_keys: ["configs"]`. **C19 note:** the EDS pass mutates `load_assignment` in-place, so the `BootstrapConfigDump` entry (`configs[0]`) shows the **populated** `load_assignment` for the static EDS cluster (a known minor divergence vs Envoy, which shows it as-configured) — NOT asserted; the `EndpointsConfigDump` is the faithful resolved-endpoints surface. Fixtures 0014/0026/0027/0028 hold (no EDS cluster → no Endpoints entry → their `configs[]` indices NOT displaced). **Phase-27 reload (ADR-0068 §Decision-2):** the `static_endpoint_configs[].endpoint_config.endpoints` reflects the **POST-RELOAD** endpoints, rendered through the live swappable handle; still NO `version_info`/`last_updated` (the phase-21 shape is unchanged on reload — the SPEC's `last_updated`-changes projection was WRONG). |
| `/server_info` | GET | JSON object | Required keys `state`, `version`, `node`, `uptime_current_epoch_seconds`, `uptime_all_epochs_seconds`, `hot_restart_version`, `command_line_options`. `state` value-exact, sourced from `DrainState::current()` via the mapping `Live | HealthcheckFailing → "LIVE"`, `Draining → "DRAINING"` (08.1 emitted the literal constant `"LIVE"` as a placeholder; 08.2's D5e patches the value-binding source at Task 5 — the struct shape is unchanged at the 08.1 → 08.2 boundary); `node.*` value-exact from the parsed bootstrap; `version` + `hot_restart_version` + `command_line_options` allowlist-each-side (envoy-rust emits its own version string; Envoy emits its own); `uptime_*` name-required-value-may-differ (wall clock). |
| `/clusters` | GET | text/plain | Set-equal `<cluster_name>::observability_name::<name>` + `<cluster_name>::default_priority::endpoints` lines per Envoy v1.33's plain-text format. Per-endpoint numeric fields (success/error/timeout counts) name-required-value-may-differ; envoy-rust at 08.1 emits only the minimum two lines per cluster (architecture-decision lock-in #10) — Envoy's richer output is allow-listed envoy-only on fixture 0014. Cluster output order is deterministic by name (sorted in `ClusterManager::clusters()`). |
| `/listeners` | GET | text/plain | Set-equal `<listener_name>::<address>:<port>` lines. Order: sorted-by-name (deterministic on both sides). **LDS extension (phase 19, L5/ADR-0050):** LDS-supplied listeners appear in the output alongside static ones — envoy-rust migrated the endpoint to enumerate the merged `all_listeners()` set (static + LDS-delivered), so fixture 0027's `dynamic_listener` line is emitted on both sides. The per-side address shapes are **prefix-matched** (Envoy binds `dynamic_listener::0.0.0.0:<port>`; envoy-rust binds `dynamic_listener::127.0.0.1:<kernel-ephemeral>`) — the differential harness matches on the `dynamic_listener::` line prefix bilaterally with per-side `allowlist_*_line_prefixes` for the address+port tail. |
| `/drain_listeners` | POST | empty | Status 200; empty body (`content-length: 0`); effect-only endpoint. Invokes `DrainState::drain()`. Sticky — repeat POSTs are idempotent. Both proxies emit 200 OK on first AND subsequent POSTs. |
| `/healthcheck/fail` | POST | empty | Status 200; empty body; effect-only endpoint. Invokes `DrainState::fail_healthcheck()`. Flips `/ready` to 503 (per parent-08 SPEC §5.5 wire-state mapping); `/server_info.state` stays `"LIVE"` (server-state is independent of healthcheck-failure). |
| `/healthcheck/ok` | POST | empty | Status 200; empty body; effect-only endpoint. Invokes `DrainState::ok_healthcheck()`. Restores from `HealthcheckFailing` → `Live`. Sticky-drain: `/healthcheck/ok` AFTER `/drain_listeners` does NOT un-drain (the `HealthcheckFailing → Live` compare_exchange fails silently against the `Draining` state). |

---

## Admin-action effect equivalence

> Authored per phase 08.2 SPEC §2.3. States the cross-proxy invariant that
> admin-action POSTs (`/drain_listeners`, `/healthcheck/fail`,
> `/healthcheck/ok`) must drive observable wire-level effects on both
> proxies. The internal mechanism is implementation-specific; only the
> wire-level observable is contract.
>
> **For `POST /drain_listeners`, the bilateral wire-level invariant is
> `data_plane_connection_refused` on the data-plane listener** —
> kernel-side ECONNREFUSED / immediate-EOF / RST within the 5s
> `DRAIN_BUDGET`. The admin-bookkeeping `/ready` flip is NOT a
> bilateral invariant on `/drain_listeners`: upstream Envoy v1.33's
> `/ready` does NOT flip to 503 on `POST /drain_listeners` without the
> server-level `--drain-strategy immediate` CLI flag (NOT
> bootstrap-configurable); envoy-rust per parent-08 SPEC §5.5 flips
> `/ready` immediately on drain. Fixture 0015 (D17.2) therefore pairs
> the `data_plane_connection_refused` post-assertion (the bilateral
> wire-level invariant) with a `/server_info` JSON scrape (bilaterally
> 200-with-JSON across the drain transition; `state` key presence is
> the bilateral structural invariant; `state` VALUE is permitted to
> differ across proxies). The envoy-rust-side `/ready=503 DRAINING`
> flip is verified in isolation by the in-process backstop at
> `crates/envoy-bin/tests/admin_drain_listeners.rs` (Task 10), which
> does not face the cross-proxy `--drain-strategy` asymmetry. The
> `/healthcheck/fail` + `/healthcheck/ok` rows below DO assert the
> bilateral `/ready` flip because both proxies flip `/ready`
> synchronously on those endpoints (no CLI-flag gap).

| Action | Wire-level invariant |
|---|---|
| `POST /drain_listeners` | Both proxies MUST refuse-or-immediately-close new connections on their data-plane listeners within the drain window (5s `DRAIN_BUDGET`). The harness `AdminAssertion::DataPlaneConnectionRefused { listener_address, within_ms }` polls for ECONNREFUSED OR immediate-EOF on connect; either disposition satisfies the invariant. Admin listener stays serving during drain (operator reachability per parent-08 SPEC §5.5). Sticky — subsequent `POST /healthcheck/ok` does NOT un-drain. |
| `POST /healthcheck/fail` | Both proxies MUST flip `/ready` to 503 within 100ms; `/server_info.state` stays `"LIVE"` (server-state independent of healthcheck-failure). |
| `POST /healthcheck/ok` | Both proxies MUST flip `/ready` back to 200 within 100ms IF and ONLY IF current state is `HealthcheckFailing`; if current state is `Draining`, the action is a no-op (sticky drain). |

---

## Access log field mapping

> **To be filled per-phase as needed.**
>
> Upstream Envoy's default-format access log is specified as a fixed sequence
> of substitution tokens (`%START_TIME%`, `%REQ(…)%`, `%RESPONSE_CODE%`, etc.).
> envoy-rust must reproduce every token's semantic content, but the underlying
> data source inside envoy-rust may differ. This section records the mapping
> from token → envoy-rust internal field so the harness can diff accurately.
>
> Populated in phase 06 when access logs first ship. Extended whenever a new
> filter adds new log-only fields.

**06.2 first-time population (per parent-06 SPEC §2.2).** Envoy's default
access-log format (per upstream Envoy v1.33's documentation) is a fixed
sequence of 14 tokens emitted per request:

```
[%START_TIME%] "%REQ(:METHOD)% %REQ(X-ENVOY-ORIGINAL-PATH?:PATH)% %PROTOCOL%" %RESPONSE_CODE% %RESPONSE_FLAGS% %BYTES_RECEIVED% %BYTES_SENT% %DURATION% %RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)% "%REQ(X-FORWARDED-FOR)%" "%REQ(USER-AGENT)%" "%REQ(X-REQUEST-ID)%" "%REQ(:AUTHORITY)%" "%UPSTREAM_HOST%"
```

Tokens absent on a given record (e.g., `%REQ(USER-AGENT)%` when the
request did not carry a `User-Agent:` header) emit `-` in their
position. Quoted tokens emit `"-"` (a literal `"-"` between the
surrounding quotes).

| Token | envoy-rust internal source | Equivalence disposition | Rationale |
|---|---|---|---|
| `%START_TIME%` | `AccessLogRecord.start_time: SystemTime`, formatted by `default_format::format_iso8601` as `YYYY-MM-DDTHH:MM:SS.sssZ` (UTC, ms resolution). Captured at HCM `serve_connection` request-arrival time. | name-required, value-may-differ | Wall-clock non-determinism: the two proxies stamp the response at slightly different instants. The harness asserts ISO-8601 parse via `AccessLogLineRule::Iso8601Format`. |
| `%REQ(:METHOD)%` | `AccessLogRecord.method`, sourced from `Request.method` at HCM record-build time. | value-exact | Both proxies receive the same method bytes; rendering matches. |
| `%REQ(X-ENVOY-ORIGINAL-PATH?:PATH)%` | `AccessLogRecord.path`, populated at HCM record-build time by checking `Request.headers` for `x-envoy-original-path` (case-insensitive); if present, that value; else `Request.path`. | value-exact | Both proxies see the same request bytes; both render the same path. |
| `%PROTOCOL%` | `AccessLogRecord.protocol`, determined by the dispatch path: `"HTTP/1.1"` on the H1 HCM (`envoy_http1::hcm`), `"HTTP/2"` on the H2 HCM (`envoy_http2::hcm`). | value-exact | The protocol is fixed by which HCM module is dispatching; both proxies emit the same string. |
| `%RESPONSE_CODE%` | `AccessLogRecord.response_code: u16`, sourced from `Response.status`. | value-exact | Both proxies route the request through the same VH/route/action; both produce the same response code. |
| `%RESPONSE_FLAGS%` | `AccessLogRecord.response_flags: String`. Renders Envoy's no-flags sentinel `"-"` on every path EXCEPT six witnessed failure paths: the no-route 404 path renders `NR` (NoRoute), the no-healthy-upstream 503 path renders `UH` (NoHealthyUpstream), the circuit-breaker overflow 503 path renders `UO` (UpstreamOverflow), the retry-limit-exceeded 503 path renders `URX` (UpstreamRetryLimitExceeded), the upstream-connect-failure 503 path renders `UF` (UpstreamConnectionFailure), and the upstream-disconnect-before-headers 503 path renders `UC` (UpstreamConnectionTermination). **Per-flag equivalence — `NR`:** a config-deterministic single static constant (no combination, brace-free), set on BOTH H1 no-route `synth_404` arms (host-miss + route-miss), derived 1:1 from `%RESPONSE_CODE_DETAILS%` = `route_not_found` at the H1 record-build site (`hcm.rs:1377`); the 404 status/body/headers/`%RESPONSE_CODE_DETAILS%` are unchanged. **Per-flag equivalence — `UH`:** likewise a config-deterministic single static constant (no combination, brace-free), set on the single H1 `pick()->None` no-healthy synth-503 arm, derived 1:1 from `%RESPONSE_CODE_DETAILS%` = `no_healthy_upstream` at the same H1 record-build site (`hcm.rs:1377`); the 503 status/body/headers/`%RESPONSE_CODE_DETAILS%` are unchanged. **Per-flag equivalence — `UO`:** likewise a config-deterministic single static constant (no combination, brace-free), derived 1:1 from `%RESPONSE_CODE_DETAILS%` = `upstream_reset_before_response_started{overflow}` at the H1 record-build site, set on BOTH pool-overflow arms (the `outcome:None` discriminator) AND the request-budget (`max_requests`) arm; the 503 status/body/headers are unchanged. **Per-flag equivalence — `URX`:** likewise a config-deterministic single static constant (no combination, brace-free), but — UNLIKE `NR`/`UH`/`UO` — **NOT derived from `%RESPONSE_CODE_DETAILS%`**: the retry-limit-exceeded path's rcd is the SHARED `via_upstream` (the final attempt is a real upstream 503, already matching Envoy — the FIRST flag not 1:1 with a unique rcd). It is instead derived from the `retry_limit_exceeded_for_log` boolean set on the single H1 retry-limit-exceeded loop-exit (the same gate as the `upstream_rq_retry_limit_exceeded` counter increment — `attempts > 1 && !retry_budget_blocked && final_retriable`), read by the H1 record-build derive (`hcm.rs:1377`); the 503 status/body/headers AND the `via_upstream` `%RESPONSE_CODE_DETAILS%` are unchanged. **Per-flag equivalence — `UF`:** likewise a config-deterministic single static constant (no combination, brace-free), and — like `URX` — **NOT derived from `%RESPONSE_CODE_DETAILS%`**: the connect-failure path's rcd is the SHARED `via_upstream` AND carries the OS-derived transport-failure reason (the SECOND flag not 1:1 with a unique rcd). It is instead derived from the `connect_failure_for_log` boolean set post-loop when the FINAL attempt's `AttemptOutcome` is `ConnectFailure` (a connect-failure retried to success is NOT flagged), read by the same H1 record-build derive (`hcm.rs:1377`, ordered after the `URX` branch). The connect-failure response is the synth-**503** (corrected from a previously-unvalidated synth-502 to match Envoy — envoy-rust already returns 503 on the sibling overflow path), but the connect-failure `%RESPONSE_CODE_DETAILS%` AND response body carry the non-deterministic OS transport-failure reason and are therefore **NOT witnessed** (the fixture logs `{rc, rf}` only; the byte-exact-access-log driver does not compare the body). **Per-flag equivalence — `UC`:** likewise a config-deterministic single static constant (no combination, brace-free), and — UNLIKE `URX`/`UF` (whose rcds genuinely stay the shared `via_upstream`) — **derived 1:1 from `%RESPONSE_CODE_DETAILS%` = `upstream_reset_before_response_started{connection_termination}`** (the `UO`/`{overflow}` pattern), set by phase 54 (ADR-0111) on the pure-reset final-outcome path at the post-loop reconciliation region (overriding the in-loop shared `via_upstream`, guarded `!retry_limit_exceeded_for_log` so a retry-exhausted reset keeps `via_upstream` and renders `URX`), read by the H1 record-build rcd-match (`hcm.rs:1377`, the arm after `{overflow} => "UO"`). The phase-53 boolean discriminator was RETIRED. The reset response is the synth-**503**. The reset `%RESPONSE_CODE_DETAILS%` `upstream_reset_before_response_started{connection_termination}` is DETERMINISTIC (a fixed reset-reason enum, NOT OS-derived — UNLIKE the connect-failure rcd) and is now witnessed byte-exact at phase 54 (ADR-0111), fixture **0062**. **On H2, `UC` is now derived EXACTLY as on H1** (fixture **0070**, phase 65, ADR-0122): the H2 pure-reset path sets the DETERMINISTIC `%RESPONSE_CODE_DETAILS%` = `upstream_reset_before_response_started{connection_termination}` post-loop (overriding the in-loop shared `via_upstream`, guarded `!retry_limit_exceeded_for_log_h2` so a retry-exhausted reset keeps `via_upstream` and renders `URX`), and `UC` derives **1:1 from that rcd** (the `UO`/`{overflow}` pattern), read by the H2 record-build rcd-match in `crates/envoy-http2/src/hcm.rs`. The phase-64 boolean discriminator was RETIRED — **CONSUMING carry-forward M64-1**. H2's `URX`/`UF` remain boolean-derived (their rcds genuinely stay `via_upstream`), so both protocols now share the identical derivation split: `{NR, UH, UO, UC}` rcd-derived, `{URX, UF}` boolean-derived. Other non-`-` flags (`DC`) remain unwitnessed (M45-2, non-deterministic surfaces) and still need their own per-flag rules. | value-exact (`-` no-flags case + `NR` no-route case + `UH` no-healthy case + `UO` overflow case + `URX` retry-limit-exceeded case + `UF` connect-failure case + `UC` upstream-reset case) | Fixture 0012's direct_response happy-path produces `-`; both proxies emit `-`. Phase 48 (ADR-0105) fixture **0056** witnesses `NR` byte-exact on BOTH the route-miss and host-miss 404 arms; both proxies emit `NR`. Phase 49 (ADR-0106) fixture **0057** witnesses `UH` byte-exact on the no-healthy-upstream 503 arm; both proxies emit `UH`. Phase 50 (ADR-0107) fixture **0058** witnesses `UO` byte-exact on the circuit-breaker overflow 503 path; both proxies emit `UO`. Phase 51 (ADR-0108) fixture **0059** witnesses `URX` byte-exact on the H1 retry-limit-exceeded 503 path; both proxies emit `URX` (rcd unchanged at `via_upstream`). Phase 52 (ADR-0109) fixture **0060** witnesses `UF` byte-exact on the H1 upstream-connect-refused 503 path; both proxies emit `UF` (rcd/body NOT logged/compared — the non-deterministic transport-failure reason). Phase 53 (ADR-0110) fixture **0061** witnesses `UC` byte-exact on the H1 upstream-disconnect-before-headers 503 path; both proxies emit `UC` (rcd `connection_termination` deterministic, witnessed at phase 54 (M53-1 consumed, fixture 0062)). Phase 54 (ADR-0111) fixture **0062** witnesses the reset `%RESPONSE_CODE_DETAILS%` `upstream_reset_before_response_started{connection_termination}` byte-exact on the same path; `UC` now derives 1:1 from that rcd (the prior boolean discriminator retired). The request-budget (`max_requests`) overflow UO is now ALSO differentially witnessed byte-exact at the access-log level by fixture **0063** (`0063-accesslog-rf-overflow-request-budget`, phase 55, ADR-0112) — the SECOND of the two set-sites phase 50 (ADR-0107) tagged with the identical rcd string (fixture 0058 witnesses only the pool-overflow arm); this CONSUMES carry-forward **M50-C**. Fixture 0025 (phase 17, ADR-0046/ADR-0047) already proves the SAME disposition at the wire/stats level; the pre-existing `upstream_cx_total` connection-pool-prefetch divergence noted there (`BEHAVIOR_CONTRACT.md:401`) is UNCHANGED by this fixture (it requires a REACHABLE endpoint, unlike 0058's dead-literal-address topology, precisely to avoid re-triggering that divergence). The H2 access-log differential driver now exists (`Driver::Http2AccessLogByteExact`, phase 56, ADR-0113) and `NR` is witnessed byte-exact on H2 by fixture **0064** — CONSUMING carry-forward **M45-1**. `UH` is now ALSO witnessed byte-exact on H2 by fixture **0065** (phase 57, ADR-0114), which ALSO corrects the H2 no-healthy synth status 502 → 503 to match Envoy (the H2 `synth_h2_no_healthy_upstream()` helper, mirroring the H1 `synth_no_healthy_upstream` precedent) — ADVANCING carry-forward **M56-1** (the `UH` slice consumed). `UO` is now ALSO witnessed byte-exact on H2 by fixture **0066** (phase 58, ADR-0115) — set on BOTH the H2 pool-overflow arm (the `outcome:None` discriminator, mirroring H1's phase-50 pattern) AND the H2 request-budget arm (mirroring H1's own direct tag) — ADVANCING carry-forward **M56-1** (the `UO` slice consumed). UNLIKE phases 50/57, NO status-code correction was needed — envoy-rust's H2 overflow status was already correct (503, via the pre-existing `synth_h2_overflow()`). `URX` is now ALSO witnessed byte-exact on H2 by fixture **0067** (phase 61, ADR-0118) — set via the retry-loop's post-loop limit-exceeded exit boolean (NOT derivable from `%RESPONSE_CODE_DETAILS%`, which stays the shared `via_upstream` on this path — the SAME non-rcd-derivable pattern H1 established at phase 51), threaded through `finalize_h2_stream` as a new parameter — ADVANCING carry-forward **M56-1** (the `URX` slice consumed). UNLIKE phases 50/57, NO status-code correction was needed — envoy-rust's H2 retry-limit-exceeded mechanics (status 503, `x-envoy-attempt-count: 2`, all four retry counters) were ALREADY correct and ALREADY covered by an existing phase-16 in-process test. `UF` is now ALSO witnessed byte-exact on H2 by fixture **0068** (phase 63, ADR-0120) — set via a NEW loop-scoped final-outcome capture + a post-loop boolean (NOT derivable from `%RESPONSE_CODE_DETAILS%`, which stays the shared `via_upstream` on this path, exactly as H1's phase-52 `UF` found), threaded through `finalize_h2_stream` as a second new parameter, ordered AFTER `URX` in the derive — ADVANCING carry-forward **M56-1** (the `UF` slice consumed, leaving ONLY `UC`). UNLIKE `URX`, this phase ALSO corrected a genuine status-code divergence — envoy-rust's H2 connect-failure arm previously emitted a previously-unvalidated `502` (via the generic `synth_h2_502()`); it now emits `503` via a dedicated `synth_h2_connect_failure()` helper, matching upstream Envoy. `UC` is now ALSO witnessed byte-exact on H2 by fixture **0069** (phase 64, ADR-0121) — set — **as of phase 64** — via the SAME final-outcome-capture mechanism as `URX`/`UF` (a post-loop boolean discriminator reading the EXISTING `final_outcome_h2` capture a second time, since the H2 reset rcd then stayed the shared `via_upstream`, deferred as carry-forward **M64-1**), threaded through `finalize_h2_stream` as a third new parameter, ordered AFTER `UF` in the derive (**phase 65 (ADR-0122) has since CONSUMED M64-1** — that rcd is now the deterministic `upstream_reset_before_response_started{connection_termination}`, H2's `UC` derives 1:1 from it, and the boolean discriminator + its `finalize_h2_stream` parameter were RETIRED) — **this phase-64 witness CLOSED carry-forward M56-1**: all six H2 `%RESPONSE_FLAGS%` values (`NR`/`UH`/`UO`/`URX`/`UF`/`UC`) are now witnessed, full parity with H1's own six-flag completion at phase 53. Like `UF`, this phase ALSO corrected a genuine status-code divergence — envoy-rust's H2 post-connect-dispatch-failure arm previously emitted a previously-unvalidated `502` (via the generic `synth_h2_502()`, renamed in place); it now emits `503` via `synth_h2_reset()`, matching upstream Envoy and closing out the whole per-arm H2 status-correction sweep phases 52 (H1) / 57 / 63 / 64 progressively made. Phase 65 (ADR-0122) fixture **0070** witnesses the H2 reset `%RESPONSE_CODE_DETAILS%` `upstream_reset_before_response_started{connection_termination}` byte-exact on the same path; H2's `UC` now derives 1:1 from that rcd (the phase-64 boolean discriminator retired) — **CONSUMING carry-forward M64-1** and completing full H1/H2 parity for the deterministic upstream-reset rcd. NO `%RESPONSE_FLAGS%` value changed: fixture `0069`'s emitted line is byte-identical, `UC` merely arriving via the rcd-match. |
| `%BYTES_RECEIVED%` | `AccessLogRecord.bytes_received: u64`, from `Request.body.as_ref().map_or(0, |b| b.len() as u64)`. Header bytes NOT counted (matches Envoy's semantic). | value-exact | Both proxies see the same wire request body bytes. |
| `%BYTES_SENT%` | `AccessLogRecord.bytes_sent: u64`, from `response.body.len() as u64`. Symmetric to `%BYTES_RECEIVED%`. | value-exact | Both proxies render the same response body bytes. |
| `%DURATION%` | `AccessLogRecord.duration: Duration`, from `start.elapsed()` at HCM record-build time. Rendered as integer milliseconds via `Duration::as_millis()`. | name-required, value-may-differ | Per-request wall-clock latency diverges across runtimes/processes/HCM impls. The harness asserts non-negative-integer parse via `AccessLogLineRule::DurationMs`. |
| `%RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%` | `AccessLogRecord.upstream_service_time: Option<Duration>`, populated at HCM record-build time by reading `Response.headers` for `x-envoy-upstream-service-time`. When present (router-proxy path per 04.3 emission), rendered as `Duration::as_millis()`; when absent (direct_response path), rendered as literal `-`. | name-required, value-may-differ (when present); value-exact `-` (when absent on direct_response paths) | The header value's equivalence is inherited from the 04.3-landed `Header allow-list` row for the same header. Fixture 0012's direct_response path produces `-` on both sides. |
| `%REQ(X-FORWARDED-FOR)%` | `AccessLogRecord.forwarded_for: Option<String>`, read from `Request.headers` (lowercased per the 04.x normalization posture). | value-exact | If present on the request both proxies see the same bytes; if absent both emit `-`. |
| `%REQ(USER-AGENT)%` | `AccessLogRecord.user_agent: Option<String>`, sourced symmetrically. | value-exact | Same rationale as `%REQ(X-FORWARDED-FOR)%`. |
| `%REQ(X-REQUEST-ID)%` | `AccessLogRecord.request_id: Option<String>`, sourced symmetrically. envoy-rust never injects `x-request-id` per 04.3 SPEC §4; fixture 0012's `envoy.yaml` sets `generate_request_id: false` to align both proxies on the omit-injection posture. | value-exact | Both proxies omit injection; both render `-`. |
| `%REQ(:AUTHORITY)%` | `AccessLogRecord.authority: Option<String>`, populated from the `Host:` header on the H1 path (envoy_http1::codec produces this from the request-line) or the `:authority` pseudo-header on the H2 path (translated by 05.2 D3's adapter). | value-exact | Both proxies see the same wire-level request authority; both render the same value. |
| `%UPSTREAM_HOST%` (differentially witnessed phase 44, ADR-0101) | `AccessLogRecord.upstream_host: Option<String>`, populated at HCM record-build time from the router-arm's resolved upstream `SocketAddr` (formatted via `SocketAddr` Display = `<ip>:<port>`, IPv4 unbracketed). `None` on direct_response paths. | value-exact `-` (direct_response, fixture 0012); **DIFFERENTIALLY WITNESSED byte-exact** by **fixture 0052** (`0052-accesslog-upstream-host`) for a real routed upstream; value-may-differ only for multi-A non-deterministic resolution | Fixture 0012's direct_response path produces `-`; both proxies emit `-`. Fixture 0052 (phase 44) routes through a `{{BACKEND_IP}}` shared-host-LAN-IP **STATIC** cluster — both proxies dial the IDENTICAL `<host-LAN-IP>:<port>` and render the IDENTICAL `%UPSTREAM_HOST%` (`SocketAddr::to_string()` = `<ip>:<port>` = Envoy's format), asserted cross-proxy-equal by the `http1_access_log_byte_exact` driver (no static literal — the `<ip>:<port>` is dynamic-but-shared per CI run). This CLOSES the gap fixture 0051 left (0051 excluded `%UPSTREAM_HOST%` because its STRICT_DNS `{{BACKEND_HOST}}` cluster resolves per-side-divergent). |
| `%ROUTE_NAME%` (phase 41, ADR-0098) | `AccessLogRecord.route_name: Option<String>`, populated at the HCM record-build site (H1 `serve_connection`; H2 `finalize_h2_stream` via a `route_name_for_log_h2` parameter computed at the `handle_one_stream` route-match site) from the matched route's config `name` (`bootstrap::Route.name`); an empty `name` (unnamed route) → `None`. An `Option<String>` IDENTICAL in shape to `%UPSTREAM_HOST%`: text/mixed present → the name, absent → the `-` sentinel; json single-op present → quoted string, absent → `null`. | value-exact (config-deterministic) | The route `name` is static config; both proxies match the same route and render the same `name`. Fixture 0049 drives a NAMED route (`name: myroute`) → `single_rn:"myroute"` / `rn:"r=myroute"` (live-captured from v1.33.0). Default-absent on all fixtures 0001-0048 (no named route, no `%ROUTE_NAME%`), keeping them byte-identical. |
| `%RESPONSE_CODE_DETAILS%` (phase 42, ADR-0099; no-healthy failure path phase 45, ADR-0102; route-miss failure path phase 46, ADR-0103; host-miss failure path phase 47, ADR-0104) | `AccessLogRecord.response_code_details: Option<String>`, populated at the HCM record-build sites (H1 writer-arm; H2 `finalize_h2_stream` via a `response_code_details_for_log_h2` parameter) from the response path — a `direct_response` route → `Some("direct_response")`, a proxy-success → `Some("via_upstream")`, **the no-healthy-upstream synth-503 (the `pick()->None` path) → `Some("no_healthy_upstream")` (phase 45; the H1 `else`-branch at the Proxy arm `hcm.rs:~996`)**, **BOTH route-walk synth-404 arms → `Some("route_not_found")` — the no-matching-route (route-miss) arm `hcm.rs:1554` (phase 46) AND the no-matching-virtual_host (host-miss) arm `hcm.rs:1536` (phase 47)**, **the overflow synth-503 (the `outcome:None` overflow discriminator at the H1 retry-loop consumption site — the pool-overflow arms `hcm.rs:508`/`hcm.rs:515` AND the request-budget arm) → `Some("upstream_reset_before_response_started{overflow}")` (phase 50)**, **the pure-reset synth-503 (the final-outcome `AttemptOutcome::Reset` path, guarded `!retry_limit_exceeded_for_log`, at the post-loop reconciliation region `hcm.rs:~1200`) → `Some("upstream_reset_before_response_started{connection_termination}")` (phase 54, overriding the in-loop `via_upstream`)** — and, on **H2**, the pure-reset synth-503 (the final-outcome `AttemptOutcome::Reset` path, guarded `!retry_limit_exceeded_for_log_h2`, at the post-loop reconciliation region of `crates/envoy-http2/src/hcm.rs`) → `Some("upstream_reset_before_response_started{connection_termination}")` (phase 65, ADR-0122, overriding the in-loop `via_upstream`; H2's `UC` `%RESPONSE_FLAGS%` derives 1:1 from it) —, other error/filter synths → `None`. An `Option<String>` shaped like `%ROUTE_NAME%`/`%UPSTREAM_HOST%`: text/mixed present → the string, absent → the `-` sentinel; json single-op present → quoted string, absent → `null`. | value-exact | The response-code-details string is response-path deterministic; both proxies emit the same value for the same response disposition. Fixture 0050 drives a `direct_response` route → `single_rcd:"direct_response"` / `rcd:"d=direct_response"`; **fixture 0053 (`0053-accesslog-rcd-no-healthy`, phase 45) drives a `metadata_match` NO_FALLBACK subset-miss → the no-healthy-upstream 503 → `rcd:"no_healthy_upstream"` (the FIRST differentially-witnessed FAILURE-path detail); fixture 0054 (`0054-accesslog-rcd-route-not-found`, phase 46) drives a route-miss (a single `/specific` route + a `/nomatch` probe) → the 404 → `rcd:"route_not_found"` (the SECOND failure-path detail); fixture 0055 (`0055-accesslog-rcd-host-not-found`, phase 47) drives a host-miss (a `domains:["match.test"]` vhost + a `Host: nomatch.test` probe) → the 404 → `rcd:"route_not_found"` cross-proxy byte-exact (live-captured from v1.33.0; the THIRD failure-path detail — CONSUMES carry-forward M46-1); fixture 0058 (`0058-accesslog-rf-overflow`, phase 50, ADR-0107) drives the H1 circuit-breaker overflow → the 503 → `rcd:"upstream_reset_before_response_started{overflow}"` cross-proxy byte-exact (live-captured from v1.33.0; the FOURTH failure-path detail). The overflow detail's brace content (`overflow`) is a FIXED reset-reason enum (NOT the OS-derived connect-failure phrase), so it is witnessed byte-exact deterministic — refining M45-2 + ADR-0102 §B such that ONLY the connect-failure rcd remains non-deterministic.** fixture **0062** (`0062-accesslog-rcd-upstream-reset`, phase 54, ADR-0111) drives the accept-then-close upstream-disconnect-before-headers path → the 503 → `rcd:"upstream_reset_before_response_started{connection_termination}"` cross-proxy byte-exact (live-captured from v1.33.0; the FIFTH failure-path detail and the FIRST deterministic upstream-reset rcd). Its brace content `connection_termination` is a FIXED reset-reason enum (like `{overflow}`), so byte-exact deterministic — refining M45-2 such that ONLY the connect-failure rcd remains non-deterministic. `route_not_found` is now ALSO witnessed on H2 (fixture **0064**, phase 56, ADR-0113) — the H2 access-log differential driver now exists. `no_healthy_upstream` is now ALSO witnessed on H2 (fixture **0065**, phase 57, ADR-0114) — phase 57 investigated and FIXED the previously un-recon'd note that "the H2 no-healthy arm returns 502": the H2 `pick()->None` arm now emits envoy-rust's dedicated `synth_h2_no_healthy_upstream()` helper (503, mirroring the H1 `synth_no_healthy_upstream` precedent) instead of the generic `synth_h2_502()`, and `response_code_details_for_log_h2` is now set to `Some("no_healthy_upstream")` on that arm (the caller-loop `else` branch). `upstream_reset_before_response_started{overflow}` is now ALSO witnessed on H2 (fixture **0066**, phase 58, ADR-0115), set on BOTH the H2 pool-overflow arm and the H2 request-budget arm. The H2 retry-limit-exceeded path (fixture **0067**, phase 61, ADR-0118) is now ALSO witnessed on H2, but its `%RESPONSE_CODE_DETAILS%` stays the shared `via_upstream` (a REAL completing 503, unchanged) — `%RESPONSE_FLAGS%`=`URX` is the discriminating signal there, NOT this field. The H2 connect-failure path (fixture **0068**, phase 63, ADR-0120) is now ALSO witnessed on H2, but its `%RESPONSE_CODE_DETAILS%` ALSO stays the shared `via_upstream` AND carries the OS-derived non-deterministic transport-failure reason (M45-2) — `%RESPONSE_FLAGS%`=`UF` is the discriminating signal there, NOT this field (the rcd is OMITTED from the fixture entirely, mirroring the H1 `0060` precedent). The H2 upstream-reset path (fixture **0070**, `0070-accesslog-h2-rcd-upstream-reset`, phase 65, ADR-0122) now ALSO witnesses `rcd:"upstream_reset_before_response_started{connection_termination}"` cross-proxy byte-exact — the H2 sibling of the H1 witness at fixture `0062`, and the value H2's `UC` flag now derives from 1:1 (the phase-64 boolean discriminator retired). This **CONSUMES carry-forward M64-1**; `M56-1` was already fully closed at phase 64. The connect-failure rcd (H1 `0060` / H2 `0068`) remains the sole non-deterministic reset-reason (OS-derived text, M45-2) and stays unwitnessed. Default-absent on all fixtures 0001-0049 + 0051-0052 + 0056-0057 + 0059-0061 + 0063-0069 (no `%RESPONSE_CODE_DETAILS%` logged or no reset path), keeping them byte-identical — `0069` in particular drives the SAME H2 reset path as `0070` but logs no `rcd`, so it stays byte-identical across the phase-65 migration; `0070` is the new rcd-logging fixture. |
| `%UPSTREAM_CLUSTER%` (phase 43, ADR-0100) | `AccessLogRecord.upstream_cluster: Option<String>`, set at the HCM proxy-ARM entry from `BuildOutcome::Proxy { cluster }` (H1 writer-arm; H2 `finalize_h2_stream` param) whenever a route resolves to a cluster — populated even on upstream-dial/connect failure (the cluster is known the moment routing resolves, avoiding the M42-1 record-build-on-error gap). An `Option<String>` shaped EXACTLY like `%UPSTREAM_HOST%`: text/mixed present → the cluster name, absent → the `-` sentinel; json single-op present → quoted string, absent → `null`. `None` on `direct_response` paths (no cluster routed). | value-exact (config-deterministic) | The cluster name is static config; both proxies route the same request to the same cluster and render the same name (independent of the backend's wire address or response). Fixture 0051 drives a routed request (`route: { cluster: backend }`) → `uc:"backend"` / `mixed:"c=backend"` (live-captured from v1.33.0). `%UPSTREAM_HOST%` is excluded from 0051 (per-side ip:port mismatch). Default-absent on all fixtures 0001-0050 (no cluster-routed access-log), keeping them byte-identical. |

> **NOTE (06.2-era posture, NOW PARTIALLY SUPERSEDED by phases 32 + 38).** The
> paragraph below records the original 06.2 stance that format-string
> customization was wholly out of scope. Phase 32 (ADR-0079) SUPERSEDES it for
> the modern `log_format.text_format_source.inline_string` path, which is now
> a supported configurable command-operator format engine — see the
> [Phase 32 (ADR-0079)](#phase-32-adr-0079-configurable-command-operator-format-engine)
> subsection below. **Phase 38 (ADR-0092) FURTHER SUPERSEDES it for `json_format`**,
> which is now the supported v1.33.0 oneof sibling of `text_format_source` — see the
> [Phase 38 (ADR-0092)](#phase-38-adr-0092-the-json_format-access-log-encoder)
> subsection below; **Phase 39 (ADR-0094) EXTENDS `json_format` to the full
> recursive `google.protobuf.Struct`** (nested objects + lists + `bool`/`null`
> literal leaves) — see the
> [Phase 39 (ADR-0094)](#phase-39-adr-0094-the-recursive-nested-json_format-encoder)
> subsection below. The remaining out-of-scope claims (`typed_json_format` — NOT a
> v1.33.0 field, ADR-0092 §C; the DEPRECATED inline `text_format` scalar; and the
> top-level `format` field) STILL HOLD and are still rejected.

Format-string customization via `typed_json_format`, the DEPRECATED inline
`text_format` field, and the top-level `format` field is OUT of scope. The
`envoy-config` validator at `validate_access_logs` rejects
non-`envoy.access_loggers.file` access-log names; those format fields are not
modeled on envoy-rust's `FileAccessLog` struct and serde `deny_unknown_fields`
rejects them. (The modern `log_format.text_format_source.inline_string` and
`log_format.json_format` paths are the EXCEPTIONS — they ARE modeled and ARE
supported per phases 32 + 38 below.) Future
observability-family phases extend this section with new tokens
(`%FILTER_STATE%`, `%DYNAMIC_METADATA%`, `%RESPONSE_CODE_DETAILS%`, etc.) when
the corresponding machinery lands.

### Phase 32 (ADR-0079): configurable command-operator format engine

> Phase 32 lands a configurable access-log format engine in the
> `envoy-accesslog` crate. A `log_format.text_format_source.inline_string`
> supplied on the `FileAccessLog` typed_config is compiled at config-load into
> a sequence of literals + command operators and rendered per record. The
> default format (the 14-token string above) is now re-expressed THROUGH this
> engine but remains byte-identical to the legacy 06.2 concatenator output.

**Grammar.** A `log_format` string is a sequence of literal text and command
operators. The operator forms are:

| Form | Meaning |
|---|---|
| `%OP%` | The operator `OP` with no argument. |
| `%OP(ARG)%` | The operator `OP` with argument `ARG` (e.g. a header name). |
| `%OP(ARG):N%` | As above, then BYTE-count truncate the resolved value to at most `N` bytes. |
| `%%` | A single literal `%`. This is the ONLY way to emit a literal `%`. |

`:N` truncation is a BYTE count, rounded DOWN to the nearest UTF-8 character
boundary (it never splits a multi-byte char and never panics; for ASCII values
`:N` = the first `N` bytes). The `?`-alternate form `%REQ(PRIMARY?ALT)%` (and
`%RESP(PRIMARY?ALT)%`) resolves to `ALT` when `PRIMARY` is absent; when combined
with `:N`, truncation applies to the RESOLVED value (whichever branch was used).

**Absent value renders `-`.** A resolved value that is absent renders as a
single dash `-` (never the empty string). A missing `%REQ(NAME)%` / `%RESP(NAME)%`
header, a `%UPSTREAM_HOST%` with no upstream (direct_response), and a
`%RESPONSE_FLAGS%` with no flags set (Envoy's no-flags sentinel) all render `-`.

**Boot-fatal config validity (ADR-0049 posture).** The format string is compiled
at config-load by `envoy-config`'s `validate_access_logs`, which calls
`envoy_accesslog::parse_format`. The following are ALL config-load errors that
abort boot (`ConfigError::InvalidAccessLogFormat`):

- an unknown operator keyword;
- a malformed / unterminated operator (e.g. `%REQ(` with no closing `)%`);
- an empty operator `%()%`;
- a stray / lone / trailing single `%` (the only literal `%` is `%%`);
- a `%REQ` / `%RESP` header name outside the §B support matrix below.

**§B operator support matrix (name → field allow-list).** The `AccessLogRecord`
struct has 15 named fields and NO generic header map, so `%REQ` / `%RESP`
operators resolve via a FIXED allow-list — any other header name is a config
error. Header-name matching is case-insensitive (ASCII). A `%REQ` / `%RESP`
operator is valid iff at least one of its branches (the primary name OR the `?`
alternate) maps to a backed field.

| Operator | Argument (header name) | → `AccessLogRecord` field |
|---|---|---|
| `%REQ(…)%` | `:method` | `method` |
| `%REQ(…)%` | `:authority` | `authority` |
| `%REQ(…)%` | `:path` | `path` |
| `%REQ(…)%` | `x-envoy-original-path` | `path` |
| `%REQ(…)%` | `x-forwarded-for` | `forwarded_for` |
| `%REQ(…)%` | `user-agent` | `user_agent` |
| `%REQ(…)%` | `x-request-id` | `request_id` |
| `%RESP(…)%` | `x-envoy-upstream-service-time` | `upstream_service_time` (ms) |

Standalone operators (no argument): `%PROTOCOL%`, `%RESPONSE_CODE%`,
`%RESPONSE_FLAGS%`, `%BYTES_RECEIVED%`, `%BYTES_SENT%`, `%UPSTREAM_HOST%`,
`%START_TIME%`, `%DURATION%`. Their internal sources and equivalence
dispositions are exactly as documented in the 06.2 token→source table above.

**Trailing-newline rule (Fact 7).** Upstream Envoy emits a custom
`inline_string` VERBATIM with NO auto-appended `\n`; the DEFAULT format string
carries its own trailing `\n`. envoy-rust matches this exactly: the engine
renders the format string verbatim and `FileSink::emit` appends NOTHING.
Consequently the default-format render is byte-identical to the legacy 06.2
concatenator output plus its trailing `\n` (proven by the
`compiled_default_matches_legacy_concatenator` unit test). ⇒ **fixture 0012
stays byte-identical, UNCHANGED.**

**Deterministic vs non-deterministic classification.** Operators split by
whether their resolved value is byte-stable across the two proxies:

| Class | Operators | Cross-proxy disposition |
|---|---|---|
| DETERMINISTIC | `%REQ(:METHOD)%`, `%REQ(:PATH)%`, `%REQ(X-ENVOY-ORIGINAL-PATH?:PATH)%` (both branches resolve to the deterministic `path` field), `%REQ(:AUTHORITY)%`, `%REQ(X-FORWARDED-FOR)%`, `%REQ(USER-AGENT)%`, `%PROTOCOL%`, `%RESPONSE_CODE%`, `%RESPONSE_FLAGS%`, `%BYTES_RECEIVED%`, `%BYTES_SENT%`, `%UPSTREAM_HOST%` | Whole-line byte-exact cross-proxy. Proven by **fixture 0040** (`0040-accesslog-command-operators`). `%UPSTREAM_HOST%` renders `-` on the direct_response path (byte-identical); its real `ip:port` render is proven in the in-process backstop. |
| NON-DETERMINISTIC | `%START_TIME%`, `%DURATION%`, `%REQ(X-REQUEST-ID)%`, `%RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%` | NEVER placed in a cross-proxy byte-exact fixture. Proven ONLY in the in-process backstop (the `envoy-accesslog` evaluator unit tests). Where surfaced cross-proxy, equivalence is asserted via the existing `AccessLogLineRule::Iso8601Format` / `DurationMs` allow-list rules. |

**Witness fixtures.** The cross-proxy byte-exact differential witness is
**fixture 0040** (`0040-accesslog-command-operators`), which exercises the
deterministic operators end-to-end. The in-process backstop — the
`envoy-accesslog` `command_operator` / `default_format` / `file_sink` unit
tests plus the H1 `compiled_log_format` wiring test — covers the
non-deterministic operators, the real-`ip:port` `%UPSTREAM_HOST%` render, and
the boot-fatal parse errors.

### Phase 33 (ADR-0081): the `%DYNAMIC_METADATA%` operator + `set_metadata`

> Phase 33 lands the smallest end-to-end dynamic-metadata loop: a per-request
> string-only dynamic-metadata store (namespace → key → value), the
> `envoy.filters.http.set_metadata` HTTP filter (a static-value metadata
> emitter), and the `%DYNAMIC_METADATA(namespace:key)%` access-log command
> operator. The §A facts below are LOCKED by ADR-0081 against
> `envoyproxy/envoy:v1.33.0`; the cross-proxy witness is **fixture 0041**
> (`0041-http-set-metadata-dynamic-metadata`).

**The `%DYNAMIC_METADATA(namespace:key)%` operator.** A new command operator in
the phase-32 engine. Its argument is a SINGLE-LEVEL, TWO-SEGMENT,
`:`-separated `namespace:key` path. Both segments are matched
CASE-SENSITIVELY (unlike `%REQ` / `%RESP` header names, which are
case-insensitive — dynamic-metadata namespaces and keys are exact-match map
keys). The operator carries NO `truncate` field.

- **Resolution.** The operator resolves
  `record.dynamic_metadata.get(namespace)?.get(key)` — the per-request store
  copied from `FilterRequest.dynamic_metadata` at the HCM record-build site.
- **Present value → RAW, UNQUOTED scalar string (§A3).** A scalar string leaf
  (`prod`) renders the bytes `prod` verbatim — NO surrounding quotes
  (`od -c` → `[ p r o d ]`, never `[ " p r o d " ]`). The value is emitted
  verbatim and is NOT re-parsed (a `%` inside a stored value is a literal `%`,
  never an operator).
- **Absent → `-` (§A4).** An absent KEY (`%DYNAMIC_METADATA(envoy.test:missing)%`)
  AND an absent NAMESPACE (`%DYNAMIC_METADATA(envoy.absent:k)%`) BOTH render a
  single dash `-` — never empty, never `{}`, never `null`. This reuses the
  engine's existing absent sentinel.

**Boot-fatal grammar (§A2, ADR-0049 posture).** The operator is compiled at
config-load by `parse_format` / `validate_access_logs`; the following are ALL
boot-fatal (`ConfigError::InvalidAccessLogFormat`, via
`FormatParseError::MalformedArgument { keyword: "DYNAMIC_METADATA", .. }`):

- **No argument** — `%DYNAMIC_METADATA%` with no `(…)` (Envoy:
  `DYNAMIC_METADATA requires parameters`).
- **A `:N` length suffix** — `%DYNAMIC_METADATA(envoy.test:tier):2%` (Envoy:
  `DYNAMIC_METADATA does not allow length to be specified.`, exit 1). The
  operator does NOT compose with `:N` truncation, unlike `%REQ`/`%RESP`.
- **A 1-segment (whole-namespace) arg** — `%DYNAMIC_METADATA(envoy.test)%`.
- **A 3+-segment (nested-path) arg** — `%DYNAMIC_METADATA(a:b:c)%`. (Envoy
  ACCEPTS nested struct traversal; the string-only single-level MVP rejects it
  — stricter than Envoy; the §2.2 nested-path deferral; NOT differentially
  exercised — fixture 0041 uses only `ns:key`.)

**Deterministic classification (cross-proxy).** A `%DYNAMIC_METADATA%` render of
a STATIC-config value is a pure function of static config (no host-address /
clock term), so both proxies emit a byte-identical line. The operator is
therefore DETERMINISTIC and is placed in the cross-proxy whole-line byte-exact
fixture **0041** (`Driver::Http1AccessLogByteExact` +
`assert_access_log_lines_byte_identical`, reused verbatim from phase 32). The
fixture's present + absent probe PAIR (`tier=prod missk=- missns=-`) guards
against an echo-the-config-literal implementation: the absent probe must
resolve `-` through the SAME store path.

**The `set_metadata` filter config shape (§A1).** The HTTP filter
`envoy.filters.http.set_metadata` writes static metadata into the per-request
store on the decode side (`Continue`-only; encode inert).

- **`@type`** is `type.googleapis.com/envoy.extensions.filters.http.set_metadata.v3.Config`
  — the proto message is named `Config`, NOT `SetMetadata` (the SPEC-projected
  `…v3.SetMetadata` DOES NOT EXIST; Envoy boot-fatal on it).
- **Modern repeated form only:** `metadata: [{ metadata_namespace, value,
  allow_overwrite }]`. Each entry merges its flat string→string `value` map
  into the request store under `metadata_namespace`, honoring `allow_overwrite`
  (Envoy default `false`).
- **String-only `value`.** `value` is a `BTreeMap<String, String>`; a non-string
  YAML scalar (`value: { tier: 7 }`) fails serde deserialization → boot-fatal
  in envoy-rust (the §2.2 non-string-Value deferral boundary; the fixture uses
  string values only).
- **Empty namespace → boot-fatal.** An empty `metadata_namespace` →
  `ConfigError::SetMetadataEmptyNamespace` (Envoy: PGV length ≥ 1; envoy-rust
  matches under ADR-0049 all-fatal). A name mismatch → `UnsupportedHttpFilter`.

**§2.2 deferrals (documented, NOT differentially exercised).** The following are
out of the string-only single-level-leaf MVP scope: non-string `Value`s (a
struct/object leaf renders as JSON WITH literal quotes in Envoy —
`{"sub":"deepval"}` — and a whole-namespace read renders a sorted JSON object;
the MVP resolves only scalar-string leaves → raw unquoted); nested paths
(`ns:a:b`); whole-namespace reads (`ns` alone); the DEPRECATED top-level
`set_metadata` form (`metadata_namespace` + `value` at `Config` top level — warn
in Envoy, but `deny_unknown_fields` rejects it boot-fatal in envoy-rust); and
the `:N` length suffix on `%DYNAMIC_METADATA%`.

### Phase 34 (ADR-0084): the `header_to_metadata` HTTP filter

> Phase 34 lands the `envoy.filters.http.header_to_metadata` HTTP filter — a
> request-side, header-driven dynamic-metadata emitter. Unlike `set_metadata`
> (which writes static config values), `header_to_metadata` inspects incoming
> request headers and writes the result into the per-request dynamic-metadata
> store. The §A facts below are LOCKED by ADR-0084 against
> `envoyproxy/envoy:v1.33.0`; the cross-proxy witness is **fixture 0042**
> (`0042-http-header-to-metadata`). Scope defined by ADR-0083.

**A1 — Wire shape (config-load, §A1-LOCKED by ADR-0084).**

- **`@type`** is
  `type.googleapis.com/envoy.extensions.filters.http.header_to_metadata.v3.Config`
  — the proto message is `Config` (NOT `HeaderToMetadata`).
- **`request_rules`** is the sole config field used at phase-34 scope: a
  repeated list of `Rule` objects, one per header source.
- Each `Rule` has three fields: `header` (string, the request header name to
  inspect), `on_header_present` (a `KeyValuePair` action applied when the header
  is present and non-empty), and `on_header_missing` (a `KeyValuePair` action
  applied ONLY when the header is FULLY ABSENT — not present at any value,
  including empty; a present-but-empty header is a distinct third disposition
  that writes NOTHING and does NOT trigger `on_header_missing` — see A4).
- A `KeyValuePair` action has three fields: `metadata_namespace` (string,
  defaults to the filter canonical name when omitted — see A2), `key` (string,
  the metadata key to write), and `value` (string, a static override — see A3).
  The `type` field is **string-only at phase-34 scope** (see §2.2 deferrals).

**A2 — Default `metadata_namespace` (§A2-LOCKED by ADR-0084).** When
`metadata_namespace` is **omitted** from a `KeyValuePair`, it defaults to
`envoy.filters.http.header_to_metadata` — the filter's canonical name — **NOT**
`envoy.lb`. This is the Envoy proto default (the field carries no proto default
override). The fixture always supplies `metadata_namespace` explicitly; the
default is exercised by the in-process backstop only. When the namespace is
provided (as in fixture 0042: `envoy.lb`), that value is used verbatim.

**A3 — Static `value` WINS over header value (§A3-LOCKED by ADR-0084).**
On a present, non-empty header:

- If `on_header_present.value` is **set** (non-empty string), that **static
  value** is written to the metadata store — the actual header value is IGNORED.
- If `on_header_present.value` is **absent** (not set), the **header value
  verbatim** is written to the metadata store.

The stored scalar is rendered by the `%DYNAMIC_METADATA(ns:key)%` operator as a
**RAW, UNQUOTED byte string** — the same phase-33 scalar render (a stored value
`prod` emits `prod`, never `"prod"`). The header value flow (no static `value`)
is proven by the in-process backstop; fixture 0042 exercises both paths.

**A4 — Absent and empty-header behavior (§A4-LOCKED by ADR-0084).**

- **Present but EMPTY header** (the header line exists but the value is the
  empty string): the `on_header_present` action does NOT fire. The key is left
  **UNSET** in the store; `%DYNAMIC_METADATA(ns:key)%` renders `-`.
- **Absent header** (no header line at all) with `on_header_missing` configured:
  the `on_header_missing` action fires — writing `on_header_missing.value` (the
  static value; see A-missing below) to the store.
- **Absent header** with NO `on_header_missing` configured: the key is left
  **UNSET**; `%DYNAMIC_METADATA(ns:key)%` renders `-`.

The **present-but-empty → UNSET** behavior is the load-bearing refinement: an
empty-valued header does NOT trigger `on_header_present` (it is neither
present-with-value nor absent-triggering-missing — it is a third disposition,
"present-but-empty", which results in no write). Fixture 0042 asserts the
absent-with-missing-action path; the empty-header-→-UNSET path is backstop-only.

**A-missing — `on_header_missing` REQUIRES a `value` (§A4-LOCKED by ADR-0084).**
An `on_header_missing` action without a `value` field set is **boot-fatal**
(`ConfigError::HeaderToMetadataInvalidRule` — see A5). A config that sets
`on_header_missing` without a value is rejected at config-load (Envoy rejects it
with `Filter: on_header_missing must have a value`). The `on_header_missing.key`
is likewise required to be non-empty.

**A5 — Malformed config is boot-fatal (§A5-LOCKED by ADR-0084, ADR-0049 posture).**
All of the following are startup-fatal (`ConfigError::HeaderToMetadataInvalidRule`
— the listener name and a human-readable `detail` string are included in the
error); the process exits before construction completes:

| Violation | Detail |
|---|---|
| Empty `header` field (the request header name is the empty string) | `"header name must not be empty"` |
| A rule with NEITHER `on_header_present` NOR `on_header_missing` set (a no-op rule) | `"rule must have at least one action (on_header_present or on_header_missing)"` |
| An `on_header_present.key` or `on_header_missing.key` that is the empty string | `"action key must not be empty"` |
| An `on_header_missing` action whose `value` is absent / empty | `"on_header_missing must have a non-empty value"` |
| An unknown field in the `Config` / `Rule` / `KeyValuePair` structs | `deny_unknown_fields` serde reject → `ConfigError::YamlError` |

**Deterministic classification (cross-proxy).** A `header_to_metadata` extraction
is a pure function of the (fixed) request headers and static filter config — no
host-address term, no clock term. Both upstream Envoy and envoy-rust therefore
produce a byte-identical access-log line for fixture 0042
(`Driver::Http1AccessLogByteExact` + `assert_access_log_lines_byte_identical`).
The fixture's present + absent probe pair guards against trivial implementations:
the absent probe must traverse the same store path and render `-` via
`%DYNAMIC_METADATA(ns:key)%`. The extraction is therefore DETERMINISTIC and the
whole-line byte-exact assertion is authoritative (§A6, ADR-0084).

**§2.2 deferrals (phase 34; documented, NOT differentially exercised).**

The following are out of the request-side string-only MVP scope (ADR-0083 / ADR-0084):

- **`response_rules`** — response-side header extraction. The `response_rules`
  field is present on the upstream Envoy `Config` proto but is **unmodeled in
  envoy-rust** at phase-34 scope; a config supplying `response_rules` is rejected
  boot-fatal by `deny_unknown_fields`.
- **Typed (non-string) values** — `type` field values other than the default
  `STRING` (e.g. `NUMBER`, `PROTOBUF_VALUE`). The `type` field is not modeled;
  a non-string type in a config is rejected boot-fatal.
- **`encode: BASE64`** — base64-encoding of the stored value. Not modeled;
  boot-fatal if supplied.
- **`regex_value_rewrite`** — regex-based value transformation. Not modeled;
  boot-fatal if supplied.
- **`remove`** — remove the source header after extraction. Not modeled;
  boot-fatal if supplied.
- **`cookie`** — extract from a cookie rather than a plain header. Not modeled;
  boot-fatal if supplied (stricter than Envoy, which accepts it).
- **Per-route config** (`typed_per_filter_config`) — per-route override of the
  filter config. Not modeled at phase-34 scope.

### Phase 35 (ADR-0086): the RBAC `metadata` Permission/Principal condition

> Phase 35 lands the RBAC `metadata` Permission/Principal condition — the FIRST
> dynamic-metadata CONSUMER in envoy-rust (the phase-34 `header_to_metadata`
> filter is its producer). An RBAC policy entry can now gate access on a value
> previously written into the per-request dynamic-metadata store. The §A facts
> below are LOCKED by ADR-0086 against `envoyproxy/envoy:v1.33.0`; the
> cross-proxy witness is **fixture 0043** (`0043-http-rbac-dynamic-metadata`).
> Scope defined by ADR-0085.

**A1 — Wire shape (config-load, §A1-LOCKED by ADR-0086).** A `metadata`
Permission/Principal entry is
`{ metadata: { filter: <string>, path: [{ key: <string> }, …], value: { string_match: <StringMatcher> } } }`.
The field names `filter` / `path` / `key` / `value` / `string_match` round-trip
**verbatim (snake_case)** through `/config_dump`. The entry is accepted under
**BOTH** `permissions[]` and `principals[]` with an identical shape. `value` is
**REQUIRED** — Envoy rejects an omitted `value` boot-fatal with
`MetadataMatcherValidationError.Value: value is required`; envoy-rust models a
string-only `ValueMatcher` (only `string_match` is accepted — see A6).

**A2 — `filter`→namespace correspondence (§A2-LOCKED by ADR-0086).**
`MetadataMatcher.filter` is matched against the dynamic-metadata **namespace**
(the producer's `metadata_namespace`). The phase-34 default producer namespace
`envoy.filters.http.header_to_metadata` (ADR-0084) is matchable; a custom
namespace is matchable by an equal `filter`. **Producer-before-consumer chain
order is REQUIRED** — a reversed `[rbac, header_to_metadata, …]` chain evaluates
RBAC against EMPTY metadata (so `X-Tier: prod` is wrongly `403`'d under an ALLOW
policy). Fixture 0043 orders the chain `[header_to_metadata, rbac, router]`.

**A3 — Match semantics + byte-exact verdicts (§A3-LOCKED by ADR-0086).** Runtime
eval is
`req.dynamic_metadata.get(&filter).and_then(|ns| ns.get(&path[0].key)).is_some_and(|v| value.matches(v))`
— an absent namespace OR an absent key → no match. The FULL 04.x StringMatcher
set flows through (`exact` AND `prefix` both confirmed live — do NOT restrict to
`exact`). `X-Tier: prod` → `tier=prod` → match → ALLOW → `200` + `ok\n` (3 bytes);
`X-Tier: dev` / absent → no match → DENY → `403` + `RBAC: access denied` (19
bytes, no trailing newline — the phase-10 / ADR-0034 deny body).

**A4 — Config-validity is boot-fatal (§A4-LOCKED by ADR-0086, ADR-0049 posture).**
An empty `filter` → boot-fatal (Envoy PGV `min_len 1`; envoy-rust
`ConfigError::RbacMetadataMatcherInvalid`); an empty `path: []` → boot-fatal
(Envoy PGV `min_items 1`; envoy-rust via the path-len≠1 check — see A5); a
missing `value` → boot-fatal (serde — a required non-`Option` field). The
structs carry `deny_unknown_fields`.

**A5 — MATERIAL DIVERGENCE: multi-segment `path`.** Envoy ACCEPTS a multi-segment
`path: [{key}, {key}]` (a nested-struct descent). envoy-rust's flat string-only
metadata store cannot resolve a nested path, so it is **STRICTER**:
`path.len() != 1` is boot-fatal (`ConfigError::RbacMetadataMatcherInvalid`,
detail "metadata matcher path must have exactly one segment …"). The nested path
is the deferred §2.2 work (it rides the future structured-`Value`
generalization).

**A6 — MATERIAL DIVERGENCE: non-`string_match` value.** Envoy ACCEPTS the full
`ValueMatcher` oneof (`present_match` / `null_match` / `double_match` /
`bool_match` / `list_match` / `or_match`); the string-only MVP rejects any
non-`string_match` key **BOOT-FATAL** via its hand-rolled "exactly one key"
visitor (an `unknown_field` serde error). `present_match` is the cheapest
deferred follow-up.

**A7 — MATERIAL DIVERGENCE: deprecation (NON-DIFFERENTIAL).** Both
`rbac.v3.Permission.metadata` AND `.Principal.metadata` are **DEPRECATED** in
v1.33.0 — Envoy boots with a stderr `warning` ("Using deprecated option … will
be removed from Envoy soon"), but the fields are FULLY FUNCTIONAL and accepted
at the pin (both verdicts are correct). The warning is stderr-only and therefore
**NON-DIFFERENTIAL** (no response / access-log / stats impact; envoy-rust does
not emit it). The future pin-refresh phase (D-3.7) inherits the flag that a
later Envoy may remove the field outright.

**§2.2 deferrals (phase 35; documented, NOT differentially exercised).** Out of
the string-only single-segment MVP scope (ADR-0085 / ADR-0086): non-string
`Value`s (the other `ValueMatcher` oneof arms — see A6); nested / multi-segment
`path` (see A5); `MetadataMatcher.invert` (negate the match); `shadow_rules`
(observe-only RBAC eval); per-route `typed_per_filter_config` overrides; and
other metadata producers / consumers (e.g. `jwt_authn` `payload_in_metadata`,
`ext_authz`, `ext_proc`). One honest limitation: a `SafeRegex` supplied in a
metadata `value`'s `string_match` is accepted at config-load but — matching the
pre-existing RBAC `header` matcher path — is NOT compiled by the RBAC validator,
so a runtime `matches` on a `SafeRegex` value would panic (a pre-existing
limitation, not exercised by fixture 0043, which uses `exact`). The
`parse_bootstrap` fuzz corpus seed (`hcm_rbac_metadata.yaml`) mirrors fixture
0043's `[header_to_metadata, rbac, router]` chain but is a separate corpus
artifact, NOT the differential fixture itself.

### Phase 36 (ADR-0088): RBAC matcher-VALUE enrichment — `present_match` + RBAC `safe_regex`

> Phase 36 enriches the phase-35 RBAC `metadata` matcher-VALUE surface with two
> features, both LOCKED by ADR-0088 against `envoyproxy/envoy:v1.33.0`: **F1** a
> `present_match` `ValueMatcher` variant (gate on KEY PRESENCE), and **F2**
> `safe_regex` `StringMatcher` compilation on the RBAC path (header + metadata
> values) — closing carry-forward M35-1. The cross-proxy witness is **fixture
> 0044** (`0044-http-rbac-matcher-value-enrichment`). Scope defined by ADR-0087.

**A1 — F1 `present_match` presence semantics (§A1-LOCKED by ADR-0088).** A
`metadata` value of `{ present_match: <bool> }` is accepted under BOTH
`permissions[]` and `principals[]` and round-trips **verbatim (snake_case)**
through `/config_dump`. Runtime semantics are **`match = present && want`** where
`present` is whether the metadata `filter:key` resolves to a stored value:
`present_match: true` matches IFF the key is present (any value); **`present_match:
false` NEVER matches** (even when the key is present). This is a MATERIAL
DIVERGENCE from the existing `HeaderMatcherMode::PresentMatch` (`want ? present :
true`) — the RBAC `ValueMatcher::PresentMatch` does NOT use that precedent.
A present-but-empty header → the `header_to_metadata` producer writes nothing
(ADR-0084) → key UNSET → `present=false` → `present_match: true` DENIES. Verdicts
are byte-exact: present → `200` + `ok\n` (3 bytes); absent → `403` + `RBAC:
access denied` (19 bytes, no trailing newline, ADR-0034). This supersedes the
phase-35 A6 note that `present_match` is "the cheapest deferred follow-up" — it
is now landed.

**A2 — F2 RBAC `safe_regex` is now compiled (§A3/A4-LOCKED by ADR-0088 — closes
M35-1).** A `safe_regex` `StringMatcher` supplied in an RBAC `metadata` value's
`string_match` OR in a `Permission`/`Principal` `header` `safe_regex_match` is now
**compiled at `rbac.rs` lowering time** (`lower_permission`/`lower_principal` are
fallible; the compiled `regex::Regex` is stored in `SafeRegex::compiled`). This
**SUPERSEDES the phase-35 §2.2 limitation note** that a `SafeRegex` RBAC value "is
accepted at config-load but NOT compiled … so a runtime `matches` would panic":
that latent panic (carry-forward M35-1) is **CLOSED**. A malformed RBAC
`safe_regex` is **BOOT-FATAL** (a returned `Err` from `build_from_config` fails
the listener build — pre-traffic startup, differentially equivalent to Envoy's
`--mode validate` reject), NOT a first-request panic. No new `ConfigError` /
`FilterError` variant (reuses `ConfigError::InvalidRegex` → `FilterError::InvalidConfig`).
The `metadata`-value and `header` `safe_regex` paths behave byte-identically.

**A3 — F2 anchoring (MATERIAL DIVERGENCE; carry-forward M36-1).** Envoy
`safe_regex` is **RE2 FULL match (anchored)**: with `prod|staging`,
`staging-2`/`xstaging`/`production` → `403` (the WHOLE string must match).
envoy-rust's `StringMatcher::matches` SafeRegex uses `regex::Regex::is_match` =
**PARTIAL** (substring), so an UNANCHORED pattern diverges (`is_match("prod|staging",
"staging-2") == true`). This is PRE-EXISTING (phase 04.2) and cross-cutting (the
route-config header `safe_regex` shares the path), masked because every existing
fixture uses an anchored pattern. **Disposition:** fixture 0044 + every phase-36
backstop LOCK an ANCHORED pattern (`^(prod|staging)$`) so partial==full and the
differential is byte-identical WITHOUT a SafeRegex-semantics change. The
unanchored partial-vs-full gap is **carry-forward M36-1 — OUT of phase-36 scope**
(a proper full-match fix touches the shared route-config SafeRegex; its own
future phase). DISTINCT from M35-1 (the latent panic), which F2 DOES close.

**A4 — Config-validity for non-`string_match`/`present_match` keys (§A5-LOCKED by
ADR-0088 — unchanged stricter posture).** envoy-rust ADDS ONLY `present_match` to
the hand-rolled "exactly one key" `ValueMatcher` visitor (`KEYS = ["string_match",
"present_match"]`). Envoy ACCEPTS the full oneof (`null_match` / `bool_match` /
`double_match` / `or_match` / `list_match`); envoy-rust STRICTER-rejects every
other oneof key **BOOT-FATAL** (`unknown_field`), preserving the phase-35 A6
posture for the remaining arms. The phase-35 A5 (multi-segment `path`) and A7
(deprecation) divergences are unchanged.

**Fixture 0044 probe matrix.** Chain `[header_to_metadata (x-tier→tier,
x-present→present_probe), rbac (action ALLOW; OR'd policies f2_regex + f1_present),
router]`; route `direct_response 200 "ok\n"`. probe a `x-tier: staging` → f2_regex
match → `200`; probe b `x-tier: dev` → no match → `403`+19B; probe c `x-present: 1,
x-tier: dev` → f1_present match → `200`; probe d `x-tier: dev` (no `x-present`) →
no match → `403`+19B.

**§2.2 deferrals carried (phase 36).** Unanchored SafeRegex full-vs-partial
(M36-1, above); plus the phase-35 carries that remain (nested/multi-segment
`path`; `MetadataMatcher.invert`; `shadow_rules`; per-route
`typed_per_filter_config`; other metadata producers/consumers; the remaining
non-string `ValueMatcher` oneof arms). The `parse_bootstrap` fuzz corpus gains two
seeds (`rbac_present_match.yaml`, `rbac_safe_regex.yaml`) — NO new fuzz target.

### Phase 37 (ADR-0089/0090): the RBAC `url_path` Permission/Principal condition

> Phase 37 adds the `url_path` condition type (Envoy `type.matcher.v3.PathMatcher`,
> `url_path: { path: { <StringMatcher> } }`) to BOTH the RBAC `Permission` and
> `Principal` enums on the existing phase-10 filter. `url_path` matches the request
> path with the `?query` STRIPPED (ADR-0090 §B: query-strip ONLY — Envoy applies NO
> percent-decode / dot-segment / slash-merge / case normalization by default at
> v1.33.0). The cross-proxy witness is **fixture 0045** (`0045-http-rbac-url-path`):
> `/allowed`→200, `/denied`→403+`RBAC: access denied` (19B), `/allowed?x=1`→200 (the
> query-strip discriminator). `safe_regex` is RE2 FULL-match against the stripped path
> (anchored patterns are portable; M36-1). Config-validity (empty/path-less PathMatcher,
> unknown sub-key, malformed regex) is boot-fatal on BOTH proxies (ADR-0090 §D).
> CARRY-FORWARD M37-1: `#fragment` in the request-target is rejected at the H1 codec
> (400) before url_path matching — a separate codec surface, OUT of phase-37 scope.

---

### Phase 38 (ADR-0092): the `json_format` access-log encoder

> Phase 38 adds the `json_format` output mode to
> `envoy.config.core.v3.SubstitutionFormatString` (the `log_format` on
> `FileAccessLog`). `SubstitutionFormatString` becomes the v1.33.0 oneof
> `{text_format_source | json_format}`; `json_format` is a
> `google.protobuf.Struct` of key → command-operator value string, modelled on
> envoy-rust as a `BTreeMap<String,String>`. Each value compiles through the
> EXISTING phase-32 command-operator engine (`parse_format`); a NEW
> `CompiledJsonFormat` renders ONE JSON object per request. The text/default
> render path is BYTE-FROZEN — JSON is a strict sibling (`LogFormat::{Text,Json}`).
> The cross-proxy witness is **fixture 0046** (`0046-accesslog-json-format`): a
> bare `GET /` on an H1 `direct_response` listener emits one byte-identical JSON
> object. All facts below are empirically locked against live
> `envoyproxy/envoy:v1.33.0` (ADR-0092 §A–§F).

**§A — key ordering.** Keys emit SORTED by UTF-8 bytes (digits < uppercase <
lowercase), exactly `BTreeMap<String>` iteration order — regardless of the order
the keys appear in the config. (Envoy's `json_format` sorts; the `BTreeMap`
config model reproduces it with no custom serde.)

**§B — value type inference.** A value is TYPE-INFERRED, not always a string:

| value shape | JSON token |
|---|---|
| EXACTLY one operator, numeric (`%RESPONSE_CODE%`, `%BYTES_RECEIVED%`, `%BYTES_SENT%`, `%DURATION%`) | unquoted number (`200`) |
| EXACTLY one operator, string-valued + present (`%REQ(:METHOD)%`, `%PROTOCOL%`, `%RESPONSE_FLAGS%`, `%START_TIME%`, present `%UPSTREAM_HOST%`/`%REQ(...)%`/`%RESP(...)%`/`%DYNAMIC_METADATA%`) | quoted, JSON-escaped string (`"GET"`) |
| EXACTLY one operator, Option-backed + ABSENT | `null` (NOT `"-"`) |
| literals + operator(s) (multi-segment) OR a literal-only value | quoted string via the engine; an ABSENT operator inside renders the `-` sentinel (`code-%RESPONSE_CODE%` → `"code-200"`; `x=%REQ(X-FORWARDED-FOR)%` → `"x=-"`; `1` → `"1"`) |

> Only OPERATORS are typed. `%DYNAMIC_METADATA%`'s single-operator classification
> (quoted-when-present / `null`-when-absent) follows §B's general rule; it is not
> in fixture 0046 and was not separately recon'd — backstop-test only.

**§C — `typed_json_format` is NOT a v1.33.0 field.** The typed behavior is
inherent to plain `json_format`; type inference is folded INTO this phase
(mandatory for byte-exactness), not deferred.

**§D — separators + escaping.** Compact separators `{"k":v,"k2":v2}`; ONE trailing
`\n` per object; escaping matches `serde_json` defaults (`"`→`\"`, `\`→`\\`,
`\n`/`\t`/`\b`/`\f`/`\r` short escapes, other C0 controls → `\u00XX`, non-ASCII
verbatim UTF-8, `/` NOT escaped). Hand-rolled (no new dependency). Both KEYS and
string VALUES are escaped this way.

**§E — validity (all boot-fatal, ADR-0049).** Exactly-one-of
`{text_format_source, json_format}`: BOTH-set AND NEITHER-set are boot-fatal
(`ConfigError::AmbiguousLogFormat`). An empty `json_format: {}` is VALID → emits
`{}\n`. An unknown key under `log_format` is boot-fatal (`deny_unknown_fields`). A
malformed value-operator is boot-fatal (`InvalidAccessLogFormat`, reusing the
phase-32 per-value `parse_format`).

**§F — authoritative fixture-0046 line** (bare `GET /`, Host `envoy-rust.test`,
`direct_response` `{status:200, body:"ok\n"}`):

```
{"bytes_rcvd":0,"bytes_sent":3,"flags":"-","method":"GET","mixed":"code-200","path":"/","protocol":"HTTP/1.1","status":200,"upstream":null}
```

### Phase 39 (ADR-0094): the RECURSIVE (nested) `json_format` encoder

> Phase 39 makes the phase-38 `json_format` encoder RECURSIVE — a `json_format`
> value may itself be a nested OBJECT (key → value map, to arbitrary depth), a
> LIST (sequence of values), or a `bool`/`null` literal, matching Envoy's
> `google.protobuf.Struct`. The config model becomes
> `json_format: Option<BTreeMap<String, JsonFormatValue>>` where
> `JsonFormatValue` is a `#[serde(untagged)]` enum
> `{ Null, Bool(bool), Format(String), Array(Vec<…>), Object(BTreeMap<String,…>) }`;
> the encoder becomes a recursive `CompiledJsonValue`. The phase-38 LEAF helpers
> (`encode_json_value`/`encode_single_op`/`json_escape_into`) are REUSED VERBATIM
> at every recursion leaf — the only new behavior is the `{…}`/`[…]` structural
> envelope + the `bool`/`null` literal arms. The text/default/flat-JSON render
> paths are BYTE-FROZEN (fixture 0046 stays byte-identical — the depth-1 instance
> of the recursive model is the regression witness). The cross-proxy witness is
> **fixture 0047** (`0047-accesslog-json-nested`). All facts below are
> empirically locked against live `envoyproxy/envoy:v1.33.0` (ADR-0094 §A–§H).

**§A — per-level key sorting.** Keys are SORTED by UTF-8 bytes at EVERY object
level independently (top level AND each nested object), exactly `BTreeMap`
iteration order. Phase-38 §A applied recursively, with zero extra work.

**§B — list order = config order.** A LIST emits its elements in CONFIG order
(NOT sorted) — lists are `Vec`, only objects sort.

**§C — at-depth type inference.** The SAME phase-38 per-leaf rule (§B above)
applies at every depth: a nested single numeric op → unquoted number; a nested
string op → quoted; a nested/in-list absent op → `null`; mixed/literal → quoted
string with the `-` sentinel. `encode_json_value` is reused verbatim.

**§D — non-string scalar leaves are NATIVE-TYPED.** A `bool` literal → `true` /
`false` (unquoted); a `null`/`~` literal → `null` (unquoted) — byte-exact and IN
this phase. **NUMERIC literal leaves are DEFERRED (CF-39-1):** a literal YAML
number in a `json_format` value routes through Envoy's protobuf-`double` JSON
serialization, a non-portable formatting rabbit hole (`1000000`→`1e+06`
scientific notation; `1.5`→`"1.5"` a YAML→Struct quirk). envoy-rust **boot-rejects
a numeric-literal `json_format` value** (a YAML number matches NO `#[serde(untagged)]`
arm → parse error) — a documented, narrow divergence (Envoy accepts it; operators
are strings and `bool`/`null` cover the realistic constant-field cases).

**§E — separators / terminator / nesting whitespace.** Compact separators
(`{"k":v,"k2":v2}` / `[a,b,c]`, NO spaces) at every level; exactly ONE trailing
`\n` on the WHOLE top-level object; NO inter-element / inter-level `\n` (nested
objects/lists are inline). The single `\n` is appended only by the top-level
render.

**§F — degenerate nesting.** Empty nested object `{}` → `{}`; empty list `[]` →
`[]`; a list containing an absent-operator leaf → `null` in place. All valid.

**§G — validity (all boot-fatal, ADR-0049).** A malformed operator in a NESTED
leaf at ANY depth is boot-fatal via the EXISTING `InvalidAccessLogFormat`
(the per-leaf `parse_format` validator now recurses the tree). The exactly-one-of
`{text_format_source, json_format}` (`AmbiguousLogFormat`) and the empty-top-map
acceptance (`{}\n`) are UNCHANGED from phase 38. **NO new `ConfigError` variant.**

**§H — authoritative fixture-0047 line** (bare `GET /`, Host `envoy-rust.test`,
`direct_response` `{status:200, body:"ok\n"}`; nested `arequest` object +
`blist` list + top scalars):

```
{"arequest":{"aaa":200,"method":"GET","zpath":"/"},"blist":["GET",200,null],"mtop":"code-200","zouter":"HTTP/1.1"}
```

---

### Phase 40 (ADR-0096): `omit_empty_values` — the absent-operator sentinel swap

> Phase 40 adds `SubstitutionFormatString.omit_empty_values` (a plain
> `#[serde(default)]` `bool`, default `false`). The SPEC's original "drop empty
> KEYS" framing is **VOID** — ADR-0096's empirical recon against live
> `envoyproxy/envoy:v1.33.0` overturned it. `omit_empty_values` is a **sentinel
> SWAP, NOT a key filter**: when `true`, the command-operator engine renders an
> absent operator as the EMPTY STRING `""` instead of the `-` sentinel, in the
> MULTI-SEGMENT render path. It threads as an `omit_empty: bool` into the EXISTING
> `render_value_segments` (the four `.unwrap_or("-")` sites become
> `.unwrap_or(if omit_empty {""} else {"-"})`), carried on the compiled
> `CompiledFormat` / `CompiledJsonFormat`. The cross-proxy witness is **fixture
> 0048** (`0048-accesslog-omit-empty`); fixture 0047 + all `0001`-`0047` (no flag)
> are the default-off byte-preservation witnesses. Facts §A–§E are empirically
> locked against live v1.33.0 (ADR-0096).

**§A — NO key/entry drop.** Every `json_format` key (and list entry) ALWAYS
emits. `omit_empty_values` does NOT omit JSON keys — the "omit" names the
absent-operator value rendering, not the key set. Typed-`null` keys survive as
`null`; an empty-literal value (`""`) survives as `""`.

**§B — the swap on the MULTI-SEGMENT render (BOTH formats).** An absent operator
inside a multi-segment / literal-prefixed value renders as `""` (not `-`) for
BOTH `text_format` and `json_format`:
`up=%UPSTREAM_HOST%` → `up=` (was `up=-`); `x=%REQ(X-FORWARDED-FOR)%` → `x=`
(was `x=-`); text `m=%REQ(:METHOD)% up=%UPSTREAM_HOST%` → `m=GET up=` (was
`m=GET up=-`). It is a render-engine-level behavior shared by both format arms.

**§C — single-operator-typed json values are UNAFFECTED (the carve-out).** A
`json_format` value that is EXACTLY one operator routes through the typed encoder
(`encode_single_op`: numeric→number, string→quoted, **absent→`null`**), NOT
through `render_value_segments`. A single absent op stays `null` under BOTH
`omit_empty_values` and the default — NOT `""`, NOT dropped. `encode_single_op`
is UNCHANGED by this phase.

**§D — recursive.** The swap threads through the recursive `render_into` to
EVERY leaf (nested objects + lists): a mixed value at depth gets the `-`→`""`
swap; a single-op value at depth stays `null`.

**§E — all-absent / config-validity.** A `json_format` of only single-absent ops
→ the keys survive as `null` (NOT dropped, NOT `{}`). `omit_empty_values: true`
composes with either arm (`text_format_source` or `json_format`);
`deny_unknown_fields` rejects typos; the exactly-one-of validator + empty-top-map
acceptance are UNCHANGED. **NO new `ConfigError` variant.**

**Authoritative fixture-0048 line** (bare `GET /`, `direct_response`
`{status:200, body:"ok\n"}`; `json_format` + `omit_empty_values: true`;
live-captured from `envoyproxy/envoy:v1.33.0`):

```
{"method":"GET","proto":"HTTP/1.1","single_up":null,"up":"up=","xff":"x="}
```

The flag-off control over the SAME map (live-captured) keeps the `-` sentinel:

```
{"method":"GET","proto":"HTTP/1.1","single_up":null,"up":"up=-","xff":"x=-"}
```

---

### Phase 70 (ADR-0140/0141): `status_code_filter` — the per-record emission gate

> Every access-log subsection ABOVE this one describes how a record is
> RENDERED. Phase 70 opens the orthogonal FILTER axis: WHETHER a record is
> emitted at all. An `AccessLog` entry gains an optional `filter`
> (`envoy.config.accesslog.v3.AccessLogFilter`, a oneof); this phase ships the
> single `status_code_filter` arm. When `filter` is absent the sink logs every
> record, so all 27 pre-phase-70 access-log fixtures are untouched. The
> cross-proxy witness is **fixture 0076**
> (`0076-accesslog-status-code-filter`): one file sink with a `GE 500` filter
> and two `direct_response` routes — the 503 record is emitted, the 200 is
> dropped, byte-exact on both proxies. All facts below are empirically locked
> against live `envoyproxy/envoy:v1.33.0` (ADR-0140 state-1 recon).

**§A — the gate is PER SINK, not per HCM.** `filter` sits on the `AccessLog`
ENTRY, beside its `typed_config` — not on the HCM. Each sink in `access_log: []`
carries its own independent predicate, so one HCM can hold an unfiltered sink
and a `GE 500` sink and each decides separately for the same record. A sink
whose predicate rejects a record writes NOTHING for it (and does not tick
`access_logs_total`, which counts EMITTED records only).

**§B — the schema.** The `filter` block nests exactly:

```yaml
filter:
  status_code_filter:
    comparison:
      op: GE                    # EQ | GE | LE
      value:                    # envoy.config.core.v3.RuntimeUInt32
        default_value: 500      # uint32
        runtime_key: unused     # non-empty string (see §D)
```

`op` is the upstream `ComparisonFilter.Op` enum — exactly `{EQ, GE, LE}`. An
unknown token (`NE`, `GT`, `LT`, lowercase `ge`) is boot-fatal via serde
(`ConfigError::Yaml`); upstream has no such operators.

**§C — the decision.** `op(status, default_value)` on the **FINAL response
code** decides emission — the same value `%RESPONSE_CODE%` renders, evaluated
after the response is complete. The comparison is
`status <op> default_value` with the config value on the RIGHT: `GE 500` drops
a 200 and keeps a 503; `EQ 404` keeps only a 404; `LE 200` keeps a 200 and
drops a 201. Boundaries are inclusive on both `GE` and `LE` (`GE 500` keeps a
500; `LE 200` keeps a 200).

**§D — `runtime_key` is REQUIRED but RTDS-INERT.** Upstream PGV marks
`RuntimeUInt32.runtime_key` `min_len 1`, so envoy-rust requires it non-empty
for LOAD PARITY — a config upstream rejects must not boot here. It has NO
behavioral effect: upstream would consult its runtime layer under this key for
a `default_value` override, but envoy-rust has no runtime subsystem, so the
comparison ALWAYS reads `default_value`. The key never reaches the wire, the
log, or the compiled runtime filter (`StatusCodeComparison` carries only
`{op, threshold}` — the key is dropped at compile time). Two configs differing
only in `runtime_key` are behaviorally identical; fixture 0076 uses the literal
`unused` to say so at the config site.

**§E — validity (all boot-fatal, ADR-0049).** Three fail-loud rules:

| config | outcome |
|---|---|
| `filter: {}` — the `AccessLogFilter` oneof sets NO arm | `ConfigError::AmbiguousAccessLogFilter` |
| `runtime_key: ""` — present but empty | `ConfigError::EmptyStatusCodeFilterRuntimeKey` |
| an unknown `op` token, or an unknown key anywhere under `filter` | `ConfigError::Yaml` / `deny_unknown_fields` |

The oneof cardinality is fail-loud rather than defaulted: a zero-arm `filter`
is an ambiguous instruction (log everything? nothing?), so it is rejected at
boot rather than silently guessed. Omitting `filter` ENTIRELY is the valid way
to say "log every record".

**§F — `direct_response` IS logged, and its 503 carries `%RESPONSE_FLAGS%` =
`-`.** A response the HCM authors itself (no upstream involved) is a normal
loggable record — the filter sees its status and gates on it exactly as it
would an upstream-derived one. The 503 in fixture 0076 renders the NO-FLAGS
sentinel `-`, NOT an upstream-failure flag such as `UF`/`UC`: no upstream was
attempted, so no failure flag applies. This is what keeps the fixture
deterministic (measured, ADR-0140 state-1 recon).

**§G — authoritative fixture-0076 file** (two probes — `GET /log` → 503 KEPT,
`GET /nolog` → 200 DROPPED; `text_format_source` inline
`STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)% FLAGS=%RESPONSE_FLAGS%\n`; `GE 500`).
Each proxy's file holds EXACTLY ONE line:

```
STATUS=503 PATH=/log FLAGS=-
```

The assertion is pure cross-proxy equality — both proxies must agree on the
KEPT half AND the DROPPED half. The line above documents the measured value; it
is not the oracle.

---

## xDS wire state machine

> **To be filled per-phase as needed.**
>
> The xDS state machine describes the legal sequence of
> `DiscoveryRequest` / `DiscoveryResponse` messages on both SotW (State of the
> World) and delta streams: which version and nonce fields are populated in
> which direction, how ACK and NACK are represented, how initial-fetch timeouts
> manifest, and how reconnection + resource caching interact. envoy-rust must
> match this state machine on the wire; effective-config snapshots must match
> upstream Envoy's config_dump on identical inputs.
>
> Populated when the xDS family (§9 of `BOOTSTRAP_PROMPT.md`) enters
> `in-progress`.

### Filesystem transport (`path_config_source`) — phase 18

> The xDS-family opener (ADR-0048 SPEC / ADR-0049 PLAN). file-based CDS loads
> clusters from `dynamic_resources.cds_config.path_config_source.path` at
> startup. All findings below are the §6.2 empirical lock-ins (L1–L12),
> verified against `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd…`,
> 2026-06-02) and reconciled by ADR-0049; what is bilaterally asserted lives in
> fixture 0026, the negative paths live in the in-process backstop
> (`crates/envoy-bin/tests/xds_file_based_cds.rs`).

**(a) The CDS file envelope (L1).** Both the bare `resources:` list AND the full
`DiscoveryResponse` shape (`version_info` + `resources`) are accepted; Envoy
treats `version_info` as load-bearing, envoy-rust accepts-and-ignores it. Each
resource MUST carry an `@type` (omitting it → Envoy `update_failure: 1` + log
`missing @type in Any is only allowed for an empty object` + the route 503s);
CDS files carry Cluster resources only. The byte-exact minimal working CDS file:

```yaml
resources:
- "@type": type.googleapis.com/envoy.config.cluster.v3.Cluster
  name: dynamic_backend
  type: STRICT_DNS
  dns_lookup_family: V4_ONLY
  load_assignment:
    cluster_name: dynamic_backend
    endpoints:
    - lb_endpoints:
      - endpoint:
          address:
            socket_address: { address: host.docker.internal, port_value: 8124 }
```

**Recorded divergence (ADR-0049): parser selection.** Envoy selects its parser by
**file extension** — `.yaml`/`.yml` → YAML parser (which also accepts JSON
syntax); any other or absent extension → JSON-only parser (YAML content in a
`.json`/extensionless file fails with `update_failure`). envoy-rust's
`parse_cds_file` is **always-YAML** (`serde_yaml`, regardless of extension) —
strictly more lenient on non-`.yaml` extensions. No differential observable: the
fixture's CDS file ends in `.yaml` and the Envoy-side container path
(`/etc/envoy-cds/cds.yaml`) is structurally `.yaml`. envoy-rust requires the
`@type` per resource (the ADR-0014 internally-tagged-on-`@type` pattern; a
non-Cluster `@type` rejects loudly).

**(b) Initial-load / readiness ordering (L2).** Readiness implies loaded on both
proxies. Envoy's startup log order: `cds: add 1 cluster(s)` → `cm init: all
clusters initialized` → `all dependencies initialized. starting workers`; the
dynamic cluster is routable the instant `/ready` first returns 200. envoy-rust
mirrors this naturally — `load_dynamic_resources` runs **synchronously** (a
`std::fs::read_to_string` between `parse_bootstrap` and `ClusterManager`
construction, before listeners bind). No settle/timing machinery is needed on
either side; fixture 0026's single GET fires after readiness and routes through
the CDS-supplied cluster.

**(c) Negative-path disposition (L4) — recorded divergence (ADR-0049).** This is
the load-bearing reconciliation. Envoy's disposition is a **3-way split**:

| CDS load fault | Envoy | envoy-rust |
|---|---|---|
| Nonexistent `path` | hard startup failure (container exits non-zero; `paths must refer to an existing path in the system` — a bootstrap-level PGV check) | **FATAL** (`CdsFileError`; process exits) — agrees with Envoy on this one class |
| File exists, malformed YAML/JSON | **starts and serves** (`/ready` 200), `cluster_manager.cds.update_failure: 1`, `active_clusters: 0`, the route 503s | **FATAL** (`CdsParseError`; process exits) — **diverges** |
| Valid YAML, semantically-invalid resource (PGV violation, e.g. empty `name`; cluster-build failure) | starts and serves, `update_rejected: 1` ticks (NOT `update_failure`), the route 503s | **FATAL** (per-cluster `validate_cluster` failure; process exits) — **diverges** |
| Unknown field inside a resource | **warn-accepted** (lenient protobuf parsing) | **FATAL** (`deny_unknown_fields` on the `Cluster` schema; process exits) — **diverges** |
| STRICT_DNS cluster with no `load_assignment` (zero-endpoint) | accepted as a zero-endpoint cluster (route → `no healthy upstream` 503) | **FATAL** (the existing `EmptyClusterEndpoints` invariant) — **diverges** |

envoy-rust treats **ALL CDS load errors as FATAL at startup** — the project's
fail-loud posture (every deferred field rejects loudly today). The warn-and-serve
alternative would require honoring `validate_clusters: false` at runtime + a
503-on-unknown-cluster data-plane path — machinery with zero differential
coverage (a deliberately-broken Envoy-side fixture is not a thing this project
does). **Consequence for the stats contract:** `cluster_manager.cds.update_failure`
and `cluster_manager.cds.update_rejected` register at 0 and are **structurally
unreachable non-zero** in envoy-rust (the process exits before any non-zero
state). fixture 0026 asserts both at 0 bilaterally (satisfiable on both sides — a
successful load); the negative paths are **backstop-only** (Envoy exits the
process on a fatal CDS error, which the differential harness cannot observe as a
data-plane response).

**(d) Static/dynamic name collision: STATIC WINS (L9) — ADR-0049.** A cluster
defined both statically and in the CDS file: **both proxies keep the STATIC one
and skip the CDS entry** as unmodified; no error, no startup failure. Envoy logs
`added/updated 0 cluster(s), skipped 1 unmodified cluster(s)`, `update_success`
still ticks 1, `/config_dump` shows the cluster under `static_clusters` only, and
the data plane serves the static endpoint. envoy-rust mirrors — on collision the
dynamic cluster is SKIPPED (with a `tracing::warn!`), the static cluster wins, no
error. (This reverses the SPEC D1 projection; the projected `DuplicateClusterName`
ConfigError variant was DROPPED. The backstop asserts the static endpoint's
distinct body serves on the data plane and that `dynamic_active_clusters` is
absent.)

**(e) Bootstrap prerequisites (L12) — recorded divergence (ADR-0049).**
- **`node.id` + `node.cluster` are REQUIRED by Envoy when CDS is configured** —
  without them Envoy exits at startup (`node 'id' and 'cluster' are required`).
  Both fixture sides carry a `node:` block (every existing fixture already does);
  envoy-rust parses `Node { id, cluster }` (phase 01) but adds **no mirror
  requirement validator** (both sides are always configured; no differential
  observable).
- **The static `route_config` referencing a CDS-supplied cluster requires
  `validate_clusters: false`** — without it Envoy exits at startup (`route:
  unknown cluster 'dynamic_backend'`), because Envoy's inline route-table
  validation runs against the static cluster set only. Both fixture sides set it.
  envoy-rust gains `RouteConfiguration.validate_clusters: Option<bool>` as
  **parse-and-accept** (the ADR-0024/0026 parse-only precedent) and does **NOT**
  honor its literal runtime-503 semantics. Instead envoy-rust enforces references
  via **defer-then-revalidate**: cluster-reference checks DEFER while
  `dynamic_resources` is configured-but-unloaded (`Bootstrap::cds_configured_but_unloaded()`)
  and RE-ENFORCE post-merge (inside `load_dynamic_resources`, against the
  effective static+dynamic list). **Recorded narrow divergence:** a route to a
  cluster in NEITHER list still **fails envoy-rust startup** (`UnknownCluster`),
  vs Envoy's runtime-503 under `validate_clusters: false`.

**(f) gRPC/ADS message-sequence state machine: UNPOPULATED.** The SotW/delta
`DiscoveryRequest`/`DiscoveryResponse` wire sequence (version/nonce population,
ACK/NACK representation, init-fetch timeouts, reconnection + resource caching)
remains **deferred to the gRPC-xDS phase**, which also **supersedes ADR-0014**
(the YAML-native typed-config shim) per ADR-0048. The intro blockquote above
describes that machine; phase 18 populates only the filesystem transport, which
has no on-the-wire message sequence (it is a synchronous file read).

### Filesystem transport (`path_config_source`) — phase 19 LDS extension

> The xDS-family continuation (ADR-0050 SPEC / PLAN). file-based LDS loads
> listeners from `dynamic_resources.lds_config.path_config_source.path` at
> startup. The lock-ins below (L1–L10) are the §6.2 empirical findings, verified
> against `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd…`, 2026-06-02) and
> reconciled by ADR-0050; what is bilaterally asserted lives in fixture 0027, the
> negative/fatal paths + the static/dynamic collision live in the in-process
> backstop (`crates/envoy-bin/tests/xds_file_based_lds.rs`). The LDS transport
> mirrors the phase-18 CDS transport structurally — the per-finding letters below
> intentionally parallel the CDS §(a)–(f).

**(a) The LDS file envelope (L1).** Same dual-envelope posture as CDS: both the
bare `resources:` list AND the full `DiscoveryResponse` shape (`version_info` +
`resources`) are accepted; Envoy treats `version_info` as load-bearing, envoy-rust
accepts-and-ignores it. Each resource MUST carry an `@type` (omitting it → Envoy
`lds.update_failure: 1`); LDS files carry **Listener** resources only, with the
type URL `type.googleapis.com/envoy.config.listener.v3.Listener`. envoy-rust's
`parse_lds_file` is **always-YAML** (`serde_yaml`, regardless of extension — the
same strictly-more-lenient stance as `parse_cds_file`; the Envoy-side container
path is structurally `.yaml`) and **requires** the `@type` per resource (the
ADR-0014 internally-tagged-on-`@type` pattern; a non-Listener `@type` rejects
loudly).

**(b) Initial-load / readiness ordering (L2).** Readiness implies loaded on both
proxies; the dynamic listener accepts connections the instant `/ready` first
returns 200. Envoy's startup log order with zero static listeners +
both LDS+CDS configured: `loading 0 listener(s)` → cds init → `cds: add N
cluster(s)` → `cm init: all clusters initialized` → `lds: add/update listener
'dynamic_listener'` → `all dependencies initialized. starting workers`.
**Clusters initialize BEFORE listeners are added** — this mirrors the §5.7
merge-ordering invariant (the dynamic listener's route_config can reference the
CDS-supplied cluster only because clusters land first). envoy-rust mirrors
naturally: `load_dynamic_resources` runs **synchronously** (CDS merge then LDS
merge, before listeners bind), so the sync-load order reproduces Envoy's
cds→clusters→lds→workers sequence. No settle/timing machinery is needed; fixture
0027's two GETs fire after readiness.

**(c) Negative-path disposition (L4) — recorded divergence (ADR-0050).** Envoy's
LDS disposition is the **same 3-way split** as its CDS split:

| LDS load fault | Envoy | envoy-rust |
|---|---|---|
| Nonexistent `path` | hard startup failure (container exits non-zero; `paths must refer to an existing path in the system` — a bootstrap-level PGV check) | **FATAL** (process exits) — agrees with Envoy on this one class (backstop path (ii)) |
| File exists, malformed YAML / missing `@type` | **starts and serves** (`/ready` 200), `listener_manager.lds.update_failure: 1` | **FATAL** (process exits) — **diverges** (backstop path (iii)) |
| Valid YAML, semantically-invalid listener (PGV violation) | starts and serves, `lds.update_rejected: 1` ticks (NOT `update_failure`) | **FATAL** (process exits) — **diverges** (backstop path (iv)) |
| Unknown field inside a resource | **warn-accepted** (lenient protobuf parsing) | **FATAL** (`deny_unknown_fields`; process exits) — **diverges** |

envoy-rust treats **ALL LDS load errors as FATAL at startup** — the ADR-0049
decision-2 all-fatal posture extended to LDS (pre-ratified by ADR-0050): missing/
unreadable file, malformed YAML, missing `@type`, unknown fields, per-listener
validation failure all exit the process before construction completes.
**Consequence for the stats contract:** `listener_manager.lds.update_failure` and
`listener_manager.lds.update_rejected` register at 0 and are **structurally
unreachable non-zero** in envoy-rust. fixture 0027 asserts both at 0 bilaterally
(satisfiable on both sides — a successful load); the negative paths are
**backstop-only** (Envoy exits the process on a fatal LDS error, which the
differential harness cannot observe as a data-plane response).

**(d) Static/dynamic listener name collision: STATIC WINS (L7) — ADR-0050.** A
listener defined both statically and in the LDS file: **both proxies keep the
STATIC one and skip the LDS entry**; only the static listener's port binds, no
error, no startup failure. Envoy: `lds.update_success` still ticks 1,
`listener_added: 1`, `/config_dump` shows `static_listeners` only, no
error/warning log. envoy-rust mirrors — on collision the dynamic listener is
SKIPPED (with a `tracing::warn!`), the static listener wins. The backstop (path
(v)) asserts the static listener's port serves while the LDS listener's port
refuses connections, and `listener_added == 1` / `total_listeners_active == 1`.

**(e) LDS+CDS composition + the LDS-route validation divergence (L6) — recorded
divergence (ADR-0050).** The composition works on both proxies (both `/static`
and `/dynamic` routes return 200). The `route_config` inside an LDS-supplied
listener does **NOT** require `validate_clusters: false` on Envoy — **Envoy skips
inline route-table cluster validation entirely for dynamically-delivered
listeners** (no `validate_clusters` knob needed; the check that CDS-delivered
static routes need suppressed simply does not run for LDS listeners). envoy-rust's
posture is **UNCHANGED** from phase 18: dynamic-listener routes go through the
same **defer-then-revalidate** enforcement (cluster-reference checks defer while
`dynamic_resources` is configured-but-unloaded, then RE-ENFORCE post-merge inside
`load_dynamic_resources` against the effective static+dynamic list). **Recorded
narrow divergence:** an LDS-listener route to a cluster in **NEITHER** list
**fails envoy-rust startup** (`UnknownCluster`), vs Envoy's start-and-runtime-503
— extending ADR-0049 decision-4's class to LDS routes (per ADR-0050 / SPEC §5.7).
`node.id` + `node.cluster` apply identically (both fixture sides carry a `node:`
block).

**(f) L10 conditionality narrowing (recorded divergence — ADR-0050).** On the
fixture-0026 topology (CDS configured, NO `lds_config`): Envoy emits ZERO
`listener_manager.lds.*` names but DOES emit the base `listener_manager.*` names
(`listener_added`, `listener_create_success`, `total_listeners_active`,
`workers_started`) **unconditionally**, AND a `ListenersConfigDump` entry for the
static-only listeners (at `configs[2]`). envoy-rust **gates both** on `lds_config`:
the 4 `lds.*` names + the base `listener_added` register only with `lds_config`
configured, and the `ListenersConfigDump` entry is emitted only with `lds_config`
configured (`total_listeners_active` is the sole exception — unconditional, per
its 08.2 registration). The backstop's inertness path (vi) verifies this on a
CDS-only bootstrap: no `lds.*` names, no `listener_added`, and `/config_dump` does
NOT contain `"ListenersConfigDump"`.

### Filesystem transport (`path_config_source`) — phase 20 RDS extension

> The xDS-family continuation (ADR-0051 SPEC / ADR-0052 PLAN). file-based RDS
> loads route tables from `rds.config_source.path_config_source.path` on the HCM
> at startup — completing the CDS+LDS+RDS filesystem triad. The lock-ins below
> (L1–L11) are the §6.2 empirical findings, verified against
> `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd…`, 2026-06-02) and
> reconciled by ADR-0052; what is bilaterally asserted lives in fixture 0028, the
> negative/fatal paths + the exactly-one-of dispositions live in the in-process
> backstop (`crates/envoy-bin/tests/xds_file_based_rds.rs`). The RDS transport
> mirrors the phase-18 CDS / phase-19 LDS transports structurally — the
> per-finding letters below intentionally parallel the CDS/LDS §(a)–(f).

**(a) The RDS file envelope (L1).** Same dual-envelope posture as CDS/LDS: both the
bare `resources:` list AND the full `DiscoveryResponse` shape (`version_info` +
`resources`) are accepted; Envoy treats `version_info` as load-bearing, envoy-rust
accepts-and-ignores it. Each resource MUST carry an `@type` with the type URL
`type.googleapis.com/envoy.config.route.v3.RouteConfiguration`; RDS files carry
**RouteConfiguration** resources only. The `rds`-on-HCM config shape is
`rds: { route_config_name, config_source: { path_config_source: { path }, resource_api_version? } }`.
envoy-rust's RDS parse is **always-YAML** (`serde_yaml`, regardless of extension —
the same strictly-more-lenient stance as `parse_cds_file`/`parse_lds_file`; the
Envoy-side container path is structurally `.yaml`) and **requires** the `@type`
per resource (the ADR-0014 internally-tagged-on-`@type` pattern; a
non-RouteConfiguration `@type` rejects loudly).

**(b) Initial-load / readiness ordering (L2).** Readiness implies loaded on both
proxies; the RDS route table is **active before `/ready` first returns 200** —
**no warm-up**. Envoy loads the route table at HCM construction (the RDS
`config_source` resolves synchronously for filesystem transport) so the route is
routable the instant the listener serves. envoy-rust mirrors this naturally via a
**synchronous load** (the RDS file is read between bootstrap parse and HCM
construction, before listeners bind); fixture 0028's GET fires after readiness and
routes through the RDS-supplied route table.

**(c) Negative-path disposition (L4) — recorded divergence (ADR-0052).** Envoy's
RDS disposition is the **same 3-way split** as its CDS/LDS splits:

| RDS load fault | Envoy | envoy-rust |
|---|---|---|
| Nonexistent `path` | hard startup failure (container exits non-zero; `paths must refer to an existing path in the system` — a bootstrap-level PGV check) | **FATAL** (`RdsFileError`; process exits) — agrees with Envoy on this one class |
| File exists, malformed YAML / missing `@type` | **starts and serves** (`/ready` 200), `http.<prefix>.rds.<name>.update_failure: 1` | **FATAL** (`RdsParseError`; process exits) — **diverges** |
| Valid YAML, semantically-invalid route config (PGV violation) | starts and serves, `rds.<name>.update_rejected: 1` ticks (NOT `update_failure`) | **FATAL** (process exits) — **diverges** |
| `route_config_name` mismatch (the file's `RouteConfiguration.name` ≠ the HCM's `rds.route_config_name`) (L6) | starts and serves, `rds.<name>.update_rejected: 1` + runtime 404 (the named route table never installs) | **FATAL** (`RdsRouteConfigNotFound`; process exits) — **diverges** |
| Unknown field inside a resource | **warn-accepted** (lenient protobuf parsing) | **FATAL** (`deny_unknown_fields`; process exits) — **diverges** |

envoy-rust treats **ALL RDS load errors as FATAL at startup** — the ADR-0049
decision-2 all-fatal posture extended to RDS (`RdsFileError`/`RdsParseError` fatal
at startup): missing/unreadable file, malformed YAML, missing `@type`, unknown
fields, `route_config_name` mismatch, per-route validation failure all exit the
process before construction completes. **Consequence for the stats contract:**
`http.<prefix>.rds.<name>.update_failure` and `…update_rejected` register at 0 and
are **structurally unreachable non-zero** in envoy-rust. fixture 0028 asserts both
at 0 bilaterally (satisfiable on both sides — a successful load); the negative
paths are **backstop-only** (Envoy exits the process on a fatal RDS error, which
the differential harness cannot observe as a data-plane response).

**(d) The exactly-one-of route-source disposition (L9) — ADR-0052.** An HCM's
route source is an **exactly-one-of** between the inline `route_config` and the
`rds` reference. **Both** sources present, OR **neither** present, are **FATAL on
both proxies** — no differential divergence:

| Route-source fault | Envoy | envoy-rust |
|---|---|---|
| Both `route_config` AND `rds` set | hard startup failure (protobuf `oneof` reject) | **FATAL** (`AmbiguousRouteSource`; parse-time exactly-one-of check) — **agrees** |
| Neither set | hard startup failure (PGV `route_specifier required`) | **FATAL** (`MissingRouteSource`; parse-time exactly-one-of check) — **agrees** |

envoy-rust enforces the disposition at **parse time** (the exactly-one-of check on
the HCM config), matching Envoy's startup-fatal disposition on both arms.

**(e) RDS+CDS composition + the route-revalidation divergence (L7) — recorded
divergence (ADR-0052).** An RDS-supplied route to a CDS-supplied cluster resolves
at **initial load**: **CDS merges BEFORE the RDS-route re-validation** (the §5.7
merge-ordering invariant — clusters land first, then the RDS route table
re-validates its cluster references against the effective static+dynamic list). An
RDS→CDS route needs **NO `validate_clusters: false`** — RDS behaves like LDS (the
dynamically-delivered route table is not subject to the static inline-validation
that CDS-static routes need suppressed), **not like a CDS-static route**; the
ADR-0050 L6 finding (LDS routes skip inline cluster validation) is **confirmed for
RDS**. envoy-rust's posture is the same **defer-then-revalidate** as phases 18/19:
cluster-reference checks defer while `dynamic_resources` is configured-but-unloaded,
then RE-ENFORCE post-merge. **Recorded narrow divergence:** an RDS-route to a
cluster in **NEITHER** list **fails envoy-rust startup** (`UnknownCluster`), vs
Envoy's start-and-runtime-503 — the same defer-then-revalidate narrow divergence
recorded for CDS (ADR-0049 §(e)) and LDS (ADR-0050 §(e)).

**(f) L5 conditional-emission narrowing (recorded divergence — ADR-0052).**
envoy-rust emits a `RoutesConfigDump` `/config_dump` entry **ONLY when some HCM
uses `rds`**; vs Envoy's **always-emitted** `RoutesConfigDump` (Envoy emits it with
`static_route_configs` even without any RDS — the inline route tables surface
there). On fixture 0028 the entry lands at **different `configs[]` indices** per
side, reconciled by a per-side `JsonSubtreeRule` path override in the harness:

| Side | `configs[]` layout (fixture 0028) | RoutesConfigDump index |
|---|---|---|
| Envoy | Bootstrap[0] / Clusters[1] / Listeners[2] / ScopedRoutes[3] / Routes[4] / Secrets[5] | `configs[4]` |
| envoy-rust | Bootstrap[0] / Clusters[1] / Routes[2] (Listeners gated off — no `lds_config` on 0028) | `configs[2]` |

The per-side path override bridges the index gap; **fixtures 0026/0027 hold** —
their Clusters[1] / Listeners[2] assertions are NOT displaced (the RoutesConfigDump
entry is RDS-conditional and absent on those topologies).

**Note (L8): the RDS file is SHAREABLE.** Unlike the per-side LDS templates (the
LDS file's static-listener address differs per proxy), one `rds.yaml` is consumed
**verbatim by both proxies** — the RDS route table carries no per-side address. A
single fixture file serves both Envoy and envoy-rust.

**Note (L10): an inline-route HCM emits zero `http.<prefix>.rds.*` names.** The
conditional registration (§Stat-name mapping) means an HCM whose route table is the
static inline `route_config` (no `rds`) participates in NO RDS update lifecycle and
registers none of the 5 `rds.*` names — verified by the backstop's inertness path
and by the 27 pre-existing fixtures seeing zero new names.

**Note (L11): version is Envoy-only.** Envoy's RDS update carries a `version_info`
(load-bearing on the wire); envoy-rust accepts-and-ignores it (per §(a)), and the
`rds.<name>.version` / `version_text` stats are **Envoy-only, not asserted** (per
the §Stat-name mapping Envoy-only enumeration).

### Filesystem transport (`path_config_source`) — phase 26 RDS HOT-RELOAD extension

> Phase 26 (`26-xds-rds-hot-reload`, ADR-0065 SPEC / ADR-0066 §6.2 reconciliation)
> makes the phase-20 file-based RDS route table **hot-reloadable**: a running HCM
> whose route table is RDS-supplied re-reads the edited file and atomically swaps
> the new table onto live traffic WITHOUT a restart. The lock-ins below are the
> §6.2 empirical findings, verified against `envoyproxy/envoy:v1.33.0` (digest
> `sha256:56da5afd…`, 2026-06-16, on Linux) — see ADR-0066 for the full probe
> transcript. **Fixture 0034's differential reload is Linux-CI-authoritative on a
> NATIVE-Linux runner** (the reload trigger is unobservable under macOS / Docker
> Desktop virtiofs — §5.7 / ADR-0049 Provenance); the in-process backstop
> (`tokio::time`-controlled) is the deterministic local complement for the negative
> paths. Initial-load semantics are unchanged (the phase-20 §(a)–(f) above hold).

**(g) Watch trigger + mechanism divergence (§6.2 P1/P2).** Envoy's default
file-watch (no `watched_directory`) reloads on **atomic-rename / move-into-path
ONLY** — an **in-place truncate-rewrite is NEVER detected** (verified 3× — Envoy's
filesystem subscription keys on the directory rename/move event, not on content
mtime), and `watched_directory` does **not** change that (it only redirects WHICH
directory's move events Envoy watches, e.g. for k8s ConfigMap symlink swaps).
**Consequence:** the differential harness must rewrite the RDS file via
**atomic-rename** (write-temp-then-`rename`) so BOTH proxies reload deterministically;
in-place rewrite would silently reload only envoy-rust. envoy-rust's watcher is
**poll-based on `std::fs::metadata().modified()` (mtime)** — the recorded mechanism
divergence (Envoy = inotify-on-move, near-instant; envoy-rust = interval poll) — and
mtime detects BOTH an atomic-rename (the moved-in inode carries a fresh mtime) AND an
in-place rewrite, so envoy-rust is strictly more permissive; both CONVERGE post-settle.
No `watched_directory` config schema is added (ADR-0066 — Task 9 N/A). Settle latency
~50 ms on Envoy; the harness waits for convergence on a discriminating observable
(the routed-to cluster / the `config_reload` counter advancing), never a fixed sleep.

**(h) Atomic-apply + in-flight isolation (§6.2 P1/P7).** The reload swaps the route
table with no listener drop and no dropped traffic (verified under concurrent load).
A request that began under the old table **completes under the old table** — each
request reads the current route-table `Arc` ONCE at entry and holds that snapshot for
its lifetime (§5.4 read-once); only NEW requests see the swapped table. Verified
bilaterally on Envoy (a 5 s in-flight request completed under the pre-reload route
table when the reload landed mid-flight).

**(i) Reload disposition — the warm-reject taxonomy (§6.2 P5; §5.5; ADR-0066).** At
RELOAD the proxy is already serving, so — unlike the ADR-0049 all-fatal STARTUP
posture — a bad reload is **warm-rejected** (last-good table KEPT, the failure counter
ticked, NO crash, NO dropped traffic):

| Reloaded-file fault | Envoy | envoy-rust |
|---|---|---|
| Valid change (new route table) | apply; `update_attempt`+`update_success`+`config_reload` each +1 | apply (atomic `store`); same counters +1 — **agrees** |
| Malformed YAML / IO / parse error | keep last-good; `update_failure` +1 | keep last-good; `update_failure` +1 — **agrees** |
| `route_config_name` absent from the envelope | keep last-good; `update_rejected` +1 | keep last-good; `update_rejected` +1 — **agrees** |
| Route → UNKNOWN cluster | **ACCEPT + apply** the broken table; `update_success`+`config_reload` +1; serve `503`/`no_cluster` on that route; last-good NOT kept | **keep last-good; `update_rejected` +1** (re-validates route→cluster refs against the immutable live cluster set) — **RECORDED DIVERGENCE (ADR-0066)** |

The unknown-cluster divergence exists because envoy-rust's request path resolves a
route's cluster via `cluster_mgr.get(name).expect(…)` (`crates/envoy-http1/src/hcm.rs:818`)
— installing an unknown-cluster route would panic the proxy (worse than Envoy's 503).
Matching Envoy's accept-and-503 would need a request-time missing-cluster→503 synth
path on both codecs (out of minimum-viable scope). The divergence is **unobservable in
fixture 0034** (its bad-reload probe drives the malformed → `update_failure` case, where
both proxies agree) and surfaces only in the envoy-rust-only in-process backstop.

**(j) `/config_dump` `RoutesConfigDump` on reload (§6.2 P6).** The
`dynamic_route_configs[]` entry reflects the reload: its embedded `route_config`
shows the NEW table and `last_updated` (RFC3339 ms timestamp) changes. **No
`version_info` key** is emitted for file-based RDS on EITHER proxy (it is simply never
populated — already the phase-20 envoy-rust `RoutesConfigDump` shape, §(f); confirmed
unchanged on reload). Fixtures 0026/0027 `configs[]` indices are unaffected. The
renderer reads through the swappable route-table handle (`current_route_config()`) so a
post-reload dump is current (Task 6). Emission stays RDS-conditional (phase-20 §(f)).

### Filesystem transport (`path_config_source`) — phase 21 EDS extension

> The xDS-family continuation (ADR-0053 SPEC / ADR-0054 PLAN). file-based EDS
> loads a cluster's `ClusterLoadAssignment` (endpoints) from
> `eds_cluster_config.eds_config.path_config_source.path` for a cluster declared
> `type: EDS` at startup — extending the filesystem-dynamic-config surface to the
> endpoint layer. The lock-ins below (L1–L11) are the §6.2 empirical findings,
> verified against `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd…`,
> 2026-06-05; ran LOCALLY) and reconciled by ADR-0054; what is bilaterally
> asserted lives in fixture 0029, the negative/fatal paths + the exactly-one-of
> dispositions live in the in-process backstop
> (`crates/envoy-bin/tests/xds_file_based_eds.rs`). The EDS transport mirrors the
> phase-18 CDS / phase-19 LDS / phase-20 RDS transports structurally — the
> per-finding letters below intentionally parallel the CDS/LDS/RDS §(a)–(f).

**(a) The EDS file envelope (L1) + the numeric-IP constraint.** Same dual-envelope
posture as CDS/LDS/RDS: both the bare `resources:` list AND the full
`DiscoveryResponse` shape (`version_info` + `resources`) are accepted; Envoy treats
`version_info` as load-bearing, envoy-rust accepts-and-ignores it. Each resource
MUST carry an `@type` with the type URL
`type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment` (omitting it →
Envoy `update_failure: 1`); EDS files carry **ClusterLoadAssignment** resources
only. The minimal working CLA is `cluster_name` + `endpoints[].lb_endpoints[].endpoint.address.socket_address`.
**NEW CONSTRAINT (L1): the `socket_address.address` MUST be a NUMERIC IP** — a
hostname (`host.docker.internal`) is rejected at load (Envoy: `malformed IP
address: … Consider setting resolver_name or setting cluster type to 'STRICT_DNS'`
→ `update_rejected: 1`, 503). EDS endpoints are treated as already-resolved socket
addresses (the STATIC semantics, NOT STRICT_DNS); the envoy-rust `Eds` cluster-build
arm shares the `Static` arm (numeric-IP `SocketAddr::from_str`). The
`eds_cluster_config`-on-cluster config shape is `type: EDS` + `eds_cluster_config:
{ eds_config: { path_config_source: { path }, resource_api_version? }, service_name? }`
(no inline `load_assignment`); `resource_api_version` and `service_name` are both
OPTIONAL. envoy-rust's EDS parse is **always-YAML** (`serde_yaml`, regardless of
extension — the same strictly-more-lenient stance as `parse_cds_file`/`parse_lds_file`/
`parse_rds_file`; the Envoy-side container path is structurally `.yaml`) and
**requires** the `@type` per resource (the ADR-0014 internally-tagged-on-`@type`
pattern; a non-ClusterLoadAssignment `@type` rejects loudly).

**(b) Initial-load / readiness ordering (L2) — warming resolves synchronously.**
Readiness implies loaded on both proxies; the EDS endpoint set is **active before
`/ready` first returns 200** — **no warm-up window**, the first `GET /` succeeds
immediately. An EDS cluster with no assignment "warms" in Envoy (held out of the
active set until its first `ClusterLoadAssignment` arrives or `initial_fetch_timeout`
fires), but for file-based EDS at initial load the file is read **synchronously** at
startup so warming resolves immediately (`cm init: all clusters initialized` at
boot; `cluster.<name>.warming_state: 0` after load). envoy-rust mirrors this
naturally via a **synchronous load** (`load_dynamic_resources` reads the EDS file
and populates `load_assignment` between bootstrap parse and `ClusterManager`
construction, before listeners bind — no runtime endpoint mutability, no locks, no
watch tasks); fixture 0029's GET fires after readiness and routes through the
EDS-supplied endpoint.

**(c) Negative-path disposition (L4) — recorded divergence (ADR-0054).** Envoy's
EDS disposition is a **warm-and-503** posture (only the missing-FILE-PATH is fatal),
which DIVERGES from envoy-rust's all-fatal posture on (b)–(d):

| EDS load fault | Envoy | envoy-rust |
|---|---|---|
| Nonexistent `path` | hard startup failure (container exits non-zero; `paths must refer to an existing path in the system` — a bootstrap-level PGV check) | **FATAL** (`EdsFileError`; process exits) — agrees with Envoy on this one class |
| File exists, malformed YAML / missing `@type` | **starts and serves** (`/ready` 200), `cluster.<name>.update_failure: 1`, 0 hosts, route 503 | **FATAL** (`EdsParseError`; process exits) — **diverges** |
| Missing/mismatched `ClusterLoadAssignment` (the file lacks a CLA matching the `service_name`/cluster name) | starts and serves, `cluster.<name>.update_rejected: 1` (NOT `update_failure`), 0 hosts, `/ready`=LIVE, route 503 (`Unexpected EDS cluster (expecting <name>): <other>`) | **FATAL** (`EdsClusterLoadAssignmentNotFound`; process exits) — **diverges** |
| Empty `resources: []` (no endpoints) | starts and serves, `cluster.<name>.update_empty: 1` **AND** `update_success: 1`, route 503 | **FATAL** (the existing `EmptyClusterEndpoints` validator; process exits) — **diverges** |
| Unknown field inside a resource | **warn-accepted** (lenient protobuf parsing) | **FATAL** (`deny_unknown_fields`; process exits) — **diverges** |

envoy-rust treats **ALL EDS load errors as FATAL at startup** — the ADR-0049
decision-2 all-fatal posture extended to EDS: missing/unreadable file
(`EdsFileError`), malformed YAML / missing `@type` (`EdsParseError`), missing/
mismatched CLA (`EdsClusterLoadAssignmentNotFound`), empty endpoints
(`EmptyClusterEndpoints`) all exit the process before construction completes.
**Consequence for the stats contract:** `cluster.<name>.update_failure`,
`…update_rejected`, and `…update_empty` register at 0 and are **structurally
unreachable non-zero** in envoy-rust. fixture 0029 asserts `update_failure`/
`update_empty` at 0 bilaterally (satisfiable on both sides — a successful load); the
negative paths are **backstop-only** (Envoy exits the process only on a missing
file PATH; its warm-and-503 dispositions cannot be observed as a clean differential
data-plane response). Only the missing-FILE-PATH class is fatal on BOTH proxies.

**(d) The exactly-one-of-and-consistent validation (L6) — 6b/6c match, 6a diverges
(ADR-0054).** An EDS cluster's endpoint source is an **exactly-one-of-and-consistent**
between the inline `load_assignment` and the `eds_cluster_config` reference, keyed
on `cluster_type`:

| Consistency fault | Envoy | envoy-rust |
|---|---|---|
| (6b) `type: STATIC` with `eds_cluster_config` | hard startup failure (`eds_cluster_config set in a non-EDS cluster`) | **FATAL** (`EdsConfigOnNonEdsCluster`) — **agrees** |
| (6c) `type: EDS` with neither `eds_cluster_config` nor `load_assignment` | hard startup failure (`cannot create an EDS cluster without an EDS config`) | **FATAL** (`MissingEdsClusterConfig`) — **agrees** |
| (6a) `type: EDS` with an inline `load_assignment` | **ACCEPTS** (runs the EDS subscription, silently ignores the inline `load_assignment`, serves 200) | **FATAL** (`LoadAssignmentOnEdsCluster` — STRICTER reject) — **diverges** |
| A non-EDS cluster with no `load_assignment` (now that the field is `Option`) | accepted (zero-endpoint / per type) | **FATAL** (`MissingLoadAssignment` — the migration's new required-source check) — **diverges** |

envoy-rust enforces the disposition at **validation time** before any EDS file is
read, matching Envoy's startup-fatal disposition on the 6b/6c arms; the 6a arm is a
**recorded narrow divergence** (envoy-rust rejects what Envoy accepts-and-ignores —
the established fail-loud posture; backstop-only).

**(e) The `service_name`-or-cluster-name selection (L8) — MATCH.**
`eds_cluster_config.service_name` selects WHICH `ClusterLoadAssignment` in the EDS
file feeds the cluster: when **unset**, the file's `ClusterLoadAssignment.cluster_name`
must equal the **cluster name**; when `service_name: X` is **set**, the file's
`cluster_name` must equal **X** (a mismatch → `update_rejected`, 503 on Envoy /
`EdsClusterLoadAssignmentNotFound` fatal on envoy-rust, per §(c)). The selection key
is `eds_cluster_config.service_name.unwrap_or(cluster.name)` on both proxies. (Note:
the cluster-build name-mismatch check `LoadAssignmentNameMismatch` applies to inline
non-EDS clusters only — an EDS cluster's populated CLA `cluster_name` equals
`service_name`, not necessarily the cluster name, so re-checking it against the
cluster name would falsely reject.)

**(f) L5 EndpointsConfigDump emission + `configs[]` ordering (recorded divergence —
ADR-0054).** Three Envoy behaviors diverge from the projection: **(1)** Envoy OMITS
`EndpointsConfigDump` from the DEFAULT `/config_dump` — it surfaces ONLY under
`/config_dump?include_eds`. **(2)** file-based EDS endpoints land under
`static_endpoint_configs[]`, NOT `dynamic_endpoint_configs[]` (Envoy classifies
file/path-based EDS as "static" config-dump-wise). **(3)** the `configs[]` order
under `?include_eds` interposes Endpoints between Clusters and Listeners. envoy-rust
emits a `EndpointsConfigDump` entry **ONLY when some cluster is `type: EDS`**, using
`static_endpoint_configs[].endpoint_config` (matching Envoy's file-based-EDS-is-static
taxonomy), pushed AFTER the (conditional) `ClustersConfigDump` and BEFORE the
(conditional) `ListenersConfigDump`. envoy-rust emits it on **EVERY** `/config_dump`
for an EDS bootstrap, **IGNORING** the `?include_eds` query param (a deliberate,
recorded narrowing vs Envoy's `?include_eds`-gated emission); to make the bilateral
scrape work, **envoy-rust's admin path dispatch strips the query string** (routing
`/config_dump?include_eds` to the `ConfigDump` endpoint — Envoy does the same; no
existing fixture uses query strings, so it is inert there). On fixture 0029 the entry
lands at **different `configs[]` indices** per side, reconciled by a per-side
`JsonSubtreeRule` path override in the harness (REUSING the ADR-0052 mechanism — no
new harness JSON code):

| Side | `configs[]` layout (fixture 0029, scraped with `?include_eds`) | EndpointsConfigDump index |
|---|---|---|
| Envoy | Bootstrap[0] / Clusters[1] / Endpoints[2] / Listeners[3] / ScopedRoutes[4] / Routes[5] / Secrets[6] | `configs[2]` |
| envoy-rust | Bootstrap[0] / Endpoints[1] (no `cds_config` on 0029 → no ClustersConfigDump) | `configs[1]` |

The per-side path override bridges the index gap; **fixtures 0014/0026/0027/0028
hold** — no EDS cluster → no Endpoints entry → their `configs[]` indices are NOT
displaced (§5.5). The `EndpointsConfigDump` is the **faithful resolved-endpoints
surface**; the surrounding `configs` array otherwise differs per side
(`value_may_differ_keys: ["configs"]`).

**Note (L1/numeric-IP, the load-bearing harness reconciliation D6): the EDS file is
a SHARED TEMPLATE with a per-side NUMERIC-IP marker.** A minimal `ClusterLoadAssignment`
is accepted verbatim, BUT the endpoint `socket_address.address` must be a NUMERIC IP
(per §(a)) that differs per side — the host backend is reachable from the Envoy
container only via the host-gateway (numeric IP varies by platform: `192.168.65.254`
on macOS Docker Desktop, the bridge gateway on Linux CI) and from the envoy-rust host
subprocess via `127.0.0.1`. So the EDS file is a SHARED template (one `eds.yaml`)
rendered per-side via a NEW `{{EDS_BACKEND_IP}}` kv marker (upstream → the
runtime-discovered numeric host-gateway IP; subject → `127.0.0.1`); the harness
DISCOVERS the numeric host-gateway IP at runtime (a one-shot `getent hosts
host.docker.internal` in the pinned Envoy image, gated to EDS fixtures). The EDS
rendition joins the backend-detection + `uses_host_gateway` scans (the phase-18
scan-ALL-rendered-sources bug-class lesson — fixture 0029's backend lives ONLY in
the EDS file). Contrast the SHAREABLE RDS file (§phase-20 Note L8): the RDS route
table carries no per-side address.

**Note (C19): the BootstrapConfigDump shows the POPULATED `load_assignment`.** The
EDS pass mutates `load_assignment` in-place on the bootstrap, so the
`BootstrapConfigDump` entry for a static EDS cluster shows the **populated**
`load_assignment` (a known minor divergence vs Envoy, which shows the cluster
as-configured with no resolved endpoints in BootstrapConfigDump). This is **NOT
asserted** — fixture 0029's config_dump probe asserts only the `EndpointsConfigDump`
`cluster_name` subtree; the surrounding `configs` array is `value_may_differ`. The
`EndpointsConfigDump` (§(f)) is the faithful resolved-endpoints surface.

**Note (L7): route-to-EDS-endpoint wire shape is MATCH.** A GET routed to an
EDS-supplied endpoint is byte-identical to a static-endpoint response (200 + backend
body byte-exact + `x-envoy-upstream-service-time` + `server: envoy` + the standard
allow-list; NO EDS-specific response header).

**Note (L11): version / warming gauges are Envoy-only.** `cluster.<name>.version` is
a nonzero xxhash of the assignment, `version_text` echoes `version_info` when present
(else `""`), `warming_state: 0` after load — all **Envoy-only, not asserted** (per
the §Stat-name mapping Envoy-only enumeration).

### Filesystem transport (`path_config_source`) — phase 27 EDS HOT-RELOAD extension

> Phase 27 (`27-xds-eds-hot-reload`, ADR-0067 SPEC / ADR-0068 §6.2 reconciliation)
> makes the phase-21 file-based EDS endpoint set **hot-reloadable**: a running
> cluster whose endpoints are EDS-supplied re-reads the edited file and atomically
> swaps the new endpoint set onto live traffic WITHOUT a restart. The lock-ins below
> are the §6.2 empirical findings, verified at the state-2 PLAN-write against
> `envoyproxy/envoy:v1.33.0` (on Linux) — see ADR-0068 for the full probe transcript.
> **Fixture 0035's differential reload is Linux-CI-authoritative on a NATIVE-Linux
> runner** (the reload trigger is unobservable under macOS / Docker Desktop virtiofs —
> §5.7 / ADR-0049 Provenance); the in-process backstop is the deterministic local
> complement for the negative paths. Initial-load semantics are unchanged (the
> phase-21 §(a)–(f) above hold; only the RELOAD path differs, notably apply-empty §(i)).

**(g) Watch trigger + mechanism divergence (carried from ADR-0066/§6.2).** Envoy's
default file-watch reloads on **atomic-rename / move-into-path** (inotify-on-move,
near-instant); an in-place truncate-rewrite is NEVER detected by Envoy. **Consequence:**
the differential harness rewrites the EDS file via **atomic-rename** (write-temp-then-
`rename`) so BOTH proxies reload deterministically. envoy-rust's watcher is **poll-based
on `std::fs::metadata().modified()` (mtime)** — the recorded mechanism divergence (Envoy
= inotify-on-move; envoy-rust = interval poll) — and mtime detects BOTH an atomic-rename
(the moved-in inode carries a fresh mtime) AND an in-place rewrite, so envoy-rust is
strictly more permissive; both CONVERGE post-settle. The harness waits for convergence on
a discriminating observable (the routed-to backend marker / `cluster.<name>.update_success`
advancing), never a fixed sleep. The watcher is generalized (the phase-26 RDS watcher → an
`XdsFileWatcher` shared by RDS and EDS — Task 3).

**(h) Atomic-apply + in-flight isolation (V6).** The reload swaps the cluster's
endpoint set with no listener drop and no dropped traffic. The set lives behind a
`RwLock<Arc<Vec<SocketAddr>>>` handle that each LB selection reads **ONCE** per pick
(§5.4 read-once); a request that already picked an endpoint **completes against it**,
and the next pick sees the new set. Verified bilaterally.

**(i) Reload disposition — the warm-reject / apply-empty taxonomy (ADR-0068; §5.5).**
At RELOAD the proxy is already serving, so — unlike the ADR-0049 all-fatal STARTUP
posture — a bad reload is **warm-rejected** (last-good endpoint set KEPT, the failure
counter ticked, NO crash, NO dropped traffic). **Unlike the phase-26 RDS unknown-cluster
RECORDED DIVERGENCE, the EDS taxonomy MIRRORS Envoy in ALL FIVE classes** — the headline
is the **apply-empty MIRROR** (the SPEC's projected warm-reject-empty divergence became a
MATCH):

| Reloaded-file fault | Envoy | envoy-rust |
|---|---|---|
| Valid change (new endpoint set) | apply; `update_attempt`+`update_success` each +1 | apply (atomic swap); same counters +1 — **agrees** |
| Malformed YAML / IO / parse error | keep last-good; `update_failure` +1 | keep last-good; `update_failure` +1 — **agrees** |
| No CLA matches the cluster's selection name (`service_name`/cluster name; envelope non-empty) | keep last-good; `update_rejected` +1 | keep last-good; `update_rejected` +1 — **agrees** |
| Matched CLA has an unparseable / non-numeric endpoint address | keep last-good; `update_rejected` +1 | keep last-good; `update_rejected` +1 — **agrees** |
| Matched CLA has `endpoints: []` (apply-empty) | `update_attempt`+`update_success` +1; **APPLY the empty set** (0 hosts → 503 "no healthy upstream"); last-good NOT kept | same: `update_attempt`+`update_success` +1; **apply empty** → `pick()` None → `synth_no_healthy_upstream` 503 (19 bytes); last-good NOT kept — **agrees (MIRROR, ADR-0068)** |
| `resources: []` (empty envelope, zero CLAs) | keep last-good; `update_empty` +1 (no-op) | keep last-good; `update_empty` +1 — **agrees** |

**Apply-empty asymmetry:** the apply-empty MIRROR is safe because `pick()` already
returns None on an empty endpoint set and the `synth_no_healthy_upstream` 503 path
exists. **Note the deliberate startup-vs-reload asymmetry:** `from_bootstrap` STILL
rejects an empty cluster at STARTUP (the all-fatal posture — the `EmptyClusterEndpoints`
validator, phase-21 §(c)); only the RELOAD path applies-empty to mirror Envoy. The
`update_rejected` counter is PROMOTED from Envoy-only (now emitted by envoy-rust — §2.1
phase-27 block / ADR-0068 §Decision-3). The taxonomy is **backstop-only** beyond the
successful-reload class (fixture 0035 drives the malformed bad-reload, where both agree).

**(j) `/config_dump` `EndpointsConfigDump` on reload (ADR-0068 §Decision-2).** The
`static_endpoint_configs[].endpoint_config.endpoints` reflects the **NEW** endpoints
after reload — rendered through the live swappable handle (Task 5), so a post-reload
dump is current. **NO `last_updated` / `version_info` key changes** — file-based EDS
emits neither (already the phase-21 `EndpointsConfigDump` shape, §(f); the SPEC's
`last_updated`-changes projection was WRONG — ADR-0068 §Decision-2). The other
config_dump sections' `configs[]` indices are unaffected. Emission stays EDS-conditional
(phase-21 §(f)).

**(k) MVP scope note — plain clusters only; EDS+HC/OD gets NO watcher (deferred
non-goal).** EDS hot-reload covers **PLAIN clusters only** (no active health checking,
no outlier detection). An EDS cluster configured WITH health checks or outlier detection
gets **NO watcher** — its endpoints are frozen at initial load (a recorded deferred
non-goal). The endpoint-index-aligned health-array / outlier-array rebuild that a live
endpoint-set swap would require defers — the same per-endpoint lifecycle churn that
defers CDS hot-reload.

---

## Timing tolerances

> **To be filled per-phase as needed.**
>
> Timing is not compared by default: envoy-rust and upstream Envoy run inside
> different processes/containers under different runtimes, so absolute latency
> numbers are incomparable in CI. A phase may opt in to a latency bound when
> the feature is fundamentally time-sensitive (e.g. outlier-detection
> ejection windows, timeout filter semantics, rate-limit windows). Every such
> opt-in records:
>
> - which metric is being bounded (p50, p99, absolute delta, count-in-window, …);
> - the bound itself and its justification;
> - whether the bound is one-sided (envoy-rust must not be slower than X) or
>   symmetric (both must lie within a shared window).
>
> Default: no opt-in, no timing comparison.

_(empty; no phase has opted in yet)_
