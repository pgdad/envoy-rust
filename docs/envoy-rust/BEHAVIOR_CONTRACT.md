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

_(empty; populated starting phase 06)_

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

_(empty; populated starting phase 06)_

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

_(empty; populated when xDS family begins)_

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
