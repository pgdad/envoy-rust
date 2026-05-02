# Phase 05.1 — fixture-hardening preamble: `ClusterType::StrictDns` + 5-fixture coordinated edit + phase-02.1 I3 close

- **Phase id:** `05.1`
- **Parent phase:** `05-http2` (split per **ADR-0022**; parent SPEC at `docs/envoy-rust/phases/05-http2/SPEC.md`, committed at parent-05 state-1 SHA `cd1a70e`).
- **Slug:** `05.1-fixture-hardening`
- **Title:** Close the cross-phase Docker-gated `host.docker.internal`/`type: STATIC` regression that has been latent across phases 02.2 → 03.1 → 03.2 → 04.1 → 04.2 → 04.3 by adding a second variant to `ClusterType` (`StrictDns`), extending `Cluster`'s constructor with a build-time DNS resolution branch, and coordinating a 5-fixture YAML edit (`tests/fixtures/{0003,0004,0005,0006,0008}/{envoy.yaml,envoy-rust.yaml}`). Lands **ADR-0023** at Task 1. Closes phase-02.1 REVIEW I3 (positive `ClusterType::Static` regression guard).
- **Depends on:** `04` (parent ROADMAP row `done` as of `e626862`, the 04.3 phase-done commit that also flipped parent-04 `done`); strictly precedes `05.2` (downstream H2C codec/HCM/h2spec) and `05.3` (upstream H2C client + router H2-arm + parent-05 close).
- **Differential surface when done:** **no new fixtures.** The 5 pre-existing fixtures `tests/fixtures/0003-tcp-proxy/`, `tests/fixtures/0004-tls-downstream/`, `tests/fixtures/0005-tls-upstream/`, `tests/fixtures/0006-tls-sni/`, and `tests/fixtures/0008-http1-router-upstream/` are restored to Docker-gated green by the coordinated `type: STATIC` → `type: STRICT_DNS` flip + the schema/runtime additions that make the new type accept-validate. The 3 unaffected fixtures `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, and `tests/fixtures/0007-http1-direct-response/` are not edited and remain green (they don't reference `host.docker.internal` at any cluster — fixture 0001 has no upstream cluster, fixture 0002 only exercises the admin endpoint, fixture 0007 is `direct_response`-only with no upstream).
- **Seeded by:** parent-05 SPEC §1 (the C-1 trace and the goal-paragraph for sub-phase 05.1), §3 D1.1–D4.1 (the four 05.1 deliverables), §4 (non-goals — the subset that binds on 05.1, especially the `LOGICAL_DNS` deferral and `dns_refresh_rate` deferral), §5 (3-way split decision context — the rationale for placing the fixture-hardening preamble in its own sub-phase), §6 signposts 14 (Cargo.lock cadence), 16 (PLAN.md cadence — pre-Task-1 standalone commit per `c02eea7` precedent), 17 (fixture 0010 projection note), and 21 (ADR ledger projection: ADR-0022 lands at parent-05 state-2; ADR-0023 lands at 05.1 Task 1), §7 (ADR-0023 projection text), §8 (parent-05 artifact list, scoped to 05.1's slice).

This SPEC is the design contract for sub-phase 05.1. The next session converts it into `PLAN.md` per the phase lifecycle (§5 of `BOOTSTRAP_PROMPT.md` / `SKILL_ROUTING.md`). It is self-contained per doctrine D-3.4; a stranger reading only this file plus the stable doctrine documents (`MISSION.md`, `BEHAVIOR_CONTRACT.md`, `DECISIONS.md`, `BOOTSTRAP_PROMPT.md`) and the landed phase-04 surface (via `git log` and the in-tree `envoy-config` / `envoy-cluster` shape at HEAD `e626862` — the 04.3 phase-done commit which also closed parent-04) must be able to execute it without consulting the parent `05-http2/SPEC.md`. The C-1 regression trace is reproduced inline below (§1) for that reason.

---

## 1. Goal and acceptance signal

**Goal.** Land the fixture-hardening preamble for parent phase 05 in three coordinated parts that ship in a single sub-phase:

1. **Schema growth** in `crates/envoy-config/src/bootstrap.rs`: extend the `ClusterType` enum from its current single-variant `Static` shape (verifiable at task-1 time by `git show e626862:crates/envoy-config/src/bootstrap.rs | grep -A 5 'enum ClusterType'`; at HEAD `e626862` the enum reads `pub enum ClusterType { Static }` at lines 58–62 with `#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]`) to `Static | StrictDns`. The validator's accept-path is extended to recognise `STRICT_DNS` as a permitted serde tag; a new typed error variant `ConfigError::ClusterDnsResolutionFailed { cluster: String, address: String, source: std::io::Error }` is added to the existing `ConfigError` enum for the runtime resolution failure case (§3 D1).

2. **Runtime growth** in `crates/envoy-cluster/src/cluster.rs`: extend the existing `Cluster` constructor surface with a `STRICT_DNS` resolution branch. For `Static` clusters the existing literal-IP construction path stays unchanged (regression-guarded by the new positive-`Static` test that closes phase-02.1 REVIEW I3); for `STRICT_DNS` clusters, the constructor calls `tokio::net::lookup_host(format!("{}:{}", address, port)).await` and stores the resolved `SocketAddr`s in the cluster's endpoint list (§3 D2). The DNS lookup is performed once at cluster-build time, matching Envoy v1.33's `STRICT_DNS` semantics with default `dns_refresh_rate` (periodic re-resolution is a §4 non-goal in 05.1; the simpler one-shot resolution suffices for the C-1 fix). On `lookup_host` returning zero results or erroring, the constructor returns `ConfigError::ClusterDnsResolutionFailed`.

3. **Coordinated 5-fixture edit** to `tests/fixtures/{0003,0004,0005,0006,0008}/{envoy.yaml,envoy-rust.yaml}`: flip `type: STATIC` to `type: STRICT_DNS` on every cluster whose `endpoints[*].address` is `host.docker.internal` (the `BACKEND_HOST` substitution per ADR-0015, landed at commit `435c6fa`). The flip is mechanical: 10 YAML files, ~2 lines diff each (`type: STATIC` → `type: STRICT_DNS` plus the optional `dns_lookup_family: V4_ONLY` knob if needed at planner discretion — see §6 signpost 6 below). Fixtures 0001/0002/0007 are NOT touched: 0001 has no upstream cluster, 0002 is admin-only, 0007 is `direct_response`-only. After the edit, all 5 affected Docker-gated tests pass against upstream Envoy v1.33.0 again (§3 D3).

**No HTTP/2 work in 05.1.** This sub-phase is purely a fixture-hardening preamble. The H2 codec layer, HCM-on-H2 dispatch, h2spec conformance gate, upstream H2 client, and router H2-arm all defer to sub-phases 05.2 and 05.3 per the parent-05 split decision (ADR-0022). The `envoy-http2` crate is NOT created in 05.1; the `h2 = "0.4"` dep is NOT added in 05.1; `CodecType::HTTP2` continues to reject in 05.1 (it lands accept-validation in 05.2). 05.1 introduces no new top-level Cargo deps — `tokio::net::lookup_host` is part of the existing `tokio` foundation already pulled by `envoy-cluster` (verifiable by `cargo tree -p envoy-cluster`).

**Cross-phase items closed at 05.1.** Two deferred items close at 05.1's state-6 commit:

- **Phase-04.3 REVIEW C-1** (the cross-phase Docker-gated `host.docker.internal`/`STATIC` regression). 04.3's REVIEW.md (committed at `eb030d1`) flagged this as Important cross-phase carryforward C-1 and proposed three forward-work options in the 04.3 STATE.md handoff (committed at `e626862`): (a) fold into 05 as a Task-1 preamble, (b) split into a dedicated fixture-hardening sub-phase, (c) ratify the deferral. The phase-05 brainstorm (committed at `cd1a70e`) selected option (b) implemented as sub-phase 05.1 inside parent 05 — see parent-05 SPEC §5. The substantive close-out is the 5-fixture green re-baseline at 05.1 state-4 (§3 D4).
- **Phase-02.1 REVIEW I3** (positive `ClusterType::Static` variant-name regression guard). I3 was deferred at phase-02.1 close because the single-variant `ClusterType { Static }` enum had no other variant against which to discriminate `Static` structurally; the deferral chained through phases 02.2 / 03.1 / 03.2 / 04.1 / 04.2 / 04.3 unchanged. Adding `StrictDns` in 05.1 unblocks I3: the new `Cluster::new` constructor's `match cluster_type` arm produces a structural test target (the `Static` arm is exercised by a literal-IP cluster and the `StrictDns` arm by a DNS-name cluster). The positive `Static` regression guard lands as part of D2's test suite (§3 D2 below), with a `closes phase-02.1 REVIEW I3` cross-reference in PROGRESS.md at the corresponding task.

**Cross-phase items unblocked but not closed at 05.1.** One:

- **Phase-04.1 REVIEW M-claim** (the per-function `drive_http1` unit test that was masked by the Docker-gated regression on fixtures 0003–0008). 05.1's fix unblocks the masking — the differential harness now has 5 Docker-gated runs of `drive_http1` exercising it in production again — but the M-claim's own scope (a separate per-function unit test that mocks `tokio::io::AsyncRead`/`AsyncWrite` against a known-good HTTP/1.1 byte stream and asserts `drive_http1` parses the response correctly) stays deferred per the 04.3 disposition. 05.1 does NOT extend the harness; the masking-unblock is a side effect of D3, recorded in PROGRESS.md but not consumed by any new test.

**Scope-shape inheritance from the parent-05 brainstorm.** The brainstorm explicitly bounded 05.1 to: schema growth (ClusterType extension only — NOT the cluster-side `Http2ProtocolOptions` work which lives in 05.3, NOT the listener-side HCM `codec_type: HTTP2` accept-flip which lives in 05.2), runtime growth (Cluster::new DNS resolve only — NOT the H2 codec, NOT any HCM dispatch changes), fixture edits (the 5 pre-existing fixtures only — NOT new fixtures 0009 or 0010 which land in 05.2 and 05.3 respectively), verification (the 5 fixtures' green re-baseline only — NOT h2spec attachment which is 05.2). This bounding is reproduced verbatim in §4 below as 05.1's non-goals.

**C-1 regression trace, reproduced inline for self-containment per D-3.4.** Upstream Envoy v1.33.0 rejects the rendered `address: host.docker.internal` under `type: STATIC` with this critical-log line:

```
[critical][main] [source/server/server.cc:416] error initializing config '/etc/envoy/envoy.yaml':
malformed IP address: host.docker.internal. Consider setting resolver_name or setting cluster type
to 'STRICT_DNS' or 'LOGICAL_DNS'
```

The regression originates at phase-02.2's ADR-0015 landing (`host.docker.internal` introduced as the `BACKEND_HOST` substitution for cross-container reachability via Docker's `host-gateway`; commit `435c6fa`). Subsequent phases 02.2, 03.1, 03.2, 04.1, 04.2, 04.3 did not push to CI between the phase-02.1 close (CI run `24913934580`) and the phase-04.3 task-14 differential-test push (CI run `25106213773`), so the regression went undetected for ~5 phases. Envoy v1.33's tightened `socket_address.address` parse semantics expect either a literal IP (under `STATIC`) or DNS resolution opt-in (under `STRICT_DNS`/`LOGICAL_DNS`). Envoy-rust's parser was lenient (it parsed `host.docker.internal` under `STATIC` because `socket_address.address: String` accepts any UTF-8 sequence; the runtime then panicked or errored at endpoint-construction time when `SocketAddr::from_str` failed to parse the hostname as an IP — this is the `EndpointParse` arm in `crates/envoy-cluster/src/cluster.rs::ClusterError`). Both proxies rejected the config in different ways: Envoy's rejection was at `bootstrap.cc` startup; envoy-rust's rejection was at `Cluster::from_bootstrap` startup; neither side served traffic. The fix is symmetric — flip both fixtures' `type:` to `STRICT_DNS` and resolve at startup.

**Acceptance signal** — the phase-done gate from §7.5 of `BOOTSTRAP_PROMPT.md`, scoped to 05.1's feature surface:

- (a) the 5 Docker-gated differential fixtures `tests/fixtures/0003-tcp-proxy/`, `tests/fixtures/0004-tls-downstream/`, `tests/fixtures/0005-tls-upstream/`, `tests/fixtures/0006-tls-sni/`, and `tests/fixtures/0008-http1-router-upstream/` are green at the Docker-gated CI level, with the CI run URL + the 5 individual test results quoted inline in `PROGRESS.md` (§3 D4);
- (b) the 3 pre-existing differential fixtures `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, and `tests/fixtures/0007-http1-direct-response/` remain green at the Docker-gated CI level (they are not edited in 05.1; their fixtures were green at HEAD `e626862` and continue green);
- (c) no conformance suites run in 05.1 (the first one — `h2spec` — attaches in 05.2);
- (d) the existing fuzz target `parse_bootstrap` runs clean for its short-budget CI run (`cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`) against the corpus extended in 05.1 with **one new seed** (`crates/envoy-config/fuzz/corpus/parse_bootstrap/strict_dns_cluster.yaml`; a full bootstrap with one `type: STRICT_DNS` cluster whose endpoint address is `localhost`); no new fuzz target ships in 05.1;
- (e) `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job. The `cargo deny check` clearance is a no-op (05.1 introduces no new top-level deps); the planner cross-checks at state-4 alongside the Cargo.lock sync;
- (f) `REVIEW.md` for this sub-phase is approved.

The 05.1 phase-done commit flips ROADMAP row `05.1` from `in-progress` to `done`. Parent row `05` stays `in-progress` until 05.3's phase-done commit (per the ROADMAP-schema invariant "parent flips to `done` only after all sub-phases are `done`"). STATE.md advances to phase `05.2` lifecycle state 3 (05.2's SPEC was already landed at parent-05 state-2 alongside 05.1's and 05.3's SPECs in the same commit; the next session runs `superpowers:writing-plans` scoped to sub-phase 05.2).

---

## 2. Behavior-contract scope for sub-phase 05.1

**No `BEHAVIOR_CONTRACT.md` edits in 05.1.** The fixture-hardening preamble produces no new responses, no new headers, no new wire shapes — it merely restores existing fixtures to green by changing how the upstream cluster resolves its endpoints (literal-IP → DNS-resolved at build time). The five fixtures' response surfaces are unchanged: 0003 echoes a TCP payload byte-exact (matrix row 8); 0004/0005/0006 terminate or originate TLS with the same handshake sequence and the same cert-presentation surface (matrix rows 5, 6); 0008 proxies an HTTP/1.1 request and returns the upstream-determined response under the existing 04.x `Header allow-list` (rows: `server` / `date` / `x-envoy-upstream-service-time`; the last row landed in 04.3 task 10 commit `cdd0218`). The contract's `Equivalence-matrix` engagement is transitive — the 5 restored fixtures continue exercising the same matrix dimensions they did at phase-04.3 close.

Equivalence-matrix rows engaged transitively (per `BEHAVIOR_CONTRACT.md` §7.2):

- **Row 1 (Response status)** — fixture 0008 exercises this via the proxied HTTP/1.1 response (200 OK from `http1-echo-server`); rows 0003/0004/0005/0006 are TCP-shaped and don't engage this row;
- **Row 2 (Response body)** — fixture 0008 byte-exact body equivalence (`http1-echo-server`'s deterministic alphabetically-sorted-header echo body, load-bearing per parent-04.3 SPEC §6 signpost 8); fixtures 0003/0004/0005/0006 byte-exact echo body;
- **Row 3 (Response headers)** — fixture 0008's response carries the existing 04.x `HEADER_ALLOW_LIST` from `tests/differential/src/lib.rs` (3 rows: `server`, `date`, `x-envoy-upstream-service-time`);
- **Row 5 (TLS handshake)** — fixtures 0004/0005/0006 (downstream TLS / upstream TLS / TLS SNI);
- **Row 6 (TLS certificate validation)** — fixtures 0005/0006 (upstream cert validation, SNI-based cert selection);
- **Row 8 (TCP-stream byte equivalence)** — fixtures 0003/0004/0005/0006 (TCP proxy byte-stream).

No new rows engaged. **No new allow-list entries.** No new `Stat-name`, `Access log field`, `xDS wire`, or `Timing tolerances` subsections touched.

The `HEADER_ALLOW_LIST` constant in `tests/differential/src/lib.rs` (the 04.3-landed shape with 3 rows: `server` `name-required, value-may-differ`; `date` `name-required, value-may-differ`; `x-envoy-upstream-service-time` `name-required, value-may-differ`) is unedited in 05.1.

---

## 3. Deliverables

### D1 — `ClusterType::StrictDns` schema variant in `envoy-config`

`crates/envoy-config/src/bootstrap.rs::ClusterType` (currently single-variant `Static` at lines 58–62 of HEAD `e626862`; see the `git show` invocation under §1 above) gains a `StrictDns` variant. Serde tag `STRICT_DNS` matches Envoy's `Cluster.type` proto enum literal exactly. The `LOGICAL_DNS` variant is **not** added in 05.1 — it differs from `STRICT_DNS` only in re-resolution semantics (`STRICT_DNS` caches the DNS result at cluster-build time; `LOGICAL_DNS` re-resolves per-request); the simpler `STRICT_DNS` shape suffices for the C-1 fix and `LOGICAL_DNS` defers to whichever later phase first needs per-request DNS re-resolution (parent-05 SPEC §4 non-goal; ratified inline by ADR-0023's decision arm — see §7).

Schema delta:

```rust
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum ClusterType {
    Static,
    StrictDns,    // 05.1 NEW
}
```

Validator extension in `crates/envoy-config/src/bootstrap.rs`'s validate path: the enum already rejects unknown serde tags via `deny_unknown_fields` + `rename_all = "SCREAMING_SNAKE_CASE"` on the enum (i.e., `LOGICAL_DNS` would surface as a Serde "unknown variant" error at deserialize time, with the per-fixture YAML's mapping-key context, exactly as `serde_yaml` does for any unrecognised SCREAMING_SNAKE_CASE variant — Envoy-shaped diagnostic shape, not an envoy-rust-bespoke error). No new `ConfigError` variant is needed for the parse-side rejection; the runtime-side resolution-failure case lands as a new variant — see below.

`ConfigError` extension in `crates/envoy-config/src/lib.rs`: add one new variant for the DNS-resolution-failure case. The variant lives on `ConfigError` (not on a new `ClusterError` arm) because the resolution failure is a config-load-time error per the existing `envoy_config::load_bootstrap` boundary (envoy-config drives the validator, which calls into envoy-cluster's constructor at startup; the constructor returns `ConfigError::*` shapes through the existing `?` chain — see signpost 1 below).

```rust
// crates/envoy-config/src/lib.rs — ConfigError variant added in 05.1:
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    // ... existing variants (UnknownCluster, EmptyCluster, EndpointParse, ...) ...

    #[error("cluster '{cluster}' STRICT_DNS resolution of '{address}' failed: {source}")]
    ClusterDnsResolutionFailed {
        cluster: String,
        address: String,
        #[source]
        source: std::io::Error,
    },
}
```

The `cluster` field carries the cluster's configured name (for diagnostic context); `address` carries the configured DNS name (e.g., `host.docker.internal`); `source` carries the underlying `tokio::net::lookup_host` `std::io::Error` for full provenance. No `ClusterError` extension needed (envoy-cluster's `ClusterError` enum continues unchanged at HEAD `e626862` — the existing `EmptyCluster`, `DuplicateClusterName`, `EndpointParse` variants cover the non-DNS failure shapes; the DNS resolution path runs *before* the existing `EndpointParse` is reached, since it produces resolved `SocketAddr`s ready for direct use).

**Validator unit tests appended** to `crates/envoy-config/src/bootstrap.rs::tests` (~6 tests):

- `parses_cluster_with_type_strict_dns` — full bootstrap with one `type: STRICT_DNS` cluster + one endpoint at `localhost:7000`; deserializes; the parsed `ClusterType` matches `ClusterType::StrictDns`.
- `parses_cluster_with_type_static_unchanged` — regression guard against any inadvertent change to the existing `STATIC` parse path; one `type: STATIC` cluster + one literal-IP endpoint at `127.0.0.1:7000`; deserializes; the parsed `ClusterType` matches `ClusterType::Static`.
- `rejects_cluster_with_type_logical_dns` — full bootstrap with one `type: LOGICAL_DNS` cluster; deserialize fails with a serde "unknown variant" error naming `LOGICAL_DNS`. (Documents the ADR-0023 deferral at the parser surface; if a future phase lifts the deferral, this test gets renamed to `parses_cluster_with_type_logical_dns` and the assertion flips.)
- `rejects_cluster_with_unknown_type_value` — full bootstrap with `type: WEIRD_TYPE`; deserialize fails with the same serde "unknown variant" error shape; covers the `deny_unknown_fields`-equivalent posture on the variant tag.
- `parses_cluster_with_type_strict_dns_with_multi_endpoint_load_assignment` — full bootstrap with one `type: STRICT_DNS` cluster + two endpoints (e.g., `localhost:7000` and `localhost:7001`); deserializes; the parsed `endpoints` Vec carries 2 entries with the SAME DNS name and DIFFERENT ports (verifies that the DNS-name endpoints are stored as raw strings at config-parse time; resolution lands in D2 not D1).
- `validates_strict_dns_cluster_does_not_require_literal_ip_endpoints` — explicit assertion that the existing `EndpointParse` ClusterError check is NOT triggered for `STRICT_DNS` clusters (since the DNS-name endpoint won't parse as `SocketAddr` directly; this is the load-bearing parse-side discriminator). The test mocks the `from_bootstrap` boundary just enough to assert the validator passes the parse stage without invoking `lookup_host` (the `lookup_host` call lands in D2's `Cluster::new`, not in the validator — see signpost 1 below).

LoC estimate: ~15 LoC schema delta (one enum variant) + ~10 LoC validator path (mostly inherited from the existing `serde::Deserialize` derive; the variant tag is mechanical) + ~5 LoC `ConfigError` variant + ~80 LoC unit tests (6 tests × ~13 LoC each). Total D1: **~110 LoC**, mostly tests.

**Re-exports in `crates/envoy-config/src/lib.rs`** — `ClusterType` (and its `Static`/`StrictDns` variants) is already re-exported from `bootstrap.rs` per the existing crate's public surface (verifiable at HEAD `e626862`); no new re-export needed. The new `ConfigError::ClusterDnsResolutionFailed` variant is added to the existing public `ConfigError` enum and is automatically reachable by consumers.

**Fuzz corpus extension.** `crates/envoy-config/fuzz/corpus/parse_bootstrap/` gains 1 new seed:

- `strict_dns_cluster.yaml` — full bootstrap with one listener (any plaintext shape; the simplest from existing 04.x fixtures, e.g., a TCP-proxy listener with `cluster: backend`) + one cluster of `type: STRICT_DNS` whose `load_assignment.endpoints[*].lb_endpoints[*].endpoint.address.socket_address` is `address: localhost, port_value: 7000`. The seed exercises the validator's accept-path on `STRICT_DNS`; the fuzzer never executes `lookup_host` (the `parse_bootstrap` target only exercises serde + the validator's deserialize path, not the runtime-side cluster construction in D2 — same posture as 04.2's `route_with_header_matchers.yaml` seed which doesn't execute the regex against header values).

The seed uses `localhost` (universally resolvable on any developer machine and in CI; not Docker-host-dependent like `host.docker.internal`) so that if the seed ever does drive a downstream code path that resolves it, the resolution succeeds. **Plausible-but-irrelevant** is the right framing — the seed exists for the parse pipeline.

### D2 — `Cluster::new` extension for `STRICT_DNS` resolution in `envoy-cluster`

`crates/envoy-cluster/src/cluster.rs` (HEAD `e626862` shape: a `Cluster { name: String, endpoints: Vec<SocketAddr>, cursor: AtomicUsize }` struct at lines 11–16; `Cluster::name()` accessor at lines 18–26 from the 04.3 D5 close-out commit `3fdf960`; a private `pick()` method; sibling `ClusterHandle` and `ClusterManager`; the existing `ClusterError` enum at line 95 with `EmptyCluster` / `DuplicateClusterName` / `EndpointParse` variants — the planner reads the live shape at task-1 time but the e626862 snapshot is the design baseline) gains a `STRICT_DNS` resolution branch.

**Where the new code lives.** `Cluster::from_bootstrap` (the cluster-manager constructor that consumes a parsed `Bootstrap` and builds the `ClusterManager`; the analogous-to-`Cluster::new` entry point — the planner cross-checks the exact function name at task-1 time) currently iterates the `bootstrap.static_resources.clusters` Vec and, for each cluster, parses each endpoint's `address` + `port_value` into a `SocketAddr` via `SocketAddr::from_str(&format!("{}:{}", addr, port))` (the existing `EndpointParse` arm). 05.1's extension adds a `match cluster.cluster_type` arm that diverges:

```rust
// 05.1 NEW — pseudocode for the planner; exact signature lands at PLAN.md writeup:
match cluster_def.cluster_type {
    ClusterType::Static => {
        // EXISTING path — unchanged. Each endpoint's address parses as a
        // literal SocketAddr via SocketAddr::from_str. Failure surfaces as
        // ClusterError::EndpointParse (regression-guarded by I3-closing test).
        for ep in cluster_def.load_assignment.endpoints.iter()
            .flat_map(|le| le.lb_endpoints.iter())
        {
            let sa = ep.endpoint.address.socket_address;
            let parsed = format!("{}:{}", sa.address, sa.port_value)
                .parse::<SocketAddr>()
                .map_err(|e| ClusterError::EndpointParse {
                    cluster: cluster_def.name.clone(),
                    addr: sa.address.clone(),
                    source: e,
                })?;
            endpoints.push(parsed);
        }
    }
    ClusterType::StrictDns => {
        // 05.1 NEW. Each endpoint's address is a DNS name; resolve via
        // tokio::net::lookup_host at cluster-build time. The lookup is
        // performed once, matching Envoy v1.33 STRICT_DNS semantics with
        // default dns_refresh_rate (periodic re-resolution is a non-goal
        // per ADR-0023).
        for ep in cluster_def.load_assignment.endpoints.iter()
            .flat_map(|le| le.lb_endpoints.iter())
        {
            let sa = &ep.endpoint.address.socket_address;
            let target = format!("{}:{}", sa.address, sa.port_value);
            let resolved: Vec<SocketAddr> = tokio::net::lookup_host(&target)
                .await
                .map_err(|e| ConfigError::ClusterDnsResolutionFailed {
                    cluster: cluster_def.name.clone(),
                    address: sa.address.clone(),
                    source: e,
                })?
                .collect();
            if resolved.is_empty() {
                return Err(ConfigError::ClusterDnsResolutionFailed {
                    cluster: cluster_def.name.clone(),
                    address: sa.address.clone(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "DNS resolution returned zero addresses",
                    ),
                });
            }
            endpoints.extend(resolved);
        }
    }
}
```

Notes on the shape:

- The function becomes `async` if it isn't already (verifiable at task-1 time; the planner cross-checks). Cluster construction at startup is already inside a tokio runtime per the envoy-bin entry shape from 02.1; the `await` introduces no new runtime requirement. If `from_bootstrap` is currently sync, the planner promotes it to `async` and updates the single envoy-bin call site; this is mechanical and ~5 LoC of churn.
- `lookup_host` returns an iterator of `SocketAddr`; we `.collect::<Vec<_>>()` into the cluster's endpoints list. A single DNS name commonly resolves to ≥1 address (e.g., `host.docker.internal` resolves to one Docker-host-gateway address per ADR-0015). Multi-address resolution is permitted (round-robin LB pick will iterate them per the existing `Cluster::pick` cursor logic at line 31).
- The `port` in the `lookup_host` target string is the cluster-side `port_value` (the cluster's configured upstream port), NOT a DNS port. `tokio::net::lookup_host` accepts `host:port` and returns `SocketAddr`s with the port already populated — this is the standard Rust idiom for "resolve a host+port pair to a socket address."
- Defensive zero-result branch: `lookup_host` documents that it may return an empty iterator on success (e.g., NXDOMAIN on some platforms surfaces as empty rather than as an `io::Error`). The explicit zero-check guards against silent acceptance of an empty cluster. A custom `io::Error` of kind `NotFound` is synthesised so the `source` field of `ClusterDnsResolutionFailed` carries diagnostic info even in the zero-result path.

**Tests** (~3 new tests in `crates/envoy-cluster/src/cluster.rs::tests`):

1. `static_cluster_constructs_with_literal_ip` — **the I3 close-out test.** A `Bootstrap` with one `type: STATIC` cluster + one literal-IP endpoint at `127.0.0.1:7000`; `from_bootstrap` succeeds; the resulting `ClusterManager.get("backend").pick_endpoint()` returns `Some(127.0.0.1:7000)`. This is structurally a `match cluster_type { Static => ... }`-arm exercise (the test wouldn't have been writable in 02.1 because `Static` was the only variant; now that `StrictDns` exists, the arm match is meaningful — closes phase-02.1 REVIEW I3 per the M1-projected-as-I3 chain). PROGRESS.md cross-references "closes phase-02.1 REVIEW I3" at the corresponding task.

2. `strict_dns_cluster_resolves_localhost_at_build_time` — a `Bootstrap` with one `type: STRICT_DNS` cluster + one endpoint at `localhost:7000`; `from_bootstrap` succeeds; the resulting `ClusterManager.get("backend").pick_endpoint()` returns `Some(<resolved>:7000)` where `<resolved>` is 127.0.0.1 (the IPv4 loopback) or ::1 (the IPv6 loopback) — the test asserts `pick_endpoint().port() == 7000` and `pick_endpoint().ip().is_loopback()`. `localhost` is universally resolvable across dev/CI (mirrors the fuzz seed in D1's choice of host).

3. `strict_dns_cluster_returns_dns_resolution_failed_on_nxdomain` — a `Bootstrap` with one `type: STRICT_DNS` cluster + one endpoint at a hostname guaranteed not to resolve (`this-host-does-not-exist.invalid:7000`; `.invalid` is RFC 6761 reserved for non-resolvable names). `from_bootstrap` returns `Err(ConfigError::ClusterDnsResolutionFailed { cluster: "backend", address: "this-host-does-not-exist.invalid", .. })`. **Note on test reliability:** the `.invalid` TLD is mandated by RFC 6761 §6.4 to be non-resolvable, but a misconfigured DNS resolver could synthesise a positive answer; if CI flakes on this test, the planner switches to a numeric IPv6 zero-prefix string that's guaranteed-malformed at the resolver layer (e.g., `tokio::net::lookup_host` with the empty-string host fails with a typed error). Document the choice at PLAN.md writeup.

LoC estimate: ~50 LoC runtime delta (the new `match` arm + the async promotion of `from_bootstrap` if needed + the call-site update in envoy-bin) + ~50 LoC tests (3 tests × ~15 LoC each + boilerplate). Total D2: **~100 LoC**.

**Cross-crate dependency note.** `tokio::net::lookup_host` requires the `net` feature on `tokio`. `crates/envoy-cluster/Cargo.toml` already pulls `tokio = { version = "1", features = ["net", "rt", ...] }` per the 04.x shape (verifiable at task-1 time by `grep tokio crates/envoy-cluster/Cargo.toml`); if the `net` feature is missing, the planner adds it inline with the D2 task commit (no ADR needed — `net` is a core tokio feature already pulled by other workspace crates per the existing Cargo.lock). 05.1 introduces no new top-level Cargo deps — confirmed under §6 signpost 14 below and §1 acceptance signal (e).

### D3 — Coordinated 5-fixture YAML edit

The 5 fixtures' YAML pairs are edited in lockstep to flip `type: STATIC` → `type: STRICT_DNS` on the cluster whose endpoints reference `host.docker.internal`. The exact edit per fixture, with file paths and line references at HEAD `e626862` (verifiable by `grep -rn "type: STATIC" /Users/esa/git/envoy-rust/tests/fixtures/000*/envoy*.yaml`):

| Fixture | Files | Line(s) | Edit |
|---|---|---|---|
| `tests/fixtures/0003-tcp-proxy/` | `envoy.yaml` (line 27), `envoy-rust.yaml` (line 21) | 1 line each | `type: STATIC` → `type: STRICT_DNS` on the cluster whose endpoints' `address: {{BACKEND_HOST}}` |
| `tests/fixtures/0004-tls-downstream/` | `envoy.yaml` (line 37), `envoy-rust.yaml` (line 31) | 1 line each | same flip |
| `tests/fixtures/0005-tls-upstream/` | `envoy.yaml` (line 16), `envoy-rust.yaml` (line 15) | 1 line each | same flip |
| `tests/fixtures/0006-tls-sni/` | `envoy.yaml` (line 40), `envoy-rust.yaml` (line 39) | 1 line each | same flip |
| `tests/fixtures/0008-http1-router-upstream/` | `envoy.yaml` (line 49), `envoy-rust.yaml` (line 27) | 1 line each | same flip |

For each fixture pair, the harness substitutes `{{BACKEND_HOST}}` (per ADR-0015's render mechanism) — `envoy.yaml` gets the rendered value `host.docker.internal` (per the upstream-Envoy container's host-gateway), `envoy-rust.yaml` gets the rendered value `127.0.0.1` (per envoy-rust's host-process posture). With `type: STRICT_DNS`, the upstream Envoy container resolves `host.docker.internal` → host-gateway IP at startup (Envoy's STRICT_DNS resolver); envoy-rust resolves `127.0.0.1` → 127.0.0.1 at startup (`tokio::net::lookup_host` accepts literal IPs and returns them as-is, parsed into `SocketAddr`s — verified by tokio docs and the second test in D2).

**Why both sides get `STRICT_DNS` and not just `envoy.yaml`.** Envoy v1.33 rejects `host.docker.internal` under `type: STATIC` per the C-1 trace; envoy-rust at HEAD `e626862` accepts it under `STATIC` (the parse is lenient — see §1 trace) but the runtime construction fails at `SocketAddr::from_str`. The substantive fix is to make both proxies treat the address as a DNS name via `STRICT_DNS`. Symmetric fixture YAMLs keep the differential property pure (both proxies see the same cluster shape, modulo the existing per-side substitutions for bind address / admin block).

**Why the 3 unaffected fixtures are not edited.**

- `tests/fixtures/0001-tcp-echo/` — has no upstream cluster (the listener's TCP filter chain echoes the client's bytes back via a sink). `grep -rn "type: STATIC" tests/fixtures/0001-tcp-echo/` returns nothing.
- `tests/fixtures/0002-static-admin-ready/` — has no upstream cluster (admin endpoint only). Same `grep` result.
- `tests/fixtures/0007-http1-direct-response/` — has no upstream cluster (the HCM dispatches `direct_response` which produces a static body without a cluster lookup). Same `grep` result.

(Verifiable at task-1 time by `grep -rn "type: STATIC\|host.docker.internal" tests/fixtures/0001*/ tests/fixtures/0002*/ tests/fixtures/0007*/` — all three return zero matches.)

**Optional `dns_lookup_family` knob.** Envoy's `STRICT_DNS` cluster has an optional `dns_lookup_family` field (default `AUTO`; alternatives `V4_ONLY`, `V6_ONLY`, `V4_PREFERRED`, `ALL`) that controls which address family is preferred during resolution. The default `AUTO` resolves both A and AAAA records and prefers the system stack's choice. envoy-rust's 05.1 `STRICT_DNS` implementation does not parse `dns_lookup_family` (it's not added to the `Cluster` struct in 05.1; serde `deny_unknown_fields` rejects it if a fixture YAML includes it). Whether to pin the fixtures explicitly to `V4_ONLY` to match `127.0.0.1` (envoy-rust) and the IPv4 host-gateway address (Envoy) more deterministically is a planner-time call (§6 signpost 6 below) — recommended posture: **omit `dns_lookup_family` from the fixtures**, rely on the `AUTO` default, and trust both proxies to resolve loopback / host-gateway consistently. If a fixture flakes on IPv6-vs-IPv4 selection in CI, the planner adds `dns_lookup_family: V4_ONLY` to that fixture as a follow-up commit and lifts the parser deferral (small `Cluster` struct extension; ~10 LoC).

**Per-fixture commit cadence.** Two options:

- **(a) One bundled commit** for all 10 YAML edits ("fixture edit: 5 fixtures flip STATIC → STRICT_DNS"). Cleanest in `git log`; one easy-to-read diff.
- **(b) Five separate commits** (one per fixture pair), mirroring 04.3's per-fixture commit cadence (e.g., commit `89f7018` for fixture 0008 alone). More granular `git log` history; each commit can be cherry-picked independently if a future phase needs to revert one fixture.

**Recommendation:** **(a) one bundled commit**, because the 10 edits are mechanically identical and the differential property is "all 5 fixtures green simultaneously" — splitting muddies the gate signal. The 04.3 per-fixture cadence applied to landing *new* fixtures (each fixture was a substantive addition); 05.1's cadence applies to *editing* existing fixtures uniformly. The planner records the cadence choice at PLAN.md writeup.

LoC estimate: ~30 LoC of YAML diff total (10 files × 1 line × the trivial substitution; some lines carry a leading-whitespace concern from YAML indent, but the `type:` line is at the same indent as the existing line, so it's a true 1-line replacement per file). **No locally-verified Docker run is required at D3 task time** — the substantive verification happens at D4 via the CI re-push (per the established phase-precedent that local Docker runs are an optional planner convenience, not a deliverable). Total D3: **~30 LoC of YAML**.

### D4 — Verification deliverable (Docker-gated CI re-push)

**No new code in D4.** Re-push the 05.1 branch to CI (the planner's standard `git push` to the 05.1 working branch / PR). Confirm green Docker-gated runs across the 5 affected differential tests:

- `tests/differential/tests/tcp_proxy.rs` (fixture 0003)
- `tests/differential/tests/tls_downstream.rs` (fixture 0004)
- `tests/differential/tests/tls_upstream.rs` (fixture 0005)
- `tests/differential/tests/tls_sni.rs` (fixture 0006)
- `tests/differential/tests/http1_router_upstream.rs` (fixture 0008)

PROGRESS.md quotes the CI run URL + the 5 individual test results inline per the standard verification cadence (precedent: 04.3's task-15 commit `89f7018` quoted the corresponding CI run; 04.3's state-4 verification commit `cb0949e` aggregated the full suite). The aggregation lives in the 05.1 state-4 phase-done-gate verification commit per `BOOTSTRAP_PROMPT.md` §7.5.

If any fixture remains red after the schema + fixture edits, the planner re-enters state 3 (REVIEW.md re-loop per `BOOTSTRAP_PROMPT.md` §5.2). No further coding is anticipated; the schema growth (D1) + runtime growth (D2) + fixture edits (D3) are mechanically sufficient for the C-1 fix per the parent-05 SPEC §1 brainstorm finding. Possible re-loop reasons:

- A fixture's upstream Envoy container fails to start because of an additional schema field (e.g., `dns_lookup_family` defaults that diverge across Docker hosts) — the planner adds an explicit `dns_lookup_family: V4_ONLY` to that fixture's YAML and re-pushes; ~5 LoC YAML edit + a sub-deliverable on D1 to parse the field.
- envoy-rust's `tokio::net::lookup_host` at startup hangs for >5s on a CI host with a slow DNS resolver — the planner adds a timeout via `tokio::time::timeout(Duration::from_secs(5), lookup_host(...))` in D2's resolution branch and surfaces a typed `ConfigError::ClusterDnsResolutionFailed` with a synthesised `io::Error` of kind `TimedOut`; ~10 LoC runtime edit + 1 test.

Both are anticipated-but-unlikely. The recommended posture is to land 05.1 with the minimal fix (D1 + D2 + D3) and only extend if CI surfaces a concrete failure.

**This deliverable also materially closes phase-04.3 REVIEW C-1.** The C-1 carryforward chain ends at 05.1's state-4 phase-done verification: a green CI run on the 5 affected fixtures is the substantive close-out (the chain originated at phase-02.2's ADR-0015 landing `435c6fa`; the gap-detection happened at phase-04.3 task-14 commit `eb6f972`; the disposition decision happened at the 04.3 STATE.md handoff `e626862`; the implementation lands at 05.1; the verification closes the chain at 05.1 state-4). REVIEW.md for 05.1 carries an explicit "phase-04.3 REVIEW C-1: closed at this commit's CI run" entry under its "Carryforwards" section.

LoC estimate: 0 LoC (pure verification + PROGRESS/REVIEW wording). Total D4: **0 LoC**, ~50 lines of PROGRESS/REVIEW prose.

### D5 — Closes phase-02.1 REVIEW I3 (positive `Static` regression guard)

**Implemented as part of D2's test suite, not a separate deliverable.** Phase-02.1 REVIEW I3 was deferred at phase-02.1 close because the single-variant `ClusterType { Static }` enum had no second variant against which to discriminate the `Static` arm structurally. Adding `StrictDns` in D1 unblocks it; the I3 test lands in D2 as `static_cluster_constructs_with_literal_ip` (§3 D2 test 1 above).

The carryforward chain in `STATE.md` Notes section gets a final entry at the 05.1 state-6 phase-done commit:

> **Phase-02.1 REVIEW I3 — closed at phase-05.1 state-6 commit `<SHA>`.** Originating issue: positive `ClusterType::Static` variant-name regression guard could not be written when `Static` was the only `ClusterType` variant. Disposition history: deferred at phase-02.1 close; rolled forward unchanged through phases 02.2 / 03.1 / 03.2 / 04.1 / 04.2 / 04.3 (each phase's REVIEW.md re-noted the deferral without action). Substantive resolution: phase-05.1 D1 added `ClusterType::StrictDns` (the second variant), making the `match cluster_type` arm structurally meaningful; phase-05.1 D2 added `static_cluster_constructs_with_literal_ip` as the positive regression guard. Carryforward chain ends here.

D5's LoC estimate is folded into D2: **0 LoC additional** (the test that closes I3 is one of D2's 3 tests).

---

## 4. Non-goals (subset of parent SPEC §4 that bind on 05.1)

The following are out of scope for 05.1 and defer to other sub-phases or later phases. The list is a subset of parent-05 SPEC §4, scoped to items that are predictably tempting to fold into 05.1 by a planner reading only this SPEC.

**Deferred to sub-phase 05.2:**
- **Downstream H2C HCM** (`crates/envoy-http2/` crate, `Http2Codec`, HCM-on-H2 dispatch, fixture 0009 `http2-direct-response`). Parent-05 SPEC §3 D5.2–D9.2.
- **`CodecType::HTTP2` accept-validation flip.** At 05.1 close, the existing `CodecType::HTTP2` reject-path landed in 04.1 continues unchanged. 05.2 lands the accept-flip alongside `Http2ProtocolOptions` schema (parent-05 SPEC §3 D6.2).
- **`h2spec` ≥95% conformance gate.** Attaches in 05.2 at parent-05 SPEC §3 D10.2.
- **`Http2ProtocolOptions` listener-side schema.** Per parent-05 SPEC §3 D6.2; 05.1 does not parse this struct.

**Deferred to sub-phase 05.3:**
- **Upstream H2C origination** (`envoy-http2::Client`, fixture 0010 `http2-router-upstream`, `tests/helpers/http2-echo-server/` helper). Parent-05 SPEC §3 D11.3–D15.3.
- **Router H2-arm dispatch.** Parent-05 SPEC §3 D13.3.
- **`Http2ProtocolOptions` cluster-side schema** (via `typed_extension_protocol_options`). Parent-05 SPEC §3 D12.3.
- **`Cluster.upstream_protocol: UpstreamProtocol { Http1, Http2 }` field.** Parent-05 SPEC §3 D12.3 + signpost 5.
- **Parent ROADMAP row `05` flip to `done`.** Happens at sub-phase 05.3's state-6 phase-done commit, not 05.1's.

**Deferred to later phases (per parent-05 SPEC §4 — items relevant to the cluster-type / DNS surface):**

- **`LOGICAL_DNS` cluster type.** 05.1 ships only `STRICT_DNS`. The two differ in re-resolution semantics: `STRICT_DNS` caches the DNS result at cluster-build time (one-shot lookup, no per-request cost); `LOGICAL_DNS` re-resolves per-request (used when DNS-layer LB is intended, with a single resolved address per request rather than the full set). `STRICT_DNS` matches the C-1 fixture-fix need cleanly: `host.docker.internal` resolves to a single Docker-host-gateway address that doesn't change during the fixture run. `LOGICAL_DNS` defers to whichever phase first needs per-request DNS re-resolution (likely a later phase in the cluster-discovery family — possibly tied to the EDS/CDS xDS work). Ratified inline by **ADR-0023** (§7 below).

- **`dns_refresh_rate` / periodic DNS re-resolution for `STRICT_DNS` clusters.** The 05.1 implementation resolves once at cluster-build time. Periodic re-resolution is an Envoy knob (`Cluster.dns_refresh_rate: Duration`, default 5s) that defers to a later phase. Adding it would require a background tokio task per cluster + a `tokio::time::interval` driver + a swap-in mechanism for the resolved endpoints (likely an `Arc<RwLock<Vec<SocketAddr>>>` replacing the current `Vec<SocketAddr>` field on `Cluster`); the surface is non-trivial and not needed for the C-1 fix.

- **`dns_lookup_family` knob.** Envoy's `STRICT_DNS` cluster has this optional field (default `AUTO`); 05.1 does not parse it (serde `deny_unknown_fields` rejects it). The default `AUTO` resolves both A and AAAA records and trusts the system stack to pick. If a CI host's resolver picks AAAA where the test expected A (or vice versa), the planner's recommended posture (§3 D3 above) is to add `dns_lookup_family: V4_ONLY` to the fixture as a follow-up YAML edit + a small `Cluster` struct extension in 05.1 — but only if a CI failure surfaces; not anticipated.

- **`respect_dns_ttl` knob.** Envoy's `STRICT_DNS` cluster has this optional field (default `false`); 05.1 does not parse it. Defers with `dns_refresh_rate`.

- **`dns_resolvers` array (alternative resolver pool).** Envoy supports configuring a non-system resolver via `Cluster.dns_resolvers: Vec<Address>`; 05.1 uses the system resolver via `tokio::net::lookup_host` (which delegates to libc's `getaddrinfo`). The `dns_resolvers` field is rejected at parse time (serde `deny_unknown_fields`); defers to whichever phase first needs custom resolver pools — likely tied to xDS-driven dynamic cluster discovery.

- **`trust-dns-resolver` / `hickory-resolver` / async-DNS-resolver alternatives.** D-3.2's permitted-foundations list (verified at SPEC writeup against `MISSION.md` lines 45–62) covers the standard library and `tokio` (which carries `tokio::net::lookup_host`); it does NOT cover any third-party DNS resolver crate. 05.1 uses `tokio::net::lookup_host` exclusively; this is a doctrine choice, not a knob to flip later. If a future phase's DNS needs outgrow `tokio::net::lookup_host` (e.g., periodic re-resolution under `dns_refresh_rate`, or DNS-over-TLS, or DoH), that phase lands a permitted-foundations-extension ADR for the resolver crate it picks. **05.1 explicitly does not introduce this dependency** (§6 signpost 5 below).

- **HCM `server_name` field, `Cargo.lock` ratification ADRs, header allow-list extensions, etc.** All inherited from parent-05 SPEC §4 unchanged; 05.1 binds on none of them.

**Not deferred — confirmed in scope for 05.1** (for clarity, since these have predictable confusion points):

- `tests/fixtures/0009/` and `tests/fixtures/0010/` are NOT created in 05.1 (they land in 05.2 and 05.3 respectively); the differential surface delta in 05.1 is "5 fixtures restored to green" — no new fixtures.
- `BEHAVIOR_CONTRACT.md` is NOT edited (per §2 above).
- `crates/envoy-http2/` is NOT created (lands in 05.2).
- The `h2 = "0.4"` Cargo dep is NOT added (lands in 05.2).
- `cluster.transport_socket` (the upstream-TLS extension landed in 03.2) is unchanged in 05.1; its existing parse + accept paths continue to work for TLS-bearing clusters under `type: STRICT_DNS` (the 5 fixtures include 4 with TLS-on-cluster — 0004/0005/0006 are TLS fixtures + 0008 may carry TLS upstream depending on the fixture's posture; the planner verifies `transport_socket` interaction at task-1 time).

---

## 5. Cross-sub-phase architectural rules inherited from parent SPEC §3

These rules are non-negotiable across the three sub-phases of parent phase 05; sub-phase 05.1 inherits them verbatim per parent-05 SPEC §3 cross-sub-phase architectural rules section. Reproduced here in brief paraphrase with parent-SPEC pointers:

1. **`envoy-http2` is the SOLE workspace dep on `h2`.** No other crate calls `h2::*` directly. (Parent-05 SPEC §3 architectural rule 1.) **Bearing on 05.1:** trivially satisfied — 05.1 doesn't add `h2` or any new crate at all. The rule binds 05.2/05.3.

2. **HCM-on-H2 reuses 04.x's `HCMConfig` and route-walk wholesale; only the codec layer at the connection edge changes.** (Parent-05 SPEC §3 architectural rule 2.) **Bearing on 05.1:** trivially satisfied — 05.1 makes no HCM changes.

3. **`:authority` → `Host:` mapping at the H2-to-envoy-Request translation boundary.** (Parent-05 SPEC §3 architectural rule 3.) **Bearing on 05.1:** trivially satisfied — 05.1 makes no request-translation changes.

4. **H2-forbidden hop-by-hop headers stripped at the codec edges, not at the HCM core.** (Parent-05 SPEC §3 architectural rule 4.) **Bearing on 05.1:** trivially satisfied — 05.1 introduces no new codec edges.

5. **No H2-specific edits to `envoy-config`'s `RouteConfiguration` or `HeaderMatcher` schemas.** (Parent-05 SPEC §3 architectural rule 5.) **Bearing on 05.1:** trivially satisfied. 05.1's only `envoy-config` edit is the `ClusterType` extension — orthogonal to RouteConfiguration / HeaderMatcher.

6. **`codec_type: AUTO` continues to behave as `HTTP1`-only.** (Parent-05 SPEC §3 architectural rule 6.) **Bearing on 05.1:** trivially satisfied — 05.1 makes no `CodecType` changes.

7. **`http` crate is permitted as a transitive surface only.** (Parent-05 SPEC §3 architectural rule 7.) **Bearing on 05.1:** trivially satisfied — 05.1 doesn't import `http::*`.

The rules are listed for completeness; all 7 are no-ops for 05.1's actual surface. They become load-bearing at 05.2 / 05.3 when the H2 codec attaches.

---

## 6. Implementation signposts for the planner

Notes flagging predictable planner questions so the 05.1 planner resolves them in-plan rather than mid-execution. Inherits parent-05 SPEC §6 signposts where they bind on 05.1, plus 05.1-local signposts.

**Inherited signposts from parent-05 SPEC §6:**

1. **Signpost 14 (Cargo.lock sync cadence) — 05.1 is a no-op for new top-level deps.** Per parent-05 SPEC §6 signpost 14, the Cargo.lock sync cadence follows the established phase-precedent (phase-01 `4955252`, phase-02.1 `dea4d16`, phase-02.2 `2146014`, phase-03.1 `eb039e6`, phase-03.2 `85685a3`, phase-04.x inline). 05.1 introduces no new top-level deps (`tokio::net::lookup_host` lives in the existing `tokio` foundation already pulled by `envoy-cluster`; the `net` feature may need to be added to `crates/envoy-cluster/Cargo.toml` if not already present, which is a minor feature-flag edit, not a new top-level dep). Cargo.lock sync at state-4 is expected to be a no-op or a near-no-op (only feature-resolution differences if the `net` feature wasn't already activated in the workspace's resolved feature set). The state-4 phase-done verification commit either includes a single-line Cargo.lock diff or none at all — the planner records the choice at PLAN.md writeup.

2. **Signpost 16 (PLAN.md cadence) — standalone pre-Task-1 commit.** Per parent-05 SPEC §6 signpost 16, each sub-phase's planner commits PLAN.md cleanly at state-2 close-out, before any Task 1 commit. Precedent: phase-04.3's `c02eea7` commit per the parent-05 STATE.md handoff. **The 05.1 PLAN.md is committed standalone, not folded into the Task 1 commit.** The 04.1 / 04.2 inline-PLAN deviation (where PLAN.md was committed alongside the first task's code) is no longer the precedent.

3. **Signpost 17 (Fixture 0010 STRICT_DNS projection) — informational for the 05.1 planner.** Per parent-05 SPEC §6 signpost 17, fixture 0010 (which lands in 05.3 with an H2C upstream at `host.docker.internal`) is projected to declare `type: STRICT_DNS` at writeup time, depending on the 05.1-landed schema variant. **Bearing on 05.1:** the 05.1 planner does not create fixture 0010. The dependency is forward (05.3 depends on 05.1's schema); 05.1 has no upstream dependency on 05.3.

4. **Signpost 21 (ADR ledger projection) — ADR-0023 lands at 05.1 Task 1.** Per parent-05 SPEC §6 signpost 21 + §7 ADR-0023 projection, **ADR-0023** is appended to `docs/envoy-rust/DECISIONS.md` at 05.1 Task 1 alongside the schema variant addition (D1) and the runtime extension (D2). Mirrors phase 04.2's ADR-0021 inline-at-Task-1 pattern (commit `984aedd`). The DECISIONS.md ledger head is currently **ADR-0022** (landed at parent-05 state-2 commit alongside the three sub-phase SPECs); 05.1 Task 1's commit lands ADR-0023 at the next-sequential number.

**05.1-local signposts:**

5. **`tokio::net::lookup_host` is the chosen DNS resolver primitive — NOT `trust-dns-resolver` / `hickory-resolver`.** Per D-3.2 (`MISSION.md` lines 45–62), permitted foundations include `tokio` (which provides `tokio::net::lookup_host`), `tokio-util`, `bytes`, `h2`, `httparse`, `quinn`, `rustls`/`webpki`/`rustls-pki-types`, `prost`/`prost-types`/`prost-build`, `tonic`/`tonic-build`, `serde`/`serde_yaml`/`serde_json`, `tracing`/`tracing-subscriber`, `opentelemetry`/`opentelemetry_sdk`/`tracing-opentelemetry`, `thiserror` (and `anyhow` only in `envoy-bin`), and `testcontainers`. **`trust-dns-resolver` and `hickory-resolver` are NOT on the list and are NOT permitted in 05.1.** The 05.1 planner uses `tokio::net::lookup_host` exclusively. Any future phase that wants a richer DNS resolver (for e.g. periodic re-resolution, DoT/DoH, custom resolver pools) lands a permitted-foundations-extension ADR — but that's not 05.1's call.

6. **`dns_lookup_family` field — NOT parsed in 05.1; planner-recommended posture is "rely on AUTO default."** As noted in §3 D3, Envoy's `STRICT_DNS` cluster has an optional `dns_lookup_family` field (default `AUTO`) that 05.1 does not parse. The fixtures (post-05.1 D3 edit) declare `type: STRICT_DNS` without an explicit `dns_lookup_family`, relying on Envoy's `AUTO` default to resolve both A and AAAA records. envoy-rust's `tokio::net::lookup_host` performs the same posture (resolves both). If CI flakes on IPv6-vs-IPv4 selection (unlikely; loopback and Docker host-gateway both have stable IPv4 representations), the planner adds `dns_lookup_family: V4_ONLY` to the fixture YAMLs as a follow-up + a small `Cluster` struct extension to parse it in 05.1's schema. Document the call at PLAN.md writeup. **The recommended posture is to NOT extend the schema preemptively** — only add it if CI surfaces a concrete failure.

7. **`FIXTURE_HOST` literal under `STRICT_DNS` — `host.docker.internal` resolves via Docker host-gateway per ADR-0015.** ADR-0015 (committed at `435c6fa`) established the host-gateway posture for upstream-Envoy-container-to-host-process reachability. Under `STRICT_DNS` (post-05.1 D3 edit), the upstream Envoy container's startup-time resolver consults `/etc/hosts` (where Docker injects `host.docker.internal` → host-gateway IP per `with_host(..., Host::HostGateway)`); the resolution succeeds at Envoy startup. envoy-rust's `tokio::net::lookup_host` against `127.0.0.1` (the `envoy-rust.yaml` substitution per the harness's per-side render) trivially succeeds. The differential property is preserved: both proxies resolve their cluster endpoints at startup; both proxies forward request bytes to the same backend (the host-process echo server / TLS echo server / HTTP/1.1 echo server, depending on fixture). ADR-0015's host-gateway grant is unchanged in 05.1.

8. **Per-fixture commit cadence vs. one bundled commit.** As discussed in §3 D3 above, the recommendation is **one bundled commit** for the 5-fixture YAML edit; 04.3's per-fixture cadence applied to landing new fixtures, not to editing existing ones. The planner records the cadence choice at PLAN.md writeup. Either choice is acceptable; 04.3's per-fixture-PR approach is also defensible (cleaner `git bisect` story if a single fixture flakes for an unrelated reason).

9. **`from_bootstrap` async promotion (if needed).** As discussed in §3 D2, if the existing `from_bootstrap` (or whichever cluster-manager constructor) is sync at HEAD `e626862`, the 05.1 D2 implementation promotes it to `async` to call `tokio::net::lookup_host(..).await`. The single envoy-bin call site (`crates/envoy-bin/src/main.rs`) needs an `await` added at the call. This is mechanical and ~5 LoC of churn. The planner verifies the live shape at task-1 time; if `from_bootstrap` is already async (it may be — phase-03+ entry points are increasingly async-aware), no churn.

10. **Unit-test-target choice for the DNS resolution test (signpost 5 in D2).** The recommended target is `localhost` (universally resolvable, loopback-bound, matches the `parse_bootstrap` fuzz seed in D1). Alternatives considered:
    - `127.0.0.1` — works but doesn't exercise DNS-layer behavior (it's a literal IP that `lookup_host` accepts and returns directly; the test would pass even if `lookup_host` were stubbed out as a no-op pass-through). Rejected.
    - `host.docker.internal` — environment-dependent (only resolves on a Docker-running host). Rejected; this is exercised at fixture level (D3) not unit level.
    - `example.com` — externally resolved via DNS; introduces network dependency in the test. Rejected; tests should be hermetic.
    
    Decision: **`localhost`**. Documented at the test rustdoc + cross-referenced in PLAN.md.

11. **NXDOMAIN test stability (signpost 6 in D2 test 3).** As noted in §3 D2 test 3, `.invalid` is RFC 6761 §6.4 reserved as non-resolvable, but a misconfigured DNS resolver could synthesise a positive answer. If CI flakes, fall back to a target string guaranteed-malformed at the resolver layer: `tokio::net::lookup_host("")` (empty-string host) returns a typed `io::Error` reliably. Document the fallback at PLAN.md writeup; recommended primary is the `.invalid` TLD for diagnostic-shape readability.

12. **`cargo deny check` is a no-op at 05.1.** No new top-level Cargo deps; no new transitive surface; the `[licenses]` allow-list and `[bans]` list in `deny.toml` are unchanged. The state-4 phase-done verification commit confirms `cargo deny check` passes; if a transient-network resolution against the crates.io advisory database flakes (a known intermittent issue in CI), the planner re-runs the gate and documents the flake in PROGRESS.md.

13. **`#![forbid(unsafe_code)]` is unchanged on `crates/envoy-config/src/lib.rs` and `crates/envoy-cluster/src/lib.rs`.** D-3.8 carries forward; no `unsafe` introduced in 05.1. `tokio::net::lookup_host` is safe; the only call site is in safe envoy-cluster code.

14. **`anyhow` boundary unchanged.** envoy-cluster returns `ClusterError` (typed) from its constructor; envoy-config returns `ConfigError` (typed) from validate; only envoy-bin uses `anyhow` (per D-3.2). 05.1's new `ConfigError::ClusterDnsResolutionFailed` variant flows through the existing typed-error chain to `envoy-bin::main` where `anyhow` absorbs it via the existing `?` chain at startup.

15. **No `BEHAVIOR_CONTRACT.md` edits.** Confirmed in §2 above. The 04.1+04.3 `HEADER_ALLOW_LIST` constant in `tests/differential/src/lib.rs` is unedited in 05.1.

16. **Fuzz seed file path consistency.** New seed lands at `crates/envoy-config/fuzz/corpus/parse_bootstrap/strict_dns_cluster.yaml`. Mirrors the existing 04.x seed shape (e.g., `route_with_header_matchers.yaml` at the same directory; landed in 04.2 task 1 commit `984aedd`).

17. **STATE.md "Carryforwards" / "Notes" section bookkeeping.** At 05.1 state-6 phase-done commit, STATE.md's Notes section gains:
    - "Phase-04.3 REVIEW C-1 — closed at this commit's CI run; the 5-fixture STRICT_DNS flip + the schema growth materially restored Docker-gated green across `0003`/`0004`/`0005`/`0006`/`0008`. Carryforward chain ends here."
    - "Phase-02.1 REVIEW I3 — closed at this commit; the positive `Static` regression guard `static_cluster_constructs_with_literal_ip` lands as part of D2's test suite. Carryforward chain ends here."
    - "Phase-04.1 REVIEW M-claim (drive_http1 per-function unit test) — unblocked by the 05.1 fix's removal of the Docker-gated regression mask, but stays deferred per the 04.3 disposition (the M-claim is a separate per-function unit test; 05.1 does not extend the harness to add it). Carryforward chain continues."

18. **05.2 SPEC + 05.3 SPEC are landed alongside this SPEC.** Per parent-05 state-2 lifecycle (mirrors phase-04 state-2 commit `1d9740d`), the parent-05 state-2 commit lands ADR-0022 + all three sub-phase SPECs (`05.1-fixture-hardening/SPEC.md`, `05.2-http2-downstream/SPEC.md`, `05.3-http2-upstream/SPEC.md`). 05.1 execution starts after that commit. The 05.2 and 05.3 SPECs are unedited during 05.1 execution; their PLAN.md / PROGRESS.md / REVIEW.md land in their own sub-phase execution windows.

---

## 7. ADRs expected from this sub-phase

**One ADR lands during 05.1 execution**, appended to `docs/envoy-rust/DECISIONS.md` at Task 1 alongside the schema variant addition (D1) + the runtime extension (D2). Mirrors phase 04.2 Task 1's ADR-0021 inline-landing pattern (commit `984aedd`); also mirrors phase 03.1 Task 1's ADR-0018 + ADR-0019 inline-landing pattern.

### ADR-0023 — `ClusterType::StrictDns` accepted; `LOGICAL_DNS` deferred

- **Date:** 2026-05-01 (or whatever date 05.1 Task 1 lands; backdated to ADR landing day per the ADR-0021 / ADR-0018 / ADR-0014 precedent).
- **Status:** accepted.
- **Context:** Phase 05.1 is the fixture-hardening preamble for parent phase 05 (HTTP/2 cleartext data plane). The cross-phase Docker-gated `host.docker.internal`/`type: STATIC` regression — originating at phase-02.2's ADR-0015 landing (commit `435c6fa`) and discovered at phase-04.3 task 14 (commit `eb6f972`) per the C-1 trace in parent-05 SPEC §1 — must be closed before any new H2 surfaces are layered on top of the 5 affected fixtures (0003/0004/0005/0006/0008). Upstream Envoy v1.33.0's `socket_address.address` parse semantics expect either a literal IP (under `type: STATIC`) or DNS resolution opt-in (under `type: STRICT_DNS` or `type: LOGICAL_DNS`); envoy-rust's parser currently accepts only `STATIC` (single-variant `ClusterType { Static }` enum at HEAD `e626862`, lines 58–62 of `crates/envoy-config/src/bootstrap.rs`). Phase 02.1 REVIEW I3 (positive `ClusterType::Static` variant-name regression guard) has been deferred since phase-02.1 close because the single-variant enum had no second variant against which to discriminate `Static` structurally; adding a second variant unblocks I3 mechanically.
- **Options considered:**
  - **(i) Add only `StrictDns`.** Resolves DNS at cluster-build time; results cached for the cluster's lifetime. Sufficient for the C-1 fix because `host.docker.internal` resolves to a single Docker-host-gateway address that doesn't change during the fixture run. **Chosen.**
  - **(ii) Add both `StrictDns` and `LogicalDns`.** Mirrors Envoy's full proto more completely. Rejected: `LogicalDns`'s per-request re-resolution semantics require a non-trivial runtime extension (the cluster must drop the cached resolution after the resolved addresses are picked once, vs. round-robining over the cached set indefinitely under `StrictDns`); no 05.1 fixture exercises this distinction; D-3.6's "every phase is a green build" + the §6.1 split-gate reward minimal forward landings.
  - **(iii) Add `StrictDns` + a configurable `dns_refresh_rate` knob to enable periodic re-resolution.** Rejected: same as (ii); no 05.1 fixture needs it; defers to a later phase per parent-05 SPEC §4.
  - **(iv) Defer the entire `STRICT_DNS` extension; fix C-1 by replacing `host.docker.internal` with a literal IP across the 5 fixtures.** Rejected: would require either (a) testcontainers-side IP discovery at fixture-render time (the host-gateway IP varies across Docker setups), which is brittle, or (b) baking a static IP into the YAMLs, which is platform-specific. ADR-0015's `host.docker.internal` posture is the right cross-platform choice; the right fix is to make envoy-rust accept the DNS-name shape, not to abandon it.
  - **(v) Use a different DNS resolver (e.g., `trust-dns-resolver` / `hickory-resolver`) instead of `tokio::net::lookup_host`.** Rejected: `tokio::net::lookup_host` is part of the existing `tokio` permitted foundation per D-3.2 (no new dep needed; no new ADR scope-extension required). A third-party resolver crate is not on D-3.2's permitted list and would require its own permitted-foundations-extension ADR, which 05.1 does not need.
- **Decision:** Extend `crates/envoy-config/src/bootstrap.rs::ClusterType` from single-variant `Static` to `Static | StrictDns`. Validator accepts the `STRICT_DNS` serde tag; runtime resolution lives in `crates/envoy-cluster/src/cluster.rs::Cluster::from_bootstrap` (the cluster-manager constructor) via `tokio::net::lookup_host(format!("{}:{}", address, port)).await`. Resolution failures surface as a new `ConfigError::ClusterDnsResolutionFailed { cluster: String, address: String, source: std::io::Error }` variant. The `LOGICAL_DNS` variant is **NOT** added in 05.1; a future phase that needs per-request DNS re-resolution lands `LogicalDns` then. The existing `Static` variant's parse + runtime paths are unchanged (regression-guarded by the new positive `static_cluster_constructs_with_literal_ip` test, which closes phase-02.1 REVIEW I3).
- **Rationale:** `STRICT_DNS` is the simpler, more common case and is mechanically sufficient for the C-1 fix (`host.docker.internal` resolves locally via Docker's `host-gateway` mechanism per ADR-0015, and the resolved address doesn't change during the fixture run, so per-request re-resolution offers no value). `tokio::net::lookup_host` is the chosen resolver primitive because it's part of the existing `tokio` foundation under D-3.2 and requires no new permitted-foundations grant. Deferring `LOGICAL_DNS` follows D-3.6's minimalism principle ("every phase is a green build" — narrow scope = clean acceptance gate). Adding the second `ClusterType` variant unblocks the multi-phase phase-02.1 REVIEW I3 carryforward at zero additional cost (the I3 close-out test is one of D2's 3 unit tests).
- **Consequences:**
  - `crates/envoy-config/src/bootstrap.rs::ClusterType` gains the `StrictDns` variant (~15 LoC including doc comments).
  - `crates/envoy-config/src/lib.rs::ConfigError` gains the `ClusterDnsResolutionFailed { cluster, address, source }` variant (~5 LoC).
  - `crates/envoy-cluster/src/cluster.rs::Cluster::from_bootstrap` gains a `STRICT_DNS` resolution branch via `tokio::net::lookup_host(..).await` (~50 LoC including the per-cluster resolution loop, the zero-result defensive branch, and the `async` promotion if needed).
  - `tests/fixtures/{0003,0004,0005,0006,0008}/{envoy.yaml,envoy-rust.yaml}` flip `type: STATIC` → `type: STRICT_DNS` (~30 LoC YAML diff total across 10 files).
  - `crates/envoy-config/fuzz/corpus/parse_bootstrap/strict_dns_cluster.yaml` is added (1 new seed).
  - **Phase-02.1 REVIEW I3 closes** at this commit (the positive `Static` regression guard `static_cluster_constructs_with_literal_ip` is one of D2's 3 unit tests).
  - **Phase-04.3 REVIEW C-1 closes** at the 05.1 state-4 phase-done verification commit (the 5 affected Docker-gated fixtures pass simultaneously).
  - **Phase-04.1 REVIEW M-claim** (drive_http1 per-function unit test) is unblocked but stays deferred per the 04.3 disposition; carryforward chain continues.
  - `cargo deny check` remains clean: no new top-level Cargo deps; the `tokio` `net` feature (which `lookup_host` requires) may already be activated in the workspace's resolved feature set, and if not, adding it is a feature-flag edit on `crates/envoy-cluster/Cargo.toml`, not a new dep. Cargo.lock sync at state-4 is expected to be a no-op or near-no-op.
  - Future phases that need `LogicalDns` (per-request DNS re-resolution), `dns_refresh_rate` (periodic re-resolution under `StrictDns`), `dns_lookup_family` (A/AAAA selection control), `respect_dns_ttl` (TTL-driven re-resolution), or `dns_resolvers` (custom resolver pool) extend then; this ADR's narrow scope is deliberate.
- **Provenance:** This ADR was projected as the next-sequential available ADR number in parent-05 SPEC §7 (`docs/envoy-rust/phases/05-http2/SPEC.md`, committed at parent-05 state-1 SHA `cd1a70e`); ADR-0022 (parent-05 split decision) lands at parent-05 state-2 alongside the three sub-phase SPECs (mirrors phase-04 state-2 commit `1d9740d`); ADR-0023 lands at this commit (05.1 Task 1). The DECISIONS.md ledger head before this commit is ADR-0022; ADR-0023 lands at the next-sequential number with no renumbering needed (no inter-ADR landings between parent-05 state-2 and this commit). Closes phase-02.1 REVIEW I3 (positive `Static` variant-name regression guard, deferred since phase-02.1 close, rolled forward unchanged through phases 02.2/03.1/03.2/04.1/04.2/04.3). Materially closes phase-04.3 REVIEW C-1 at the 05.1 state-4 phase-done verification commit (the C-1 carryforward chain originated at phase-02.2's ADR-0015 landing `435c6fa`, was discovered at phase-04.3 task 14 commit `eb6f972`, dispositioned at the phase-04.3 STATE.md handoff commit `e626862`, and ends at the 05.1 state-4 verification commit). Phase-04.1 REVIEW M-claim is unblocked but stays deferred.

**No conditional ADRs anticipated for 05.1.** The schema growth and runtime growth are mechanically scoped; no Y/N decision points are projected at execution time. Possible additional ADRs land only if execution proves they're needed (per D-3.5 ambiguity-resolution discipline) — none anticipated.

If a Y/N decision surfaces during execution that isn't covered by ADR-0023 (e.g., a `cargo deny check` flip on a non-anticipated transitive license, or a `dns_lookup_family` parsing requirement forced by a CI flake), the planner appends the next-sequential ADR (likely ADR-0024) at the time it lands.

---

## 8. Artifacts this sub-phase produces

Created during execution (relative to repo root):

- `docs/envoy-rust/phases/05.1-fixture-hardening/PLAN.md` (lands at standalone pre-Task-1 commit per §6 signpost 16)
- `docs/envoy-rust/phases/05.1-fixture-hardening/PROGRESS.md` (per-task progress notes)
- `docs/envoy-rust/phases/05.1-fixture-hardening/REVIEW.md` (state-5 review)
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/strict_dns_cluster.yaml` (new fuzz seed; §3 D1)

Amended during execution:

- `crates/envoy-config/src/bootstrap.rs` — extend the existing `ClusterType` enum from `Static` to `Static | StrictDns`; add ~6 new validator unit tests covering the parse-side surface (parses_cluster_with_type_strict_dns; parses_cluster_with_type_static_unchanged; rejects_cluster_with_type_logical_dns; rejects_cluster_with_unknown_type_value; parses_cluster_with_type_strict_dns_with_multi_endpoint_load_assignment; validates_strict_dns_cluster_does_not_require_literal_ip_endpoints).
- `crates/envoy-config/src/lib.rs` — add the `ConfigError::ClusterDnsResolutionFailed { cluster, address, source }` variant.
- `crates/envoy-cluster/src/cluster.rs` — extend `Cluster::from_bootstrap` (or whichever cluster-manager constructor lives there at task-1 time) with a `STRICT_DNS` resolution branch via `tokio::net::lookup_host(..).await`; promote the function to `async` if not already; add ~3 new unit tests (`static_cluster_constructs_with_literal_ip` — closes phase-02.1 REVIEW I3; `strict_dns_cluster_resolves_localhost_at_build_time`; `strict_dns_cluster_returns_dns_resolution_failed_on_nxdomain`).
- `crates/envoy-cluster/Cargo.toml` — verify (and add if missing) the `net` feature on the existing `tokio` dep. No new top-level dep.
- `crates/envoy-bin/src/main.rs` — if `Cluster::from_bootstrap` is promoted to `async`, add `.await` at the single call site. No other changes.
- `tests/fixtures/0003-tcp-proxy/{envoy.yaml,envoy-rust.yaml}` — flip `type: STATIC` → `type: STRICT_DNS` on the cluster whose endpoints reference `{{BACKEND_HOST}}`.
- `tests/fixtures/0004-tls-downstream/{envoy.yaml,envoy-rust.yaml}` — same flip.
- `tests/fixtures/0005-tls-upstream/{envoy.yaml,envoy-rust.yaml}` — same flip.
- `tests/fixtures/0006-tls-sni/{envoy.yaml,envoy-rust.yaml}` — same flip.
- `tests/fixtures/0008-http1-router-upstream/{envoy.yaml,envoy-rust.yaml}` — same flip.
- `docs/envoy-rust/DECISIONS.md` — append ADR-0023 at Task 1 (per §7 above).
- `docs/envoy-rust/ROADMAP.md` — row `05.1` `status` `in-progress` → `done` at the state-6 phase-done commit. Parent row `05` stays `in-progress` (flips at 05.3's state-6 commit per the ROADMAP-schema invariant).
- `docs/envoy-rust/STATE.md`:
  - At the state-6 phase-done commit: active phase advances from `05.1-fixture-hardening` to `05.2-http2-downstream`; lifecycle state advances to phase 05.2 state 3 (PLAN.md does not exist yet for 05.2; 05.2's SPEC was landed at parent-05 state-2 alongside this SPEC).
  - Next-skill: `superpowers:writing-plans` scoped to sub-phase 05.2.
  - Notes section gains the carryforward bookkeeping per §6 signpost 17 above (C-1 closed; I3 closed; M-claim still deferred).
- `Cargo.lock` — sync at state-4 phase-done gate per the established phase-precedent. Expected to be a no-op or single-line diff (only feature-resolution differences if `tokio`'s `net` feature wasn't already active in the workspace's resolved feature set).
- `deny.toml` — no edits anticipated (no new top-level deps; no new transitive licenses). Cross-checked at state-4.

Not touched in 05.1 (belong to other phases or are frozen):

- `docs/envoy-rust/phases/05-http2/SPEC.md` (parent) — unedited; remains the design artifact committed at parent-05 state-1 SHA `cd1a70e`.
- `docs/envoy-rust/phases/05.2-http2-downstream/SPEC.md`, `docs/envoy-rust/phases/05.3-http2-upstream/SPEC.md` — landed at parent-05 state-2 alongside this SPEC; unedited in 05.1 (their PLAN/PROGRESS/REVIEW land in their own sub-phase execution).
- `docs/envoy-rust/phases/04*` (parent-04 + 04.1 + 04.2 + 04.3) — closed at the 04.3 phase-done commit `e626862`; unedited in 05.1.
- `docs/envoy-rust/phases/{00,01,02,02.1,02.2,03,03.1,03.2}/*` — closed in earlier phases.
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — no edits in 05.1 (per §2 above).
- `crates/envoy-tls/`, `crates/envoy-tcp/`, `crates/envoy-http1/`, `crates/envoy-listener/`, `tests/helpers/{tcp,tls,http1}-echo-server/`, `tests/differential/src/{lib,backend,upstream}.rs` — unchanged. The 5 fixture YAML edits don't require harness changes (the existing `BACKEND_HOST` substitution mechanism per ADR-0015 is unchanged; the harness continues to inject `host.docker.internal` for the upstream-Envoy side and `127.0.0.1` for the envoy-rust side).
- `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0007-http1-direct-response/` — unedited; their fixtures must remain green at the 05.1 state-4 gate (they don't reference `host.docker.internal` per §3 D3 above).
- `tests/fixtures/0009-http2-direct-response/`, `tests/fixtures/0010-http2-router-upstream/` — do not exist at 05.1 close (they land in 05.2 and 05.3 respectively).
- Root `Cargo.toml` — no `[workspace] members` changes (no new crates in 05.1).
- `rust-toolchain.toml`, `docs/envoy-rust/ENVOY_TARGET.md` — frozen per D-3.7 / D-3.9.
- `crates/envoy-config/fuzz/Cargo.toml`, `crates/envoy-config/fuzz/fuzz_targets/` — unchanged. Only the corpus directory grows (1 new seed file).

---

## 9. Final commit message format (for state 6 of the 05.1 lifecycle)

The 05.1 phase-done commit flips ROADMAP row `05.1` `in-progress` → `done`; parent row `05` stays `in-progress` (flips at 05.3's phase-done commit). Format models the 04.x sub-phase shape (e.g., 04.2's `phase 04.2: HTTP route header matchers + ADR-0021 (regex permitted)` title shape; commit `984aedd` for the Task-1 ADR landing pattern):

```
phase 05.1: ClusterType::StrictDns + 5-fixture coordinated edit [ADR-0023]

ClusterType extends from single-variant Static to Static | StrictDns in
crates/envoy-config/src/bootstrap.rs; ADR-0023 lands at Task 1 narrowly
permitting STRICT_DNS for the C-1 fixture-hardening fix and explicitly
deferring LOGICAL_DNS to a later phase. crates/envoy-cluster/src/cluster.rs
gains a STRICT_DNS resolution branch in Cluster::from_bootstrap via
tokio::net::lookup_host (part of the existing tokio foundation under D-3.2;
no new top-level dep). On STRICT_DNS clusters, DNS resolution runs once at
cluster-build time; results cached for the cluster's lifetime per Envoy
v1.33's STRICT_DNS semantics with default dns_refresh_rate (periodic
re-resolution is a §4 non-goal). Resolution failure surfaces as the new
ConfigError::ClusterDnsResolutionFailed { cluster, address, source }
variant. ~6 new envoy-config validator tests + ~3 new envoy-cluster runtime
tests + 1 new fuzz seed (strict_dns_cluster.yaml).

Coordinated 5-fixture YAML edit flips type: STATIC -> type: STRICT_DNS on
the cluster referencing {{BACKEND_HOST}} in tests/fixtures/0003-tcp-proxy,
0004-tls-downstream, 0005-tls-upstream, 0006-tls-sni, and
0008-http1-router-upstream (10 YAMLs total, ~30 LoC YAML diff). Both
envoy.yaml and envoy-rust.yaml flip in lockstep; the existing per-side
substitutions ({{BACKEND_HOST}} -> host.docker.internal for the upstream
Envoy container per ADR-0015's host-gateway grant; 127.0.0.1 for the
envoy-rust host process) are unchanged. Fixtures 0001-tcp-echo,
0002-static-admin-ready, and 0007-http1-direct-response are not edited
(they don't reference host.docker.internal at any cluster).

Closes phase-04.3 REVIEW C-1 (cross-phase Docker-gated host.docker.internal
/STATIC regression latent across phases 02.2 -> 03.1 -> 03.2 -> 04.1 ->
04.2 -> 04.3 since ADR-0015's landing at 435c6fa, discovered at 04.3 task
14 eb6f972, dispositioned at 04.3 STATE.md handoff e626862). Closes
phase-02.1 REVIEW I3 (positive ClusterType::Static variant-name regression
guard, deferred since phase-02.1 close because the single-variant enum had
no second variant against which to discriminate; the new D2 test
static_cluster_constructs_with_literal_ip closes the carryforward).
Phase-04.1 REVIEW M-claim (drive_http1 per-function unit test) is
unblocked by the fixture-mask removal but stays deferred per the 04.3
disposition.

NO HTTP/2 work in 05.1. The envoy-http2 crate, h2 dep, HCM-on-H2 dispatch,
fixture 0009/0010, and h2spec conformance gate all defer to sub-phases
05.2 and 05.3 per ADR-0022 (parent-05 split decision).

Differential surface: tests/fixtures/0001-tcp-echo green (unchanged);
  tests/fixtures/0002-static-admin-ready green (unchanged);
  tests/fixtures/0003-tcp-proxy green (RESTORED — STRICT_DNS flip);
  tests/fixtures/0004-tls-downstream green (RESTORED — STRICT_DNS flip);
  tests/fixtures/0005-tls-upstream green (RESTORED — STRICT_DNS flip);
  tests/fixtures/0006-tls-sni green (RESTORED — STRICT_DNS flip);
  tests/fixtures/0007-http1-direct-response green (unchanged);
  tests/fixtures/0008-http1-router-upstream green (RESTORED — STRICT_DNS
  flip; HTTP/1.1 router-upstream proxy through to http1-echo-server).
Conformance: none (h2spec attaches in 05.2).
```

ROADMAP row `05.1` flips `in-progress` → `done` at this commit. Parent row `05` stays `in-progress` (flips at 05.3's state-6 phase-done commit per the ROADMAP-schema invariant "parent flips to `done` only after all sub-phases are `done`"). STATE.md advances to phase `05.2` lifecycle state 3 (05.2's SPEC was landed at parent-05 state-2 alongside this one); next-skill `superpowers:writing-plans` scoped to sub-phase 05.2 (downstream H2C codec + HCM-on-H2 + fixture 0009 + h2spec ≥95% gate per parent-05 SPEC §3 D5.2–D10.2). Phase-05's projected ADR ledger after this commit: ADR-0022 (parent-05 split decision; landed at parent-05 state-2 commit), ADR-0023 (this sub-phase's Task-1 commit). Future ADRs from 05.2 / 05.3 land at the next-sequential numbers (ADR-0024+).
