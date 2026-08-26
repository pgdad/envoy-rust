# Fixture 0090 — h2-response-trailers

**Phase:** 111.

**Surface:** HTTP/2 response TRAILER forwarding, upstream → downstream. An HTTP/2 cleartext (H2C) downstream listener proxies to an HTTP/2 cleartext upstream that answers with a response TRAILER block; the trailers must reach the downstream client. This is the **first fixture in the project to exercise the `Response trailers` row** of the equivalence matrix in `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — a row seeded at phase 00 and never once witnessed until now.

**Why it exists:** it discharges the FIRST of the gRPC family's two blocking prerequisites. `ADR-0048` rejected the gRPC family in 2026-06 because "gRPC requires HTTP/2 trailer propagation … and the code survey confirmed trailers are discarded today"; `ADR-0177` re-affirmed the block for the data path. Scoped by `ADR-0181`, planned and empirically reconciled by `ADR-0182`. (The SECOND prerequisite — data/trailer hooks on the filter API — is explicitly NOT this phase.)

**Configuration:** copied VERBATIM from fixture `0010-http2-router-upstream`, with exactly two lines changed per side — the `node.id`/`cluster` labels and the cluster endpoint's port token. There is **NO trailer-related config on either side**: upstream Envoy forwards H2 response trailers on a stock config with no knob at all (`ADR-0181` DECISION 3, re-confirmed as `PLAN.md` PV-1), so this phase adds ZERO new config surface.

- Downstream listener: `http2_listener` binds `0.0.0.0:{{PORT}}` (Envoy) / `127.0.0.1:{{PORT}}` (envoy-rust); HCM `codec_type: HTTP2`; one virtual host (`domains: ["*"]`), one route (`prefix: "/"`) to cluster `backend`.
- Upstream cluster: `backend`, `type: STRICT_DNS`, resolving `{{BACKEND_HOST}}:{{HTTP2_TRAILERS_BACKEND_PORT}}`, with `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options: {}` selecting upstream H2.
- `generate_request_id: false` and the six-entry `request_headers_to_remove` list are inherited from `0010` on the upstream side only, and are **load-bearing**: they are what keep the deterministic echo body byte-equal across the two proxies. Removing either fails this fixture for a reason unrelated to trailers.

**Per-side divergences from `envoy.yaml`** (carried over verbatim from `0010`, which records the same list): bind `127.0.0.1` instead of `0.0.0.0`; no `admin` block; `request_headers_to_remove` omitted; `generate_request_id` omitted; `dns_lookup_family` omitted (envoy-rust ignores it at runtime per 05.4 D2 — only the upstream-Envoy side observes `V4_ONLY`).

**Backend:** `tests/helpers/http2-echo-server --trailers`, spawned by `Http2TrailersBackend` (`tests/differential/src/backend.rs`) and selected by the `{{HTTP2_TRAILERS_BACKEND_PORT}}` token. The mode keeps the deterministic echo body UNCHANGED and adds:

- the response header `trailer: x-trail-a` — the RFC 7230 §4.4 announce header, naming **only one of the two trailers it sends**, on purpose;
- a trailer block of `x-trail-a: alpha` (announced) and `x-trail-b: beta` (**NOT** announced).

Upstream Envoy forwards **both**, measured on the pinned `envoyproxy/envoy:v1.33.0`. So the rule under test is **"forward the block"**, not "forward what was announced" — which is why filtering by the announce header would itself be a divergence.

**Test driver:** `Driver::Http2 { method: GET, path: "/", host: "envoy-rust.test", expected_status: 200, expected_headers: set_equal_modulo_allow_list, expected_trailers: set_equal_modulo_allow_list }`. `expected_trailers` is new at phase 111 and this is its only user; it reuses `diff_headers` and the 3-entry `HEADER_ALLOW_LIST` verbatim, because the contract row's own wording is "Set-equal under the same allow-list discipline" (`PLAN.md` PV-4). Neither `x-trail-a` nor `x-trail-b` is allow-listed, so both are compared VALUE-EXACT — that comparison is this fixture's entire witness.

**Cells this fixture deliberately does NOT probe**, each excluded on a measurement rather than an omission (a fixture that goes RED for the wrong reason is worse than no fixture):

| cell | why excluded |
|---|---|
| a trailer block containing `connection` / `transfer-encoding` / `upgrade` / `keep-alive` / `proxy-connection` / `te` ≠ `trailers` | **CF-111-5** — envoy-rust returns `503` where Envoy returns `200` + body + `RST_STREAM(NO_ERROR)`. Root cause is `h2`'s RECEIVE-side validation inside the pre-existing body-drain loop; it reproduces with no trailer code in the tree at all. |
| a pseudo-header in the trailer block | **CF-111-6** — Envoy drops the whole block and resets; envoy-rust would forward the survivors. A divergence this phase would CREATE. |
| duplicate trailer NAMES | **CF-111-8** — Envoy forwards and preserves both, but `diff_headers` collapses names into a set and compares only the first value, so multiplicity is unassertable by any fixture. |
| trailer wire ORDER | **CF-111-9** — doubly invisible: `HeaderMap` iteration order is not insertion order, and the harness compares sets. |
| any stat assertion | **CF-111-7** — Envoy's `http2.trailers` and `cluster.<name>.http2.trailers` stats exist and stayed `0` across eight trailer-forwarding responses. |
| trailers on an EMPTY body; trailers on a non-200; five-trailer blocks | real and measured, but each needs a second backend mode or a second fixture. Covered instead by unit tests at the emit seam (`crates/envoy-http2/src/response.rs`), which is where the logic lives. |

**Regression witness:** fixture `0010-http2-router-upstream` is itself the no-trailers H2-in/H2-out case and stays untouched, so gate (b) over all 89 pre-existing fixtures IS the regression test for the three-way end-of-stream fork this phase introduces. Both no-trailer branches are additionally pinned by unit tests in `crates/envoy-http2/src/response.rs`.

**No `inputs/` directory** — the H2 driver does not read one.

**Cross-references:**

- `docs/envoy-rust/phases/111-h2-response-trailer-forwarding/SPEC.md` §3 D8 — fixture surface.
- `docs/envoy-rust/phases/111-h2-response-trailer-forwarding/PLAN.md` §1 PV-1/PV-2/PV-3 — the measurements this fixture and its exclusions rest on; Task 9 — this fixture.
- `docs/envoy-rust/DECISIONS.md` `ADR-0181` (scope), `ADR-0182` (empirical reconciliation).
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` `## Response trailers` — the contract section this fixture witnesses.
- Fixture `0010-http2-router-upstream` — the topology parent and the no-trailers regression witness.

**Acceptance signal:** `tests/differential/tests/h2_response_trailers.rs` green cross-proxy at the Docker-gated level.
