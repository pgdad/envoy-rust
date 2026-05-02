# Phase 05.4 — fixture-hardening follow-up: substantive close of phase-04.3 REVIEW C-1 (6 root-cause fixes)

- **Phase id:** `05.4`
- **Parent phase:** `05-http2` (split per **ADR-0022**; parent SPEC at `docs/envoy-rust/phases/05-http2/SPEC.md`, committed at parent-05 state-1 SHA `cd1a70e`).
- **Slug:** `05.4-fixture-hardening-followup`
- **Title:** Substantively close phase-04.3 REVIEW C-1 by landing the 6 root-cause fixes that 05.1's STRICT_DNS preamble proved necessary but not sufficient. The schema (`ClusterType::StrictDns`) + runtime (`tokio::net::lookup_host`) + 5-fixture YAML flip landed in 05.1 (commits `bfabcb6` / `f7a555d` / `0ce0aa2`) but the Docker-gated CI run on the canonical 05.1 head (`4768fcd`, CI run `25258722850`) revealed 6 distinct latent regressions that the STRICT_DNS flip exposed: (1) helper backends bound to loopback only — Docker host-gateway can't reach 127.0.0.1; (2) STRICT_DNS without `dns_lookup_family` — Envoy v1.33 default `AUTO` prefers AAAA on macOS Docker but backends are IPv4-only; (3) `envoy-config::Cluster` struct lacks the `dns_lookup_family` field — `deny_unknown_fields` rejects the V4_ONLY knob added in fix 2; (4) STRICT_DNS settle time too short — 500ms insufficient for DNS resolution to stabilise on host-gateway fixtures; (5) envoy-http1::client injects `content-length: 0` on empty-body GET — RFC 7230 §3.3.2 violation; Envoy v1.33 omits it; breaks fixture 0008's byte-equal echo body; (6) fixture 0006 `envoy.yaml` lacks the explicit `tls_inspector` listener filter — Envoy v1.33 on macOS Docker does not auto-inject it for SNI-based filter chain selection. Lands **ADR-0024** (Cluster `dns_lookup_family` schema), **ADR-0025** (envoy-http1 `content-length: 0` suppression on empty-body requests), and **ADR-0026** (Listener `listener_filters` parse-and-ignore field). Substantively closes phase-04.3 REVIEW C-1.
- **Depends on:** `05.1` (ROADMAP row `done` as of the 05.1 state-6 commit; the STRICT_DNS schema + runtime + YAML preamble is the precondition for these fixes — without `ClusterType::StrictDns` accepting `STRICT_DNS`, fix 2's `dns_lookup_family: V4_ONLY` knob has nothing to attach to). Strictly precedes `05.2` (downstream H2C codec/HCM/h2spec) per the `STATE.md` soft-gate established at the 05.1 state-6 commit (parent-05 SPEC §3 explicitly notes "05.2 depends on 05.1's restored Docker-gated baseline" — that baseline is what 05.4 substantively delivers).
- **Differential surface when done:** **no new fixtures.** All 8 pre-existing Docker-gated fixtures are simultaneously green: the 5 affected by C-1 (`tests/fixtures/0003-tcp-proxy/`, `tests/fixtures/0004-tls-downstream/`, `tests/fixtures/0005-tls-upstream/`, `tests/fixtures/0006-tls-sni/`, `tests/fixtures/0008-http1-router-upstream/`) restored to green by the 6 root-cause fixes; the 3 unaffected (`0001-tcp-echo`, `0002-static-admin-ready`, `0007-http1-direct-response`) remain green (5 of these 8 had been red on 05.1 head per CI run `25258722850`'s "NOT RUN" + the one RED 0008 binary; the 6 fixes were locally verified green in the 05.1-aborted attempt at `9279895` — "340 passed, 0 failed, 1 ignored; all 8 Docker-gated fixtures pass" per the backup-branch commit message).
- **Seeded by:** parent-05 SPEC §1 (the C-1 trace and the C-1 carryforward bookkeeping), §3 D1.1 (the 05.1-landed schema preamble that 05.4 builds on), §4 (non-goals — the deferral of `LogicalDns`, `dns_refresh_rate`, `respect_dns_ttl`, `dns_resolvers` continues unchanged in 05.4); 05.1 SPEC §1 (the M-claim unblocking and C-1 close-out projection), 05.1 PROGRESS.md Task 4 (the per-fixture CI matrix at run `25258722850` and the executor's narrative on the aborted in-session expansion); 05.1 REVIEW.md §3 I1 + §5 R1 (the disposition decision: defer to a follow-up sub-phase scoped against the captured CI artifacts + the backup-branch patch series); STATE.md "Disposition decision" section (option (b) — free-standing post-05.1 sibling sub-phase under parent-05, not a child of 05.1). The 6 root-cause fixes were pre-validated locally on backup branch `backup/task4-scope-creep-2026-05-02` (commit `9279895`, "340 passed, 0 failed, 1 ignored; all 8 Docker-gated fixtures pass") and are adopted verbatim under proper SPEC + ADR discipline in 05.4 — the procedural defect at the 05.1 attempt (no SPEC anchor, no ADRs, blew Task 4's 0-LoC contract) is corrected here, not the technical content.

This SPEC is the design contract for sub-phase 05.4. The next session converts it into `PLAN.md` per the phase lifecycle (§5 of `BOOTSTRAP_PROMPT.md` / `SKILL_ROUTING.md`). It is self-contained per doctrine D-3.4; a stranger reading only this file plus the stable doctrine documents (`MISSION.md`, `BEHAVIOR_CONTRACT.md`, `DECISIONS.md`, `BOOTSTRAP_PROMPT.md`) and the landed phase-05.1 surface (via `git log` and the in-tree `envoy-config` / `envoy-cluster` shape at HEAD `1d05cd0` — the 05.1 phase-done commit) must be able to execute it without consulting the parent `05-http2/SPEC.md` or the `05.1-fixture-hardening/SPEC.md`. The C-1 trace, the 05.1 partial close, and the 6 distinct root causes are reproduced inline below (§1, §3) for that reason.

---

## 1. Goal and acceptance signal

**Goal.** Land the 6 root-cause fixes that substantively close phase-04.3 REVIEW C-1 (the cross-phase Docker-gated `host.docker.internal`/`type: STATIC` regression that 05.1's STRICT_DNS preamble was projected to close but only partially closed; per 05.1 REVIEW.md §3 I1 the original Envoy v1.33 `malformed IP address` startup error IS gone after the 05.1 landing, but fixture 0008 surfaces a different `response_status: exact` mismatch — upstream Envoy returns 503, envoy-rust subject returns 200 — and fixtures 0003/0004/0005/0006 are NOT RUN because `cargo test` exits at the first failing binary). The 6 fixes are independently small (~5–25 LoC each) and were locally verified green on backup branch `backup/task4-scope-creep-2026-05-02` (commit `9279895`); 05.4 adopts them verbatim under SPEC + ADR discipline + per-task PROGRESS narration.

The 6 fixes (in execution order; full details under §3 deliverables):

1. **Helpers bind 0.0.0.0 instead of 127.0.0.1.** `tests/helpers/{tcp,tls,http1}-echo-server/src/main.rs` each have `TcpListener::bind(("127.0.0.1", port))` at HEAD `1d05cd0`; Docker containers reach the host process via `host.docker.internal`'s `host-gateway` mapping (per ADR-0015), which delivers traffic to a non-loopback host interface — backends bound to loopback do not see it. Bind to `0.0.0.0` so the host process accepts on all interfaces.
2. **`dns_lookup_family: V4_ONLY` on the 5 affected `envoy.yaml` files.** Envoy v1.33's `STRICT_DNS` cluster default is `dns_lookup_family: AUTO`, which prefers AAAA on dual-stack hosts; macOS Docker resolves `host.docker.internal` to an IPv6 address but the helper backends listen on IPv4 only (per fix 1). Forcing V4_ONLY restores reachability.
3. **`envoy-config::Cluster.dns_lookup_family: Option<DnsLookupFamily>` field + `DnsLookupFamily` enum.** With `#[serde(deny_unknown_fields)]` on `Cluster`, parsing the updated fixture YAMLs (post-fix-2) would fail. Adds the field as `Option<DnsLookupFamily>` with `#[serde(default)]` so fixtures that don't declare it parse cleanly. Adds the new `DnsLookupFamily` enum with three variants (`V4Only`, `V6Only`, `Auto`) plus 1 unit test. Lands **ADR-0024** at Task 1 alongside the schema growth.
4. **STRICT_DNS settle time 500 ms → 2000 ms for `host_gateway = true` fixtures.** `tests/differential/src/upstream.rs` currently sleeps a flat 500 ms after the upstream Envoy container's "starting main dispatch loop" log line; that's insufficient for DNS resolution to complete on `host.docker.internal`-via-host-gateway fixtures (the resolution races the first test probe). Conditional-bump to 2000 ms for `host_gateway = true` fixtures only (the 3 unaffected fixtures continue with 500 ms).
5. **Suppress `content-length: 0` on empty-body GET in `envoy-http1::client`.** Per RFC 7230 §3.3.2 "A user agent SHOULD NOT send a Content-Length header field when the request message does not contain a payload body and the method semantics do not anticipate such a body." Envoy v1.33 honors this; envoy-rust currently injects `content-length: 0` for every request. Fixture 0008's deterministic-echo body is byte-compared between the two proxies; the spurious `content-length: 0` lands in the echo body on the envoy-rust side only and breaks `response_body: byte_exact`. Suppress when the request body is empty AND no explicit Content-Length is set on the request. Lands **ADR-0025** at Task 5 alongside the behaviour change. Updates `tests/fixtures/0008-http1-router-upstream/expectations.yaml` to remove the spurious `content-length: 0` line from the expected echo body. Updates the 1 affected envoy-http1 unit test to flip its CL: 0 assertion.
6. **`tls_inspector` listener filter on fixture 0006 `envoy.yaml` + `envoy-config::Listener.listener_filters: Vec<serde_yaml::Value>` parse-and-ignore field.** Envoy v1.33 on macOS Docker does NOT auto-inject the TLS inspector listener filter when all filter chains use `server_names` + `DownstreamTlsContext` (the auto-injection works on Linux but not on the Docker-Desktop/macOS combination — verified by the 05.1 aborted attempt). Without it, SNI-based filter chain selection fails and the upstream Envoy returns a TLS handshake error. Add an explicit `listener_filters: [{name: envoy.filters.listener.tls_inspector, ...}]` block to fixture 0006's `envoy.yaml` only (envoy-rust performs SNI dispatch at the rustls layer per phase 03.2, so `envoy-rust.yaml` does not need it; the field would be rejected by envoy-rust's `deny_unknown_fields` if added there). To make `envoy-config` parse the new shape (the `Listener` struct currently has no `listener_filters` field — `deny_unknown_fields` would reject the new envoy.yaml-side block on any envoy-config parse), add `listener_filters: Vec<serde_yaml::Value>` to `envoy-config::Listener` with `#[serde(default)]` — parse-and-ignore semantics (envoy-rust never executes listener filters; the field is accepted purely for upstream-Envoy `envoy.yaml` compatibility under any future test path that parses envoy.yaml through envoy-config; see §6 signpost 4 below for the open question of which path that is). Lands **ADR-0026** at Task 3 alongside the new schema field. Adds the `listener_filters: vec![]` initialiser to the one hand-written `Listener` test fixture in `crates/envoy-tls/src/tests.rs::synth_listener_two_tls_chains`.

**No HTTP/2 work in 05.4.** This sub-phase is purely the substantive completion of 05.1's fixture-hardening preamble. The H2 codec layer, HCM-on-H2 dispatch, h2spec conformance gate, upstream H2 client, and router H2-arm all defer to sub-phases 05.2 and 05.3 per the parent-05 split decision (ADR-0022). The `envoy-http2` crate is NOT created in 05.4; the `h2 = "0.4"` dep is NOT added in 05.4; `CodecType::HTTP2` continues to reject in 05.4 (it lands accept-validation in 05.2). 05.4 introduces no new top-level Cargo deps — every new typed surface (DnsLookupFamily enum, listener_filters field, empty-body-CL suppression) lives in existing crates with their existing dep sets.

**Cross-phase items closed at 05.4.** One:

- **Phase-04.3 REVIEW C-1** (the cross-phase Docker-gated `host.docker.internal`/`STATIC` regression). 05.1 REVIEW.md §3 I1 dispositioned C-1 as "partially closed" — the schema + runtime + YAML preamble is necessary but not sufficient. 05.4's state-4 phase-done verification (D7) substantively closes C-1 by re-pushing CI with the 6 root-cause fixes landed and observing all 5 affected fixtures + all 3 unaffected fixtures green simultaneously. The C-1 carryforward chain (originating at phase-02.2's ADR-0015 landing `435c6fa`, latent across phases 02.2 → 03.1 → 03.2 → 04.1 → 04.2 → 04.3, surfaced at phase-04.3 task 14 commit `eb6f972`, partially closed at the 05.1 state-6 commit) ends at the 05.4 state-4 phase-done verification commit.

**Cross-phase items unblocked but not closed at 05.4.** One:

- **Phase-04.1 REVIEW M-claim** (the per-function `drive_http1` unit test that was masked by the Docker-gated regression on fixtures 0003–0008). 05.4's fix substantively unblocks the masking — fixture 0008 now exercises `drive_http1` end-to-end at every CI run — but the M-claim's own scope (a separate per-function unit test that mocks `tokio::io::AsyncRead`/`AsyncWrite` against a known-good HTTP/1.1 byte stream and asserts `drive_http1` parses the response correctly) stays deferred per the 04.3 disposition. 05.4 does NOT extend the harness; the masking-unblock is a side effect of D7, recorded in PROGRESS.md but not consumed by any new test. **Reasoning for not closing M-claim in 05.4:** the M-claim is an additive in-isolation test, not a fix to a regression; landing it in 05.4 would conflate two scopes (regression closure vs. test-coverage extension); it defers cleanly to whichever later phase first adds a third `Driver::Http1` consumer.

**Scope-shape inheritance from the parent-05 brainstorm + the 05.1 disposition.** The brainstorm explicitly bounded 05.4 to: schema growth (Cluster.dns_lookup_family + Listener.listener_filters; NOT the cluster-side `Http2ProtocolOptions` work which lives in 05.3, NOT the listener-side HCM `codec_type: HTTP2` accept-flip which lives in 05.2), runtime growth (envoy-http1 CL: 0 suppression only — NOT the H2 codec, NOT any HCM dispatch changes), helper changes (3 echo-server bind-address flips + 1 harness settle-time bump), fixture edits (1 `envoy.yaml` listener_filters block on fixture 0006 + 5 `envoy.yaml` `dns_lookup_family` knobs + 1 `expectations.yaml` echo-body update on fixture 0008 — NOT new fixtures 0009 or 0010 which land in 05.2 and 05.3 respectively). This bounding is reproduced in §4 below as 05.4's non-goals.

**The C-1 trace, reproduced inline for self-containment per D-3.4.** Upstream Envoy v1.33.0 originally rejected the rendered `address: host.docker.internal` under `type: STATIC` with this critical-log line:

```
[critical][main] [source/server/server.cc:416] error initializing config '/etc/envoy/envoy.yaml':
malformed IP address: host.docker.internal. Consider setting resolver_name or setting cluster type
to 'STRICT_DNS' or 'LOGICAL_DNS'
```

This trace is **GONE** after the 05.1 landing — both proxies now start cleanly with the STRICT_DNS-flipped fixtures. What 05.1 surfaced (and the canonical CI run `25258722850` against HEAD `4768fcd` captured) is a different defect on fixture 0008: upstream Envoy returns 503 (its upstream cluster cannot reach the backend); envoy-rust subject returns 200 (the envoy-rust side reaches `127.0.0.1` literal IP cleanly because envoy-rust.yaml carries the literal IP not the DNS name). Per CI run `25258722850`'s testcontainers logs (cited in 05.1 PROGRESS.md Task 4), the upstream-Envoy-side 503 traces to: (a) `host.docker.internal` resolves to an IPv6 address on macOS Docker default-AUTO `dns_lookup_family`; (b) the host-process http1-echo-server binds 127.0.0.1:HTTP1_BACKEND_PORT and does not accept on the IPv6 interface; (c) the upstream Envoy cluster's STRICT_DNS resolution succeeds (returning the IPv6 address) but then upstream connect fails with `Connection refused` and the router filter returns 503 to the client. The fix is in two parts: pin Envoy's `dns_lookup_family: V4_ONLY` (fix 2) so it resolves to IPv4, and bind the helper on 0.0.0.0 (fix 1) so the IPv4 endpoint is reachable from Docker's host-gateway. Fixtures 0003/0004/0005/0006 carry the same shape and were NOT RUN at the 05.1 head only because alphabetic test-binary ordering picks 0008 first; once 0008 is green, the four prior fixtures will also surface. Fixes 4/5/6 address related but independent regressions that the same CI re-push will surface.

**Acceptance signal** — the phase-done gate from §7.5 of `BOOTSTRAP_PROMPT.md`, scoped to 05.4's feature surface:

- (a) **all 5 Docker-gated differential fixtures restored to green simultaneously** at the Docker-gated CI level — `tests/fixtures/0003-tcp-proxy/`, `tests/fixtures/0004-tls-downstream/`, `tests/fixtures/0005-tls-upstream/`, `tests/fixtures/0006-tls-sni/`, and `tests/fixtures/0008-http1-router-upstream/` — with the CI run URL + the 8 individual test results (5 RESTORED + 3 unchanged) quoted inline in `PROGRESS.md` (§3 D7);
- (b) **all 3 pre-existing unaffected fixtures remain green** at the Docker-gated CI level — `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0007-http1-direct-response/`. The harness settle-time bump (D6) for `host_gateway = true` fixtures should not affect these (0001/0002/0007 do not set `host_gateway = true`); planner verifies at PLAN-write time;
- (c) no conformance suites run in 05.4 (the first one — `h2spec` — attaches in 05.2);
- (d) the existing fuzz target `parse_bootstrap` runs clean for its short-budget CI run (`cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`) against the corpus extended in 05.1 — **no new fuzz seed in 05.4** (the existing `strict_dns_cluster.yaml` seed at `crates/envoy-config/fuzz/corpus/parse_bootstrap/` continues to parse cleanly through the schema additions; the planner may optionally add a `cluster_with_dns_lookup_family.yaml` seed exercising the new field at PLAN discretion; not required by the gate);
- (e) `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job. `cargo deny check` is a no-op (05.4 introduces no new top-level Cargo deps);
- (f) `REVIEW.md` for this sub-phase is approved.

The 05.4 phase-done commit flips ROADMAP row `05.4` from `in-progress` to `done`. Parent row `05` stays `in-progress` until 05.3's phase-done commit (per the ROADMAP-schema invariant "parent flips to `done` only after all sub-phases are `done`"). STATE.md advances active phase from `05.4-fixture-hardening-followup` to `05.2-http2-downstream`; lifecycle state advances to phase 05.2 state 2 (PLAN.md does not exist yet for 05.2; 05.2's SPEC was landed at parent-05 state-2 alongside this SPEC's predecessor sub-phase SPECs in the same commit `f1804a7`). The next session runs `superpowers:writing-plans` scoped to sub-phase 05.2.

---

## 2. Behavior-contract scope for sub-phase 05.4

**No `BEHAVIOR_CONTRACT.md` edits in 05.4.** The 6 root-cause fixes produce no new responses, no new headers, and no new wire shapes that aren't already covered by 04.x's `Header allow-list` rows. The behaviour change in fix 5 (suppress `content-length: 0` on empty-body requests) brings envoy-rust into RFC 7230 §3.3.2 compliance and Envoy v1.33 parity — it removes a header from the upstream-bound request that Envoy never sent in the first place; the request-side header allow-list is unchanged because BEHAVIOR_CONTRACT.md only enumerates response-side headers. The five fixtures' response surfaces are unchanged: 0003 echoes a TCP payload byte-exact (matrix row 8); 0004/0005/0006 terminate or originate TLS with the same handshake sequence and the same cert-presentation surface (matrix rows 5, 6); 0008 proxies an HTTP/1.1 request and returns the upstream-determined response under the existing 04.x `Header allow-list` (rows: `server` / `date` / `x-envoy-upstream-service-time`).

Equivalence-matrix rows engaged transitively (per `BEHAVIOR_CONTRACT.md` §7.2):

- **Row 1 (Response status)** — fixture 0008 exercises this via the proxied HTTP/1.1 response (200 OK from `http1-echo-server`); fixtures 0003/0004/0005/0006 are TCP-shaped and don't engage this row;
- **Row 2 (Response body)** — fixture 0008 byte-exact body equivalence (`http1-echo-server`'s deterministic alphabetically-sorted-header echo body; fix 5 makes the body byte-equal across proxies by removing the spurious `content-length: 0` from the envoy-rust-side echo body); fixtures 0003/0004/0005/0006 byte-exact echo body;
- **Row 3 (Response headers)** — fixture 0008's response carries the existing 04.x `HEADER_ALLOW_LIST` from `tests/differential/src/lib.rs` (3 rows: `server`, `date`, `x-envoy-upstream-service-time`);
- **Row 5 (TLS handshake)** — fixtures 0004/0005/0006 (downstream TLS / upstream TLS / TLS SNI; fix 6 restores the SNI-based filter chain selection on fixture 0006);
- **Row 6 (TLS certificate validation)** — fixtures 0005/0006 (upstream cert validation, SNI-based cert selection);
- **Row 8 (TCP-stream byte equivalence)** — fixtures 0003/0004/0005/0006 (TCP proxy byte-stream).

No new rows engaged. **No new allow-list entries.** No new `Stat-name`, `Access log field`, `xDS wire`, or `Timing tolerances` subsections touched.

The `HEADER_ALLOW_LIST` constant in `tests/differential/src/lib.rs` (the 04.3-landed shape with 3 rows) is unedited in 05.4.

---

## 3. Deliverables

The 7 deliverables (D1–D7) map 1:1 to the 7 tasks the planner writes into PLAN.md. ADRs land at the first task that touches their typed surface, per the ADR-0021 / ADR-0023 inline-at-Task-1 precedent. Order is dependency-driven: D1 (schema) precedes D2 (fixture YAML edit that needs the schema); D3 (Listener.listener_filters schema) is independent of D1/D2 and may run in parallel; D4 (helper bind) is independent; D5 (envoy-http1 CL: 0 suppression) is independent; D6 (harness settle-time) is independent; D7 (state-4 verification) depends on D1–D6.

### D1 — `Cluster.dns_lookup_family` field + `DnsLookupFamily` enum in `envoy-config`

`crates/envoy-config/src/bootstrap.rs::Cluster` gains a new optional field `pub dns_lookup_family: Option<DnsLookupFamily>` with `#[serde(default)]`; `crates/envoy-config/src/bootstrap.rs` gains a new enum `pub enum DnsLookupFamily { V4Only, V6Only, Auto }` with the established `#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]` derive. Re-exported from `crates/envoy-config/src/lib.rs`. The enum has three variants because Envoy's proto enum has three (V4_ONLY / V6_ONLY / AUTO; the `V4_PREFERRED` and `ALL` variants from later Envoy versions are NOT in v1.33 per `ENVOY_TARGET.md` and are not added here); 05.4 only USES `V4_ONLY` in the 5 fixture YAMLs, but parsing the other two variants is required because future fixtures may use them and the parser must accept the full v1.33 surface.

Schema delta:

```rust
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum DnsLookupFamily {
    V4Only,
    V6Only,
    Auto,
}

// In `Cluster`:
#[serde(default)]
pub dns_lookup_family: Option<DnsLookupFamily>,
```

Parse tests added (1 minimum, per the backup-branch precedent; planner may add more if scope warrants):

- `parses_cluster_with_dns_lookup_family_v4_only` — STRICT_DNS cluster with `dns_lookup_family: V4_ONLY`; asserts the field deserialises to `Some(DnsLookupFamily::V4Only)`.

Hand-written `Cluster` initialisers in `crates/envoy-cluster/src/cluster.rs::tests` (2 sites at lines ~432 and ~474 of the backup-branch diff) gain `dns_lookup_family: None` to keep them building.

**Lands ADR-0024** at this task — see §7. ADR projection: schema grant for the V4_ONLY/V6_ONLY/AUTO surface; no runtime semantics in envoy-rust (envoy-rust's STRICT_DNS resolution via `tokio::net::lookup_host` already returns whatever the system stack delivers; the field is parsed-and-ignored at runtime in 05.4 — the upstream Envoy side is the only consumer that observes the knob, via fix 2's envoy.yaml edit).

LoC estimate: ~30 LoC (15 schema + 1 parse test + 2 hand-written initialiser updates + ADR-0024 ~13 LoC in DECISIONS.md).

### D2 — Coordinated 5-fixture `envoy.yaml` edit: `dns_lookup_family: V4_ONLY`

Add `dns_lookup_family: V4_ONLY` to the cluster definition in `tests/fixtures/{0003,0004,0005,0006,0008}/envoy.yaml` — exactly 5 files, exactly 1 line added per file, immediately after `type: STRICT_DNS`. **Only `envoy.yaml` is edited; `envoy-rust.yaml` is NOT edited** because envoy-rust uses `127.0.0.1` (literal IP) at the substituted `{{BACKEND_HOST}}` site and DNS resolution doesn't apply (the `tokio::net::lookup_host("127.0.0.1:port")` call returns the literal IP through unchanged; there is no IPv4-vs-IPv6 selection to make).

The single-bundled-commit cadence (vs the alternative per-fixture cadence) is the recommended posture per 05.1 SPEC §6 signpost 8 — the 5 edits are mechanically identical and the differential property is "all 5 fixtures green simultaneously"; splitting would muddy the gate signal. **Recommended: single bundled commit** matching 05.1 Task 3's posture (`0ce0aa2`).

LoC estimate: ~5 LoC YAML diff total.

### D3 — `Listener.listener_filters: Vec<serde_yaml::Value>` parse-and-ignore field + fixture 0006 `tls_inspector` block

`crates/envoy-config/src/bootstrap.rs::Listener` gains a new field `pub listener_filters: Vec<serde_yaml::Value>` with `#[serde(default)]`. Semantics: parse-and-ignore — envoy-rust performs SNI dispatch at the rustls layer (per phase 03.2's design; verified at `crates/envoy-tls/src/lib.rs` SNI handler) and does NOT execute listener filters; the field is accepted purely so that `envoy.yaml` fixtures including `listener_filters: [...]` for upstream-Envoy compatibility do not trigger `deny_unknown_fields` rejection on any path that parses envoy.yaml through envoy-config.

**This is a new pattern in `envoy-config`.** Every previous YAML divergence between `envoy.yaml` and `envoy-rust.yaml` (e.g., fixture 0005's `admin:` section, fixture 0008's `request_headers_to_remove` + `generate_request_id: false`) used field-set divergence: the field exists in envoy.yaml and is absent from envoy-rust.yaml; envoy-rust's `deny_unknown_fields`-bearing parser is never asked to parse the envoy.yaml side. The parse-and-ignore pattern flips this: the field is added to envoy-config's typed surface but stored as opaque `serde_yaml::Value` (not parsed into a typed struct) and ignored at runtime. This is the right call for `listener_filters` specifically because: (a) the field carries arbitrary listener-filter typed_config payloads (`tls_inspector` is one of many possible filters; future Envoy versions may surface more); typing the variants exhaustively would be a non-trivial growth surface; (b) envoy-rust never executes listener filters by design (architectural choice from phase 03.2 — SNI lives in the rustls layer); (c) making the parse-and-ignore explicit at the schema level is more honest than maintaining field-set divergence forever.

**Lands ADR-0026** at this task — see §7. ADR projection: introduces the parse-and-ignore pattern as a documented, narrowly-scoped envoy-config posture; explicitly bounds it to `listener_filters` and lists the criteria for adding future parse-and-ignore fields (must be Envoy-config-only with no envoy-rust runtime semantics; must be required for upstream-Envoy `envoy.yaml` parseability under any test path; must be reviewed under D-3.5 ambiguity-resolution discipline).

Parse test added: `parses_listener_with_tls_inspector_listener_filter` — full bootstrap with one TLS-bearing listener carrying a `listener_filters: [{name: envoy.filters.listener.tls_inspector, ...}]` block; asserts the listener parses cleanly and `listener.listener_filters.len() == 1`.

Hand-written `Listener` initialiser in `crates/envoy-tls/src/tests.rs::synth_listener_two_tls_chains` gains `listener_filters: vec![]`.

Fixture 0006 `envoy.yaml` (only `envoy.yaml` — NOT `envoy-rust.yaml`) gains the explicit listener_filters block:

```yaml
listener_filters:
  - name: envoy.filters.listener.tls_inspector
    typed_config:
      "@type": type.googleapis.com/envoy.extensions.filters.listener.tls_inspector.v3.TlsInspector
```

placed immediately after the `address:` line on the `tcp_listener` listener.

LoC estimate: ~25 LoC core + ~85 LoC parse test (per backup-branch diff) + ~9 LoC fixture YAML + ~13 LoC ADR-0026 = ~130 LoC total.

### D4 — Echo-server helpers bind 0.0.0.0

Three single-line edits across three helper crates:

- `tests/helpers/tcp-echo-server/src/main.rs` line ~118: `TcpListener::bind(("127.0.0.1", port))` → `TcpListener::bind(("0.0.0.0", port))`.
- `tests/helpers/tls-echo-server/src/main.rs` line ~109: same flip.
- `tests/helpers/http1-echo-server/src/main.rs` line ~98: same flip.

The corresponding `tracing::info!` lines update to log `0.0.0.0:{port}`. The doc-comments at the file headers update to drop "localhost-only" language (the http1-echo-server header comment carries this string at HEAD `1d05cd0` per the backup-branch diff).

**No new tests.** The bind-address change is mechanically observable (the helpers continue to accept on 127.0.0.1 — 0.0.0.0 binds all interfaces including loopback); existing per-helper tests continue to pass unchanged.

**No ADR.** This is a test-helper bug fix to remove an over-restrictive bind. ADR-0015's host-gateway grant is the operative cross-reference (already landed at `435c6fa`); the bind-address fix is consistent with that grant's intent ("the upstream Envoy reaches the host process via host-gateway") — the previous loopback-only bind was an under-implementation, not a deliberate architectural choice.

LoC estimate: ~10 LoC across 3 files.

### D5 — Suppress `content-length: 0` on empty-body GET in `envoy-http1::client`

`crates/envoy-http1/src/client.rs` (around lines 91–110 of HEAD `1d05cd0`) currently injects `content-length: <len>` for every request that doesn't already carry an explicit Content-Length header. For empty-body requests (e.g., GET with no body), this emits `content-length: 0`, which Envoy v1.33 omits per RFC 7230 §3.3.2:

> A user agent SHOULD NOT send a Content-Length header field when the request message does not contain a payload body and the method semantics do not anticipate such a body.

The behaviour change: only inject the synthetic Content-Length when the body is non-empty (or when the request explicitly carries one — pass-through unchanged in that case). Pseudocode:

```rust
let request_has_cl = request.headers.iter().any(|(n, _)| n.eq_ignore_ascii_case(hdr::CONTENT_LENGTH));
let body_is_nonempty = request.body_bytes().is_some_and(|b| !b.is_empty());
if !request_has_cl && body_is_nonempty {
    wire.extend_from_slice(b"content-length: ");
    wire.extend_from_slice(request.body_len_string().as_bytes());
    wire.extend_from_slice(b"\r\n");
}
```

The 1 affected envoy-http1 unit test in `crates/envoy-http1/src/client.rs::tests` (the test that currently asserts `s.contains("content-length: 0\r\n")` per the backup-branch diff) flips its assertion to `!s.contains("content-length: 0\r\n")` with a doc-comment cross-referencing ADR-0025.

`tests/fixtures/0008-http1-router-upstream/expectations.yaml` updates the `expected_body` line from:

```
"method: GET\npath: /\nheaders:\n  content-length: 0\n  host: envoy-rust.test\nbody: \n"
```

to:

```
"method: GET\npath: /\nheaders:\n  host: envoy-rust.test\nbody: \n"
```

(removing the `  content-length: 0\n` line from the expected echo body; the http1-echo-server's deterministic echo lists request headers alphabetically, and removing the spurious header from the request makes the body a 1-line shorter alphabetic list).

**Lands ADR-0025** at this task — see §7. ADR projection: behaviour change in envoy-http1::Client for RFC 7230 §3.3.2 compliance and Envoy v1.33 parity; bounded to empty-body requests; explicitly preserves the existing behaviour for non-empty-body requests + for requests that explicitly carry a Content-Length.

LoC estimate: ~10 LoC core + ~5 LoC unit test flip + ~1 LoC fixture expectations.yaml + ~13 LoC ADR-0025 = ~30 LoC total.

### D6 — STRICT_DNS settle time 500ms → 2000ms for `host_gateway = true` fixtures

`tests/differential/src/upstream.rs` (around line ~88 of HEAD `1d05cd0`, per the backup-branch diff at `tests/differential/src/upstream.rs:85-88`) currently sleeps a flat `Duration::from_millis(500)` after the upstream Envoy container's "starting main dispatch loop" log line, before reading the host-mapped port and returning the `UpstreamProxy`. For host-gateway fixtures (those that set `host_gateway = true` to opt into the `with_host("host.docker.internal", Host::HostGateway)` testcontainers config), DNS resolution to `host.docker.internal` may not have completed by the 500 ms mark — the first test probe races the resolution and triggers the 503 race condition.

Conditional bump:

```rust
let settle_ms = if host_gateway { 2000 } else { 500 };
tokio::time::sleep(Duration::from_millis(settle_ms)).await;
```

The 3 unaffected fixtures (0001, 0002, 0007) do not set `host_gateway = true` — verified at PLAN-write time via `grep -n "host_gateway" tests/differential/tests/`; the 5 affected fixtures all set it (per ADR-0015's host-gateway grant). The 2000 ms ceiling is the backup-branch's empirical choice (locally verified green at commit `9279895`); the planner may tighten it at PLAN-write time (e.g., 1000 ms) if local testing supports the lower bound — recommended posture is to keep 2000 ms as the safe upper bound and let a future hardening pass tighten if needed.

**No ADR.** This is a test-harness timing constant adjustment, not an architectural decision. The conditional shape (host-gateway-only) cleanly bounds the cost (the 3 unaffected fixtures continue at 500 ms; total CI time impact is ~1.5 s × 5 fixtures = ~7.5 s at most).

LoC estimate: ~5 LoC.

### D7 — State-4 phase-done gate verification

Materialises the §7.5 phase-done gate evidence in git history per the established phase-04.3 / phase-05.1 cadence:

- `cargo build --workspace --all-targets` clean;
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean;
- `cargo fmt --all -- --check` clean;
- `cargo test --workspace` clean (includes the differential suite locally; if local Docker is unavailable, the planner may exclude `differential` and rely on the CI matrix as authoritative — phase-05.1 PROGRESS.md Task 4 set this precedent);
- `cargo deny check` clean (no-op; no new deps);
- `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30` clean;
- **Docker-gated CI re-push: all 5 affected fixtures GREEN + all 3 unaffected fixtures GREEN, simultaneously**, with the CI run URL + the per-fixture matrix quoted inline in PROGRESS.md Task 7.

The differential surface delta: 8 of 8 Docker-gated fixtures green (vs the canonical 05.1 head's 3-of-8 green + 1-RED + 4-NOT-RUN matrix at CI run `25258722850`). PROGRESS.md Task 7's narrative explicitly cross-references the 05.1 partial-close per the carryforward chain bookkeeping.

**Substantively closes phase-04.3 REVIEW C-1 at this commit.** STATE.md's "Carryforwards" / "Notes" section bookkeeping at the 05.4 state-6 commit records: "Phase-04.3 REVIEW C-1 — closed at this commit's CI run; the 6 root-cause fixes substantively restored Docker-gated green across `0003`/`0004`/`0005`/`0006`/`0008`. The C-1 carryforward chain (originating at phase-02.2's ADR-0015 landing `435c6fa`, partially closed at the 05.1 state-6 commit) ends here."

**Lands no ADR.** D7 is a verification deliverable.

LoC estimate: 0 LoC core (PROGRESS.md narrative + STATE.md bookkeeping at state-6).

---

## 4. Non-goals (subset of parent SPEC §4 + 05.1 SPEC §4 that bind on 05.4)

The following are out of scope for 05.4 and defer to other sub-phases or later phases.

**Deferred to sub-phase 05.2:**
- **Downstream H2C HCM** (`crates/envoy-http2/` crate, `Http2Codec`, HCM-on-H2 dispatch, fixture 0009 `http2-direct-response`).
- **`CodecType::HTTP2` accept-validation flip.** At 05.4 close, the existing `CodecType::HTTP2` reject-path landed in 04.1 continues unchanged.
- **`h2spec` ≥95% conformance gate.**
- **`Http2ProtocolOptions` listener-side schema.**

**Deferred to sub-phase 05.3:**
- **Upstream H2C origination, Router H2-arm, http2-echo-server helper, fixture 0010, parent ROADMAP row `05` flip to `done`.**

**Deferred to later phases (per parent-05 SPEC §4 + 05.1 SPEC §4 — items relevant to the cluster/DNS surface):**

- **`LOGICAL_DNS` cluster type.** 05.1 shipped only `STRICT_DNS`; 05.4 does not extend.
- **`dns_refresh_rate` / periodic DNS re-resolution for `STRICT_DNS` clusters.** 05.4 does not introduce.
- **`respect_dns_ttl` knob.** Defers with `dns_refresh_rate`.
- **`dns_resolvers` array (alternative resolver pool).** Continues to be rejected at parse time (serde `deny_unknown_fields`); 05.4 does not extend.
- **`trust-dns-resolver` / `hickory-resolver` / async-DNS-resolver alternatives.** D-3.2's permitted-foundations posture is unchanged in 05.4.

**Deferred to whichever phase first needs it (05.4 explicit non-extensions):**

- **Listener filters beyond `tls_inspector`.** 05.4's parse-and-ignore field accepts any listener-filter typed_config payload as an opaque `serde_yaml::Value`; envoy-rust does not interpret any of them. The `original_dst`, `original_src`, `proxy_protocol`, and `http_inspector` listener filters all parse cleanly through the same field at PLAN-write time but envoy-rust does not execute them. Whichever phase first needs to execute a listener filter lands a typed-variant extension on the field plus a runtime dispatch arm. ADR-0026 explicitly bounds the parse-and-ignore pattern; future typed extensions are out of scope for 05.4.
- **Listener filter execution at runtime.** envoy-rust's SNI dispatch lives at the rustls layer (phase 03.2 architectural choice). The 05.4 schema growth is purely for `envoy.yaml` parseability under any test path; no runtime change.
- **Request-side header allow-list extension for `content-length` parity.** Fix 5 brings envoy-rust into RFC 7230 §3.3.2 compliance for empty-body requests; it does NOT extend the response-side `HEADER_ALLOW_LIST`. The differential harness's request-side parity check is implicit (the helper echoes received headers; symmetric request-side request-line/header-set comparisons happen via the response body byte-equivalence). Whichever later phase first explicitly compares request-side headers across proxies may need to revisit.
- **`V6Only` and `Auto` runtime semantics in envoy-rust.** 05.4 lands the `DnsLookupFamily { V4Only, V6Only, Auto }` enum for parser surface; envoy-rust's `tokio::net::lookup_host` (the existing 05.1-landed STRICT_DNS resolution path) returns whatever the system stack delivers and does NOT filter by family. If a future fixture sets `dns_lookup_family: V6Only` or `dns_lookup_family: Auto` and envoy-rust observably misbehaves vs Envoy, that's a follow-up; 05.4 does not pre-emptively wire it. ADR-0024 bounds the runtime non-implementation explicitly.

**Not deferred — confirmed in scope for 05.4** (for clarity, since these have predictable confusion points):

- `tests/fixtures/0009/` and `tests/fixtures/0010/` are NOT created in 05.4 (they land in 05.2 and 05.3 respectively).
- `BEHAVIOR_CONTRACT.md` is NOT edited (per §2 above).
- `crates/envoy-http2/` is NOT created (lands in 05.2).
- The `h2 = "0.4"` Cargo dep is NOT added (lands in 05.2).
- `Cluster.dns_lookup_family` is parsed-and-stored on envoy-rust's typed Cluster struct but **NOT consumed at runtime** (the existing 05.1-landed `tokio::net::lookup_host` resolution path is unchanged; envoy-rust does not filter resolved addresses by family). The field is consumed only on the upstream Envoy side via the per-fixture `envoy.yaml` D2 edit. ADR-0024 documents this.
- `Listener.listener_filters` is parsed-and-stored on envoy-rust's typed Listener struct but **NOT executed at runtime** (envoy-rust performs SNI dispatch at the rustls layer per phase 03.2's design). The field is consumed only on the upstream Envoy side via the D3 fixture YAML edit. ADR-0026 documents this.

---

## 5. Cross-sub-phase architectural rules inherited from parent SPEC §3

Same 7 rules as 05.1 SPEC §5; all 7 remain no-ops for 05.4's actual surface. They become load-bearing at 05.2 / 05.3 when the H2 codec attaches.

1. **`envoy-http2` is the SOLE workspace dep on `h2`.** Trivially satisfied — 05.4 doesn't add `h2` or any new crate.
2. **HCM-on-H2 reuses 04.x's `HCMConfig` and route-walk wholesale.** Trivially satisfied — 05.4 makes no HCM changes.
3. **`:authority` → `Host:` mapping at the H2-to-envoy-Request boundary.** Trivially satisfied — no request-translation changes.
4. **H2-forbidden hop-by-hop headers stripped at codec edges.** Trivially satisfied — no codec edges introduced.
5. **No H2-specific edits to `envoy-config`'s `RouteConfiguration` or `HeaderMatcher` schemas.** 05.4's envoy-config edits are `Cluster.dns_lookup_family` + `Listener.listener_filters` — orthogonal.
6. **`codec_type: AUTO` continues to behave as `HTTP1`-only.** No `CodecType` changes.
7. **`http` crate is permitted as a transitive surface only.** No `http::*` imports.

---

## 6. Implementation signposts for the planner

Notes flagging predictable planner questions so the 05.4 planner resolves them in-plan rather than mid-execution. Inherits parent-05 SPEC §6 + 05.1 SPEC §6 signposts where they bind on 05.4, plus 05.4-local signposts.

**Inherited signposts:**

1. **PLAN.md cadence — standalone pre-Task-1 commit.** Per parent-05 SPEC §6 signpost 16 + 05.1 SPEC §6 signpost 2, each sub-phase's planner commits PLAN.md cleanly at state-2 close-out, before any Task 1 commit. Precedent: phase-04.3's `c02eea7`, phase-05.1's `f23d08f`. **The 05.4 PLAN.md is committed standalone.**

2. **Cargo.lock cadence — 05.4 is a no-op for new top-level deps.** No new Cargo deps; Cargo.lock should diff empty at state-4. The state-4 phase-done verification commit either includes a single-line Cargo.lock diff or none at all (recommended: none); planner records the choice at PLAN.md writeup.

3. **ADR ledger projection — ADR-0024 / ADR-0025 / ADR-0026 land in this sub-phase.** The DECISIONS.md ledger head before this commit is **ADR-0023** (landed at 05.1 Task 1 commit `bfabcb6`). 05.4 lands three new ADRs at the first task that touches their typed surface: ADR-0024 at Task 1 (D1, DnsLookupFamily); ADR-0026 at Task 3 (D3, listener_filters); ADR-0025 at Task 5 (D5, content-length suppression). The numeric order (0024 → 0025 → 0026) does NOT match task order (1 → 3 → 5); the ADR numbers reflect the doctrine "next-sequential at landing time" not "ordered by task number". See §7.

**05.4-local signposts:**

4. **OPEN: which test path actually parses `envoy.yaml` through envoy-config?** Fix 3 of the backup branch (D1 here) and Fix 6 (D3 here) were both motivated by `deny_unknown_fields`-rejection of new envoy.yaml fields. But envoy-rust's binary parses ONLY `envoy-rust.yaml` (verified via `grep -rn "parse_bootstrap" crates/envoy-bin/src/` returning a single hit at `crates/envoy-bin/src/main.rs:52`); the differential harness does NOT parse envoy.yaml through envoy-config (verified via `grep -rn "parse_bootstrap\|envoy_config::" tests/differential/src/` returning empty). The most likely consumer is the envoy-config fuzz target's corpus walk (`crates/envoy-config/src/bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` enumerates the in-tree corpus and parses each seed; if the planner adds a fuzz seed exercising `dns_lookup_family` or `listener_filters`, the schema growth is required). The other plausible consumer is a unit test in envoy-config that wants to assert envoy.yaml parses too. **Planner action:** at PLAN-write time, grep for `envoy.yaml` references across `crates/envoy-config/` and `tests/`; if no consumer exists, the schema additions are still defensive (zero downside; the field is parsed-and-stored at zero cost) but PROGRESS.md Task 1 / Task 3 should explicitly disclose the "no current consumer" status. ADR-0024 and ADR-0026 should both name this open question and decide on the parse-and-ignore posture as the right defensive default.

5. **`tokio::net::lookup_host` is unchanged in 05.4.** The 05.1-landed STRICT_DNS resolution path is consumed as-is. Fix 2's `dns_lookup_family: V4_ONLY` is purely a knob on the upstream Envoy side; envoy-rust's resolver does NOT filter resolved addresses by family. ADR-0024 documents this.

6. **`#![forbid(unsafe_code)]` is unchanged on every workspace crate.** D-3.8 carries forward; no `unsafe` introduced in 05.4. None of the 6 fixes touches an unsafe-bearing surface.

7. **`anyhow` boundary unchanged.** envoy-cluster returns `ClusterError` (typed); envoy-config returns `ConfigError` (typed); envoy-http1 returns `Http1Error` (typed); only envoy-bin uses `anyhow` (per D-3.2). 05.4's new typed surfaces (DnsLookupFamily enum, listener_filters field, empty-body-CL suppression) all flow through existing typed-error chains.

8. **No `BEHAVIOR_CONTRACT.md` edits.** Confirmed in §2 above.

9. **ADR landing order is by task, not by number.** ADR-0024 lands at Task 1 (D1) per the inline-at-Task-1 precedent; ADR-0026 lands at Task 3 (D3); ADR-0025 lands at Task 5 (D5). The DECISIONS.md ledger after 05.4 reads `... ADR-0023 (05.1) | ADR-0024 (05.4 Task 1) | ADR-0025 (05.4 Task 5) | ADR-0026 (05.4 Task 3) | ...`. This ordering is fine per the append-only ledger discipline (ADRs are listed in landing-time order, not in any other order).

10. **Backup-branch patches are the reference implementation.** All 6 root-cause fixes were locally verified green on `backup/task4-scope-creep-2026-05-02` (commit `9279895`, "340 passed, 0 failed, 1 ignored; all 8 Docker-gated fixtures pass"). The planner reviews each patch at PLAN-write time and adopts verbatim where the diff is mechanically clean; reshapes only where the new SPEC's discipline (e.g., test-naming convention, ADR cross-reference text, deviation-from-PLAN narration) requires it. **The patches are NOT cherry-picked from the backup branch into the 05.4 execution branch** — they are re-applied per task as part of the normal TDD discipline (test first, impl second, per `superpowers:test-driven-development` in D-3.1). The backup branch is preserved as a diagnostic reference, not as a merge source.

11. **Fixture 0008's `expectations.yaml` change is mechanically coupled to D5.** Removing `content-length: 0` from the expected echo body must land in the same task as the envoy-http1 client change (Task 5). Splitting them would leave one of the two intermediate states red — either the test asserts a body with the spurious header that the client no longer emits, or vice versa.

12. **Fixture 0006's `envoy.yaml` listener_filters block is mechanically coupled to D3.** Adding the explicit `tls_inspector` block must land in the same task as the Listener.listener_filters schema growth (Task 3). Splitting them would either red the parser (block exists; field doesn't) or red the upstream Envoy on macOS Docker (field exists; block missing).

13. **Local Docker availability at state-4.** `cargo test --workspace` at the state-4 gate may exclude the differential suite if local Docker is unavailable on the executor's machine; the Docker-gated suite IS authoritative via CI (per phase-05.1's precedent at PROGRESS.md Task 4). The state-4 PROGRESS.md narrative MUST quote the CI run URL + per-fixture matrix verbatim. If the executor has local Docker and runs the full suite, that's a bonus — but the CI run is the gate.

14. **State-4 verification commit cadence** mirrors phase-05.1 Task 4's `b7fe910` shape: a dedicated `phase 05.4: state-4 phase-done gate verification (task 7)` commit that touches PROGRESS.md only (the verification narrative + the per-fixture CI matrix). Substantive code changes land in Tasks 1–6; Task 7 is verification-only.

15. **Per-fixture commit cadence vs. one bundled commit for D2.** As 05.1 SPEC §6 signpost 8 noted, the recommendation is **one bundled commit** for the 5-fixture D2 edit. 04.3's per-fixture cadence applied to landing new fixtures, not to editing existing ones. The planner records the cadence choice at PLAN.md writeup. **Recommended: single bundled commit** matching 05.1 Task 3's posture.

16. **Settle-time tightening (D6) — recommended to NOT tighten in 05.4.** The backup-branch's 2000ms is empirically green; tightening to e.g. 1000ms would require additional CI runs to validate and risks reintroducing flake. Recommended: ship 2000ms in 05.4; defer tightening to a future hardening pass.

17. **STATE.md "Carryforwards" / "Notes" section bookkeeping at 05.4 state-6.** The bookkeeping records:
    - "Phase-04.3 REVIEW C-1 — closed at this commit's CI run; the 6 root-cause fixes (helper bind 0.0.0.0; dns_lookup_family V4_ONLY; envoy-config DnsLookupFamily schema; settle-time bump; envoy-http1 CL: 0 suppression; tls_inspector listener filter) substantively restored Docker-gated green across `0003`/`0004`/`0005`/`0006`/`0008`. The C-1 carryforward chain (originating at phase-02.2's ADR-0015 landing `435c6fa`, latent across phases 02.2 → 03.1 → 03.2 → 04.1 → 04.2 → 04.3, partially closed at the 05.1 state-6 commit) ends here."
    - "Phase-04.1 REVIEW M-claim (drive_http1 per-function unit test) — substantively unblocked by the 05.4 fix's restoration of fixture 0008's end-to-end exercise, but stays deferred per the 04.3 disposition. Carryforward chain continues."
    - No new I3-style or A-style closures expected at 05.4.
    - Active phase advances from `05.4-fixture-hardening-followup` to `05.2-http2-downstream`; lifecycle state advances to phase 05.2 state 2; next-skill `superpowers:writing-plans` scoped to sub-phase 05.2.

---

## 7. ADRs expected from this sub-phase

**Three ADRs land during 05.4 execution**, appended to `docs/envoy-rust/DECISIONS.md` at the task that first touches their typed surface. Mirrors phase 04.2 Task 1's ADR-0021 inline-landing pattern (`984aedd`) and phase 05.1 Task 1's ADR-0023 inline-landing pattern (`bfabcb6`).

The DECISIONS.md ledger head before this sub-phase is **ADR-0023**; 05.4 lands at the next-sequential numbers with no renumbering needed (no inter-ADR landings between 05.1's state-6 commit and 05.4's Task-1 commit).

### ADR-0024 — `Cluster.dns_lookup_family` field + `DnsLookupFamily` enum (parse-only)

- **Date:** 2026-05-02 (or whatever date 05.4 Task 1 lands).
- **Status:** accepted.
- **Context:** Phase 05.1's STRICT_DNS schema landing exposed a cross-platform regression on macOS Docker: Envoy v1.33's `STRICT_DNS` cluster default `dns_lookup_family: AUTO` prefers AAAA records; macOS Docker resolves `host.docker.internal` to an IPv6 address; the fixture helper backends bind on IPv4 only (per D4 in 05.4 / Fix 1 of the backup branch) and the upstream Envoy's connect to the resolved IPv6 endpoint fails with `Connection refused` → 503 to the client. Fixing this requires forcing Envoy to resolve V4_ONLY via the cluster-level `dns_lookup_family` knob; making `envoy-config`'s parser accept the new field on the existing `Cluster` struct requires extending the schema.
- **Options considered:**
  - **(i) Add `dns_lookup_family: Option<DnsLookupFamily>` parse-only field with a 3-variant enum (V4Only / V6Only / Auto).** Schema growth is ~15 LoC; runtime is unchanged (envoy-rust's `tokio::net::lookup_host` returns the system-stack default; the field is parsed-and-stored at zero cost). **Chosen.**
  - **(ii) Add the field as a typed runtime knob and filter `lookup_host` results by family.** Rejected: scope inflation. envoy-rust's runtime is consuming a literal IP at the substituted `127.0.0.1:port` site (envoy-rust.yaml is unchanged in 05.4); the family filter has no observable runtime effect on envoy-rust. Adding it would land code with no test that exercises it.
  - **(iii) Add only the V4_Only variant; reject V6_Only and Auto at parse time.** Rejected: brittle. Envoy's proto enum has 3 variants for v1.33 (per `ENVOY_TARGET.md` pin); accepting only one would force any future fixture using V6_Only or Auto to fail-parse with a doctrine-correct reason but no clear remediation. Better to accept the full v1.33 surface upfront.
  - **(iv) Defer the schema growth; rely on field-set divergence (envoy.yaml has the field; envoy-rust.yaml does not).** Rejected: signpost 4 above documents that some test path may need to parse envoy.yaml through envoy-config (likely the fuzz corpus walk if a planner adds a seed). Defensive parse acceptance is the right posture.
- **Decision:** Extend `crates/envoy-config/src/bootstrap.rs::Cluster` with `pub dns_lookup_family: Option<DnsLookupFamily>` field (defaults to `None` via `#[serde(default)]`). Add `pub enum DnsLookupFamily { V4Only, V6Only, Auto }` with `#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]`. Re-export from `crates/envoy-config/src/lib.rs`. **The field is parsed-and-stored on envoy-rust's typed Cluster struct but NOT consumed at runtime** in 05.4; the existing 05.1-landed `tokio::net::lookup_host` resolution path is unchanged. The runtime non-consumption is a deliberate scope-cap matching the C-1 fix's actual need: only the upstream Envoy side observes the V4_ONLY knob (via the per-fixture envoy.yaml D2 edit).
- **Rationale:** `dns_lookup_family` is required for upstream Envoy v1.33 on macOS Docker to bypass the AAAA/A selection regression. envoy-rust's typed parser must accept the field for symmetry with Envoy's proto and for potential future test paths that parse envoy.yaml through envoy-config. The runtime non-consumption preserves D-3.6 minimalism (no code with no test exercises it); whichever later phase first needs envoy-rust to filter resolved addresses by family lands the runtime extension then, with its own test.
- **Consequences:**
  - `crates/envoy-config/src/bootstrap.rs::Cluster` gains the `dns_lookup_family: Option<DnsLookupFamily>` field (~5 LoC).
  - `crates/envoy-config/src/bootstrap.rs` gains the `DnsLookupFamily` enum (~10 LoC).
  - `crates/envoy-config/src/lib.rs` re-exports `DnsLookupFamily` from the public API (~1 LoC).
  - 1 new `envoy-config` parse test exercising V4_ONLY (~25 LoC).
  - 2 hand-written `Cluster` initialiser updates in `crates/envoy-cluster/src/cluster.rs::tests` (`dns_lookup_family: None`; ~2 LoC).
  - **D2 (5-fixture envoy.yaml `dns_lookup_family: V4_ONLY` edit) becomes safe to land** — the parser accepts the new field across any test path.
  - **Phase-04.3 REVIEW C-1's IPv6/IPv4 selection regression closes** at D7 (substantively, alongside the other 5 fixes).
  - V6Only and Auto runtime semantics in envoy-rust are explicitly NOT implemented in 05.4; future phase that needs them lands the runtime extension.
  - Fuzz corpus growth deferred to PLAN-discretion (signpost 4 above; not required by the gate).
- **Provenance:** This ADR was conditionally projected as ADR-0024 in 05.1 STATE.md ("the C-1 follow-up sub-phase brainstorm has first priority on this number; if the follow-up does not land an ADR, ADR-0024 stays available for 05.2 Task 1"); the 05.4 brainstorm exercises the priority. The DECISIONS.md ledger head before this commit is ADR-0023 (landed at 05.1 Task 1 `bfabcb6`); ADR-0024 lands at the next-sequential number with no renumbering needed.

### ADR-0025 — Suppress `content-length: 0` on empty-body GET in `envoy-http1::client` (RFC 7230 §3.3.2 + Envoy v1.33 parity)

- **Date:** 2026-05-02 (or whatever date 05.4 Task 5 lands).
- **Status:** accepted.
- **Context:** envoy-http1's client at HEAD `1d05cd0` injects a synthetic `content-length: <len>` header on every outbound request that doesn't already carry an explicit Content-Length. For empty-body requests (e.g., the HTTP/1.1 GET that fixture 0008's `Driver::Http1` issues), this emits `content-length: 0` on the wire. Envoy v1.33 honors RFC 7230 §3.3.2 ("A user agent SHOULD NOT send a Content-Length header field when the request message does not contain a payload body and the method semantics do not anticipate such a body") and OMITS Content-Length on empty-body requests. Fixture 0008's deterministic-echo body shape is a byte-for-byte alphabetic list of received headers + the body bytes; the spurious envoy-rust-side `content-length: 0` lands in the echoed body and breaks `response_body: byte_exact` against the upstream Envoy side that omits it.
- **Options considered:**
  - **(i) Suppress synthetic `content-length: 0` on empty-body requests; pass through explicit Content-Length unchanged.** Behaviour change: only inject when body is non-empty AND no explicit CL is set. **Chosen.**
  - **(ii) Always emit `content-length: <len>` (status quo).** Rejected: violates RFC 7230 §3.3.2; breaks fixture 0008 differential equivalence.
  - **(iii) Always emit `content-length: 0` for empty-body GET; update upstream Envoy fixture to inject `content-length: 0` on its side too via `request_headers_to_add`.** Rejected: increases envoy.yaml-side asymmetry burden; is the wrong direction (fixture YAML bending around envoy-rust's misbehaviour rather than fixing envoy-rust); doesn't honor the RFC.
  - **(iv) Make the suppression a configurable HCM/Router knob.** Rejected: scope inflation. RFC compliance is not a per-request opt-in; it's the correct default. If a future use case wants explicit CL: 0 (e.g., to comply with a specific upstream's quirks), it can pass an explicit Content-Length on the request, which the new code correctly passes through unchanged.
- **Decision:** Modify `crates/envoy-http1/src/client.rs::ClientStream` request-write path: only inject the synthetic `content-length: <len>` header when (a) the request does not carry an explicit Content-Length AND (b) the request body is non-empty. Pseudocode:
  ```rust
  let request_has_cl = request.headers.iter().any(|(n, _)| n.eq_ignore_ascii_case(hdr::CONTENT_LENGTH));
  let body_is_nonempty = request.body_bytes().is_some_and(|b| !b.is_empty());
  if !request_has_cl && body_is_nonempty {
      wire.extend_from_slice(b"content-length: ");
      wire.extend_from_slice(request.body_len_string().as_bytes());
      wire.extend_from_slice(b"\r\n");
  }
  ```
  The 1 affected envoy-http1 unit test in `crates/envoy-http1/src/client.rs::tests` flips its assertion from `s.contains("content-length: 0\r\n")` to `!s.contains("content-length: 0\r\n")`. Fixture 0008's `expectations.yaml` removes `  content-length: 0\n` from the expected echo body.
- **Rationale:** RFC 7230 §3.3.2 is unambiguous: empty-body requests SHOULD NOT carry Content-Length. Envoy v1.33 honors this. Fixture 0008's differential property is "envoy ↔ envoy-rust byte-equal echo body" — both proxies must omit the header for the fixture to be green. The fix is small (~10 LoC) and correctly bounded to empty-body requests: requests with explicit Content-Length pass through unchanged (preserving any caller's deliberate Content-Length emission); requests with non-empty body continue to emit synthetic Content-Length (preserving the existing happy path).
- **Consequences:**
  - `crates/envoy-http1/src/client.rs` request-write path gains a `body_is_nonempty` check (~5 LoC).
  - 1 envoy-http1 unit test flips its CL: 0 assertion (~5 LoC).
  - `tests/fixtures/0008-http1-router-upstream/expectations.yaml` `expected_body` line drops `  content-length: 0\n` (~1 LoC YAML).
  - **Fixture 0008 `response_body: byte_exact` differential equivalence holds** — substantively closes one of phase-04.3 REVIEW C-1's three latent regressions (the other two being the IPv4/IPv6 selection issue addressed by ADR-0024 and the listener-filter issue addressed by ADR-0026).
  - envoy-rust now matches Envoy v1.33's request-side header emission set on empty-body requests.
  - Future requests with non-empty body continue to receive synthetic Content-Length — no regression.
- **Provenance:** This ADR was conditionally projected as ADR-0024 or ADR-0025 in 05.1 STATE.md (the C-1 follow-up's projected ADRs start at ADR-0024 onward). ADR-0024 is taken by the DnsLookupFamily schema (lands at 05.4 Task 1); this ADR lands at 05.4 Task 5 alongside the envoy-http1 client behaviour change. ADR-0026 (listener_filters parse-and-ignore) lands at 05.4 Task 3 numerically before this one but the landing-time order in DECISIONS.md is by task-execution order: ADR-0024 (Task 1) → ADR-0026 (Task 3) → ADR-0025 (Task 5). The ledger remains append-only with no renumbering.

### ADR-0026 — `Listener.listener_filters` parse-and-ignore field in `envoy-config` (new pattern)

- **Date:** 2026-05-02 (or whatever date 05.4 Task 3 lands).
- **Status:** accepted.
- **Context:** Phase 05.1 Task 4's CI run revealed that fixture 0006's TLS-SNI test was masked behind fixture 0008's earlier failure (alphabetic ordering); after the other fixes land, fixture 0006 surfaces as RED on macOS Docker because Envoy v1.33 does NOT auto-inject the TLS inspector listener filter for SNI-based filter chain selection (the auto-injection works on Linux but not on the Docker-Desktop/macOS combination). The fix on the upstream Envoy side is to declare the listener filter explicitly in `envoy.yaml`: `listener_filters: [{name: envoy.filters.listener.tls_inspector, ...}]`. envoy-rust performs SNI dispatch at the rustls layer (per phase 03.2's design) and does NOT execute listener filters; the field has no envoy-rust runtime semantics. However, with `envoy-config`'s parser using `#[serde(deny_unknown_fields)]` on every struct including `Listener`, any test path that parses fixture 0006's envoy.yaml through envoy-config would fail-reject the new field — and even though no current test path does so (signpost 4 above documents this open question), defensive acceptance is doctrinally cleaner than perpetual field-set divergence.
- **Options considered:**
  - **(i) Add `Listener.listener_filters: Vec<serde_yaml::Value>` parse-and-ignore field with `#[serde(default)]`.** Stores listener-filter blocks as opaque `serde_yaml::Value`; envoy-rust never inspects or executes them. **Chosen.**
  - **(ii) Add `Listener.listener_filters: Vec<ListenerFilter>` typed-and-ignored field with a typed `ListenerFilter` enum exhausting the v1.33 set (`tls_inspector`, `original_dst`, `original_src`, `proxy_protocol`, `http_inspector`).** Rejected: scope inflation. The typed enum would need ~5 variants × ~10 LoC each + per-variant typed_config payloads + 5+ parse tests; envoy-rust gains zero runtime semantics from typing them; only one variant is needed for fixture 0006's actual fix.
  - **(iii) Defer the schema growth; rely on field-set divergence (envoy.yaml has listener_filters; envoy-rust.yaml does not; no test path parses envoy.yaml through envoy-config).** Rejected: brittle. Signpost 4 documents that the open question of "which test path parses envoy.yaml through envoy-config" may be answered YES by a future planner who adds an envoy.yaml-parsing test or fuzz seed. Defensive acceptance is the right default.
  - **(iv) Add the field as a strict `#[serde(skip)]` ignored-at-deserialization field.** Rejected: this would skip the field at deserialization time entirely (the `Vec<serde_yaml::Value>` would always be empty), losing the ability for any future test or audit to introspect the parsed listener filters. Storing them as `Vec<serde_yaml::Value>` preserves the ability to inspect (e.g., a test could assert "fixture 0006 declares the tls_inspector filter" without typing the inspector itself).
- **Decision:** Extend `crates/envoy-config/src/bootstrap.rs::Listener` with `pub listener_filters: Vec<serde_yaml::Value>` field (defaults to `vec![]` via `#[serde(default)]`). The field is parsed-and-stored as opaque YAML values; envoy-rust does NOT interpret or execute them at runtime. Add a parse test (`parses_listener_with_tls_inspector_listener_filter`) exercising the full bootstrap with a TLS-bearing listener carrying the tls_inspector block. Add `listener_filters: vec![]` to the one hand-written `Listener` initialiser in `crates/envoy-tls/src/tests.rs::synth_listener_two_tls_chains`. Add the explicit `tls_inspector` block to fixture 0006's `envoy.yaml` (only — `envoy-rust.yaml` is unchanged because envoy-rust's SNI dispatch lives at the rustls layer).
- **Rationale:** This is the **introduction of a new pattern in envoy-config**: parse-and-ignore for fields that envoy-rust cannot or will not consume at runtime but that upstream Envoy requires for fixture validity. Every prior YAML divergence used field-set divergence (the field exists in envoy.yaml and is absent from envoy-rust.yaml). The parse-and-ignore pattern is the right call for `listener_filters` specifically because: (a) the field carries arbitrary listener-filter typed_config payloads (multiple filter types possible; future Envoy versions may surface more); typing the variants exhaustively would be a non-trivial growth surface; (b) envoy-rust never executes listener filters by design (architectural choice from phase 03.2 — SNI lives in the rustls layer); (c) making the parse-and-ignore explicit at the schema level is more honest than maintaining field-set divergence forever, and prepares for any future test path that parses envoy.yaml through envoy-config.
- **Consequences:**
  - `crates/envoy-config/src/bootstrap.rs::Listener` gains the `listener_filters: Vec<serde_yaml::Value>` field (~5 LoC).
  - 1 new `envoy-config` parse test exercising the tls_inspector block (~85 LoC including the YAML payload).
  - 1 hand-written `Listener` initialiser in `crates/envoy-tls/src/tests.rs` updated (`listener_filters: vec![]`; ~1 LoC).
  - `tests/fixtures/0006-tls-sni/envoy.yaml` gains the explicit listener_filters block (~9 LoC YAML).
  - **Fixture 0006's TLS-SNI handshake succeeds** on macOS Docker — substantively closes one of phase-04.3 REVIEW C-1's three latent regressions.
  - **The parse-and-ignore pattern is now a documented envoy-config posture.** Future fields that meet the criteria (Envoy-config-only with no envoy-rust runtime semantics; required for upstream-Envoy `envoy.yaml` parseability under any test path; reviewed under D-3.5 ambiguity-resolution discipline) may follow the same pattern. Whichever later phase first needs to ACTUALLY EXECUTE a listener filter lands a typed-variant extension on the field plus a runtime dispatch arm — not a new ADR (extending an existing pattern).
- **Provenance:** This ADR was conditionally projected as ADR-0024–0026 in 05.1 STATE.md (the C-1 follow-up's projected ADRs start at ADR-0024 onward). ADR-0024 (DnsLookupFamily) lands at 05.4 Task 1; this ADR lands at 05.4 Task 3; ADR-0025 (CL: 0 suppression) lands at 05.4 Task 5. The DECISIONS.md ledger is append-only and lists ADRs in landing-time order: ADR-0023 → ADR-0024 → ADR-0026 → ADR-0025.

**No conditional ADRs anticipated for 05.4 beyond the three above.** The 6 root-cause fixes are mechanically scoped per the backup-branch reference; no Y/N decision points are projected at execution time. Possible additional ADRs land only if execution proves they're needed (per D-3.5 ambiguity-resolution discipline) — none anticipated.

If a Y/N decision surfaces during execution that isn't covered by the three projected ADRs (e.g., an unanticipated `cargo deny check` flip on a transitive license, or a settle-time empirical-bound decision that warrants record), the planner appends the next-sequential ADR (likely ADR-0027) at the time it lands.

---

## 8. Artifacts this sub-phase produces

Created during execution (relative to repo root):

- `docs/envoy-rust/phases/05.4-fixture-hardening-followup/PLAN.md` (lands at standalone pre-Task-1 commit per §6 signpost 1).
- `docs/envoy-rust/phases/05.4-fixture-hardening-followup/PROGRESS.md` (per-task progress notes; Task 7 carries the verification narrative + the per-fixture CI matrix).
- `docs/envoy-rust/phases/05.4-fixture-hardening-followup/REVIEW.md` (state-5 review).

Amended during execution:

- `crates/envoy-config/src/bootstrap.rs` — extend `Cluster` with `dns_lookup_family: Option<DnsLookupFamily>`; add `DnsLookupFamily` enum; extend `Listener` with `listener_filters: Vec<serde_yaml::Value>`; add ~2 new validator unit tests (`parses_cluster_with_dns_lookup_family_v4_only`; `parses_listener_with_tls_inspector_listener_filter`).
- `crates/envoy-config/src/lib.rs` — re-export `DnsLookupFamily` from the public API.
- `crates/envoy-cluster/src/cluster.rs` — add `dns_lookup_family: None` to 2 hand-written `Cluster` test initialisers; no other changes (the runtime resolution path is unchanged from 05.1).
- `crates/envoy-tls/src/tests.rs` — add `listener_filters: vec![]` to the 1 hand-written `Listener` initialiser in `synth_listener_two_tls_chains`.
- `crates/envoy-http1/src/client.rs` — modify the request-write path to suppress synthetic `content-length: 0` on empty-body requests; update the 1 affected unit test's CL assertion.
- `tests/helpers/tcp-echo-server/src/main.rs` — bind 0.0.0.0 instead of 127.0.0.1; update tracing log line + drop "localhost-only" doc-comment language.
- `tests/helpers/tls-echo-server/src/main.rs` — same bind flip.
- `tests/helpers/http1-echo-server/src/main.rs` — same bind flip.
- `tests/differential/src/upstream.rs` — conditional settle-time bump (500ms → 2000ms for `host_gateway = true` fixtures only).
- `tests/fixtures/0003-tcp-proxy/envoy.yaml` — add `dns_lookup_family: V4_ONLY` after `type: STRICT_DNS`.
- `tests/fixtures/0004-tls-downstream/envoy.yaml` — same.
- `tests/fixtures/0005-tls-upstream/envoy.yaml` — same.
- `tests/fixtures/0006-tls-sni/envoy.yaml` — same + add the explicit `listener_filters: [tls_inspector]` block on the `tcp_listener`.
- `tests/fixtures/0008-http1-router-upstream/envoy.yaml` — same `dns_lookup_family: V4_ONLY` add.
- `tests/fixtures/0008-http1-router-upstream/expectations.yaml` — remove `  content-length: 0\n` from the expected echo body.
- `docs/envoy-rust/DECISIONS.md` — append ADR-0024 at Task 1, ADR-0026 at Task 3, ADR-0025 at Task 5.
- `docs/envoy-rust/ROADMAP.md` — at brainstorm-time (this commit): add row `05.4`; extend parent row `05`'s `sub-phases` column to include `05.4`. At state-6 commit: row `05.4` `status` flips `in-progress` → `done`. Parent row `05` stays `in-progress` (flips at 05.3's state-6 commit per the ROADMAP-schema invariant).
- `docs/envoy-rust/STATE.md`:
  - At brainstorm-time (this commit): active phase advances from "free-standing post-05.1 follow-up sub-phase under parent-05 at lifecycle state 0" to `05.4-fixture-hardening-followup` lifecycle state 2 (SPEC.md committed; PLAN.md does not exist yet). Next-skill: `superpowers:writing-plans`. Notes section gains a "Phase-05.4 brainstorm" subsection summarising the disposition + the 6-fix decomposition + the 3-ADR projection.
  - At state-6 phase-done commit: active phase advances from `05.4-fixture-hardening-followup` to `05.2-http2-downstream` lifecycle state 2; next-skill `superpowers:writing-plans` scoped to sub-phase 05.2; Notes section gains a "Phase-05.4 rollovers" subsection per §6 signpost 17.
- `Cargo.lock` — no-op at state-4 (no new top-level deps; no new transitive surface).
- `deny.toml` — no edits (no new top-level deps).

Not touched in 05.4 (belong to other phases or are frozen):

- `docs/envoy-rust/phases/05-http2/SPEC.md` (parent) — unedited; remains the design artifact committed at parent-05 state-1 SHA `cd1a70e`.
- `docs/envoy-rust/phases/05.1-fixture-hardening/*` (predecessor) — closed at the 05.1 phase-done commit `1d05cd0`; unedited in 05.4.
- `docs/envoy-rust/phases/05.2-http2-downstream/SPEC.md`, `docs/envoy-rust/phases/05.3-http2-upstream/SPEC.md` — landed at parent-05 state-2 commit `f1804a7`; unedited in 05.4 (their PLAN/PROGRESS/REVIEW land in their own sub-phase execution).
- `docs/envoy-rust/phases/{04.x, 03.x, 02.x, 01, 00}/*` — closed in earlier phases.
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — no edits in 05.4 (per §2 above).
- `crates/envoy-http2/` — does not exist at 05.4 close (lands in 05.2).
- `crates/envoy-tcp/`, `crates/envoy-listener/`, `crates/envoy-bin/` — unchanged. No upstream or downstream code paths touched.
- `crates/envoy-cluster/src/cluster.rs` — unchanged in core logic (only the 2 hand-written test initialisers gain `dns_lookup_family: None`); the runtime STRICT_DNS resolution path is unchanged from 05.1.
- `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0007-http1-direct-response/` — unedited; their fixtures must remain green at the 05.4 state-4 gate (they don't reference `host.docker.internal` per the C-1 trace; they don't carry TLS-SNI; they don't exercise the http1-router-upstream path).
- `tests/fixtures/0009-http2-direct-response/`, `tests/fixtures/0010-http2-router-upstream/` — do not exist at 05.4 close (they land in 05.2 and 05.3 respectively).
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/strict_dns_cluster.yaml` — unchanged (the seed continues to parse cleanly through the schema additions; planner may optionally extend with a `dns_lookup_family: V4_ONLY` field at PLAN discretion).
- Root `Cargo.toml` — no `[workspace] members` changes (no new crates).
- `rust-toolchain.toml`, `docs/envoy-rust/ENVOY_TARGET.md` — frozen per D-3.7 / D-3.9.
- `crates/envoy-config/fuzz/Cargo.toml`, `crates/envoy-config/fuzz/fuzz_targets/` — unchanged.
- The 6 patches on `backup/task4-scope-creep-2026-05-02` are NOT cherry-picked or merged — they are the diagnostic reference; per-task TDD discipline re-derives them.

---

## 9. Final commit message format (for state 6 of the 05.4 lifecycle)

The 05.4 phase-done commit flips ROADMAP row `05.4` `in-progress` → `done`; parent row `05` stays `in-progress` (flips at 05.3's phase-done commit). Format models the 04.x / 05.1 sub-phase shape:

```
phase 05.4: 6 root-cause fixes + Docker-gated 5-fixture green re-baseline [ADR-0024, ADR-0025, ADR-0026]

Substantively closes phase-04.3 REVIEW C-1 by landing the 6 root-cause fixes
that 05.1's STRICT_DNS preamble proved necessary but not sufficient. The
schema (ClusterType::StrictDns) + runtime (tokio::net::lookup_host) +
5-fixture YAML flip landed in 05.1 (commits bfabcb6 / f7a555d / 0ce0aa2) and
the canonical CI run 25258722850 against 05.1 head 4768fcd revealed 6
distinct latent regressions exposed by the STRICT_DNS flip. 05.4 lands them
under proper SPEC + ADR discipline; the procedural defect at 05.1's aborted
attempt (no SPEC anchor, no ADRs, blew Task 4's 0-LoC contract; preserved
on backup branch backup/task4-scope-creep-2026-05-02 commit 9279895) is
corrected here, not the technical content.

Fix 1 (D4) — 3 echo-server helpers (tcp/tls/http1) bind 0.0.0.0 instead of
127.0.0.1; Docker host-gateway cannot reach loopback. Fix 2 (D2) —
dns_lookup_family: V4_ONLY added to the 5 affected fixture envoy.yaml files
to bypass Envoy v1.33's macOS-Docker AAAA-preference under the default
AUTO. Fix 3 (D1, ADR-0024) — envoy-config Cluster.dns_lookup_family field +
DnsLookupFamily enum (V4Only/V6Only/Auto) for parser surface; runtime
non-consumption deliberate per ADR-0024. Fix 4 (D6) — STRICT_DNS settle
time 500ms → 2000ms for host_gateway = true fixtures in
tests/differential/src/upstream.rs; the 3 unaffected fixtures continue at
500ms. Fix 5 (D5, ADR-0025) — envoy-http1::client suppresses synthetic
content-length: 0 on empty-body GET (RFC 7230 §3.3.2 + Envoy v1.33 parity);
fixture 0008's expectations.yaml drops the spurious header from the
expected echo body. Fix 6 (D3, ADR-0026) — envoy-config Listener.listener_filters
parse-and-ignore field (Vec<serde_yaml::Value>; new pattern in envoy-config) +
fixture 0006 envoy.yaml gains the explicit tls_inspector block (Envoy v1.33
on macOS Docker does not auto-inject the TLS inspector for SNI-based filter
chain selection).

Closes phase-04.3 REVIEW C-1 (cross-phase Docker-gated host.docker.internal/
STATIC regression; latent across phases 02.2 → 03.1 → 03.2 → 04.1 → 04.2 →
04.3 since ADR-0015's landing at 435c6fa; partially closed at 05.1 state-6;
substantively closed here at the state-4 verification commit). Phase-04.1
REVIEW M-claim (drive_http1 per-function unit test) is unblocked by the
fixture-mask removal but stays deferred per the 04.3 disposition.

NO HTTP/2 work in 05.4. The envoy-http2 crate, h2 dep, HCM-on-H2 dispatch,
fixtures 0009/0010, and h2spec conformance gate all defer to sub-phases
05.2 and 05.3 per ADR-0022 (parent-05 split decision).

Differential surface: tests/fixtures/0001-tcp-echo green (unchanged);
  tests/fixtures/0002-static-admin-ready green (unchanged);
  tests/fixtures/0003-tcp-proxy green (RESTORED — V4_ONLY + 0.0.0.0 bind);
  tests/fixtures/0004-tls-downstream green (RESTORED — same);
  tests/fixtures/0005-tls-upstream green (RESTORED — same);
  tests/fixtures/0006-tls-sni green (RESTORED — same + tls_inspector block);
  tests/fixtures/0007-http1-direct-response green (unchanged);
  tests/fixtures/0008-http1-router-upstream green (RESTORED — same +
  content-length: 0 suppression).
Conformance: none (h2spec attaches in 05.2).
```
