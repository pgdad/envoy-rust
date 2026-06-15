# Phase 26 (`26-xds-rds-hot-reload`) — SPEC

- **Phase id:** `26`
- **Slug:** `26-xds-rds-hot-reload`
- **Status before this SPEC lands:** _not yet in ROADMAP.md_ (per `docs/envoy-rust/ROADMAP.md` at HEAD `28e03a831`, the phase-25.2 state-6 deterministic close-out commit; the sequential rows END at `25.2` and the "xDS / dynamic config family" §9 table carries rows `18`–`21`, all `done`). **This SPEC's landing commit adds the FIRST hot-reload row beneath the "xDS / dynamic config family" heading** (the family's fifth concrete row, after 18 CDS / 19 LDS / 20 RDS / 21 EDS), with `status: planned`. It is the **first phase that mutates a running manager's state post-construction** — every prior xDS phase loaded dynamic resources ONCE at startup and kept the managers immutable thereafter (ADR-0048 §5.3 / ADR-0050 / ADR-0051 / ADR-0053).
- **Charter source:** `BOOTSTRAP_PROMPT.md` §9 — *"xDS / dynamic config family — ADS, delta xDS, LDS, CDS, RDS, EDS, SDS, RTDS, reconnection, initial-fetch timeout."* Every xDS phase (18/19/20/21) named **file watching / hot reload** "the family's prime follow-up" with explicitly-increasing ROI ("one watching phase now lights up CDS+LDS+RDS+EDS"). This phase lands that follow-up at its minimum-viable increment: **hot-reload of file-based RDS route configurations** — a running HCM whose route table is RDS-supplied picks up an edited RDS file WITHOUT a restart, re-routing live traffic, observable via the per-HCM `http.<stat_prefix>.rds.<route_config_name>.{update_*,config_reload}` counters ticking PAST their initial-load values and the `RoutesConfigDump` reflecting the new version.
- **Position in the project:** the **fourteenth post-MVP-trunk feature-family phase** and the **fifth concrete xDS-family phase**. The MVP trunk 00→08, the eight HTTP-filter-family phases (07.2/09/10/11/22/23/24/25.2), the six Upstream-robustness-family phases (12–17), and the four xDS-family phases (18 CDS, 19 LDS, 20 RDS, 21 EDS) all stand `done`. The **33-Docker-gated-fixture regression baseline** established at phase-25.2 close (`0001-tcp-echo` through `0033-http-filter-buffer`) carries forward unchanged per `BOOTSTRAP_PROMPT.md` §7.5 (b) — **including fixture `0028-xds-file-based-rds`, whose envoy-rust side now gains an idle file-watcher (the critical regression-sensitivity witness — §5.2)**.
- **depends-on:** `01 04 06 08 12 20` — phase `01` (the `envoy-config` bootstrap loader + `rds.rs` RDS-file parser this phase re-invokes on reload), phase `04` (the HCM + `RouteConfiguration` + router whose route table this phase makes hot-swappable), phase `06` (the `envoy-stats` foundation the `rds.*` counters register against — they already exist from phase 20; this phase makes them tick per reload), phase `08` (the admin `/config_dump` `RoutesConfigDump` section whose version/`last_updated` this phase updates on reload), phase `12` (the `envoy-health::Scheduler` periodic-background-task primitive + its `CancellationToken` shutdown discipline — the watcher is its FIFTH instance), and phase `20` (the file-based RDS load path, the per-HCM `rds.*` stat topology, the `RoutesConfigDump` entry, and fixture `0028` — ALL extended in place by this phase).
- **Brainstorm narrative:** see the "Phase-26 state-1 brainstorm" subsection of `docs/envoy-rust/STATE.md` for the family-pivot rationale and the alternatives weighed (the HTTP-filters family — the light/deterministic vein EXHAUSTED at buffer, every remaining member external-service / engine / timing / byte-fragile; the LB-family opener — foundational but needs a new distinguishable-backend harness, deferred at the phase-25 brainstorm as "larger/riskier"; EDS/CDS/LDS hot-reload first — harder live mutations [endpoint-pool / cluster-lifecycle / socket-bind-drain] than the stateless route-table swap; the gRPC/ADS transport — still protos-blocked under ADR-0014; the Network-filters opener — low leverage). The scoping decision is ratified in **ADR-0065** (landed at this brainstorm commit).

---

## 0. Critical scoping findings (READ FIRST) — RDS hot-reload is the cleanest live mutation; the route table is already an `Arc` and the periodic-task primitive already exists

The state-1 brainstorm identified five findings that make RDS hot-reload a **single, bounded, deterministic phase** — the first post-construction live mutation in the project, but the LEAST invasive one:

1. **The route table is already held as a swappable `Arc` — the live-mutation seam is a single atomic pointer swap, and route matching is per-request stateless.** `HCMConfig.route_config: Arc<RouteConfiguration>` (`crates/envoy-http1/src/hcm.rs:122`) is built once at `from_config` (`hcm.rs:209` via `clone_route_config`) and read on every request (`hcm.rs:1177` vhost match; the H2 mirror). Nothing else holds per-request route state — matching is a pure function of `(route_config, request)`. So hot-reload = replace the `Arc` the HCM reads. The ONLY structural change: the owned `Arc<RouteConfiguration>` becomes a **shared swappable handle** that BOTH the HCM (reads per request) and the watcher (writes on reload) hold. **No drain, no socket churn, no pool/health/outlier lifecycle** — the reasons CDS/LDS/EDS hot-reload are deferred (§4). This is why RDS is the correct FIRST hot-reload target.

2. **The periodic-background-task primitive already exists and has four instances — the watcher is the fifth, built to the established `CancellationToken` discipline.** `envoy-health::Scheduler::spawn(...)` (`crates/envoy-health/src/scheduler.rs`) is the template: it holds the `JoinHandle`s of its spawned loops + a `CancellationToken`, and `envoy-bin` shuts it down via the shared token (`crates/envoy-bin/src/main.rs:91,180`). The four existing periodic primitives (12.2 active-HC scheduler, 13.1 H1-pool idle sweeper, 13.2 H2-pool idle sweeper, 14.2 outlier-ejection sweeper) share this exact shape. The RDS watcher is a sibling: an **`RdsWatcher` (or `XdsFileWatcher`) periodic task** that polls the configured RDS file's mtime per interval and, on change, runs the reload pipeline (finding 3). **Poll-based, not inotify** — see finding 4.

3. **The reload pipeline reuses the phase-20 RDS load path verbatim; only the swap + the stat-tick-on-reload + the config_dump-version-update are new.** On a detected file change the watcher: re-reads the file → re-parses via the existing `rds.rs` envelope parser → re-selects the `RouteConfiguration` by `route_config_name` → re-validates its route→cluster references against the (immutable) live cluster set → **atomically swaps** the HCM's route-table handle. The phase-20 `http.<stat_prefix>.rds.<route_config_name>.{update_attempt,update_success,update_failure,update_rejected,config_reload}` counters — which today tick ONCE at initial load — now tick **per reload attempt** (`update_attempt`/`config_reload` on each apply; `update_failure`/`update_rejected` on a bad reload, with the old table KEPT — the one place envoy-rust does NOT go all-fatal, because the proxy is already serving traffic; §5.5). The `RoutesConfigDump` `version_info`/`last_updated` update on each successful apply.

4. **Poll-based watching is dep-free, deterministic, and behavior-equivalent to Envoy's inotify POST-SETTLE — only the post-reload data plane / stats / config_dump are differentially asserted, never the watch mechanism.** envoy-rust adds NO filesystem-watch dependency (`notify`/inotify/kqueue are platform-specific and would face `cargo deny` review): the watcher polls `path` mtime on an interval, exactly as the four existing periodic primitives poll. Envoy uses inotify (near-instant); envoy-rust polls (interval-bounded). Both CONVERGE; the fixture asserts post-settle state via the **settle-then-probe driver** (the 12.2 active-HC pattern: mutate → wait for convergence → probe). The watch-MECHANISM divergence is a recorded BEHAVIOR_CONTRACT note in the established "behavior-equivalent, mechanism may differ" style (ADR-0049's all-fatal-config divergence precedent). **Consequence (carried from ADR-0049 Provenance + the phase-21 EDS note): this phase's differential §6.2 MUST run on Linux CI** — macOS Docker Desktop's virtiofs limits filesystem-change observability inside the Envoy container, so the reload trigger is unobservable locally. Local verification is the in-process backstop (`tokio::time` is controllable) + the §6.2 wire-shape probes; the AUTHORITATIVE differential anchor is the Linux CI run (ADR-0049).

5. **Minimum-viable hot-reload adds ZERO new config schema — it is pure runtime behavior on the EXISTING `rds.config_source.path_config_source.path`.** Envoy's file-based xDS is **always-watching by default** (no opt-in field); envoy-rust matches by spawning a watcher for EVERY `rds`-configured HCM. So there is no new `ConfigError`, no new parse surface, and (projected) no new fuzz seed — UNLESS §6.2 reveals Envoy requires `watched_directory` (`ConfigSource.watched_directory`, currently rejected by `deny_unknown_fields`) to reload the fixture's chosen file-change operation (atomic-rename vs in-place rewrite — §6.2 item 2). If so, this phase adds the `watched_directory` parse-and-honor field (+ one fuzz seed). The SPEC projects the no-schema-change path (poll `path`, in-place-rewrite reload) and defers the `watched_directory` question to the §6.2 empirical verification.

**Consequence:** phase 26 needs **NO new crate** (the watcher lives in `envoy-http1` beside the HCM, or a small `envoy-config`/`envoy-bin` module — a PLAN-write call), **NO new top-level Cargo dep** (poll-based; the swappable handle uses `std::sync::RwLock<Arc<…>>` or `tokio::sync::watch` — both already available — rather than a new `arc-swap` dep, decided at PLAN-write against §5.1), and **NO new harness driver for the data plane** (reuses `Driver::Http1KeepAlive` + the 12.2 settle-then-probe pattern) — but it DOES need **one genuinely-new harness capability: mutating a mounted fixture file mid-test, then settling** (§D6). Projected surface ~1200–1600 LoC — single phase, with the §6.1 split valve reserved (the live-mutation seam + watcher primitive as a foundation slice / the reload fixture + close as the consumer).

These findings are ratified in **ADR-0065** (landed at this brainstorm commit).

---

## 1. Goal and acceptance signal

Phase 26 makes **file-based RDS route configurations hot-reloadable**. When an HCM configures `rds.config_source.path_config_source.path` and that file is edited while the proxy is running, both upstream Envoy and envoy-rust:

- **detect the change and re-load the named `RouteConfiguration`** without a restart and without dropping the listener,
- **atomically apply the new route table to live traffic** (subsequent requests match the new routes; in-flight requests are unaffected),
- **expose the reload observably**: the per-HCM `http.<stat_prefix>.rds.<route_config_name>.{update_attempt,update_success,config_reload}` counters advance past their initial-load values, and `/config_dump`'s `RoutesConfigDump` reflects the new route table (+ updated `version_info`/`last_updated`),
- **on a bad reload (malformed file / unresolved route), keep serving the last-good table** and tick `update_failure`/`update_rejected` (the proxy-already-serving warm-reject posture — §5.5).

**Differential surface added by phase 26:**

- **Fixture `0034-xds-rds-hot-reload`** — an HCM whose route table is RDS-supplied, reloaded mid-test, bilaterally asserted. Both proxies receive identical bootstraps: one static listener (HTTP/1.1 HCM, `stat_prefix: ingress_http`, `rds: { route_config_name: local_route, config_source: { path_config_source: { path: <RDS_PATH> } } }`, NO inline `route_config`) + two static clusters (`backend_a`, `backend_b`, each a distinguishable real `http1-echo-server` — distinguishable by a per-cluster marker the echo reflects, OR by the per-cluster `upstream_rq_total` discriminator). The test runs in **three phases against the running proxies**:
  1. **Pre-reload (initial load):** `GET /probe` → **200** routed through `backend_a` (the initial RDS file routes `/probe` → `backend_a`); assert `http.ingress_http.rds.local_route.{update_attempt,update_success,config_reload}` at their initial-load values (§6.2-verified, projected `1/1/1`).
  2. **Reload:** the harness **rewrites the mounted RDS file** so `/probe` → `backend_b`, then **settles** (the 12.2 wait-for-convergence pattern, bounded poll on a discriminating observable — the routed-to cluster OR the `config_reload` counter advancing).
  3. **Post-reload:** `GET /probe` → **200** routed through `backend_b` (the discriminating observable: the routed-to cluster CHANGED without a restart); assert `…update_attempt/update_success/config_reload` ADVANCED (projected `2/2/2`); assert `/config_dump` `RoutesConfigDump` now reflects the `backend_b` route. Plus a **bad-reload probe** (rewrite the RDS file to malformed content → settle → `GET /probe` still routes to `backend_b` [last-good table kept] + `…update_failure` or `…update_rejected` ticked — §6.2 item 5 fixes which).

  The discriminating differential observable is the **route-table change taking effect on live traffic without a restart** + the **reload counters advancing** — a proxy that ignored the file edit would keep routing to `backend_a` and leave the counters at their initial-load values. Probe shape, exact stat names/values on reload, the bad-reload disposition, and the config_dump-version shape are §6.2-verified (§6.2 items 1–6).

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0034-xds-rds-hot-reload` green at Docker-gated **Linux CI** (the AUTHORITATIVE anchor per ADR-0049 — this phase's reload trigger is unobservable on macOS Docker; §0 finding 4).
- **(b)** All **33 pre-existing differential fixtures** (`0001` through `0033`) **remain green simultaneously** at the same CI run (regression-equivalence per §7.5 (b)). The watcher is inert when no HCM configures `rds`; for the ONE existing RDS fixture (`0028-xds-file-based-rds`), the envoy-rust side now spawns an **idle watcher** (the file is never edited mid-test) — its `rds.*` counters stay at their initial-load values and its data plane is unchanged, so `0028` stays green (the critical regression-sensitivity witness — §5.2). The route-table-handle migration (`Arc` → swappable handle) is behavior-preserving for every non-reloading request.
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline). Phase 26 does not touch HTTP framing.
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run (projected NO new seed — no new parse surface per §0 finding 5; a `watched_directory` seed lands ONLY if §6.2 item 2 forces the schema field).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` (run PER TASK in the state-3 arc, per `project_state3_arc_skips_clippy`), `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean — plus the standalone-crate builds (`-p envoy-config -p envoy-http1 -p envoy-http2` + the watcher's crate) per `project_isolated_crate_build_blindspot`.
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a) through (e) **simultaneously** (continues the project precedent).

> **NOTE — single phase projected (see §6.1).** Phase 26's surface (the route-table-handle migration + the watcher primitive + the reload pipeline + the stat-tick-on-reload + the config_dump-version update + the mid-test-file-rewrite harness capability + fixture 0034 + in-process backstop + BEHAVIOR_CONTRACT rows) is projected at **~1200–1600 LoC / ~10–13 tasks**. If the §6.2-refined estimate fires the §6.1 gate, the recommended split seam: **`26.1`** (the route-table-handle migration + the watcher primitive + the reload pipeline + in-process backstop — the foundation slice; regression-equivalence acceptance = all 33 existing fixtures green incl. 0028's idle watcher) / **`26.2`** (the stat-tick-on-reload + config_dump version + mid-test-rewrite harness + fixture 0034 + parent-26 close). The split ADR would be ADR-0067 (§7).

---

## 2. Behavior-contract scope for phase 26

Phase 26 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with authored additions, landed at the tasks where each is first empirically exercised (the established 06.x→21 doctrine — contract extensions land at empirical-engagement task time, NOT at PLAN-write time and NOT at state-1 SPEC time).

### 2.1 "Stat-name mapping" extension — RDS reload semantics (the phase-20 rows gain reload behavior)

The phase-20 `http.<stat_prefix>.rds.<route_config_name>.*` rows (`update_attempt`/`update_success`/`update_failure`/`update_rejected`/`config_reload`) gain a **reload-semantics column**: at phase 20 they were locked at their initial-load values (`1/1/0/0/1`); phase 26 records that each **advances per reload** — `update_attempt`+`config_reload` per apply, `update_success` per successful apply, `update_failure`/`update_rejected` per bad apply (the §6.2-item-5-verified split). No NEW stat names (the phase-20 subset is reused); the change is the documented per-reload increment semantics + the value assertions in fixture 0034 (projected `2/2/…` after one successful reload). The §5.2 conditional-registration invariant is unchanged (names register only for `rds`-configured HCMs).

### 2.2 "xDS wire state machine" section — the filesystem-transport HOT-RELOAD subsection (new)

The BEHAVIOR_CONTRACT's "Filesystem transport (`path_config_source`)" subsection (populated at phases 18–21 for initial-load) gains a **hot-reload** block: (a) the watch trigger (Envoy = inotify-on-file [or directory-move with `watched_directory`]; envoy-rust = interval poll on mtime — the recorded mechanism divergence, §0 finding 4); (b) the file-change operation that triggers a deterministic reload on BOTH proxies (§6.2 item 2 — in-place truncate-rewrite vs atomic-rename; whether `watched_directory` is required); (c) the atomic-apply + last-good-retention-on-bad-reload semantics (§6.2 item 5); (d) the reload-counter advancement (§2.1); (e) the `RoutesConfigDump` `version_info`/`last_updated` update on reload (§6.2 item 6); (f) the in-flight-request isolation guarantee (a request that began under the old table completes under it).

### 2.3 DECISIONS.md amendment at SPEC time — ADR-0065 (the scoping ADR)

Like phases 18 (ADR-0048) / 19 (ADR-0050) / 20 (ADR-0051) / 21 (ADR-0053) / 25 (ADR-0062), phase 26's brainstorm DOES land an ADR: **ADR-0065** records (a) the **family pivot** (xDS hot-reload chosen over the exhausted HTTP-filters family, the LB-family opener, EDS/CDS/LDS-first hot-reload, the protos-blocked gRPC transport, and the low-leverage network-filters opener — alternatives weighed), (b) the five §0 findings, and (c) the minimum-viable scope boundary — deliver RDS-file hot-reload (poll-based watch + atomic route-table swap + reload-counter advancement + `RoutesConfigDump` version update + fixture 0034); defer CDS/LDS/EDS hot-reload, SDS/RTDS, inotify-exactness, the gRPC/ADS transport, delta xDS, and the ADR-0014 protos supersession. Conditional §6.2-reconciliation + split ADRs are enumerated in §7.

---

## 3. Deliverables

Phase 26's scope is enumerated as deliverables `D1`–`D8` below. **The state-2 PLAN-writer organizes deliverables into tasks AND evaluates the §6.1 split gate.** Deliverables are LISTED roughly in execution order; the SPEC constrains the surface, not the task organization.

### D1 — Route-table-handle migration (`Arc<RouteConfiguration>` → a shared swappable handle)

`HCMConfig.route_config: Arc<RouteConfiguration>` (`crates/envoy-http1/src/hcm.rs:122`) becomes a **shared, atomically-swappable handle** — projected `Arc<RwLock<Arc<RouteConfiguration>>>` or a `tokio::sync::watch::Receiver<Arc<RouteConfiguration>>` (NO new `arc-swap` dep per §5.1; the exact type is a PLAN-write decision weighing per-request read cost — `RwLock` read-guard vs `watch::borrow` — against §5.1). The per-request consumers (`hcm.rs:1177` vhost match; the H2 mirror; any access-log/route-metadata reader) load the current `Arc` at request entry (a cheap pointer clone) and use it for the whole request — guaranteeing **in-flight isolation** (§5.4). The construction site (`hcm.rs:209` `clone_route_config`) seeds the handle; the watcher (D2) holds the write end. **CARRY-FORWARD WARNING for the state-3 executor:** this is a workspace-compile-affecting field-type change touching every `cfg.route_config`/`self.route_config` read site across `envoy-http1` + `envoy-http2`; the PLAN-write's SPEC-correction pass enumerates the exact sites (the phase-20 `route_config`→`Option` migration is the precedent for the sweep discipline).

### D2 — The RDS file watcher (the fifth periodic-background primitive)

A new `RdsWatcher` (working name; `crates/envoy-http1/src/` beside the HCM, or a dedicated small module — PLAN-write call) built to the `envoy-health::Scheduler` template (`crates/envoy-health/src/scheduler.rs`): `spawn(rds_targets, cancel: CancellationToken) -> RdsWatcher` holding the spawned `JoinHandle`(s) + a `shutdown(self)` awaiting them on cancel. Each watched target = `(path, route_config_name, write-handle-to-the-HCM's-route-table, the rds.* stat handles)`. The loop polls `path` mtime per interval (a sensible default — the existing scheduler interval idiom; NOT a config field per §0 finding 5); on change → invoke D3. `envoy-bin` (`crates/envoy-bin/src/main.rs`, beside the 12.2/13.x/14.2 spawns at `:180-194`) constructs it once after building the listeners, passing the shared `CancellationToken` (`:91`). Inert when no HCM uses `rds` (no targets → no loop).

### D3 — The reload pipeline (re-parse → re-validate → atomic swap; stat-tick)

On a detected change for a target: re-read the file → re-parse via the existing `rds.rs` envelope parser (D2 of phase 20, reused verbatim) → select the `RouteConfiguration` by `route_config_name` → re-validate its route→cluster references against the immutable live cluster set (the phase-20 validator, reused) → on success: build a new `Arc<RouteConfiguration>` and **atomically store it into the write-handle** (D1); tick `update_attempt`+`update_success`+`config_reload`; update the config_dump version (D5). On failure (file unreadable / parse error / `route_config_name` absent / unresolved cluster ref): **KEEP the last-good handle unchanged**; tick `update_attempt`+`update_failure` (parse/IO) or `update_rejected` (semantic) per the §6.2-item-5 split. **This is the one place envoy-rust does NOT go all-fatal** (the ADR-0049 all-fatal posture governs STARTUP; at reload the proxy is already serving and Envoy warm-rejects — §5.5).

### D4 — Per-HCM `rds.*` counters tick per reload (reuse phase-20 registration)

The phase-20 conditional registration (`HCMStats` per-HCM `rds.*` sub-registration when `cfg.rds.is_some()`) is UNCHANGED; phase 26 wires the **increment sites in the reload pipeline (D3)** rather than only at initial load. The stat handles must be threaded from the HCM construction site to the watcher target (D2) — projected: the watcher target carries `Arc<Counter>` clones, the established 06.x `Arc<Counter>`-shared-handle idiom. (Initial-load increments stay where phase 20 put them; the watcher adds the per-reload increments.)

### D5 — `/config_dump` `RoutesConfigDump` version/`last_updated` update on reload

The phase-20 `RoutesConfigDump` entry (`crates/envoy-admin/src/endpoint.rs` `ConfigDumpEntry::Routes`) reads the CURRENT route table; with D1 making it swappable, the admin renderer must read through the swappable handle (not a startup snapshot) so a post-reload `/config_dump` reflects the new route config. The `version_info`/`last_updated` fields (§6.2 item 6 fixes their exact shape + whether envoy-rust populates a synthetic version on reload) update on each successful apply. Emitted ONLY when some HCM uses `rds` (the phase-20 conditionality; fixtures 0014/0026/0027 untouched).

### D6 — Harness: mid-test fixture-file rewrite + settle (the one genuinely-new harness capability)

`tests/differential/src/lib.rs` gains the ability to **rewrite a mounted dynamic-config file partway through a fixture's probe sequence, then wait for the change to settle on both proxies before the next probe**. Today the harness renders dynamic files once at startup (the phase-18→21 `{{RDS_PATH}}` machinery); this phase adds a probe-list step type that (a) writes new contents (per-side rendered) to the SAME mounted path the proxy is watching, using the file-change operation §6.2 item 2 verified triggers a reload on BOTH (in-place truncate-rewrite projected; atomic-rename + `watched_directory` if §6.2 demands), and (b) settles via the 12.2 wait-for-convergence pattern (bounded poll on a discriminating observable — the routed-to cluster, or the `config_reload` counter advancing — NOT a fixed sleep). The expectations schema gains a "reload step" + post-reload probe/assertion block.

### D7 — Fixture 0034 + Docker wrapper

`tests/fixtures/0034-xds-rds-hot-reload/` carrying `envoy.yaml` + `envoy-rust.yaml` (admin + `node` + two static `http1-echo-server` clusters `backend_a`/`backend_b` distinguishable per §6.2 item 3 + one static listener whose HCM uses `rds` + NO inline `route_config` + `validate_clusters` per the ADR-0049/0051 precedent) + the initial RDS template (`local_route`, `/probe` → `backend_a`) + the reload RDS template(s) (`/probe` → `backend_b`; + a malformed variant for the bad-reload probe) + `expectations.yaml` (the §1 three-phase reload sequence) + `README.md`. Docker-gated wrapper at `tests/differential/tests/xds_rds_hot_reload.rs`. **Linux-CI-only differential evidence per ADR-0049 (§0 finding 4)** — the wrapper notes the macOS-local-unobservability.

### D8 — In-process backstop + BEHAVIOR_CONTRACT extensions (+ conditional fuzz seed)

In-process backstop at `crates/envoy-bin/tests/xds_rds_hot_reload.rs` (start envoy-rust with a temp RDS file; assert initial routing; rewrite the file [`tokio::time` makes the poll interval controllable in-process — the deterministic local complement to the Linux-CI-only differential]; assert the swap + the counter advancement + the config_dump version; PLUS the negative paths the fixture cannot cleanly exercise: malformed reload → last-good kept + `update_failure`; `route_config_name` vanishes on reload → `update_rejected`/last-good kept; a reload introducing an unresolved cluster ref → rejected/last-good kept; in-flight-request isolation — a request begun pre-reload completes under the old table). The M18-9 test-support-extraction pressure (now N≥5 backstops) is recorded in the file header (extraction stays a future hardening task). BEHAVIOR_CONTRACT: the §2.1 reload-semantics column + the §2.2 hot-reload subsection. Conditional fuzz seed `config_source_watched_directory.yaml` ONLY if §6.2 item 2 forces the `watched_directory` schema field.

---

## 4. Out of scope (deferred non-goals)

Each schema-bearing deferred item below is rejected by `#[serde(deny_unknown_fields)]` today (a bootstrap configuring it fails parse loudly — nothing is silently under-implemented):

- **Hot-reload of CDS / LDS / EDS** — the harder live mutations: CDS cluster add/remove (spawn/teardown of pools + health checkers + outlier sweepers + budget primitives), LDS listener add/remove (socket bind/unbind + connection drain + in-place listener update), EDS endpoint add/remove (pool-entry churn + LB-state update). Each is a future hot-reload phase building on THIS phase's watcher primitive + atomic-swap pattern. RDS is first because its live mutation is a stateless route-table pointer swap (§0 finding 1).
- **inotify/`watched_directory` mechanism exactness** — envoy-rust polls (behavior-equivalent post-settle, §0 finding 4). Matching Envoy's exact inotify event semantics (coalescing, partial-write windows, directory-move atomicity) is out of scope; `watched_directory` is parsed-and-honored ONLY if §6.2 item 2 requires it for the fixture's reload trigger.
- **Cluster warming / route-config draining on reload** — Envoy warms new clusters before activating; RDS route swaps need no warming (clusters pre-exist). The new-resource-warming machinery defers with CDS hot-reload.
- **A configurable poll interval / file-watch tuning knobs** — the watcher uses a fixed sensible default (Envoy exposes no equivalent file-xDS knob). Deferred.
- **`scoped_routes` / SRDS / VHDS hot-reload**, **SDS / RTDS** (any transport), the **gRPC/ADS xDS transport** (`api_config_source`/`ads_config`; tonic + envoy-protos/prost; the ADS state machine; the **ADR-0014 protos supersession**), **delta xDS**, **`initial_fetch_timeout`**, **REST xDS** — all carried unchanged from the phase-18→21 ledger.

---

## 5. Architectural invariants

### 5.1 No new crate, no new top-level Cargo dep

The watcher polls via `std::fs::metadata().modified()` (mtime) — no `notify`/inotify/kqueue dep (platform-specific + `cargo deny` cost). The swappable route-table handle uses `std::sync::RwLock<Arc<…>>` or `tokio::sync::watch` (both already available) — NOT a new `arc-swap` dep. The reload pipeline reuses the existing `rds.rs` parser + validator. (If the PLAN-write finds a no-new-dep swap handle materially worse for the per-request read hot-path, an `arc-swap` dep is the documented fallback — but the default is dep-free.)

### 5.2 Inert-when-unconfigured + the 0028 idle-watcher regression witness

No HCM with `rds` → no watcher spawned, zero behavior change. For the ONE existing RDS fixture `0028-xds-file-based-rds`: the envoy-rust side now spawns a watcher, but the file is never edited mid-test → the watcher idles, the `rds.*` counters stay at their initial-load values, and the data plane is byte-identical. **0028 staying green is the load-bearing regression witness** — it proves the route-table-handle migration (D1) is behavior-preserving and the idle watcher is side-effect-free. All 33 existing fixtures stay green simultaneously.

### 5.3 The cluster set stays immutable; only the route table mutates

Phase 26 mutates ONLY the per-HCM route table. The `ClusterManager`, listeners, pools, health checkers, outlier state, and budgets stay immutable post-construction (the ADR-0048 §5.3 invariant holds for everything except the RDS route table). A reloaded route may reference any pre-existing (static or CDS-loaded) cluster; it may NOT introduce a new cluster (an unresolved reference → the reload is rejected, last-good kept — §5.5).

### 5.4 In-flight request isolation (atomic swap, read-once-per-request)

Each request loads the current route-table `Arc` ONCE at request entry and uses that snapshot for its whole lifetime. A reload that lands mid-request does not affect the in-flight request (it already holds its `Arc`); the next request sees the new table. The swap is a single atomic store — no torn reads, no partial route tables.

### 5.5 Reload is warm-reject, NOT all-fatal (the one ADR-0049 carve-out)

ADR-0049's all-fatal-config posture governs STARTUP. At reload the proxy is already serving traffic, and Envoy's file-xDS warm-rejects a bad update (keeps the last-good config, ticks `update_failure`/`update_rejected`). envoy-rust MATCHES this at reload: a malformed / unresolvable reloaded RDS file leaves the live route table unchanged and ticks the failure counter — it does NOT crash the proxy. (This is a deliberate, BEHAVIOR_CONTRACT-recorded divergence from the project's startup all-fatal posture, justified by the running-proxy context and Envoy parity.)

### 5.6 Poll-based watch; settle-then-probe determinism

The watcher polls on a fixed interval; the reload is interval-bounded, not instantaneous. The fixture asserts post-settle state via bounded wait-for-convergence on a discriminating observable (the 12.2 pattern), never a fixed sleep and never a mid-reload race. In-process, `tokio::time` makes the interval controllable for a deterministic backstop.

### 5.7 Linux-CI-authoritative differential evidence (ADR-0049 Provenance)

The reload trigger (a file change observed inside the Envoy container) is unobservable on macOS Docker Desktop (virtiofs). Fixture 0034's differential evidence is therefore Linux-CI-only (the phase-21 EDS precedent for watching-class phases). Local verification = the in-process backstop + the §6.2 wire-shape probes; the AUTHORITATIVE anchor is the Linux CI run.

---

## 6. Implementation signposts for the planner

### 6.1 Split-gate evaluation (split projected NOT to fire)

Projected surface: D1 handle migration ~120 LoC (+~120 tests; cross-crate read-site sweep); D2 watcher primitive ~150 (+~100 tests); D3 reload pipeline ~140 (+~150 tests); D4 stat-tick threading ~60 (+~50 tests); D5 config_dump version ~70 (+~50 tests); D6 harness mid-test-rewrite + settle ~160 (+~60 tests); D7 fixture ~220 (YAML + wrapper); D8 backstop + contract ~200. **Total ~1200–1600 LoC / ~10–13 tasks** — around the ~1500-LoC / ~25-task §6.1 gate. If the §6.2-refined estimate fires it, split at the §1 NOTE seam (`26.1` foundation = handle migration + watcher + reload pipeline + backstop, regression-equivalence acceptance incl. 0028's idle watcher / `26.2` = stat-tick + config_dump + harness + fixture 0034 + close) with ADR-0067.

### 6.2 Empirical verification at state-2 PLAN-write (HEAVY — and MUST run on Linux per ADR-0049)

The state-2 PLAN-writer dispatches a foreground general-purpose subagent (the ADR-0037/0041/0043/.../0063 methodology) running `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`) **on a Linux host / Linux CI** (NOT macOS — the reload trigger needs observable filesystem events), with an `rds`-configured HCM + a host backend + admin scrapes, and verifies:

1. **The reload happens at all + readiness:** edit the RDS file under a running Envoy — does the new route table take effect WITHOUT a restart? How long until it settles (informs the harness wait bound)? Does the listener stay up (no drop)?
2. **Which file-change operation triggers the reload (the most consequential item — D6 is built to it):** in-place truncate-rewrite? atomic-rename (write-temp-then-`mv`)? Does Envoy's default file-watch (no `watched_directory`) catch an in-place rewrite? Does it MISS an atomic-rename (needing `watched_directory`)? Capture the exact operation the harness must use so BOTH proxies reload deterministically. **This decides whether phase 26 adds the `watched_directory` schema field (§0 finding 5).**
3. **Distinguishable backends:** confirm two real `http1-echo-server` clusters are distinguishable on the wire (a per-cluster echo marker, or the per-cluster `upstream_rq_total` discriminator) so the route-change is observable. (Avoids the LB distinguishable-backend-harness gap — a single-endpoint-per-cluster pair suffices.)
4. **The `rds.*` counter values after one successful reload:** `update_attempt`/`update_success`/`config_reload` = `2/2/2` (projected)? Does `config_reload` tick on EACH reload? Any version stat? Lock the §2.1 reload-semantics values.
5. **The bad-reload disposition (locks §5.5 + D3):** rewrite the file to malformed YAML / a missing `route_config_name` / a route referencing an unknown cluster — does Envoy keep the last-good table + tick `update_failure` (parse/IO) vs `update_rejected` (semantic)? Does it ever drop traffic? Lock which counter ticks for which failure class.
6. **The `/config_dump` `RoutesConfigDump` on reload:** does `version_info` change? `last_updated`? Does the `dynamic_route_configs[].route_config` reflect the NEW routes? Capture the exact post-reload JSON shape (+ confirm fixtures 0026/0027 `configs[]` indices are unaffected).
7. **In-flight isolation (opportunistic):** does a request that began before the reload complete under the old route table? (Informs the §5.4 BEHAVIOR_CONTRACT note; hard to assert differentially — backstop-only.)

If item 2 forces a schema field, or item 4/5/6 diverges materially from the projections → land **ADR-0066** at the PLAN-write commit (mirrors ADR-0052/0054/0063).

### 6.3 In-process backstop assertions (heeds the 14.2→21 both-paths lesson)

The backstop covers the happy reload (valid edit → swap + counters + config_dump) AND every negative path (malformed reload; vanished `route_config_name`; unresolved cluster ref; in-flight isolation) — the paths the Linux-CI-only differential fixture cannot cleanly exercise. `tokio::time` controls the poll interval for determinism.

### 6.4 The 06.x stats convention + the per-reload increment site

Stat handles are `Arc<Counter>` registered once (phase-20 registration, unchanged); the NEW increment sites are inside the reload pipeline (D3). The handle threading from HCM construction to the watcher target is the one non-obvious wiring decision (D4).

### 6.5 Pre-state-4 fmt + clippy discipline (heeds `project_state3_arc_skips_clippy`)

`cargo clippy --workspace --all-targets --all-features -- -D warnings` runs PER TASK in the state-3 arc. The D1 handle migration (the read-site sweep — `needless_borrow`/lock-guard lints), the D2 watcher loop (async-lint candidates), and the D3 pipeline (match-arm lints) are the likely lint sites.

### 6.6 State-4 evidence-discipline (continues per 05.3 → … → 21 chain) + Linux-CI authority

Per-gate command outputs quoted into PROGRESS Task-N; a single Docker-gated **Linux CI** run as the AUTHORITATIVE anchor (ADR-0049 — fixture 0034's reload is macOS-unobservable; the local Docker differential corroborates the 33 EXISTING fixtures but NOT 0034's reload). Pre-build `tests/helpers/*` before `cargo test --workspace` (the cold-helper-compile flake class per `project_flaky_access_log_fixture_0012`).

### 6.7 Isolated-crate build discipline (heeds `project_isolated_crate_build_blindspot`)

The state-4 verification MUST run `cargo build -p envoy-config -p envoy-http1 -p envoy-http2` (+ the watcher's owning crate) standalone in addition to the workspace build — the route-table-handle migration (D1) ripples through envoy-http1/envoy-http2.

### 6.8 Cargo.lock cadence

No new top-level deps projected (§5.1) → no Cargo.lock churn beyond version bumps already in flight. (If the `arc-swap` fallback fires, that is a Cargo.lock change recorded in the PLAN.)

### 6.9 PLAN.md + PROGRESS.md skeleton + Task 1 preamble land alongside at state-2

The 06.2→21 standalone-PLAN cadence: one pre-Task-1 docs-only commit (PLAN + PROGRESS skeleton + Task 1 preamble + the ROADMAP `26 planned → in-progress` flip + STATE advance + any §6.2 ADR-0066).

### 6.10 Subagent-driven execution at state 3 (per `feedback_execution_style`)

The state-3 arc dispatches PLAN tasks to fresh subagents SERIALLY (`feedback_serial_subagent_dispatch`), each with two-stage review (spec-compliance THEN code-quality), TDD per task, one code commit + one PROGRESS commit per task.

### 6.11 The `xds_file.rs` generalization (carried from phases 19/20/21 REVIEWs)

This phase RE-INVOKES the RDS parser on reload — it does not add a fifth copy. If the PLAN-write touches the parser call sites, the M19-1/M20-T6-a/M21 `parse_xds_file<T>` consolidation pressure (now also relevant to the watcher's re-parse path) may be opportunistically addressed (the brainstorming "improve code you're working in" discipline), recorded in the PLAN — but it is not required scope.

---

## 7. Conditional ADRs (projected; land at PLAN-write or in-execution if they fire)

- **ADR-0065 (the scoping ADR) — LANDS AT THIS BRAINSTORM COMMIT.** The family pivot + the §0 findings + the minimum-viable scope boundary + the deferral ledger. (The ADR-0048/0050/0051/0053/0062 brainstorm-time cadence.)
- **ADR-0066 (§6.2 empirical-verification reconciliation) — PLAUSIBLE.** Fires if §6.2 item 2 forces the `watched_directory` schema field, or item 4/5/6 (reload-counter values / bad-reload disposition / config_dump version shape) diverges materially. Lands at the state-2 PLAN-write commit. Mirrors ADR-0052/0054/0063.
- **ADR-0067 (phase split) — POSSIBLE (projected NOT to fire).** Fires only if the §6.2-refined estimate exceeds ~1500 LoC / ~25 tasks. Seam per §1 NOTE / §6.1. Mirrors ADR-0064.

---

## 8. Summary

Phase 26 lands the **xDS / dynamic config family's repeatedly-deferred prime follow-up — hot reload — at its minimum-viable increment: file-based RDS**. A running HCM whose route table is RDS-supplied picks up an edited RDS file without a restart: the new route table is loaded, validated against the immutable cluster set, and atomically swapped onto live traffic, with the per-HCM `rds.*` counters advancing past their initial-load values and `/config_dump` reflecting the new version. It is the **first post-construction live mutation in the project**, deliberately chosen as the LEAST invasive one — the route table is already a swappable `Arc` and route matching is per-request stateless (§0 finding 1), the periodic-watch task is the fifth instance of an established primitive (finding 2), and the reload reuses the entire phase-20 RDS load path (finding 3) — so it needs no new crate, no new dependency (poll-based watch + a dep-free swappable handle), and no new data-plane harness driver, only a mid-test file-rewrite-and-settle capability (D6). The harder live mutations (CDS cluster lifecycle, LDS socket drain, EDS endpoint churn) layer onto this phase's watcher + atomic-swap primitive in cleanly-deferred follow-ups. Because the reload trigger is unobservable on macOS Docker, fixture `0034-xds-rds-hot-reload`'s differential evidence is Linux-CI-authoritative (ADR-0049), complemented by a deterministic in-process backstop driving the negative paths.
