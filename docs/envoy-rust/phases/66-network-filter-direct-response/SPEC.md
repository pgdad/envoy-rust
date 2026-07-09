# Phase 66 — Network-filters family opener: `envoy.filters.network.direct_response`

> **Status:** state-1 brainstorm complete (this document). Next: §5 state-2 PLAN-write
> (`superpowers:writing-plans`).
> **Pick + scope locked by ADR-0123** (`docs/envoy-rust/DECISIONS.md`).
> **ROADMAP row:** `66`.
> **Phase directory:** `docs/envoy-rust/phases/66-network-filter-direct-response/`.

This document is written for a stranger with zero prior context (doctrine D-3.4). Every
load-bearing claim below was established by reading the live tree or by driving the pinned
upstream Envoy image at this session's state-0 recon; none is recalled from memory.

---

## §0. State-0 recon — evidence

Two recon tracks ran this session: a code-read of the live tree, and a live-Envoy drive of the
pinned image `envoyproxy/envoy:v1.33.0` (digest
`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`, per
`docs/envoy-rust/ENVOY_TARGET.md`, doctrine D-3.7).

### R-0.1 — The Network-filters family has ZERO ROADMAP rows

`docs/envoy-rust/ROADMAP.md` seeds `### Network filters family` as a heading
("Redis, mongo, kafka_broker, thrift, zookeeper [scope TBD], echo, direct_response,
sni_cluster, rbac network.") with **no rows beneath it**. Phase 66 is that family's first row.

### R-0.2 — envoy-rust models network filters as a free-form name + a CLOSED `@type` enum

- `crates/envoy-config/src/bootstrap.rs:642` — `FilterChain { filters: Vec<NetworkFilter>, .. }`.
- `crates/envoy-config/src/bootstrap.rs:664-670` — `NetworkFilter { name: String, typed_config: Option<TypedConfig> }`.
  `name` is a free-form `String` at PARSE time.
- `crates/envoy-config/src/bootstrap.rs:672-681` — `TypedConfig` is an internally-tagged
  (`#[serde(tag = "@type", deny_unknown_fields)]`) enum accepting exactly **two** `@type` URLs
  today: `...network.tcp_proxy.v3.TcpProxy` and
  `...network.http_connection_manager.v3.HttpConnectionManager`.
- The accepted-name set is enforced at VALIDATE time, `bootstrap.rs:2973-3031`, against three
  consts in `crates/envoy-config/src/lib.rs:45-53`: `ECHO_FILTER`, `TCP_PROXY_FILTER`,
  `HCM_FILTER`. Any other name → `ConfigError::UnsupportedFilter` (`bootstrap.rs:3026-3029`).
  `ECHO_FILTER` must carry **no** `typed_config` (else `UnexpectedTypedConfig`, `bootstrap.rs:2977`).

### R-0.3 — Runtime dispatch is a hardcoded 3-arm match; only `filters.first()` is read

- `crates/envoy-bin/src/main.rs:214-218` reads only the FIRST filter of the FIRST chain
  (`.filter_chains.first().and_then(|c| c.filters.first())`).
- `crates/envoy-bin/src/main.rs:240` — `match filter.name.as_str()` with arms `ECHO_FILTER`
  (→ `echo::serve`), `TCP_PROXY_FILTER`, `HCM_FILTER`.
- There is **no generic network-filter chain iteration protocol**. `crates/envoy-filter/`
  (pipeline.rs / instance.rs / types.rs) models **HTTP filters only** — `HttpFilterInstance`
  (`instance.rs:35`) is an enum whose every variant is an `envoy.filters.http.*` filter; the crate
  contains no `trait` and no network-filter abstraction.
- Consequence, load-bearing for this phase: **trailing network filters are silently ignored at
  dispatch** today (each is name-validated, but only `filters.first()` is ever executed).

### R-0.4 — `echo` IS implemented; the 0001 fixture carries an ADR-0014 YAML shim

`crates/envoy-bin/src/echo.rs` implements the accept loop (`serve()` at `:20`), dispatched at
`main.rs:241-249`. Fixture `tests/fixtures/0001-tcp-echo` drives it (driver `tcp_echo`).
Note the fixture's two sides DIVERGE: `envoy.yaml` gives `echo` a
`typed_config: {"@type": ...network.echo.v3.Echo}`; `envoy-rust.yaml` gives it none — because
upstream Envoy REQUIRES the `typed_config` for echo while envoy-rust forbids it
(`UnexpectedTypedConfig`). This is the pre-existing ADR-0014 YAML-shim posture. **For
`direct_response` no such shim is needed** — both sides will carry the identical `typed_config`.

### R-0.5 — LIVE-ENVOY: `direct_response` behavior is fully deterministic

Booted the pinned image with `docker -p` port-mapping (per the recon gotcha: `--network host`
does not share the host net namespace on this host), listener on `:10000`, config:

```yaml
- name: envoy.filters.network.direct_response
  typed_config:
    "@type": type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config
    response:
      inline_string: "hello-from-direct-response\n"
```

Envoy started clean (`starting main dispatch loop`). Four probes, verbatim results:

```
[no-send]            bytes=b'hello-from-direct-response\n' len=27 eof_seen=True elapsed=0.001s
[delayed-read]       bytes=b'hello-from-direct-response\n' len=27 eof_seen=True elapsed=0.500s
[client-sends-first] bytes=b'hello-from-direct-response\n' len=27 eof_seen=True elapsed=0.001s
[second-conn]        bytes=b'hello-from-direct-response\n' len=27 eof_seen=True elapsed=0.000s
```

**Findings.** On every new connection Envoy writes the configured payload IMMEDIATELY (no wait for
client bytes), then closes the connection with a **clean EOF** (no RST observed, including on the
`client-sends-first` probe where the client had already written unread bytes). The payload is
byte-identical across connections. Client input is entirely ignored. There is **no timing
dependence** — the `delayed-read` probe (client sleeps 500 ms before reading) got the identical
bytes. This is an ideal differential surface: deterministic, byte-exact, no allow-list needed.

### R-0.6 — LIVE-ENVOY: `direct_response` is a TERMINAL network filter — and so are the other three

`--mode validate` against the pinned image, with a second filter placed AFTER the filter under test:

| Config | rc | Envoy's message |
|---|---|---|
| `direct_response` then `echo` | 1 | `terminal filter named envoy.filters.network.direct_response of type envoy.filters.network.direct_response must be the last filter in a network filter chain.` |
| `echo` then `direct_response` | 1 | `terminal filter named envoy.filters.network.echo of type envoy.filters.network.echo must be the last filter in a network filter chain.` |
| `tcp_proxy` then `echo` | 1 | `terminal filter named envoy.filters.network.tcp_proxy of type envoy.filters.network.tcp_proxy must be the last filter in a network filter chain.` |
| `http_connection_manager` then `echo` | 1 | `terminal filter named envoy.filters.network.http_connection_manager of type envoy.filters.network.http_connection_manager must be the last filter in a network filter chain.` |

So **all four** network filters envoy-rust supports are terminal in upstream Envoy. envoy-rust
validates **none** of them (R-0.3: trailing filters are silently ignored). This phase closes that
gap. *(The `echo`-then-`direct_response` probe was initially run with a `typed_config`-less `echo`
and failed with `Didn't find a registered implementation for 'envoy.filters.network.echo' with
type URL: ''` — an unrelated error. It was re-run with the correct `echo` `typed_config` to
produce the row above. Do not confuse the two messages.)*

### R-0.7 — LIVE-ENVOY: `response` is OPTIONAL; `inline_bytes` accepted; empty payload → 0 bytes + clean EOF

`--mode validate` results: `direct_response` with **no `response` field at all** → `rc=0`
(`configuration OK`). `response: { inline_bytes: "aGVsbG8=" }` → `rc=0`.
`response: { inline_string: "" }` → `rc=0`.

Runtime, with the `response` field omitted entirely:

```
bytes=b'' len=0 clean_eof=True
```

So a missing/empty `response` yields a zero-byte write followed by a clean close — not an error.

### R-0.8 — No existing config uses a multi-filter network chain (terminal validation is SAFE)

Mechanically scanned every `tests/fixtures/**/*.yaml` for a `filters:` block containing more than
one `- name: envoy.filters.network.*` entry: **zero hits**. (`tests/fixtures/0006-tls-sni/*.yaml`
and `crates/envoy-bin/tests/tls_sni.rs` each show two network filters against one `filter_chains:`
key, but those are two SEPARATE single-filter SNI chains, not one two-filter chain.) Adding the
terminal rule therefore breaks no existing fixture, test, or config.

### R-0.9 — The differential harness has no read-to-EOF raw-TCP driver

- `Driver` enum: `tests/differential/src/lib.rs:39`. The only raw-TCP variant is `Driver::TcpEcho`
  (`:40`, YAML tag `kind: tcp_echo`), implemented by `drive_tcp()` at `lib.rs:1671-1692`.
- `drive_tcp()` ALWAYS writes a payload (`write_all(payload)`, `:1675`) and then reads **exactly
  `payload.len()` bytes** via `read_exact` (`:1676-1677`) — deliberately not read-to-EOF (see its
  doc comment at `:1653-1670`, citing ADR-0006/ADR-0007: upstream Envoy's *echo* drops queued
  writes on a pre-read client FIN).
- `direct_response` sends a payload of its own choosing and sends nothing back in reply to client
  bytes, so `TcpEcho`'s send-N/read-exactly-N shape does not fit. **A new driver variant is
  required.** Fixture `0001-tcp-echo`'s `expectations.yaml` shows the target shape:
  `driver: {kind: tcp_echo}` + `equivalence.response_body.kind: byte_exact`.

### R-0.10 — Fuzzing: `parse_bootstrap` already covers this surface

Existing fuzz targets: `crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs`,
`envoy-jwt/jwt_parse.rs`, `envoy-filter/cdn_loop_parse.rs`, `envoy-accesslog/accesslog_format_parse.rs`.
Wired into `.github/workflows/ci.yml:77-124` (a `fuzz` job, `-max_total_time=30` each).
`parse_bootstrap` fuzzes bootstrap-config parsing, which is exactly and only where this phase adds
a parser surface (the new `TypedConfig` variant). See §2.3 for the §7.4 disposition.

### R-0.11 — Numbering

Next free fixture id is **`0071`** (`tests/fixtures/` currently tops out at `0070`).
`DECISIONS.md` heading ledger head is **ADR-0122** (`grep -oE '^## ADR-[0-9]+'` → max `0122`;
`ADR-0123` appears in no heading anywhere in the tree). Phase 66 claims **ADR-0123**, reclaiming
the reservation phase 65 left unfired, per the standing lapsed-reservation convention.

---

## §1. Goal

Open the **Network filters family** by implementing `envoy.filters.network.direct_response`, and
differentially witness its byte-exact raw-TCP output against upstream Envoy via a new fixture
`0071`. In the same phase, close the network-filter **terminal-validation** gap that R-0.6 and
R-0.3 expose: all four supported network filters are terminal upstream, and envoy-rust today
silently ignores any filter after the first.

This is the project's first phase in ~24 phases (42→65 were all access-log
`%RESPONSE_FLAGS%` / `%RESPONSE_CODE_DETAILS%` witnesses) to advance a NEW feature family, and it
does so with a deterministic, timing-free, byte-exact cross-proxy witness.

---

## §2. Scope

### 2.1 In scope

**(A) Config surface — `crates/envoy-config/`**

1. New const `DIRECT_RESPONSE_FILTER = "envoy.filters.network.direct_response"` in
   `src/lib.rs` alongside `ECHO_FILTER` / `TCP_PROXY_FILTER` / `HCM_FILTER` (`lib.rs:45-53`).
2. New `TypedConfig::DirectResponse(DirectResponseConfig)` variant (`bootstrap.rs:672-681`) keyed
   on `@type` = `type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config`.
3. New `DirectResponseConfig { response: Option<DataSourceInline> }`, `#[serde(deny_unknown_fields)]`.
   - `response` is `Option` because Envoy accepts its omission (R-0.7). `None` ⇒ empty payload.
   - `DataSourceInline` (`bootstrap.rs:790`) is the existing `{ inline_string: String }` struct,
     already `deny_unknown_fields`. Reusing it means `filename` / `inline_bytes` are rejected
     LOUDLY by serde as unknown fields — the deliberate fail-loud divergence in §2.2/§6.
4. `validate()` (`bootstrap.rs:2973-3031`) gains a `DIRECT_RESPONSE_FILTER` arm requiring a
   `TypedConfig::DirectResponse` (a missing/other `typed_config` is a `ConfigError`).
5. **Network-filter terminal validation (new).** A new `ConfigError::NetworkFilterNotTerminal`
   variant. Every one of the four supported network filters is terminal (R-0.6), so the rule is:
   *a terminal network filter must be the last filter in its chain.* Since envoy-rust supports
   only terminal network filters today, this makes any chain with ≥2 filters invalid — safe per
   R-0.8 (zero existing configs affected). Implemented as a per-name `is_terminal` predicate rather
   than a hardcoded "len ≤ 1" check, so a future non-terminal filter (e.g. `rbac` network,
   `sni_cluster`) drops in without re-litigating the rule.

**(B) Data plane — `crates/envoy-bin/`**

6. New `crates/envoy-bin/src/direct_response.rs`, mirroring the shape of `echo.rs` (a standalone
   `pub async fn serve(listener, payload, shutdown)` accept loop — echo is NOT a
   `ConnectionHandler` impl; `ConnectionHandler` (`envoy-listener/src/lib.rs:38`) is used by the
   tcp_proxy/HCM arms). Per connection: write the payload, then close with a clean EOF, never
   reading from the client. See PLAN-VERIFY V-3 for the unread-data/RST hazard.
7. A fourth arm in the `match filter.name.as_str()` at `main.rs:240` dispatching
   `DIRECT_RESPONSE_FILTER` → `direct_response::serve`.

**(C) Differential surface**

8. New `Driver::TcpDirectResponse` variant (`tests/differential/src/lib.rs:39`), YAML tag
   `kind: tcp_direct_response`: connect; send nothing; read to EOF (bounded by a timeout); return
   the bytes. Cross-proxy equality is asserted by the existing
   `equivalence.response_body.kind: byte_exact` machinery — the driver compares the two proxies to
   each other, never to a golden file.
9. New fixture `tests/fixtures/0071-network-filter-direct-response/` with `envoy.yaml`,
   `envoy-rust.yaml` (identical modulo the `{{PORT}}` substitution — no ADR-0014 shim needed, per
   R-0.4), `expectations.yaml`, and `README.md`.
10. New `tests/differential/tests/<name>.rs` test invoking `differential::run_fixture`.

**(D) In-process backstops — `crates/envoy-bin/tests/`**

11. A new integration test booting `envoy-bin` and asserting: payload written + clean EOF; the
    empty-`response` zero-byte case (R-0.7); client-sends-first still receives the payload (R-0.5).
12. Negative config tests: a trailing filter after a terminal one is REJECTED
    (`NetworkFilterNotTerminal`); `response: {filename: ...}` and `response: {inline_bytes: ...}`
    are REJECTED (unknown field).

**(E) Documentation**

13. `BEHAVIOR_CONTRACT.md` rows for the `direct_response` semantics, the terminal-filter rule, and
    the two recorded divergences (§6).
14. Fuzz corpus seed for the new `typed_config` shape (§2.3).

### 2.2 Out of scope (deliberate, with rationale)

- **`inline_bytes` and `filename` `DataSource` arms.** Envoy accepts both (R-0.7). envoy-rust will
  reject them loudly. Rationale: the fail-loud posture set by ADR-0049 decision 2→(b) (all config
  load errors fatal; `deny_unknown_fields` everywhere), and the existing `DataSourceInline` doc
  comment (`bootstrap.rs:785-791`) which already scopes that struct to `inline_string` only. A
  divergence with **no differential observable** (the fixture uses `inline_string`). Recorded in
  BEHAVIOR_CONTRACT, not silent (D-3.3). Carried forward as **CF-66-1**.
- **A generic network-filter chain iteration protocol.** R-0.3 shows none exists. Since all four
  supported filters are terminal, a chain is always exactly one executable filter, so iteration
  buys nothing today. Deferred to the first non-terminal network filter (`sni_cluster`, network
  `rbac`). Carried forward as **CF-66-2**.
- **Any other network filter** (echo already exists; redis/mongo/kafka/thrift/zookeeper/
  sni_cluster/rbac-network remain family headings).
- **The `filters.first()` → full-chain dispatch refactor** at `main.rs:214-218`. Terminal
  validation makes `first()` provably equal to "the only executable filter", so the refactor is
  unnecessary; it becomes necessary only alongside CF-66-2.

### 2.3 §7.4 fuzz disposition

Doctrine §7.4: *"Every phase that introduces a parser, codec, or filter ships a `cargo fuzz` target
under the relevant crate's `fuzz/` subdirectory."*

This phase introduces a **filter**, but one that **parses nothing**: `direct_response` never reads a
byte from the downstream socket (R-0.5 — it writes and closes, ignoring client input). Its only
untrusted-input surface is the **bootstrap config parser**, which is already covered by the existing
`parse_bootstrap` fuzz target (R-0.10) — the new `TypedConfig::DirectResponse` variant is reachable
from it the moment the variant lands, with no target change.

**Decision: no new fuzz target.** Instead, add a corpus seed exercising the new `typed_config`
shape to `crates/envoy-config/fuzz/corpus/parse_bootstrap/`. Two mechanical traps, both
previously-learned, that the PLAN must honor:

- the fuzz corpus directory is `*`-ignored, so the new seed needs an explicit `!`-un-ignore line in
  the fuzz `.gitignore` or it is silently untracked and invisible to CI (**verify with
  `git ls-files`**);
- a NEW fuzz target would need a hand-written `ci.yml` step (`.github/workflows/ci.yml:77-124`) —
  not applicable here, since no new target is added, but the §7.5(d) gate must be explicitly
  recorded as "satisfied by the pre-existing `parse_bootstrap` target."

This disposition is a §7.4 interpretation and is therefore recorded in **ADR-0123**, not decided
silently.

---

## §3. PLAN-VERIFY items (re-confirm against the live tree at the state-2 PLAN-write)

Every line number in this SPEC was read this session, but the state-2 PLAN-write MUST re-confirm
them fresh (line drift is routine) and MUST resolve the following open questions:

- **V-1.** Exact `TypedConfig` enum shape at `bootstrap.rs:672-681` and the precise serde attribute
  set needed for the new variant to coexist with the internally-tagged `@type` discriminator.
- **V-2.** Exact `validate()` network-filter loop shape at `bootstrap.rs:2973-3031`, and the
  `ConfigError` enum's location + naming convention (this SPEC's R-0.2 cites `bootstrap.rs`;
  `crates/envoy-config/src/error.rs` does **not** exist — locate the real definition site).
- **V-3. (Load-bearing implementation risk.)** The **unread-data RST hazard.** On Linux, closing a
  socket with unread bytes in its receive queue sends an RST, which can destroy the payload the
  peer has not yet read. Upstream Envoy closes with `FlushWrite` and our `client-sends-first` probe
  (R-0.5) saw a clean EOF and a complete payload. envoy-rust's naive
  `write_all(payload).await; shutdown().await; drop(stream)` may RST where Envoy does not.
  The PLAN must settle this empirically (drain-and-discard the read half? linger? `shutdown()`
  then read-to-EOF before drop?) and pin the chosen behavior with a test that writes client bytes
  BEFORE reading, mirroring the `client-sends-first` probe. **Do not assume the naive path works.**
- **V-4.** Whether `Driver::TcpDirectResponse` should send nothing at all, or send an optional
  fixture-supplied payload (to exercise the R-0.5 `client-sends-first` path differentially rather
  than only in-process). Recommendation: send nothing in `0071` (keeps the driver minimal); cover
  client-sends-first in the in-process backstop only. Decide at PLAN time.
- **V-5.** The read-to-EOF timeout value and its interaction with the harness's existing
  settle/poll conventions (`drive_tcp`'s 100 ms trailing-byte poll at `lib.rs:1682-1687` is the
  precedent for proving "no more bytes are coming").
- **V-6.** Whether `expectations.yaml` needs any allow-list at all. Projected: **no** — the
  response body is byte-exact and there are no headers, no timing, no stats assertions.
- **V-7.** The `is_terminal` predicate's home (envoy-config) and whether the `ConfigError` variant
  should carry the offending filter's name, its index, and the chain length.
- **V-8.** Confirm the fuzz-corpus `.gitignore` un-ignore line lands and `git ls-files` shows the
  new seed (R-0.10 / §2.3).

---

## §4. Rejected / deferred alternatives (the options this pick was chosen over)

1. **xDS file-based CDS hot-reload.** The heaviest of the three hot-reload layers deferred by
   ADR-0065 and re-deferred by ADR-0067. Recon: the phase-26/27 watcher core
   (`crates/envoy-cluster/src/xds_watch.rs`, `XdsFileWatcher`) IS generic and reusable, but
   `ClusterManager { clusters: HashMap<String, Arc<Cluster>> }` (`cluster.rs:908-910`) is a plain
   immutable map — **not** behind `ArcSwap`/`RwLock` — shared as `Arc<ClusterManager>` to the pool
   managers, health-check scheduler, and outlier manager. Unlike EDS (which swaps a
   `RwLock<Arc<Vec<SocketAddr>>>` *inside* an existing cluster, `cluster.rs:131`) CDS has no
   map-level swap primitive. Real cost is cluster-lifecycle churn: connection-pool spawn/teardown,
   health-check probe-task lifecycle, outlier sweeper lifecycle, in-flight-request safety on a
   removed cluster. Estimated 800–1200 LoC with a near-certain §6.1 split. **Deferred — a strong
   pick for a later multi-sub-phase arc, a bad pick for a single phase.**
2. **LDS hot-reload.** Heavier still: no listener registry exists (`main.rs:210` serves only the
   FIRST listener), sockets are bound once in `main()`, and an update implies rebind + drain.
3. **The cheap carry-forward leaves — M64-2** (stale `crates/envoy-http2/src/hcm.rs:236` comment
   naming the removed `synth_h2_502`), **M57-1** (`content-length` omission on
   `synth_h2_no_healthy_upstream()`), **M53-2** (a BEHAVIOR_CONTRACT "(H1)" qualifier), **M64-3**
   (idle-probe-connection inefficiency in the H2 test backends). All are cosmetic or doc-only and
   light up **no differential surface**. A phase whose entire content is doc polish contradicts the
   pattern of phases 53/54/64/65, each of which added a real cross-proxy witness. **Not consumed by
   this phase** — none of them touches the network-filter / config-validator / raw-TCP-driver
   surface this phase edits, so folding them in would be scope creep (they stay LIVE).
4. **The non-deterministic LB policies (`least_request` / `random`).** Require a
   contract-relaxation ADR FIRST (the differential contract demands exact equality on deterministic
   flows). Unchanged from every prior consideration.
5. **The `DC` downstream-disconnect `%RESPONSE_FLAGS%` value.** Timing-dependent and hard to drive
   deterministically; **stays REJECTED**, as at every consideration through ADR-0121. This phase's
   recon surfaced no new information that changes that assessment.
6. **`envoy.filters.network.echo` as the family opener.** Already implemented (R-0.4) and already
   differentially witnessed by fixture `0001`. Nothing to open.

---

## §5. Differential surface at phase end

- **NEW fixture `0071-network-filter-direct-response`** — a raw-TCP connect → read-to-EOF probe
  against a `direct_response` listener on BOTH proxies; the response body is asserted
  **byte-exact** cross-proxy, and both sides must present a clean EOF.
- **NEW driver `Driver::TcpDirectResponse`** (`kind: tcp_direct_response`) — the harness's first
  read-to-EOF raw-TCP driver (R-0.9).
- All pre-existing fixtures stay green (§7.5(b)). Terminal validation touches no existing config
  (R-0.8).
- Conformance: unchanged. `h2spec` remains the only §7.3 suite in the tree; its pass-rate gate must
  stay green. **Never trim `known-failures.txt`** to make a local run green — local h2spec scores
  invalid-preface 3.5/2 as PASS while CI fails it, so a locally-"fixed" list breaks CI.

---

## §6. `BEHAVIOR_CONTRACT.md` additions

1. **`envoy.filters.network.direct_response` semantics.** On each accepted downstream connection the
   filter writes the configured `response` payload immediately — without reading or waiting for any
   client bytes — then closes the connection with a clean EOF (no RST). A missing or empty
   `response` yields a zero-byte write followed by a clean close. Output is byte-identical across
   connections and independent of client input and of client read timing. *(Witnessed live against
   `envoyproxy/envoy:v1.33.0` at the phase-66 state-0 recon; see SPEC §0 R-0.5 and R-0.7.)*
2. **Network-filter terminal rule (bilateral).** All four network filters envoy-rust supports —
   `echo`, `tcp_proxy`, `http_connection_manager`, `direct_response` — are TERMINAL: each must be
   the last filter in its chain, and upstream Envoy rejects a config that places any of them
   before another network filter. envoy-rust now enforces the identical rule
   (`ConfigError::NetworkFilterNotTerminal`), where previously it silently ignored every filter
   after the first. *(Witnessed live; see SPEC §0 R-0.6.)*
3. **Recorded divergence — `DataSource` arms.** Upstream Envoy accepts `response.inline_bytes` and
   `response.filename`; envoy-rust accepts only `response.inline_string` and rejects the other arms
   loudly at config load (serde `deny_unknown_fields`). Deliberate, per the ADR-0049 decision-2→(b)
   fail-loud posture. No differential observable (fixture `0071` uses `inline_string`).
   Carry-forward **CF-66-1**.
4. **Recorded scope note — `echo` `typed_config` asymmetry (pre-existing, unchanged).** Upstream
   Envoy REQUIRES `typed_config` on `envoy.filters.network.echo`; envoy-rust forbids it
   (`UnexpectedTypedConfig`). Fixture `0001`'s two sides differ accordingly (ADR-0014 YAML shim).
   `direct_response` introduces **no** such asymmetry — both sides of fixture `0071` are identical.

---

## §7. ADR reservations

- **ADR-0123 — FIRED this session.** Phase-66 pick + scope (this SPEC), including the §7.4
  no-new-fuzz-target disposition (§2.3) and the §2.2 out-of-scope divergences. Reclaims the
  reservation phase 65 left unfired.
- **ADR-0124 — RESERVED, unfired.** To fire at the state-2 PLAN-write if either (a) the §6.1 split
  gate trips (projected NOT to: ~650 net LoC / ~9 tasks, well under the ~1500 LoC / ~25 task
  thresholds), or (b) a §6.2 empirical-verification reconciliation overturns any §0 recon finding
  (R-0.1 … R-0.11) — most plausibly V-3 (the RST hazard) forcing a behavior-contract amendment. If
  neither fires, ADR-0124 lapses and is reclaimed by the next new-phase pick, per the standing
  lapsed-reservation convention.

---

## §8. Estimated size (for the §6.1 split gate at state-2)

| Area | Net LoC (est.) |
|---|---|
| `envoy-config`: const, `TypedConfig` variant, `DirectResponseConfig`, validate arm | ~80 |
| `envoy-config`: `is_terminal` predicate + `NetworkFilterNotTerminal` + unit tests | ~90 |
| `envoy-bin/src/direct_response.rs` + unit tests | ~140 |
| `envoy-bin/src/main.rs` dispatch arm | ~30 |
| `tests/differential`: `Driver::TcpDirectResponse` + `drive_tcp_direct_response` | ~90 |
| fixture `0071` (4 files) + differential test | ~70 |
| `envoy-bin/tests/` in-process backstop + negative config tests | ~130 |
| BEHAVIOR_CONTRACT rows + fuzz corpus seed | ~20 |
| **Total** | **~650** |

~9 TDD tasks projected. **§6.1 split does NOT fire** (thresholds: >~25 tasks OR >~1500 LoC).
