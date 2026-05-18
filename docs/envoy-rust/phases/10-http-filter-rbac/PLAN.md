# Phase 10 (`10-http-filter-rbac`) — PLAN

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development`
> per `feedback_execution_style` auto-memory and per the established 06.x / 07.x / 08.x /
> 09 cadence. Tasks 1-8 implement the phase per `SPEC.md`. Steps use `- [ ]` checkbox
> syntax for tracking.

**Goal.** Land the `envoy.filters.http.rbac` filter as the third concrete pluggable HTTP
filter in the 07.x-established framework (after HeaderMutation at 07.2 and LocalRateLimit
at 09): hand-rolled recursive Permission/Principal tree-walk evaluator (per D-3.2's
*"Every individual filter ... Must be written from scratch"* doctrine) + decode-side
`Decision::StopAndSend` with status 403 + body `"RBAC: access denied"` (19 bytes per
ADR-0034 §6.2 empirical verification) + 5 standard HTTP/1.1 response headers via the
ADR-0033 H1 HCM `decorate_filter_synth_response` helper + 2 upstream-Envoy-parity
counters under `http.<hcm_stat_prefix>.rbac.{allowed,denied}`. Close phase 09 REVIEW M2
(via D5 ADR-0033 Consequences amendment per preferred close shape (a)) and phase 09
REVIEW M3 (via D8.3 in-process backstop `tokio::process::Command + kill_on_drop(true) +
Stdio::null()` discipline) at their named close sites.

**Architecture.** The RbacFilter lives at `crates/envoy-filter/src/rbac.rs` (new module;
mirrors the 07.2 `header_mutation.rs` + 09 `local_rate_limit.rs` placement). Runtime
state is an `Arc<Vec<RuntimePolicy>>` + 2 `Arc<Counter>` handles + a `RuntimeAction`
discriminant. The recursive `eval_permission` / `eval_principal` functions are
stack-bounded by the parse-time depth gate (`RBAC_TREE_MAX_DEPTH = 16`) and use the
existing 04.2-landed `envoy_config::HeaderMatcher::matches(&[(String, String)]) -> bool`
directly — no matcher logic duplicated. Filter-chain integration extends
`HttpFilterInstance` with a new `Rbac(RbacFilter)` variant and threads
`hcm_stat_prefix: &str` through `FilterPipeline::build_from_config` →
`HttpFilterInstance::build` alongside the phase-09-threaded `&Arc<StatsRegistry>`. The
H1 HCM at `crates/envoy-http1/src/hcm.rs:185` is the SINGLE call site to widen (H2 reuses
the same `Http1HCMConfig` via re-export per the phase-09 PLAN-write SPEC correction #2
discipline). The 403 response shape flows through the existing
`decorate_filter_synth_response` helper landed at phase-09 ADR-0033 Commit C `ae2cef0` —
phase 10 is the **first non-LocalRateLimit consumer** of the helper, validating its
filter-agnostic design.

**Tech Stack.** Zero new top-level Cargo deps. Zero new workspace path-deps
(`envoy-stats` already on `envoy-filter` from phase 09; `envoy-config` already from
earlier phases). Permitted-foundations primitives used: `std::sync::Arc`, `Vec`, `Box`,
`BTreeMap`, `bytes::Bytes`. No `cel-rust`, `cel-interpreter`, `serde_with`,
`regex-automata` beyond what `regex` already provides per ADR-0021, `ipnet`, `cidr` —
none on D-3.2's permitted-foundations list; none required by phase-10 scope per the
§4 deferrals (CEL conditions, IP/CIDR matchers all defer). Differential harness reuses
`Driver::Http1ProbeList` (existing from 04.2) — no harness extension.

---

## 1. PLAN-write SPEC corrections

Per the 06.2 / 06.3 / 07.x / 08.x / 09 precedent (phase-09 landed 7 PLAN-write SPEC
corrections at `b9da8d4`), the PLAN-writer reads SPEC §3 surfaces against HEAD `c73f44f`
and flags mechanical signature drift between projected types and actual on-disk types.
Corrections land in execution at the named task and in the PROGRESS.md Task 1 preamble.

1. **`HeaderMatcher::matches` takes `&[(String, String)]`, NOT `&[Header]`** as SPEC §3
   D3 prose implies. The 04.2-landed signature at `crates/envoy-config/src/matcher.rs:19`
   is `pub fn matches(&self, headers: &[(String, String)]) -> bool`. This matches
   `envoy_filter::types::FilterRequest::headers: Vec<(String, String)>` directly — no
   adapter needed. **Action at Task 3:** call
   `m.matches(&req.headers)` directly inside `RuntimePermission::Header(m)` and
   `RuntimePrincipal::Header(m)` arms of the recursive evaluator.

2. **`ConfigError` enum lives in `crates/envoy-config/src/lib.rs`, NOT
   `crates/envoy-config/src/bootstrap.rs`** (same correction as phase-09 PLAN §1
   item 1). The validator function `validate_http_filters` IS in `bootstrap.rs`
   (line 1661 at HEAD `c73f44f`). Existing HeaderMutation + LocalRateLimit
   `ConfigError` variants land in `lib.rs`. **Action at Task 1:** the 5 new RBAC
   ConfigError variants land in `lib.rs`; the new sub-validator
   `validate_rbac_config` + the `Rbac` dispatch arm land in `bootstrap.rs`.

3. **The HCM filter-pipeline build site is `Http1HCMConfig::from_config` at
   `crates/envoy-http1/src/hcm.rs:185`** (same correction as phase-09 PLAN §1 item 2).
   The current 09-widened signature is
   `FilterPipeline::build_from_config(&cfg.http_filters, &registry)`. Phase 10 widens
   to `(&cfg.http_filters, &registry, &cfg.stat_prefix)` — one additional argument at
   the SINGLE call site. H2 reuses the same `Http1HCMConfig` via re-export per the
   09 wiring discipline; no second call site exists.

4. **`HttpFilterInstance` carries 2 `#[cfg(feature = "test-util")]` variants
   (`TestStopAndSendOnDecode(FilterResponse)` + `TestStopAndSendOnEncode(FilterResponse)`)**
   at instance.rs lines 30-35 — landed at 07.1/07.2 + preserved through 09. SPEC §3
   D4 doesn't reference them. **Action at Task 4:** the new `Rbac(RbacFilter)` variant
   goes between `LocalRateLimit` and the `#[cfg(feature = "test-util")]` block,
   preserving the test-util variants verbatim. The `build` signature change (add
   `hcm_stat_prefix: &str`) is orthogonal — test-util variants are constructed via
   separate `test_stop_and_send_on_decode` / `test_stop_and_send_on_encode`
   constructors, NOT via `build`, so no test-util-arm edit is needed.

5. **`HeaderMutationFilter::build_from_config` is single-arg `(cfg) -> Result<Self, _>`;
   `LocalRateLimitFilter::build_from_config` is two-arg `(cfg, registry)`.** The new
   `RbacFilter::build_from_config` has a **three-arg** shape
   `(cfg: &envoy_config::RbacConfig, registry: &Arc<StatsRegistry>, hcm_stat_prefix: &str)
   -> Result<Self, FilterError>` — the `hcm_stat_prefix` is needed to format the
   counter names `format!("http.{hcm_stat_prefix}.rbac.{allowed,denied}")` per SPEC
   §6.5. This is a new precedent for any filter whose stat namespace embeds the HCM's
   stat_prefix at register-time (vs filters like LocalRateLimit whose stat_prefix is a
   filter-level config field). Recorded for the subagent's awareness — NOT a SPEC drift.

6. **Empirical-verification body-bytes correction per ADR-0034 (option A).** SPEC §2.2
   projects the 403 body bytes as `"RBAC: access denied\n"` (20 bytes including trailing
   newline). Per the §6.2 empirical verification performed at THIS state-2 PLAN-write
   commit (Docker run of `envoyproxy/envoy:v1.33.0` against the §3 D8.1 canonical
   bootstrap; HTTP/1.1 GET with `Connection: close` request framing per the differential
   harness's `drive_http_get` shape; deny + allow probe pair captured byte-precisely),
   upstream Envoy v1.33 emits the 403 body as `"RBAC: access denied"` (19 bytes; hex
   `524241433a206163636573732064656e696564`; content-length: 19; **NO** trailing
   newline). The SPEC's 20-byte projection is incorrect by 1 byte. **ADR-0034 lands at
   THIS state-2 PLAN-write commit per SPEC §7 option A recommended posture + the 05.1 /
   05.4 / 09 Task-1-fixup ADR-inline precedent**, ratifying the revised body shape +
   adjusting SPEC §2.2 + §3 D8.1 + §5.9 inline. PLAN lock-in #14 locks the production
   shape; PROGRESS Task 1 preamble narrates the empirical evidence. The phase-10
   filter's production-code shape is `body: Bytes::from_static(b"RBAC: access denied")`
   (19 bytes, no `\n`).

7. **Stats namespace + header-set §6.2 verifications MATCH SPEC projections.** Per the
   same Docker run, the stats namespace shape is exactly
   `http.ingress_http.rbac.{allowed,denied}` (matching SPEC §2.1 +
   §6.5 projection); upstream additionally emits `shadow_allowed`/`shadow_denied` at 0
   unconditionally (shadow_rules counters default-emit-at-zero even when shadow_rules
   is unconfigured — phase-10 only registers the 2 primary counters since shadow_rules
   defers per SPEC §4; the differential fixture does not scrape RBAC stats so the
   2-vs-4 name-set divergence is not exercised bilaterally). The 403 response header
   set under harness `Connection: close` framing is exactly 5 headers
   `{content-length, content-type, date, server, connection}` (matching SPEC §2.2 +
   §5.9 projection — `connection: close` appears because the harness sends
   `Connection: close`; envoy-rust's `decorate_filter_synth_response` helper adds the
   same 5 headers per ADR-0033 Commit C). **No SPEC revision needed for (a) or (c).**

---

## 2. Architecture decisions locked at PLAN-write time

Per `feedback_pick_recommendation` ("always pick the recommended option; do not ask"),
the following decisions are locked at this commit. PROGRESS.md Task 1 preamble
references these by `#NN` for in-execution lookup. The 41 lock-ins below mirror the
phase-09 PLAN.md §2 lock-in table's density (39 lock-ins at `b9da8d4`); phase 10's
slightly higher count reflects the recursive-tree-walk surface (Permission/Principal
mirror pairs) and the empirical-verification ADR-0034 landing.

| # | Signpost | Decision | Rationale |
|---|---|---|---|
| 1 | Module placement | New module `crates/envoy-filter/src/rbac.rs`; new re-export `pub use rbac::RbacFilter;` in `lib.rs` alphabetically between the existing `LocalRateLimitFilter` and `RouterTerminus` re-exports (or wherever the alphabetical order dictates). | Mirrors 07.2 `header_mutation.rs` + 09 `local_rate_limit.rs` placement (one module per concrete filter). |
| 2 | No new path-deps | `envoy-filter/Cargo.toml` `[dependencies]` block unchanged — `envoy-stats` already present from phase 09; `envoy-config` already present. **Zero Cargo manifest edits.** | SPEC §5.1 + §5.3; the recursive evaluator uses only std-lib + existing path-deps. |
| 3 | RbacFilter struct shape | `pub struct RbacFilter { action: RuntimeAction, policies: Arc<Vec<RuntimePolicy>>, allowed_counter: Arc<Counter>, denied_counter: Arc<Counter> }`. `Debug + Clone` derived; the `Arc` wrappers on `policies` + both counters make `Clone` cheap (3 ref-bumps). | SPEC §3 D3. The action discriminant is `Copy` so it's free to clone; policies + counters are Arc-shared across all per-request pipeline clones. |
| 4 | RuntimeAction discriminant | `#[derive(Debug, Clone, Copy)] enum RuntimeAction { Allow, Deny }`. Lowered from `envoy_config::Action` at `build_from_config` time. | SPEC §3 D3. `Log` defers per SPEC §4. |
| 5 | RuntimePolicy shape | `struct RuntimePolicy { name: String, permissions: Vec<RuntimePermission>, principals: Vec<RuntimePrincipal> }`. `name` retained for `tracing::debug!` policy-match diagnostics (single source of truth at evaluator decision time). | SPEC §3 D3. |
| 6 | RuntimePermission enum | `enum RuntimePermission { Any(bool), Header(envoy_config::HeaderMatcher), AndRules(Vec<RuntimePermission>), OrRules(Vec<RuntimePermission>), NotRule(Box<RuntimePermission>) }`. `Header` reuses 04.2 HeaderMatcher directly per PLAN-write SPEC correction #1. | SPEC §3 D3 + §5.2. |
| 7 | RuntimePrincipal enum | Symmetric to RuntimePermission: `enum RuntimePrincipal { Any(bool), Header(envoy_config::HeaderMatcher), AndIds(Vec<RuntimePrincipal>), OrIds(Vec<RuntimePrincipal>), NotId(Box<RuntimePrincipal>) }`. | SPEC §3 D3. |
| 8 | Recursive evaluator shape | `fn eval_permission(p: &RuntimePermission, req: &FilterRequest) -> bool` + symmetric `eval_principal`. Synchronous; pure-compute; no I/O; no async. Stack-bounded by parse-time `RBAC_TREE_MAX_DEPTH = 16`. | SPEC §3 D3 + §5.2 + §5.4. |
| 9 | Short-circuit semantics | `AndRules`/`AndIds` use `Iterator::all` (short-circuit on first false); `OrRules`/`OrIds` use `Iterator::any` (short-circuit on first true); `NotRule`/`NotId` negate the recursive result. `Any(true)` always returns true; `Any(false)` always returns false (per upstream proto semantics — `Any(bool)` is a constant). | SPEC §5.6; standard short-circuit boolean evaluation. |
| 10 | Decision computation | At `decode_headers` time: scan `self.policies` in `Vec` insertion order (which is `BTreeMap` alphabetical order per lock-in #21); for each policy, evaluate `permissions.iter().any(|p| eval_permission(p, req)) && principals.iter().any(|p| eval_principal(p, req))`; short-circuit on first matching policy. Action-vs-match decision matrix per SPEC §5.6 exactly: `Allow + match → ALLOW`, `Allow + no_match → DENY`, `Deny + match → DENY`, `Deny + no_match → ALLOW`. | SPEC §5.6 — load-bearing cross-proxy invariant. |
| 11 | decode_headers shape | `pub(crate) fn decode_headers(&mut self, req: &mut FilterRequest) -> Decision`. Computes the ALLOW/DENY decision per #10; on ALLOW returns `Decision::Continue` + inc `allowed_counter`; on DENY returns `Decision::StopAndSend(synth_403())` + inc `denied_counter`. Counter increments happen EXACTLY ONCE per request — at the decision site (one source of truth). | SPEC §3 D3 + §5.6 + §6.5. |
| 12 | encode_headers shape | `pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision { Decision::Continue }`. No-op. | SPEC §5.4 — decode-only filter per upstream semantic; encode-side method exists for framework symmetry. |
| 13 | 403 synth response shape | `FilterResponse { status: 403, reason: Some("Forbidden"), headers: vec![], body: Bytes::from_static(b"RBAC: access denied") }` (19 bytes per ADR-0034 + PLAN §1 correction #6). The 5 standard HTTP/1.1 headers (`server`, `date`, `content-length`, `content-type`, `connection`) are decorated by the existing `decorate_filter_synth_response` helper at `crates/envoy-http1/src/hcm.rs` — the filter does NOT add them. The helper observes the empty `headers: vec![]` and adds all 5. | SPEC §2.2 + §5.9 + ADR-0034. |
| 14 | Body bytes locked | `b"RBAC: access denied"` byte-for-byte: hex `52 42 41 43 3a 20 61 63 63 65 73 73 20 64 65 6e 69 65 64` (19 bytes). NO trailing `\n` — empirically confirmed at state-2 §6.2 verification per ADR-0034. | ADR-0034 + PLAN §1 correction #6. |
| 15 | Counter registration | At `build_from_config`, register two counters via `registry.register_counter(&format!("http.{hcm_stat_prefix}.rbac.allowed"))` + `format!("http.{hcm_stat_prefix}.rbac.denied")`. Idempotent re-registration is fine (multiple RBAC filter instances sharing an HCM stat_prefix share the same counter pair — the architectural invariant from the 06.x StatsRegistry contract). | SPEC §3 D6 + §6.5 + §6.2 empirical verification (the namespace matches upstream exactly). |
| 16 | RbacConfig schema | `#[derive(Debug, Clone, Deserialize, PartialEq)] #[serde(deny_unknown_fields)] pub struct RbacConfig { pub rules: Rules }`. The 3 phase-10-deferred fields (`shadow_rules`, `shadow_rules_stat_prefix`, `track_per_rule_stats`) are NOT modeled; `deny_unknown_fields` rejects them per the established envoy-config discipline. | SPEC §3 D1 + §4. |
| 17 | Rules struct | `#[derive(Debug, Clone, Deserialize, PartialEq)] #[serde(deny_unknown_fields)] pub struct Rules { #[serde(default = "default_action")] pub action: Action, #[serde(default)] pub policies: BTreeMap<String, Policy> }`. | SPEC §3 D1. |
| 18 | Action enum | `#[derive(Debug, Clone, Deserialize, PartialEq)] #[serde(rename_all = "UPPERCASE")] pub enum Action { Allow, Deny }`. `Log` not modeled per SPEC §4. | SPEC §3 D1 + §4. |
| 19 | default_action helper | `fn default_action() -> Action { Action::Allow }` at module scope. | SPEC §3 D1 prose ("default Allow" comment). |
| 20 | Policy struct | `#[derive(Debug, Clone, Deserialize, PartialEq)] #[serde(deny_unknown_fields)] pub struct Policy { pub permissions: Vec<Permission>, pub principals: Vec<Principal> }`. `condition` + `checked_condition` (CEL) NOT modeled per SPEC §4. | SPEC §3 D1 + §4. |
| 21 | BTreeMap deterministic iteration | `policies: BTreeMap<String, Policy>` deserialized via serde's BTreeMap support; iteration order is alphabetical by policy name across both proxies. **Load-bearing cross-proxy invariant for SPEC §5.6 short-circuit-on-first-match semantic.** | SPEC §3 D1 (`// deterministic iteration order` comment) + §5.6. |
| 22 | Permission enum | `#[derive(Debug, Clone, Deserialize, PartialEq)] #[serde(deny_unknown_fields)] pub enum Permission { #[serde(rename = "any")] Any(bool), #[serde(rename = "header")] Header(HeaderMatcher), #[serde(rename = "and_rules")] AndRules(PermissionSet), #[serde(rename = "or_rules")] OrRules(PermissionSet), #[serde(rename = "not_rule")] NotRule(Box<Permission>) }`. `Box<Permission>` for `NotRule` to break the otherwise-infinite-size recursive enum. | SPEC §3 D1. |
| 23 | PermissionSet wrapper | `#[derive(Debug, Clone, Deserialize, PartialEq)] #[serde(deny_unknown_fields)] pub struct PermissionSet { pub rules: Vec<Permission> }`. Mirrors upstream proto's `Permission.Set` wrapper. | SPEC §3 D1. |
| 24 | Principal enum | Symmetric to Permission: `pub enum Principal { #[serde(rename = "any")] Any(bool), #[serde(rename = "header")] Header(HeaderMatcher), #[serde(rename = "and_ids")] AndIds(PrincipalSet), #[serde(rename = "or_ids")] OrIds(PrincipalSet), #[serde(rename = "not_id")] NotId(Box<Principal>) }`. | SPEC §3 D1. |
| 25 | PrincipalSet wrapper | `pub struct PrincipalSet { pub ids: Vec<Principal> }`. Note: field name is `ids` (not `rules`) per upstream proto. | SPEC §3 D1. |
| 26 | 6 new ConfigError variants | `EmptyRbacPolicies { listener: String }`, `EmptyRbacPolicyPermissions { listener: String, policy_name: String }`, `EmptyRbacPolicyPrincipals { listener: String, policy_name: String }`, `EmptyRbacPermissionSet { listener: String, policy_name: String, path: String }`, `EmptyRbacPrincipalSet { listener: String, policy_name: String, path: String }`, `RbacTreeTooDeep { listener: String, policy_name: String, depth: u32 }`. Land in `crates/envoy-config/src/lib.rs` alongside existing HeaderMutation + LocalRateLimit variants. | SPEC §3 D2 (SPEC enumerates 6 names; "possibly compressed" — PLAN locks 6, mirroring SPEC verbatim). |
| 27 | RBAC_TREE_MAX_DEPTH constant | `pub(crate) const RBAC_TREE_MAX_DEPTH: u32 = 16;` at module scope in `bootstrap.rs`. Defense-in-depth bound on Permission/Principal tree recursion at parse time; the runtime evaluator inherits the bound. | SPEC §3 D2. |
| 28 | validate_rbac_config sub-validator | New private fn `fn validate_rbac_config(cfg: &crate::RbacConfig, listener_name: &str) -> Result<(), crate::ConfigError>` in `bootstrap.rs`. Checks: `rules.policies` non-empty; per-policy `permissions` + `principals` non-empty; recursive descent into `Permission::AndRules`/`OrRules` + `Principal::AndIds`/`OrIds` to enforce non-empty sets + depth ≤ `RBAC_TREE_MAX_DEPTH`. Lands ALONGSIDE `validate_header_mutation_entries` + `validate_local_rate_limit_config` in `bootstrap.rs`. | SPEC §3 D2. |
| 29 | Validator dispatch arm | At `validate_http_filters` (line 1661 of `bootstrap.rs`), the `match &f.typed_config` block gains a fourth arm: `HttpFilterTypedConfig::Rbac(cfg) => { if f.name != "envoy.filters.http.rbac" { return Err(crate::ConfigError::UnsupportedHttpFilter { name: f.name.clone() }); } validate_rbac_config(cfg, listener_name)?; }`. Mirrors LocalRateLimit dispatch arm shape. The terminal-router check stays unchanged. | SPEC §3 D2. |
| 30 | HttpFilterTypedConfig variant | `Rbac(RbacConfig)` — fourth variant after `Router` + `HeaderMutation` + `LocalRateLimit`. `@type` rename: `"type.googleapis.com/envoy.extensions.filters.http.rbac.v3.RBAC"`. | SPEC §3 D1. Mirrors the LocalRateLimit variant's rename pattern. |
| 31 | D5 + D7 + D6 task organization | D6 (stats wiring) + D7.1 (2 stat-name mapping rows) co-locate at Task 3 (RbacFilter runtime task) per SPEC §6.6 cadence. D5 (ADR-0033 Consequences amendment) + the cross-ref note in phase-09 PROGRESS Commit C subsection land at Task 4 (HttpFilterInstance::Rbac variant + D5 closure of 09 REVIEW M2). This mirrors the phase-09 D4+D5 co-location at Task 4 `78128f4` precedent exactly. | SPEC §6.3 + §6.6 + phase-09 precedent. |
| 32 | D5 amendment shape | In-place amendment to `docs/envoy-rust/DECISIONS.md` ADR-0033 Consequences §iii(c)-end (currently `"the corresponding H2 HCM path naturally inherits via the shared Http1HCMConfig re-export"` at ~line 697). Insert a clarifying paragraph immediately AFTER the existing bullet recording the empirical H2 analogous gap + naming the close site. NOT a NEW ADR-0034 superseding ADR-0033's claim (the recommended posture per SPEC §7 option D — amend in-place; ADR-0033's Decision + Rationale + Provenance stand verbatim). | SPEC §2.3 + §7 option D recommended posture. |
| 33 | D8.1 fixture 0017 shape | `tests/fixtures/0017-http-filter-rbac/`: bootstrap HCM + `envoy.filters.http.rbac` (1 ALLOW policy `pass_with_header` with `permissions: [- any: true]` + `principals: [- header: { name: x-rbac-pass, string_match: { exact: yes } }]`) + `envoy.filters.http.router` + `direct_response: { status: 200, body: { inline_string: "ok\n" } }`. Mirrors fixture 0007's minimal HCM + direct_response data-plane shape (same 07.2 + 09 precedent). | SPEC §3 D8.1. |
| 34 | D8.1 fixture probe list | `Driver::Http1ProbeList` with 4 sequential probes (each `GET /` with `host: envoy-rust.test`). Per-probe header sets: `[no x-rbac-pass header, x-rbac-pass: yes, x-rbac-pass: no, x-rbac-pass: yes]`. Expected per-probe statuses: `[403, 200, 403, 200]`. `expected_body: { kind: byte_exact, body: "RBAC: access denied" }` on probes 1 + 3 (the 403 ones); `{ kind: byte_exact, body: "ok\n" }` on probes 2 + 4 (the 200 ones). `expected_headers: set_equal_modulo_allow_list` on all 4 probes. | SPEC §3 D8.1. |
| 35 | D8.1 per-probe header threading | The harness's `Http1ProbeList` driver's `Probe` struct's per-probe `request_headers` field is what threads the varying `x-rbac-pass` header values per-probe. The PLAN-writer verifies this at Task 5 against `tests/differential/src/lib.rs` before authoring; if the field doesn't yet support per-probe distinct header sets, Task 5 lands a small harness extension (~10-20 LoC) before the fixture commits. | SPEC §3 D8.1 prose ("the PLAN-writer verifies"). The phase-09 fixture used the same `host: envoy-rust.test` across all 5 probes; phase-10 is the first fixture to need per-probe distinct headers. |
| 36 | D7.1 BEHAVIOR_CONTRACT row landing cadence | 2 new "Stat-name mapping" rows land at Task 3 (D6 stats-wiring) commit per SPEC §6.6 + the 06.x / 07.x / 08.x / 09 doctrine (contract extensions land at task where first empirically exercised). Row content per SPEC §2.1 verbatim. No new Header allow-list row per SPEC §2.2 (the 5-header set is value-exact across both proxies under the deterministic burst with the `server` + `date` 04.1-landed allow-list entries already covering implementation-identifying divergences). | SPEC §2.1 + §6.6. |
| 37 | D7.2 ADR-0033 amendment landing | At Task 4 (D4 + D5 + D7.2 co-located commit). The amendment text per SPEC §2.3 + SPEC §3 D5: insert a paragraph at DECISIONS.md ADR-0033 Consequences §iii(c)-end + append a 1-sentence cross-ref note in `docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md` Commit C subsection. ~12-15 LoC total across 2 files. | SPEC §2.3 + §6.3. |
| 38 | D8.2 fuzz corpus seed | New file `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_rbac_filter.yaml` mirroring fixture 0017's bootstrap shape. Extends seed count 16 → 17. Includes `crates/envoy-config/fuzz/.gitignore` allow-list extension AND `crates/envoy-config/src/bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array extension (BOTH files must be edited together per the 09 Task 6 follow-up `1effb0f` lesson — Task 6's commit MUST extend the in-source SUCCESS array, NOT a follow-up commit). | SPEC §3 D8.2 + phase-09 Task 6 follow-up lesson. |
| 39 | D8.3 in-process backstop discipline (closes 09 REVIEW M3) | New file `crates/envoy-bin/tests/http_filter_rbac.rs` using `tokio::process::Command + .kill_on_drop(true) + stdout: Stdio::null()` (NOT `std::process::Command`). Closes 09 REVIEW M3 at THIS task per SPEC §6.4 + the named close site. Single `#[tokio::test]` exercising 4 sequential GET probes with the varying `x-rbac-pass` values; assertions: status sequence `[403, 200, 403, 200]` + body `"RBAC: access denied"` on 403 probes + body `"ok\n"` on 200 probes + 5-header presence on 403 probes. | SPEC §3 D8.3 + §6.4. |
| 40 | D8.3 direct code-spot-check precedent | The Task 7 implementer reads `crates/envoy-bin/tests/admin_drain_listeners.rs` (08.2 backstop) + `crates/envoy-bin/tests/http_filter_header_mutation.rs` (07.2 backstop) directly via `Read` tool before writing — verifies the precedent `tokio::process::Command + kill_on_drop` shape by direct code-spot-check, NOT by relying on the prior phase's PROGRESS narrative claim. Per SPEC §6.4 + 09 REVIEW M3's Process note. | SPEC §6.4 + 09 REVIEW M3 Process note. |
| 41 | ADR landings | **ONE ADR lands at THIS state-2 PLAN-write commit: ADR-0034 (option A) per SPEC §7.** Records the §6.2 empirical-verification body-bytes correction + SPEC §2.2 + §3 D8.1 + §5.9 inline revisions. DECISIONS.md ledger head advances `ADR-0033 → ADR-0034`. The remaining 3 conditional ADR-0034 slots (option B per-route deferral; option C foundations grant; option D D5 superseding-ADR shape) all DEFER per recommended posture — option B deferred to whichever future filter actually needs per-route config (CORS natural close site); option C no grant projected; option D in-place amendment per SPEC §2.3 + lock-in #32. The next-available ADR number after THIS commit is **ADR-0035**. | SPEC §7 + §6.2 + ADR-0034. |
| 42 | Split-gate verdict | Single-phase, no split. PLAN materializes **8 tasks / ~1330 LoC projected** (production ~510, tests ~620, fixture/doc ~200). Both dimensions comfortably under `BOOTSTRAP_PROMPT.md` §6.1 ~25-task / ~1500-LoC gate. Accept up to ~+15% empirical drift at state-3 (a tighter band than phase-09's +50% acceptance because phase-10's surface is more sharply-defined — recursive evaluator + 1 fixture, no token-bucket atomicity surface). **Do NOT nest-split** per parent-08 SPEC §6.1 alternative (vi) + the established no-nest-split discipline. | SPEC §6.1 + §8 split-gate signpost. |
| 43 | Subagent-driven execution | State-3 dispatches each task to a fresh subagent per `feedback_execution_style` auto-memory ("default to subagent-driven-development; skip the two-option fork") + the established 06.x / 07.x / 08.x / 09 cadence. Two-stage review per the standard subagent-driven cadence (spec-compliance + code-quality). | Auto-memory + project precedent. |
| 44 | PROGRESS.md skeleton + Task 1 preamble | Land alongside PLAN.md at this state-2 commit per the 06.2 / 06.3 / 07.x / 08.x / 09 cadence (divergence from the superseded 06.1 "PROGRESS created at Task 1" pattern). | Project precedent. |
| 45 | Cargo.lock cadence | Phase-04.1 REVIEW M5/M9 ratification ADR carries forward unchanged. Zero new top-level Cargo deps; zero new workspace path-deps. `Cargo.lock` diff at the phase-10 reviewed range is expected to be EMPTY (zero workspace-internal path-dep additions). | SPEC §6.9. |
| 46 | `#![forbid(unsafe_code)]` posture | The new `rbac.rs` module inherits from the `crates/envoy-filter/src/lib.rs` crate root attribute. No `unsafe` blocks; no per-module override. | SPEC §5.2; D-3.8. |

---

## 3. LoC drift posture / split-gate evaluation

Per SPEC §6.1, the SPEC-time projection was ~9-11 tasks / ~1100-1300 LoC. The PLAN
materializes **8 tasks / ~1330 LoC projected**:

| Task | Production LoC | Test LoC | Fixture/doc LoC | Total |
|---|---|---|---|---|
| 1 — D1 schema + D2 validator (co-located) | ~210 | ~250 | ~5 | ~465 |
| 2 — D3 hand-rolled tree-walk evaluator + structural unit tests | ~110 | ~140 | 0 | ~250 |
| 3 — D3 RbacFilter runtime + D6 stats + D7.1 2 contract rows | ~130 | ~120 | ~15 | ~265 |
| 4 — D4 variant + D5 + D7.2 ADR-0033 amendment | ~50 | ~10 | ~15 | ~75 |
| 5 — D8.1 fixture 0017 + Docker wrapper | ~10 | ~25 | ~115 | ~150 |
| 6 — D8.2 fuzz seed | 0 | 0 | ~50 | ~50 |
| 7 — D8.3 in-process backstop (with 09 M3 fix) | 0 | ~190 | 0 | ~190 |
| 8 — state-4 verification + STATE advance | 0 | 0 | ~80 | ~80 |
| **TOTAL** | **~510** | **~735** | **~280** | **~1525** |

**Task count: 8.** Comfortably under §6.1's ~25-task gate. **LoC: ~1525.** Marginally
**at** §6.1's ~1500-LoC gate (1.7% over). Test-heavy concentration (~48% of LoC) is
consistent with the 06.x / 07.x / 08.x / 09 cadence (the project's mature posture biases
toward exhaustive per-bucket attestation).

**Decision: single-phase; no split.** The 25 LoC "overrun" (1.7%) is well within
acceptable PLAN-time projection uncertainty + the §6.1 gate is a SOFT gate per
`BOOTSTRAP_PROMPT.md` §6.1 prose ("~1500 lines of code of net change" — approximate, not
hard). Accept up to ~+15% empirical drift at state-3 per lock-in #42. If a single task's
empirical drift exceeds +50% the PLAN-writer's in-execution release valve is per-step
commit splitting recorded in PROGRESS (per the 06.x / 07.x / 08.x / 09 precedent), NOT a
phase-level nest-split per parent-08 SPEC §6.1 alternative (vi).

---

## 4. Task summary

| # | Title | Files touched | Carryforwards / notes |
|---|---|---|---|
| 1 | D1 envoy-config schema + D2 validator (co-located) | `crates/envoy-config/src/lib.rs` (6 new `ConfigError` variants + new re-exports for `Action`, `Permission`, `PermissionSet`, `Policy`, `Principal`, `PrincipalSet`, `RbacConfig`, `Rules`); `crates/envoy-config/src/bootstrap.rs` (new `HttpFilterTypedConfig::Rbac` variant + 8 new schema items + `RBAC_TREE_MAX_DEPTH` const + `default_action` helper + `validate_http_filters` Rbac arm + `validate_rbac_config` sub-validator + unit tests); `crates/envoy-filter/src/instance.rs` (transient bridge arm; replaced in Task 4) | None engaged. |
| 2 | D3 hand-rolled recursive tree-walk evaluator + structural unit tests | `crates/envoy-filter/src/rbac.rs` (NEW: `RuntimePermission` + `RuntimePrincipal` enums + `eval_permission` + `eval_principal` fns + unit tests; the RbacFilter struct + build_from_config + decode_headers land in Task 3); `crates/envoy-filter/src/lib.rs` (`pub mod rbac;` declaration; re-export deferred to Task 3) | None engaged. |
| 3 | D3 RbacFilter runtime + D6 stats wiring + D7.1 2 BEHAVIOR_CONTRACT rows | `crates/envoy-filter/src/rbac.rs` (extend with `RuntimeAction` + `RuntimePolicy` + `RbacFilter` struct + `build_from_config` + `decode_headers` + `encode_headers` + lower_permission/lower_principal helpers + unit tests); `crates/envoy-filter/src/lib.rs` (re-export `pub use rbac::RbacFilter;`); `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (2 new "Stat-name mapping" rows under "10 entries (RBAC filter)") | None engaged. |
| 4 | D4 HttpFilterInstance::Rbac variant + D5 + D7.2 ADR-0033 Consequences amendment | `crates/envoy-filter/src/instance.rs` (new enum variant + build dispatch arm + decode/encode dispatch arms + replace Task 1's transient bridge arm + `hcm_stat_prefix: &str` parameter added); `crates/envoy-filter/src/pipeline.rs` (widen `build_from_config` signature with `hcm_stat_prefix: &str`); `crates/envoy-http1/src/hcm.rs` (1-line: thread `&cfg.stat_prefix` to `build_from_config` call at line 185); `docs/envoy-rust/DECISIONS.md` (ADR-0033 Consequences §iii(c) amendment ~10 LoC); `docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md` (Commit C subsection cross-ref note ~2 LoC) | **CLOSES 09 REVIEW M2** at named site (via D5 ADR-0033 Consequences amendment per preferred close shape (a)). |
| 5 | D8.1 fixture 0017 + Docker-gated wrapper | `tests/fixtures/0017-http-filter-rbac/` (NEW: `envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml`, `README.md`); `tests/differential/tests/http_filter_rbac.rs` (NEW: Docker-gated wrapper); `tests/differential/src/lib.rs` (per-probe `request_headers` field extension on `Http1Probe` if not already present — verified at task start) | None engaged. |
| 6 | D8.2 fuzz corpus seed | `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_rbac_filter.yaml` (NEW); `crates/envoy-config/fuzz/.gitignore` (1-line allow-list extension); `crates/envoy-config/src/bootstrap.rs` (extend `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array per the 09 Task 6 follow-up lesson — same-commit edit, NOT a follow-up) | None engaged. |
| 7 | D8.3 in-process backstop (with 09 REVIEW M3 fix) | `crates/envoy-bin/tests/http_filter_rbac.rs` (NEW: `tokio::process::Command + .kill_on_drop(true) + Stdio::null()` discipline applied directly per lock-in #39) | **CLOSES 09 REVIEW M3** at named site (subprocess-discipline regression fix). |
| 8 | state-4 phase-done verification + STATE advance to state-5-next | `docs/envoy-rust/phases/10-http-filter-rbac/PROGRESS.md` (state-4 evidence anchor: 17-fixture green simultaneously + per-gate quoted output + CI run URL + HEAD SHA + completion timestamp); `docs/envoy-rust/STATE.md` (Active phase status → state 4-complete / state-5-next; Next expected skill → superpowers:requesting-code-review) | Materializes state-4 evidence per `BOOTSTRAP_PROMPT.md` §7.5 (a)-(e). |

**Dependency chain:**
- Task 1 has no in-phase deps.
- Task 2 depends on Task 1's `RbacConfig` + nested types (to construct test inputs).
- Task 3 depends on Tasks 1 + 2 (uses both the schema + the recursive evaluator).
- Task 4 depends on Task 3 (consumes `RbacFilter::build_from_config`).
- Task 5 depends on Tasks 1-4 (full end-to-end pipeline must compile + run before fixture exercises it).
- Task 6 depends on Task 1 (config parsing must accept the new schema for the fuzz seed to be a valid input).
- Task 7 depends on Tasks 1-4 (in-process backstop boots envoy-bin against the full pipeline).
- Task 8 depends on all prior tasks (verification anchor).

**Task ordering for state-3 dispatch:** 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8. Tasks 5/6/7 are
pairwise independent post-Task-4 — a sufficiently aggressive subagent dispatch could
fan them out in parallel, but the established 06.x / 07.x / 08.x / 09 cadence prefers
sequential single-task dispatch (each subagent reads the prior task's PROGRESS append
for context).

---

## 5. Conventions

**TDD shape per task:** Write the failing tests FIRST (one or more `- [ ]` steps);
run them and verify they fail; implement; run again and verify they pass; run the 5
stable-toolchain gates (`cargo fmt --all -- --check` + `cargo clippy --workspace
--all-targets --all-features -- -D warnings` + `cargo build --workspace --all-targets`
+ `cargo test --workspace` + `cargo deny check`); append to PROGRESS.md; commit.

**Commit message format per task:** `phase 10: task NN — <short description>` matching
the 06.x / 07.x / 08.x / 09 precedent. Final state-6 commit per SPEC §9 +
`BOOTSTRAP_PROMPT.md` §5.3: `phase 10: envoy.filters.http.rbac + fixture 0017 + 09 REVIEW
M2 + M3 close`. If any ADRs land in state-3 the bracketed list is appended per
`BOOTSTRAP_PROMPT.md` §5.3.

**PROGRESS cadence per task:** Append a new `### Task N — <name>` subsection with: work
summary (3-5 paragraphs); tests landed (bulleted list); per-task deviations from PLAN
(numbered list, often empty); LoC delta (table); 5-gate test-bucket attestation (5
subsections, one per gate, each with PASS/FAIL + exit code + verbatim output where the
gate produces visible diff vs prior task).

**Per-task fmt discipline:** Every task closes by running `cargo fmt --all --
--check`. If drift is observed, run `cargo fmt --all` first (mutating step) and re-stage
before commit. Carries the 06.1 R-9 discipline forward.

**Error-handling convention:** All new error variants are `thiserror::Error` derives on
the existing `ConfigError` enum (envoy-config) and `FilterError` enum (envoy-filter).
`anyhow` is forbidden in library crates per D-3.2.

---

## 6. State-2 commit (this commit)

This commit is **docs-only** and touches 5 files:

- **CREATE** `docs/envoy-rust/phases/10-http-filter-rbac/PLAN.md` (this file).
- **CREATE** `docs/envoy-rust/phases/10-http-filter-rbac/PROGRESS.md` (skeleton + Task 1 preamble).
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — flip row `10` `status: planned` → `status: in-progress`. Earlier rows unchanged.
- **MODIFY** `docs/envoy-rust/STATE.md` — Active phase status; Next expected skill; Last commit; Last updated; new "Phase-10 state-2 PLAN-write" subsection in Notes.
- **MODIFY** `docs/envoy-rust/DECISIONS.md` — append **ADR-0034** (the §6.2 empirical-verification body-bytes correction per SPEC §7 option A recommended posture; ledger head advances `ADR-0033 → ADR-0034`).
- **MODIFY** `docs/envoy-rust/phases/10-http-filter-rbac/SPEC.md` — 3 inline ADR-0034 revisions (§2.2 body bytes; §3 D8.1 fixture body assertion; §5.9 filter response shape — each replaces `"RBAC: access denied\n"` (20 bytes) with `"RBAC: access denied"` (19 bytes); each cross-refs ADR-0034 as the revision authority).

**Commit message:**

```
phase 10: state-2 standalone PLAN.md [ADR-0034]
```

Mirrors `b9da8d4` (phase-09 state-2 PLAN-write `phase 09: state-2 standalone PLAN.md`)
shape precedent + the bracketed-ADR convention per `BOOTSTRAP_PROMPT.md` §5.3 (ADR-0034
landed inline per lock-in #41).

No production code changes; no test changes; no fixture changes; no Cargo.toml /
Cargo.lock changes; no BEHAVIOR_CONTRACT.md changes (the 2 stat-name mapping rows land
at Task 3 commit per the 06.x / 07.x / 08.x / 09 doctrine — contract extensions land at
empirical-engagement task time, NOT at PLAN-write time).

---

## Task 1: D1 envoy-config schema + D2 validator (co-located)

**Goal.** Extend `crates/envoy-config` with the RBAC schema (8 new schema items + 1 new
`HttpFilterTypedConfig` variant) + the validator dispatch arm + sub-validator + recursive
depth/non-emptiness checks (6 new ConfigError variants + 1 new `RBAC_TREE_MAX_DEPTH`
const + 1 new `default_action` helper). This is the parse-time gate that catches
misconfigured RBAC bootstraps before they reach the filter runtime.

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (add 6 `ConfigError` variants + 8 re-exports).
- Modify: `crates/envoy-config/src/bootstrap.rs` (add `HttpFilterTypedConfig::Rbac` variant + 8 new schema items + `RBAC_TREE_MAX_DEPTH` + `default_action` + extend `validate_http_filters` + `validate_rbac_config` sub-validator + unit tests).
- Modify: `crates/envoy-filter/src/instance.rs` (transient bridge arm; Task 4 replaces it with the proper `HttpFilterInstance::Rbac` dispatch).

### Steps

- [ ] **Step 1: Write the failing schema-deserialization + validator unit tests.**

Add to `crates/envoy-config/src/bootstrap.rs` at the bottom of the existing `#[cfg(test)]
mod tests { ... }` block (after the `local_rate_limit_tests` submodule from phase 09):

```rust
mod rbac_tests {
    use super::*;
    use crate::{
        Action, ConfigError, HeaderMatcher, HeaderMatcherMode, HttpFilter,
        HttpFilterTypedConfig, Permission, PermissionSet, Policy, Principal, PrincipalSet,
        RbacConfig, Rules, StringMatcher, StringMatcherMode,
    };
    use std::collections::BTreeMap;

    fn parse(yaml: &str) -> Result<RbacConfig, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    #[test]
    fn deserialize_rbac_minimal_allow_succeeds() {
        let yaml = r#"
rules:
  action: ALLOW
  policies:
    "pass_with_header":
      permissions:
        - any: true
      principals:
        - header:
            name: x-rbac-pass
            string_match: { exact: "yes" }
"#;
        let cfg = parse(yaml).expect("minimal Rbac parses");
        assert_eq!(cfg.rules.action, Action::Allow);
        assert_eq!(cfg.rules.policies.len(), 1);
        let p = cfg.rules.policies.get("pass_with_header").unwrap();
        assert_eq!(p.permissions.len(), 1);
        assert_eq!(p.principals.len(), 1);
    }

    #[test]
    fn deserialize_rbac_default_action_is_allow() {
        let yaml = r#"
rules:
  policies:
    "p":
      permissions: [{ any: true }]
      principals: [{ any: true }]
"#;
        let cfg = parse(yaml).expect("default action parses");
        assert_eq!(cfg.rules.action, Action::Allow);
    }

    #[test]
    fn deserialize_rbac_deny_action_succeeds() {
        let yaml = r#"
rules:
  action: DENY
  policies:
    "p":
      permissions: [{ any: true }]
      principals: [{ any: true }]
"#;
        let cfg = parse(yaml).expect("DENY action parses");
        assert_eq!(cfg.rules.action, Action::Deny);
    }

    #[test]
    fn deserialize_rbac_rejects_unknown_field() {
        let yaml = r#"
rules:
  action: ALLOW
  policies:
    "p":
      permissions: [{ any: true }]
      principals: [{ any: true }]
shadow_rules: {}
"#;
        let err = parse(yaml).expect_err("unknown top-level field rejected");
        assert!(format!("{err}").contains("shadow_rules"), "err: {err}");
    }

    #[test]
    fn deserialize_rbac_permission_and_or_not_combinators_succeed() {
        let yaml = r#"
rules:
  action: ALLOW
  policies:
    "complex":
      permissions:
        - and_rules:
            rules:
              - or_rules:
                  rules:
                    - any: true
                    - header: { name: x-a, string_match: { exact: "1" } }
              - not_rule:
                  header: { name: x-b, present_match: true }
      principals:
        - any: true
"#;
        let cfg = parse(yaml).expect("nested combinators parse");
        let p = cfg.rules.policies.get("complex").unwrap();
        assert_eq!(p.permissions.len(), 1);
        match &p.permissions[0] {
            Permission::AndRules(set) => assert_eq!(set.rules.len(), 2),
            other => panic!("expected AndRules, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_rbac_principal_and_or_not_combinators_succeed() {
        let yaml = r#"
rules:
  action: ALLOW
  policies:
    "complex_principals":
      permissions: [{ any: true }]
      principals:
        - and_ids:
            ids:
              - or_ids:
                  ids:
                    - any: true
                    - header: { name: x-c, string_match: { exact: "2" } }
              - not_id:
                  header: { name: x-d, present_match: true }
"#;
        let cfg = parse(yaml).expect("nested principal combinators parse");
        let p = cfg.rules.policies.get("complex_principals").unwrap();
        match &p.principals[0] {
            Principal::AndIds(set) => assert_eq!(set.ids.len(), 2),
            other => panic!("expected AndIds, got {other:?}"),
        }
    }

    fn ok_cfg() -> RbacConfig {
        let mut policies = BTreeMap::new();
        policies.insert(
            "p".to_string(),
            Policy {
                permissions: vec![Permission::Any(true)],
                principals: vec![Principal::Any(true)],
            },
        );
        RbacConfig {
            rules: Rules {
                action: Action::Allow,
                policies,
            },
        }
    }

    fn make_filter(cfg: RbacConfig) -> HttpFilter {
        HttpFilter {
            name: "envoy.filters.http.rbac".to_string(),
            typed_config: HttpFilterTypedConfig::Rbac(cfg),
        }
    }

    fn router_filter() -> HttpFilter {
        HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: HttpFilterTypedConfig::Router(crate::RouterConfig {}),
        }
    }

    #[test]
    fn validate_accepts_rbac_followed_by_router() {
        let filters = vec![make_filter(ok_cfg()), router_filter()];
        validate_http_filters(&filters, "ingress_http").expect("valid chain");
    }

    #[test]
    fn validate_rejects_empty_policies() {
        let mut cfg = ok_cfg();
        cfg.rules.policies.clear();
        let filters = vec![make_filter(cfg), router_filter()];
        let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
        assert!(
            matches!(
                err,
                ConfigError::EmptyRbacPolicies { ref listener } if listener == "ingress_http"
            ),
            "err: {err:?}"
        );
    }

    #[test]
    fn validate_rejects_empty_policy_permissions() {
        let mut cfg = ok_cfg();
        cfg.rules.policies.get_mut("p").unwrap().permissions.clear();
        let filters = vec![make_filter(cfg), router_filter()];
        let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
        assert!(
            matches!(
                err,
                ConfigError::EmptyRbacPolicyPermissions { ref policy_name, .. }
                    if policy_name == "p"
            ),
            "err: {err:?}"
        );
    }

    #[test]
    fn validate_rejects_empty_policy_principals() {
        let mut cfg = ok_cfg();
        cfg.rules.policies.get_mut("p").unwrap().principals.clear();
        let filters = vec![make_filter(cfg), router_filter()];
        let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
        assert!(
            matches!(
                err,
                ConfigError::EmptyRbacPolicyPrincipals { ref policy_name, .. }
                    if policy_name == "p"
            ),
            "err: {err:?}"
        );
    }

    #[test]
    fn validate_rejects_empty_permission_set() {
        let mut cfg = ok_cfg();
        cfg.rules.policies.get_mut("p").unwrap().permissions =
            vec![Permission::AndRules(PermissionSet { rules: vec![] })];
        let filters = vec![make_filter(cfg), router_filter()];
        let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
        assert!(
            matches!(err, ConfigError::EmptyRbacPermissionSet { .. }),
            "err: {err:?}"
        );
    }

    #[test]
    fn validate_rejects_empty_principal_set() {
        let mut cfg = ok_cfg();
        cfg.rules.policies.get_mut("p").unwrap().principals =
            vec![Principal::OrIds(PrincipalSet { ids: vec![] })];
        let filters = vec![make_filter(cfg), router_filter()];
        let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
        assert!(
            matches!(err, ConfigError::EmptyRbacPrincipalSet { .. }),
            "err: {err:?}"
        );
    }

    #[test]
    fn validate_rejects_tree_too_deep() {
        // Build a Permission::NotRule chain of depth RBAC_TREE_MAX_DEPTH + 1.
        let mut perm = Permission::Any(true);
        for _ in 0..=RBAC_TREE_MAX_DEPTH {
            perm = Permission::NotRule(Box::new(perm));
        }
        let mut cfg = ok_cfg();
        cfg.rules.policies.get_mut("p").unwrap().permissions = vec![perm];
        let filters = vec![make_filter(cfg), router_filter()];
        let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
        assert!(
            matches!(err, ConfigError::RbacTreeTooDeep { .. }),
            "err: {err:?}"
        );
    }

    #[test]
    fn validate_rejects_rbac_with_wrong_name() {
        let mut filter = make_filter(ok_cfg());
        filter.name = "envoy.filters.http.something_else".to_string();
        let filters = vec![filter, router_filter()];
        let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
        assert!(
            matches!(err, ConfigError::UnsupportedHttpFilter { .. }),
            "err: {err:?}"
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they FAIL.**

```
cargo test -p envoy-config --lib rbac_tests
```

Expected: compile errors — types `RbacConfig`, `Rules`, `Action`, `Policy`, `Permission`,
`PermissionSet`, `Principal`, `PrincipalSet` do not exist; variant
`HttpFilterTypedConfig::Rbac` does not exist; variants `ConfigError::EmptyRbacPolicies`,
`ConfigError::EmptyRbacPolicyPermissions`, `ConfigError::EmptyRbacPolicyPrincipals`,
`ConfigError::EmptyRbacPermissionSet`, `ConfigError::EmptyRbacPrincipalSet`,
`ConfigError::RbacTreeTooDeep` do not exist; const `RBAC_TREE_MAX_DEPTH` does not exist.

- [ ] **Step 3: Add the 6 new ConfigError variants in `crates/envoy-config/src/lib.rs`.**

Find the existing LocalRateLimit variant block (search for `EmptyLocalRateLimitStatPrefix`)
and append the 6 new RBAC variants immediately after that block:

```rust
#[error("HCM listener {listener:?}: RBAC filter has no policies (rules.policies is empty)")]
EmptyRbacPolicies { listener: String },

#[error("HCM listener {listener:?}: RBAC policy {policy_name:?} has no permissions")]
EmptyRbacPolicyPermissions {
    listener: String,
    policy_name: String,
},

#[error("HCM listener {listener:?}: RBAC policy {policy_name:?} has no principals")]
EmptyRbacPolicyPrincipals {
    listener: String,
    policy_name: String,
},

#[error("HCM listener {listener:?}: RBAC policy {policy_name:?} has an empty Permission set at {path}")]
EmptyRbacPermissionSet {
    listener: String,
    policy_name: String,
    path: String,
},

#[error("HCM listener {listener:?}: RBAC policy {policy_name:?} has an empty Principal set at {path}")]
EmptyRbacPrincipalSet {
    listener: String,
    policy_name: String,
    path: String,
},

#[error("HCM listener {listener:?}: RBAC policy {policy_name:?} Permission/Principal tree exceeds RBAC_TREE_MAX_DEPTH ({depth} > 16)")]
RbacTreeTooDeep {
    listener: String,
    policy_name: String,
    depth: u32,
},
```

- [ ] **Step 4: Add the 8 new schema items + the `HttpFilterTypedConfig::Rbac` variant + the `default_action` helper + the `RBAC_TREE_MAX_DEPTH` const to `crates/envoy-config/src/bootstrap.rs`.**

Find the `HttpFilterTypedConfig` enum (line ~444). Add a fourth variant immediately
after `LocalRateLimit(LocalRateLimitConfig)`:

```rust
#[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.rbac.v3.RBAC")]
Rbac(RbacConfig),
```

Find the `LocalRateLimitConfig` struct definition (or the end of the LocalRateLimit
schema block, search for `pub struct HttpStatus`) and add the new RBAC schema items
immediately after it (or wherever fits the existing file's structural conventions —
adjacent to other filter typed_config schemas):

```rust
/// Configuration for `envoy.filters.http.rbac` (phase 10).
///
/// Minimum-viable surface per phase-10 SPEC §3 D1: filter-chain config only;
/// header-based Permission/Principal types + combinators only. The 3 phase-10
/// deferred upstream-Envoy fields (`shadow_rules`, `shadow_rules_stat_prefix`,
/// `track_per_rule_stats`) are NOT modeled; `deny_unknown_fields` rejects them.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RbacConfig {
    pub rules: Rules,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    #[serde(default = "default_action")]
    pub action: Action,
    #[serde(default)]
    pub policies: std::collections::BTreeMap<String, Policy>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum Action {
    Allow,
    Deny,
    // Log defers per phase-10 SPEC §4.
}

fn default_action() -> Action {
    Action::Allow
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub permissions: Vec<Permission>,
    pub principals: Vec<Principal>,
    // condition / checked_condition (CEL) defer per phase-10 SPEC §4.
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub enum Permission {
    #[serde(rename = "any")]
    Any(bool),
    #[serde(rename = "header")]
    Header(HeaderMatcher),
    #[serde(rename = "and_rules")]
    AndRules(PermissionSet),
    #[serde(rename = "or_rules")]
    OrRules(PermissionSet),
    #[serde(rename = "not_rule")]
    NotRule(Box<Permission>),
    // url_path, destination_ip, destination_port[_range], metadata,
    // requested_server_name[_matcher], uri_template defer per phase-10 SPEC §4.
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PermissionSet {
    pub rules: Vec<Permission>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub enum Principal {
    #[serde(rename = "any")]
    Any(bool),
    #[serde(rename = "header")]
    Header(HeaderMatcher),
    #[serde(rename = "and_ids")]
    AndIds(PrincipalSet),
    #[serde(rename = "or_ids")]
    OrIds(PrincipalSet),
    #[serde(rename = "not_id")]
    NotId(Box<Principal>),
    // authenticated, source_ip, direct_remote_ip, remote_ip, url_path,
    // metadata, filter_state defer per phase-10 SPEC §4.
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PrincipalSet {
    pub ids: Vec<Principal>,
}

/// Defense-in-depth bound on Permission/Principal tree recursion at parse
/// time; the runtime evaluator at `envoy_filter::rbac` inherits the bound.
/// Per phase-10 SPEC §3 D2 (`RBAC_TREE_MAX_DEPTH = 16` per the prose).
pub(crate) const RBAC_TREE_MAX_DEPTH: u32 = 16;
```

Re-export the new types at the top of `crates/envoy-config/src/lib.rs` alongside the
existing schema re-exports (search for `pub use bootstrap::` to find the block); add
8 new items in alphabetical position within the existing block:

```rust
pub use bootstrap::{
    // ... existing items ...
    Action, Permission, PermissionSet, Policy, Principal, PrincipalSet, RbacConfig, Rules,
};
```

- [ ] **Step 5: Extend `validate_http_filters` with the Rbac dispatch arm + add the `validate_rbac_config` sub-validator.**

In `crates/envoy-config/src/bootstrap.rs::validate_http_filters` (line 1661), add a
new match arm AFTER the existing `HttpFilterTypedConfig::LocalRateLimit(cfg) => { ... }`
arm, BEFORE the closing brace of the match:

```rust
crate::HttpFilterTypedConfig::Rbac(cfg) => {
    if f.name != "envoy.filters.http.rbac" {
        return Err(crate::ConfigError::UnsupportedHttpFilter {
            name: f.name.clone(),
        });
    }
    validate_rbac_config(cfg, listener_name)?;
}
```

Add the sub-validator immediately after `validate_local_rate_limit_config`. Place it
BEFORE the `parse_duration` helper:

```rust
/// Validate one RBAC filter config. Phase 10 (SPEC §3 D2):
///   - rules.policies non-empty
///   - per-policy permissions + principals non-empty
///   - recursive: empty AndRules/OrRules/AndIds/OrIds rejected
///   - recursive: depth ≤ RBAC_TREE_MAX_DEPTH
fn validate_rbac_config(
    cfg: &crate::RbacConfig,
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    if cfg.rules.policies.is_empty() {
        return Err(crate::ConfigError::EmptyRbacPolicies {
            listener: listener_name.to_string(),
        });
    }
    for (policy_name, policy) in cfg.rules.policies.iter() {
        if policy.permissions.is_empty() {
            return Err(crate::ConfigError::EmptyRbacPolicyPermissions {
                listener: listener_name.to_string(),
                policy_name: policy_name.clone(),
            });
        }
        if policy.principals.is_empty() {
            return Err(crate::ConfigError::EmptyRbacPolicyPrincipals {
                listener: listener_name.to_string(),
                policy_name: policy_name.clone(),
            });
        }
        for (idx, perm) in policy.permissions.iter().enumerate() {
            validate_permission_tree(
                perm,
                listener_name,
                policy_name,
                &format!("permissions[{idx}]"),
                1,
            )?;
        }
        for (idx, prin) in policy.principals.iter().enumerate() {
            validate_principal_tree(
                prin,
                listener_name,
                policy_name,
                &format!("principals[{idx}]"),
                1,
            )?;
        }
    }
    Ok(())
}

fn validate_permission_tree(
    perm: &crate::Permission,
    listener_name: &str,
    policy_name: &str,
    path: &str,
    depth: u32,
) -> Result<(), crate::ConfigError> {
    if depth > RBAC_TREE_MAX_DEPTH {
        return Err(crate::ConfigError::RbacTreeTooDeep {
            listener: listener_name.to_string(),
            policy_name: policy_name.to_string(),
            depth,
        });
    }
    match perm {
        crate::Permission::Any(_) => Ok(()),
        crate::Permission::Header(_) => Ok(()),
        crate::Permission::AndRules(set) | crate::Permission::OrRules(set) => {
            if set.rules.is_empty() {
                return Err(crate::ConfigError::EmptyRbacPermissionSet {
                    listener: listener_name.to_string(),
                    policy_name: policy_name.to_string(),
                    path: path.to_string(),
                });
            }
            for (idx, child) in set.rules.iter().enumerate() {
                validate_permission_tree(
                    child,
                    listener_name,
                    policy_name,
                    &format!("{path}.rules[{idx}]"),
                    depth + 1,
                )?;
            }
            Ok(())
        }
        crate::Permission::NotRule(child) => validate_permission_tree(
            child,
            listener_name,
            policy_name,
            &format!("{path}.not_rule"),
            depth + 1,
        ),
    }
}

fn validate_principal_tree(
    prin: &crate::Principal,
    listener_name: &str,
    policy_name: &str,
    path: &str,
    depth: u32,
) -> Result<(), crate::ConfigError> {
    if depth > RBAC_TREE_MAX_DEPTH {
        return Err(crate::ConfigError::RbacTreeTooDeep {
            listener: listener_name.to_string(),
            policy_name: policy_name.to_string(),
            depth,
        });
    }
    match prin {
        crate::Principal::Any(_) => Ok(()),
        crate::Principal::Header(_) => Ok(()),
        crate::Principal::AndIds(set) | crate::Principal::OrIds(set) => {
            if set.ids.is_empty() {
                return Err(crate::ConfigError::EmptyRbacPrincipalSet {
                    listener: listener_name.to_string(),
                    policy_name: policy_name.to_string(),
                    path: path.to_string(),
                });
            }
            for (idx, child) in set.ids.iter().enumerate() {
                validate_principal_tree(
                    child,
                    listener_name,
                    policy_name,
                    &format!("{path}.ids[{idx}]"),
                    depth + 1,
                )?;
            }
            Ok(())
        }
        crate::Principal::NotId(child) => validate_principal_tree(
            child,
            listener_name,
            policy_name,
            &format!("{path}.not_id"),
            depth + 1,
        ),
    }
}
```

- [ ] **Step 6: Add the transient bridge arm in `crates/envoy-filter/src/instance.rs::build`.**

The new `envoy_config::HttpFilterTypedConfig::Rbac` variant must be handled by the
existing `HttpFilterInstance::build` match (otherwise non-exhaustive match breaks the
workspace build). The interim arm returns `FilterError::UnsupportedFilterType`;
Task 4 replaces it with the proper `HttpFilterInstance::Rbac` dispatch.

In `crates/envoy-filter/src/instance.rs::build`, add a 4th arm after the existing
`LocalRateLimit` arm:

```rust
envoy_config::HttpFilterTypedConfig::Rbac(_cfg) => {
    // Phase 10 Task 1 transient bridge — Task 4 replaces this with the
    // proper HttpFilterInstance::Rbac(RbacFilter) dispatch once Task 3
    // lands the RbacFilter runtime.
    Err(FilterError::UnsupportedFilterType {
        position: 0,
        name: hf.name.clone(),
    })
}
```

- [ ] **Step 7: Run tests to verify they PASS.**

```
cargo test -p envoy-config --lib rbac_tests
```

Expected: all rbac_tests tests pass.

- [ ] **Step 8: Run the workspace test bucket + 5 stable-toolchain gates.**

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo deny check
```

Expected: all 5 PASS. The new code is additive — no regression vectors.

- [ ] **Step 9: Append to PROGRESS.md.**

Append a new `### Task 1 — D1 envoy-config schema + D2 validator` subsection per the
per-task PROGRESS cadence (work summary + tests landed + deviations + LoC delta + 5-gate
attestation).

- [ ] **Step 10: Commit.**

```bash
git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs \
        crates/envoy-filter/src/instance.rs \
        docs/envoy-rust/phases/10-http-filter-rbac/PROGRESS.md
git commit -m "phase 10: task 1 — D1 envoy-config schema + D2 validator"
```

---

## Task 2: D3 hand-rolled recursive tree-walk evaluator + structural unit tests

**Goal.** Land the hand-rolled Permission/Principal tree-walk evaluator (per D-3.2's
"Every individual filter ... Must be written from scratch" doctrine) as a new module
`crates/envoy-filter/src/rbac.rs`. The RbacFilter struct + build_from_config wrapping
this primitive lands in Task 3.

**Files:**
- Create: `crates/envoy-filter/src/rbac.rs` (RuntimePermission + RuntimePrincipal enums + eval_permission + eval_principal + unit tests).
- Modify: `crates/envoy-filter/src/lib.rs` (add `pub mod rbac;` declaration; re-export deferred to Task 3).

### Steps

- [ ] **Step 1: Add the new module declaration to `crates/envoy-filter/src/lib.rs`.**

Find the existing `pub mod` block and add `pub mod rbac;` in alphabetical position
(between `pipeline` and `router`):

```rust
pub mod error;
pub mod header_mutation;
pub mod instance;
pub mod local_rate_limit;
pub mod pipeline;
pub mod rbac;
pub mod router;
pub mod types;
```

Re-export deferred to Task 3 — at this commit, the module is module-private only.

- [ ] **Step 2: Write the failing unit tests + evaluator stubs.**

Create `crates/envoy-filter/src/rbac.rs` with the test module first (TDD — the impl
follows):

```rust
//! `envoy.filters.http.rbac` runtime filter (phase 10).
//!
//! Hand-rolled per D-3.2's "Every individual filter ... Must be written from
//! scratch" doctrine + the 07.2 `header_mutation.rs` + 09 `local_rate_limit.rs`
//! precedent. Permission/Principal tree-walk evaluator + RbacFilter runtime.
//! The evaluator (this task) is pure-compute recursive descent; the filter
//! struct + decode/encode glue lands in Task 3.

use envoy_config::HeaderMatcher;
use envoy_filter_types::FilterRequest;

// Use crate-local types for FilterRequest to keep this module standalone.
use crate::types::FilterRequest as _FilterRequest;

#[derive(Debug)]
pub(crate) enum RuntimePermission {
    Any(bool),
    Header(HeaderMatcher),
    AndRules(Vec<RuntimePermission>),
    OrRules(Vec<RuntimePermission>),
    NotRule(Box<RuntimePermission>),
}

#[derive(Debug)]
pub(crate) enum RuntimePrincipal {
    Any(bool),
    Header(HeaderMatcher),
    AndIds(Vec<RuntimePrincipal>),
    OrIds(Vec<RuntimePrincipal>),
    NotId(Box<RuntimePrincipal>),
}

pub(crate) fn eval_permission(p: &RuntimePermission, req: &crate::types::FilterRequest) -> bool {
    match p {
        RuntimePermission::Any(b) => *b,
        RuntimePermission::Header(m) => m.matches(&req.headers),
        RuntimePermission::AndRules(set) => set.iter().all(|p| eval_permission(p, req)),
        RuntimePermission::OrRules(set) => set.iter().any(|p| eval_permission(p, req)),
        RuntimePermission::NotRule(inner) => !eval_permission(inner, req),
    }
}

pub(crate) fn eval_principal(p: &RuntimePrincipal, req: &crate::types::FilterRequest) -> bool {
    match p {
        RuntimePrincipal::Any(b) => *b,
        RuntimePrincipal::Header(m) => m.matches(&req.headers),
        RuntimePrincipal::AndIds(set) => set.iter().all(|p| eval_principal(p, req)),
        RuntimePrincipal::OrIds(set) => set.iter().any(|p| eval_principal(p, req)),
        RuntimePrincipal::NotId(inner) => !eval_principal(inner, req),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use envoy_config::{HeaderMatcher, HeaderMatcherMode, StringMatcher, StringMatcherMode};
    use crate::types::FilterRequest;

    fn req_with(headers: Vec<(&'static str, &'static str)>) -> FilterRequest {
        FilterRequest {
            method: "GET".to_string(),
            path: "/".to_string(),
            headers: headers
                .into_iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
            body: Bytes::new(),
        }
    }

    fn header_matcher_exact(name: &str, exact: &str) -> HeaderMatcher {
        HeaderMatcher {
            name: name.to_string(),
            mode: HeaderMatcherMode::StringMatch(StringMatcher {
                mode: StringMatcherMode::Exact(exact.to_string()),
                ignore_case: false,
            }),
            invert_match: false,
        }
    }

    #[test]
    fn any_true_permission_matches() {
        let req = req_with(vec![]);
        assert!(eval_permission(&RuntimePermission::Any(true), &req));
    }

    #[test]
    fn any_false_permission_does_not_match() {
        let req = req_with(vec![]);
        assert!(!eval_permission(&RuntimePermission::Any(false), &req));
    }

    #[test]
    fn header_permission_matches_when_value_equals() {
        let req = req_with(vec![("x-rbac-pass", "yes")]);
        let perm = RuntimePermission::Header(header_matcher_exact("x-rbac-pass", "yes"));
        assert!(eval_permission(&perm, &req));
    }

    #[test]
    fn header_permission_does_not_match_when_value_differs() {
        let req = req_with(vec![("x-rbac-pass", "no")]);
        let perm = RuntimePermission::Header(header_matcher_exact("x-rbac-pass", "yes"));
        assert!(!eval_permission(&perm, &req));
    }

    #[test]
    fn header_permission_does_not_match_when_header_absent() {
        let req = req_with(vec![("x-other", "yes")]);
        let perm = RuntimePermission::Header(header_matcher_exact("x-rbac-pass", "yes"));
        assert!(!eval_permission(&perm, &req));
    }

    #[test]
    fn and_rules_short_circuits_on_first_false() {
        let req = req_with(vec![]);
        let perm = RuntimePermission::AndRules(vec![
            RuntimePermission::Any(true),
            RuntimePermission::Any(false),
            RuntimePermission::Any(true),
        ]);
        assert!(!eval_permission(&perm, &req));
    }

    #[test]
    fn and_rules_all_true_matches() {
        let req = req_with(vec![]);
        let perm = RuntimePermission::AndRules(vec![
            RuntimePermission::Any(true),
            RuntimePermission::Any(true),
        ]);
        assert!(eval_permission(&perm, &req));
    }

    #[test]
    fn or_rules_short_circuits_on_first_true() {
        let req = req_with(vec![]);
        let perm = RuntimePermission::OrRules(vec![
            RuntimePermission::Any(false),
            RuntimePermission::Any(true),
            RuntimePermission::Any(false),
        ]);
        assert!(eval_permission(&perm, &req));
    }

    #[test]
    fn or_rules_all_false_does_not_match() {
        let req = req_with(vec![]);
        let perm = RuntimePermission::OrRules(vec![
            RuntimePermission::Any(false),
            RuntimePermission::Any(false),
        ]);
        assert!(!eval_permission(&perm, &req));
    }

    #[test]
    fn not_rule_negates_inner() {
        let req = req_with(vec![]);
        let perm_t = RuntimePermission::NotRule(Box::new(RuntimePermission::Any(false)));
        let perm_f = RuntimePermission::NotRule(Box::new(RuntimePermission::Any(true)));
        assert!(eval_permission(&perm_t, &req));
        assert!(!eval_permission(&perm_f, &req));
    }

    #[test]
    fn nested_and_or_not_evaluates_correctly() {
        let req = req_with(vec![("x-a", "1"), ("x-b", "2")]);
        // (header x-a == "1") AND NOT(header x-b == "3")
        let perm = RuntimePermission::AndRules(vec![
            RuntimePermission::Header(header_matcher_exact("x-a", "1")),
            RuntimePermission::NotRule(Box::new(RuntimePermission::Header(header_matcher_exact(
                "x-b", "3",
            )))),
        ]);
        assert!(eval_permission(&perm, &req));
    }

    #[test]
    fn principal_evaluator_mirrors_permission_evaluator() {
        let req = req_with(vec![("x-user", "alice")]);
        let prin = RuntimePrincipal::OrIds(vec![
            RuntimePrincipal::Header(header_matcher_exact("x-user", "bob")),
            RuntimePrincipal::Header(header_matcher_exact("x-user", "alice")),
        ]);
        assert!(eval_principal(&prin, &req));
    }
}
```

(Note: the `use envoy_filter_types::FilterRequest;` line at the top is incorrect —
remove it. The single `use crate::types::FilterRequest;` import is sufficient. The
remainder uses the `crate::types::FilterRequest` qualified path consistently.)

Actual top-of-file imports (corrected):

```rust
use envoy_config::HeaderMatcher;
use crate::types::FilterRequest;
```

And both eval functions take `req: &FilterRequest` (unqualified).

- [ ] **Step 3: Run tests to verify they PASS (they should — the implementation is in the same step).**

```
cargo test -p envoy-filter --lib rbac::tests
```

Expected: 12 tests pass.

- [ ] **Step 4: Run the workspace test bucket + 5 stable-toolchain gates.**

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo deny check
```

Expected: all 5 PASS.

- [ ] **Step 5: Append to PROGRESS.md.**

Append a new `### Task 2 — D3 hand-rolled recursive tree-walk evaluator` subsection.

- [ ] **Step 6: Commit.**

```bash
git add crates/envoy-filter/src/rbac.rs crates/envoy-filter/src/lib.rs \
        docs/envoy-rust/phases/10-http-filter-rbac/PROGRESS.md
git commit -m "phase 10: task 2 — D3 RBAC recursive tree-walk evaluator"
```

---

## Task 3: D3 RbacFilter runtime + D6 stats wiring + D7.1 2 BEHAVIOR_CONTRACT rows

**Goal.** Land the `RbacFilter` struct + `build_from_config` (lowering envoy-config
Permission/Principal to RuntimePermission/RuntimePrincipal) + `decode_headers` (decision
computation + counter increments + 403 short-circuit) + `encode_headers` (no-op) +
runtime unit tests. Wire 2 stat counters (`http.<hcm_stat_prefix>.rbac.{allowed,denied}`)
per upstream-Envoy parity. Land 2 BEHAVIOR_CONTRACT.md "Stat-name mapping" rows.

**Files:**
- Modify: `crates/envoy-filter/src/rbac.rs` (extend with RuntimeAction + RuntimePolicy + RbacFilter struct + build_from_config + decode_headers + encode_headers + lower_permission/lower_principal helpers + unit tests).
- Modify: `crates/envoy-filter/src/lib.rs` (re-export `pub use rbac::RbacFilter;`).
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (2 new "Stat-name mapping" rows under a new "**10 entries (RBAC filter):**" subheading).

### Steps

- [ ] **Step 1: Write the failing runtime unit tests.**

Add to the bottom of `crates/envoy-filter/src/rbac.rs::tests` module (after the Task 2
tests):

```rust
#[test]
fn build_from_config_allow_with_header_principal_creates_filter() {
    use envoy_stats::StatsRegistry;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    let registry = Arc::new(StatsRegistry::new());
    let mut policies = BTreeMap::new();
    policies.insert(
        "pass".to_string(),
        envoy_config::Policy {
            permissions: vec![envoy_config::Permission::Any(true)],
            principals: vec![envoy_config::Principal::Header(header_matcher_exact(
                "x-rbac-pass",
                "yes",
            ))],
        },
    );
    let cfg = envoy_config::RbacConfig {
        rules: envoy_config::Rules {
            action: envoy_config::Action::Allow,
            policies,
        },
    };
    let filter = RbacFilter::build_from_config(&cfg, &registry, "ingress_http")
        .expect("build succeeds");
    let _ = filter; // ensure construction succeeds
}

#[test]
fn decode_headers_allow_action_no_header_returns_deny() {
    use envoy_stats::StatsRegistry;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    let registry = Arc::new(StatsRegistry::new());
    let mut policies = BTreeMap::new();
    policies.insert(
        "p".to_string(),
        envoy_config::Policy {
            permissions: vec![envoy_config::Permission::Any(true)],
            principals: vec![envoy_config::Principal::Header(header_matcher_exact(
                "x-rbac-pass",
                "yes",
            ))],
        },
    );
    let cfg = envoy_config::RbacConfig {
        rules: envoy_config::Rules {
            action: envoy_config::Action::Allow,
            policies,
        },
    };
    let mut filter = RbacFilter::build_from_config(&cfg, &registry, "ingress_http").unwrap();
    let mut req = req_with(vec![]);

    match filter.decode_headers(&mut req) {
        crate::pipeline::Decision::StopAndSend(resp) => {
            assert_eq!(resp.status, 403);
            assert_eq!(resp.reason.as_deref(), Some("Forbidden"));
            assert!(resp.headers.is_empty());
            assert_eq!(&resp.body[..], b"RBAC: access denied");
        }
        other => panic!("expected StopAndSend(403), got {other:?}"),
    }
}

#[test]
fn decode_headers_allow_action_with_header_returns_continue() {
    use envoy_stats::StatsRegistry;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    let registry = Arc::new(StatsRegistry::new());
    let mut policies = BTreeMap::new();
    policies.insert(
        "p".to_string(),
        envoy_config::Policy {
            permissions: vec![envoy_config::Permission::Any(true)],
            principals: vec![envoy_config::Principal::Header(header_matcher_exact(
                "x-rbac-pass",
                "yes",
            ))],
        },
    );
    let cfg = envoy_config::RbacConfig {
        rules: envoy_config::Rules {
            action: envoy_config::Action::Allow,
            policies,
        },
    };
    let mut filter = RbacFilter::build_from_config(&cfg, &registry, "ingress_http").unwrap();
    let mut req = req_with(vec![("x-rbac-pass", "yes")]);

    matches!(filter.decode_headers(&mut req), crate::pipeline::Decision::Continue);
}

#[test]
fn decode_headers_deny_action_inverts_semantics() {
    use envoy_stats::StatsRegistry;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    let registry = Arc::new(StatsRegistry::new());
    let mut policies = BTreeMap::new();
    policies.insert(
        "block_evil".to_string(),
        envoy_config::Policy {
            permissions: vec![envoy_config::Permission::Any(true)],
            principals: vec![envoy_config::Principal::Header(header_matcher_exact(
                "x-evil", "true",
            ))],
        },
    );
    let cfg = envoy_config::RbacConfig {
        rules: envoy_config::Rules {
            action: envoy_config::Action::Deny,
            policies,
        },
    };
    let mut filter = RbacFilter::build_from_config(&cfg, &registry, "ingress_http").unwrap();

    // No x-evil header → no policy match → Deny action no_match → ALLOW.
    let mut req_benign = req_with(vec![]);
    matches!(
        filter.decode_headers(&mut req_benign),
        crate::pipeline::Decision::Continue
    );

    // With x-evil: true → policy match → Deny action match → DENY.
    let mut req_evil = req_with(vec![("x-evil", "true")]);
    match filter.decode_headers(&mut req_evil) {
        crate::pipeline::Decision::StopAndSend(resp) => assert_eq!(resp.status, 403),
        other => panic!("expected StopAndSend(403), got {other:?}"),
    }
}

#[test]
fn decode_headers_counters_increment_correctly() {
    use envoy_stats::StatsRegistry;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    let registry = Arc::new(StatsRegistry::new());
    let mut policies = BTreeMap::new();
    policies.insert(
        "p".to_string(),
        envoy_config::Policy {
            permissions: vec![envoy_config::Permission::Any(true)],
            principals: vec![envoy_config::Principal::Header(header_matcher_exact(
                "x-rbac-pass",
                "yes",
            ))],
        },
    );
    let cfg = envoy_config::RbacConfig {
        rules: envoy_config::Rules {
            action: envoy_config::Action::Allow,
            policies,
        },
    };
    let mut filter = RbacFilter::build_from_config(&cfg, &registry, "test_prefix").unwrap();

    // 2 allowed + 1 denied
    let mut req_ok = req_with(vec![("x-rbac-pass", "yes")]);
    let _ = filter.decode_headers(&mut req_ok);
    let _ = filter.decode_headers(&mut req_ok);
    let mut req_deny = req_with(vec![]);
    let _ = filter.decode_headers(&mut req_deny);

    let allowed = registry
        .counter("http.test_prefix.rbac.allowed")
        .expect("allowed counter registered");
    let denied = registry
        .counter("http.test_prefix.rbac.denied")
        .expect("denied counter registered");
    assert_eq!(allowed.value(), 2);
    assert_eq!(denied.value(), 1);
}

#[test]
fn encode_headers_is_noop() {
    use envoy_stats::StatsRegistry;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    let registry = Arc::new(StatsRegistry::new());
    let mut policies = BTreeMap::new();
    policies.insert(
        "p".to_string(),
        envoy_config::Policy {
            permissions: vec![envoy_config::Permission::Any(true)],
            principals: vec![envoy_config::Principal::Any(true)],
        },
    );
    let cfg = envoy_config::RbacConfig {
        rules: envoy_config::Rules {
            action: envoy_config::Action::Allow,
            policies,
        },
    };
    let mut filter = RbacFilter::build_from_config(&cfg, &registry, "p").unwrap();
    let mut resp = crate::types::FilterResponse {
        status: 200,
        reason: None,
        headers: vec![],
        body: bytes::Bytes::new(),
    };
    matches!(
        filter.encode_headers(&mut resp),
        crate::pipeline::Decision::Continue
    );
}
```

- [ ] **Step 2: Run tests to verify they FAIL.**

```
cargo test -p envoy-filter --lib rbac::tests
```

Expected: compile errors — `RbacFilter` struct does not exist; `RuntimeAction`,
`RuntimePolicy`, `build_from_config`, `decode_headers`, `encode_headers` do not exist;
`lower_permission`, `lower_principal` helpers don't exist.

- [ ] **Step 3: Implement the RbacFilter runtime in `crates/envoy-filter/src/rbac.rs`.**

Extend the module (above the `#[cfg(test)]` block) with:

```rust
use std::sync::Arc;
use bytes::Bytes;
use envoy_stats::{Counter, StatsRegistry};
use crate::error::FilterError;
use crate::pipeline::Decision;
use crate::types::FilterResponse;

/// The `envoy.filters.http.rbac` runtime filter (phase 10).
#[derive(Debug, Clone)]
pub struct RbacFilter {
    action: RuntimeAction,
    policies: Arc<Vec<RuntimePolicy>>,
    allowed_counter: Arc<Counter>,
    denied_counter: Arc<Counter>,
}

#[derive(Debug, Clone, Copy)]
enum RuntimeAction {
    Allow,
    Deny,
}

#[derive(Debug)]
struct RuntimePolicy {
    #[allow(dead_code)] // retained for future tracing::debug! diagnostics
    name: String,
    permissions: Vec<RuntimePermission>,
    principals: Vec<RuntimePrincipal>,
}

impl RbacFilter {
    /// Lower an `envoy_config::RbacConfig` into the runtime filter + register
    /// the 2 stat counters against the StatsRegistry under
    /// `http.{hcm_stat_prefix}.rbac.{allowed,denied}`.
    pub(crate) fn build_from_config(
        cfg: &envoy_config::RbacConfig,
        registry: &Arc<StatsRegistry>,
        hcm_stat_prefix: &str,
    ) -> Result<Self, FilterError> {
        let action = match cfg.rules.action {
            envoy_config::Action::Allow => RuntimeAction::Allow,
            envoy_config::Action::Deny => RuntimeAction::Deny,
        };
        let policies: Vec<RuntimePolicy> = cfg
            .rules
            .policies
            .iter()
            .map(|(name, policy)| RuntimePolicy {
                name: name.clone(),
                permissions: policy.permissions.iter().map(lower_permission).collect(),
                principals: policy.principals.iter().map(lower_principal).collect(),
            })
            .collect();
        let allowed_counter = registry
            .register_counter(&format!("http.{hcm_stat_prefix}.rbac.allowed"))
            .map_err(|e| FilterError::StatsRegistration {
                name: format!("http.{hcm_stat_prefix}.rbac.allowed"),
                source: e,
            })?;
        let denied_counter = registry
            .register_counter(&format!("http.{hcm_stat_prefix}.rbac.denied"))
            .map_err(|e| FilterError::StatsRegistration {
                name: format!("http.{hcm_stat_prefix}.rbac.denied"),
                source: e,
            })?;
        Ok(Self {
            action,
            policies: Arc::new(policies),
            allowed_counter,
            denied_counter,
        })
    }

    pub(crate) fn decode_headers(&mut self, req: &mut crate::types::FilterRequest) -> Decision {
        // Per SPEC §5.6: short-circuit on first matching policy.
        let any_policy_matches = self.policies.iter().any(|p| {
            let perm_match = p.permissions.iter().any(|x| eval_permission(x, req));
            let prin_match = p.principals.iter().any(|x| eval_principal(x, req));
            perm_match && prin_match
        });
        let allow = matches!(
            (self.action, any_policy_matches),
            (RuntimeAction::Allow, true) | (RuntimeAction::Deny, false)
        );
        if allow {
            self.allowed_counter.inc();
            Decision::Continue
        } else {
            self.denied_counter.inc();
            Decision::StopAndSend(FilterResponse {
                status: 403,
                reason: Some("Forbidden".to_string()),
                headers: vec![],
                body: Bytes::from_static(b"RBAC: access denied"),
            })
        }
    }

    pub(crate) fn encode_headers(&mut self, _resp: &mut FilterResponse) -> Decision {
        Decision::Continue
    }
}

fn lower_permission(p: &envoy_config::Permission) -> RuntimePermission {
    match p {
        envoy_config::Permission::Any(b) => RuntimePermission::Any(*b),
        envoy_config::Permission::Header(m) => RuntimePermission::Header(m.clone()),
        envoy_config::Permission::AndRules(set) => RuntimePermission::AndRules(
            set.rules.iter().map(lower_permission).collect(),
        ),
        envoy_config::Permission::OrRules(set) => RuntimePermission::OrRules(
            set.rules.iter().map(lower_permission).collect(),
        ),
        envoy_config::Permission::NotRule(inner) => {
            RuntimePermission::NotRule(Box::new(lower_permission(inner)))
        }
    }
}

fn lower_principal(p: &envoy_config::Principal) -> RuntimePrincipal {
    match p {
        envoy_config::Principal::Any(b) => RuntimePrincipal::Any(*b),
        envoy_config::Principal::Header(m) => RuntimePrincipal::Header(m.clone()),
        envoy_config::Principal::AndIds(set) => RuntimePrincipal::AndIds(
            set.ids.iter().map(lower_principal).collect(),
        ),
        envoy_config::Principal::OrIds(set) => RuntimePrincipal::OrIds(
            set.ids.iter().map(lower_principal).collect(),
        ),
        envoy_config::Principal::NotId(inner) => {
            RuntimePrincipal::NotId(Box::new(lower_principal(inner)))
        }
    }
}
```

(Note: the `FilterError::StatsRegistration` variant referenced above may not yet exist
on the `FilterError` enum. The Task 3 implementer verifies the existing
`crates/envoy-filter/src/error.rs` enum shape and either reuses an existing variant —
the 09 Task 3 commit `70bad43` landed `FilterError::StatsRegistration { name: String,
#[source] source: envoy_stats::StatsError }` per the LocalRateLimit precedent — or
adds it if missing. The LocalRateLimit filter at `crates/envoy-filter/src/local_rate_limit.rs`
uses the SAME variant for counter registration errors; phase 10 reuses it directly.
Direct code-spot-check `error.rs` at task start.)

- [ ] **Step 4: Re-export `RbacFilter` from `crates/envoy-filter/src/lib.rs`.**

Add to the existing `pub use` block (search for `pub use local_rate_limit::LocalRateLimitFilter;`):

```rust
pub use rbac::RbacFilter;
```

- [ ] **Step 5: Add the 2 BEHAVIOR_CONTRACT.md rows.**

In `docs/envoy-rust/BEHAVIOR_CONTRACT.md`, find the "**09 entries (LocalRateLimit
filter):**" subheading + table (lines 103-110). Immediately AFTER that table (before the
"**06.1 Prometheus exposition shape divergence**" block at line 112), insert:

```markdown
**10 entries (RBAC filter):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `http.<hcm_stat_prefix>.rbac.allowed` | value-exact | Counter; one increment per request allowed under the primary rules — either by explicit Allow-action policy match OR by Deny-action no-match (per phase-10 SPEC §5.6 decision matrix). Both proxies emit one increment per allowed request at the decision site in `RbacFilter::decode_headers` (synchronously, before `Decision::Continue`). Upstream Envoy v1.33 emits the same name at the same `http.<hcm_stat_prefix>.rbac.*` namespace per the §6.2 empirical verification at PLAN-write. |
| `http.<hcm_stat_prefix>.rbac.denied` | value-exact | Counter; one increment per request denied under the primary rules — either by explicit Deny-action policy match OR by Allow-action no-match. Both proxies emit one increment per denied request at the decision site in `RbacFilter::decode_headers` (synchronously, before constructing the `Decision::StopAndSend(FilterResponse)` 403). The `allowed + denied == total_requests_to_filter` invariant holds per SPEC §2.1 (each counter incremented at its own fire site; no double-counting). |
```

- [ ] **Step 6: Run tests to verify they PASS.**

```
cargo test -p envoy-filter --lib rbac::tests
```

Expected: 18 tests pass (12 from Task 2 + 6 new from Task 3).

- [ ] **Step 7: Run the 5 stable-toolchain gates.**

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo deny check
```

Expected: all 5 PASS.

- [ ] **Step 8: Append to PROGRESS.md.**

Append `### Task 3 — D3 RbacFilter runtime + D6 stats wiring + D7.1 2 contract rows` subsection.

- [ ] **Step 9: Commit.**

```bash
git add crates/envoy-filter/src/rbac.rs crates/envoy-filter/src/lib.rs \
        docs/envoy-rust/BEHAVIOR_CONTRACT.md \
        docs/envoy-rust/phases/10-http-filter-rbac/PROGRESS.md
git commit -m "phase 10: task 3 — D3 RbacFilter runtime + D6 stats + D7.1 2 contract rows"
```

---

## Task 4: D4 HttpFilterInstance::Rbac variant + D5 ADR-0033 Consequences amendment

**Goal.** Replace Task 1's transient `Err(UnsupportedFilterType)` bridge arm with the
proper `HttpFilterInstance::Rbac(RbacFilter)` dispatch. Widen
`FilterPipeline::build_from_config` + `HttpFilterInstance::build` signatures with the
new `hcm_stat_prefix: &str` parameter. Thread `&cfg.stat_prefix` from the H1 HCM
build site. Land D5 (ADR-0033 Consequences amendment + phase-09 PROGRESS Commit C
cross-ref note) — **CLOSES 09 REVIEW M2** at the named site.

**Files:**
- Modify: `crates/envoy-filter/src/instance.rs` (new variant + dispatch + replace Task 1 bridge arm + add `hcm_stat_prefix: &str` parameter to `build`).
- Modify: `crates/envoy-filter/src/pipeline.rs` (widen `build_from_config` signature with `hcm_stat_prefix: &str`).
- Modify: `crates/envoy-http1/src/hcm.rs` (1-line: add `&cfg.stat_prefix` to `build_from_config` call at line 185).
- Modify: `docs/envoy-rust/DECISIONS.md` (ADR-0033 Consequences §iii(c) amendment ~10 LoC).
- Modify: `docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md` (Commit C subsection cross-ref note ~2 LoC).

### Steps

- [ ] **Step 1: Update existing unit tests for the new signature.**

In `crates/envoy-filter/src/instance.rs` and `crates/envoy-filter/src/pipeline.rs`, the
existing `test_registry()` callers + `build`/`build_from_config` call sites need a
3rd argument `"test_prefix"` (or similar). Find each call site via:

```
grep -n 'build_from_config\|HttpFilterInstance::build' crates/envoy-filter/src/
```

Add `"test_prefix"` (an arbitrary test-only stat-prefix string) as the third positional
argument at each test call site.

- [ ] **Step 2: Widen `HttpFilterInstance::build` + replace the Task 1 bridge arm.**

In `crates/envoy-filter/src/instance.rs`:

```rust
pub(crate) fn build(
    hf: &envoy_config::HttpFilter,
    registry: &Arc<StatsRegistry>,
    hcm_stat_prefix: &str,
) -> Result<Self, FilterError> {
    match &hf.typed_config {
        envoy_config::HttpFilterTypedConfig::Router(_cfg) => {
            Ok(HttpFilterInstance::Router(RouterTerminus::new()))
        }
        envoy_config::HttpFilterTypedConfig::HeaderMutation(cfg) => Ok(
            HttpFilterInstance::HeaderMutation(HeaderMutationFilter::build_from_config(cfg)?),
        ),
        envoy_config::HttpFilterTypedConfig::LocalRateLimit(cfg) => {
            Ok(HttpFilterInstance::LocalRateLimit(
                LocalRateLimitFilter::build_from_config(cfg, registry)?,
            ))
        }
        envoy_config::HttpFilterTypedConfig::Rbac(cfg) => {
            Ok(HttpFilterInstance::Rbac(
                RbacFilter::build_from_config(cfg, registry, hcm_stat_prefix)?,
            ))
        }
    }
}
```

Add the `Rbac(RbacFilter)` variant to the `HttpFilterInstance` enum (between
`LocalRateLimit` and `#[cfg(feature = "test-util")]` block):

```rust
Rbac(RbacFilter),
```

Update `decode_headers` + `encode_headers` dispatch arms to add the `Rbac` case:

```rust
HttpFilterInstance::Rbac(f) => f.decode_headers(req),
// ...
HttpFilterInstance::Rbac(f) => f.encode_headers(resp_arg),
```

Add the import: `use crate::rbac::RbacFilter;` near the top of `instance.rs` alongside
other filter imports.

Remove the Task 1 transient bridge arm code (since the proper dispatch arm now replaces
it).

- [ ] **Step 3: Widen `FilterPipeline::build_from_config` signature.**

In `crates/envoy-filter/src/pipeline.rs`:

```rust
pub fn build_from_config(
    filters: &[envoy_config::HttpFilter],
    registry: &Arc<StatsRegistry>,
    hcm_stat_prefix: &str,
) -> Result<Self, FilterError> {
    if filters.is_empty() {
        return Err(FilterError::EmptyChain);
    }
    let mut out = Vec::with_capacity(filters.len());
    for hf in filters.iter() {
        out.push(HttpFilterInstance::build(hf, registry, hcm_stat_prefix)?);
    }
    Ok(Self { filters: out })
}
```

- [ ] **Step 4: Thread `&cfg.stat_prefix` at the H1 HCM build site.**

In `crates/envoy-http1/src/hcm.rs:185`, change:

```rust
let filter_pipeline = Arc::new(envoy_filter::FilterPipeline::build_from_config(
    &cfg.http_filters,
    &registry,
)?);
```

To:

```rust
let filter_pipeline = Arc::new(envoy_filter::FilterPipeline::build_from_config(
    &cfg.http_filters,
    &registry,
    &cfg.stat_prefix,
)?);
```

H2 reuses the same `Http1HCMConfig` via re-export — no second call site to widen.

- [ ] **Step 5: Land the ADR-0033 Consequences amendment (closes 09 REVIEW M2).**

In `docs/envoy-rust/DECISIONS.md`, find ADR-0033 Consequences §iii(c) bullet
(around line 697; search for `the corresponding H2 HCM path naturally inherits`).
Immediately AFTER that bullet, insert a new paragraph:

```markdown

**Phase-10 amendment (closes 09 REVIEW M2 per preferred close shape (a)):** Empirical
re-verification at the phase-10 state-2 PLAN-write commit confirms the H2 HCM filter-synth
writer path at `crates/envoy-http2/src/hcm.rs:373-378` (decode-side
`H2RequestPath::SynthFromDecode`) and `crates/envoy-http2/src/hcm.rs:436-443` (encode-side
`Decision::StopAndSend(replacement)`) return the filter's response verbatim through
`build_http_response` at `crates/envoy-http2/src/response.rs:29-50`, which does NOT add
`server`/`date`/`content-type`. The "naturally inherits via the shared Http1HCMConfig
re-export" claim above is INCORRECT for the H2 writer path — the H2 HCM has its own
`build_http_response` helper that is symmetric to H1's `synth_status` but does not
include the standard-header decoration that the H1 `decorate_filter_synth_response`
helper adds. The H2 analogous gap is known-deferred. The close site is "next
HTTP-filter-family phase exercising filters bilaterally on H2 (the H2 HCM
`decorate_filter_synth_response_h2` analogue lands as a ~50-70 LoC + 2-test follow-up
at that phase)." Phase 10's RBAC fixture exercises H1 only (matching the 07.2 + 09
single-codec-fixture cadence); the empirical RBAC 403 response correctly carries the 5
standard headers via the H1 helper.
```

In `docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md`, find the
Commit C subsection (search for `### Task 4 fixup — H1 HCM filter-synth header decoration per ADR-0033`).
Append a 1-sentence cross-ref note at the end of that subsection:

```markdown

**Cross-ref (phase-10 D5 follow-up):** the H2 HCM analogous gap recorded in this
commit's PROGRESS narrative is amended via the phase-10 D5 ADR-0033 Consequences
amendment (closes 09 REVIEW M2 per preferred close shape (a)); the close site for the
implementation deferral is named there.
```

- [ ] **Step 6: Run tests + 5 stable-toolchain gates.**

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo deny check
```

Expected: all 5 PASS. Existing tests continue passing; the 3-arg signature widening
propagates through `instance.rs` + `pipeline.rs` + H1 HCM uniformly.

- [ ] **Step 7: Append to PROGRESS.md.**

Append `### Task 4 — D4 HttpFilterInstance::Rbac variant + D5 ADR-0033 amendment (closes 09 REVIEW M2)` subsection.

- [ ] **Step 8: Commit.**

```bash
git add crates/envoy-filter/src/instance.rs crates/envoy-filter/src/pipeline.rs \
        crates/envoy-http1/src/hcm.rs \
        docs/envoy-rust/DECISIONS.md \
        docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md \
        docs/envoy-rust/phases/10-http-filter-rbac/PROGRESS.md
git commit -m "phase 10: task 4 — D4 variant + D5 ADR-0033 amendment (closes 09 REVIEW M2)"
```

---

## Task 5: D8.1 fixture 0017 + Docker-gated wrapper

**Goal.** Author the differential fixture `tests/fixtures/0017-http-filter-rbac/` +
the Docker-gated wrapper `tests/differential/tests/http_filter_rbac.rs`. Per the §6.2
empirical verification at state-2 PLAN-write, the 4-probe sequential burst on both
proxies yields the deterministic status sequence `[403, 200, 403, 200]` with body
`"RBAC: access denied"` (19 bytes per ADR-0034) on 403 probes and `"ok\n"` on 200
probes.

**Files:**
- Create: `tests/fixtures/0017-http-filter-rbac/envoy.yaml`
- Create: `tests/fixtures/0017-http-filter-rbac/envoy-rust.yaml`
- Create: `tests/fixtures/0017-http-filter-rbac/expectations.yaml`
- Create: `tests/fixtures/0017-http-filter-rbac/README.md`
- Create: `tests/differential/tests/http_filter_rbac.rs` (Docker-gated wrapper)
- Possibly modify: `tests/differential/src/lib.rs` (per-probe `request_headers` field extension on `Http1Probe` IF not already present — verified at task start via Read)

### Steps

- [ ] **Step 1: Verify the harness's `Http1Probe` shape supports per-probe distinct request headers.**

Read `tests/differential/src/lib.rs` around the `Http1Probe` struct definition (search
for `pub struct Http1Probe`). The phase-09 fixture-0016 used uniform headers across all
5 probes; phase-10's fixture is the first to need distinct per-probe headers. If
`Http1Probe` does not yet have a `request_headers: Option<Vec<(String, String)>>` (or
similar) field, add it with `#[serde(default)]` and thread it into the request-build
code path in `drive_http_get` (or wherever the harness builds the GET request).

- [ ] **Step 2: Author `tests/fixtures/0017-http-filter-rbac/envoy.yaml`.**

```yaml
# Phase 10: 4-probe sequential burst against the HCM filter chain
#   [envoy.filters.http.rbac, envoy.filters.http.router]
# with action: ALLOW + one policy "pass_with_header" requiring the
# x-rbac-pass: yes header on the request.
#
# Per upstream Envoy v1.33's envoy.extensions.filters.http.rbac.v3.RBAC,
# the filter denies any request that doesn't match a policy under
# action: ALLOW. Empirical 403 response shape (verified at phase-10
# state-2 PLAN-write via Docker run of envoyproxy/envoy:v1.33.0):
#   - status 403
#   - body bytes "RBAC: access denied" (19 bytes, NO trailing \n)
#   - 5 standard HTTP/1.1 headers under harness Connection: close framing:
#     {content-length: 19, content-type: text/plain, date: ..., server: envoy, connection: close}
#
# Per ADR-0034 (phase-10 state-2 §6.2 empirical-verification revision):
# the SPEC §2.2 projection of "RBAC: access denied\n" (20 bytes) was
# off by 1 byte; the actual upstream body is "RBAC: access denied"
# (19 bytes). envoy-rust matches this exact body via
# `Bytes::from_static(b"RBAC: access denied")` per phase-10 PLAN
# lock-in #14.
admin:
  address:
    socket_address: { address: 0.0.0.0, port_value: 9901 }
static_resources:
  listeners:
  - name: ingress_http
    address: { socket_address: { address: 0.0.0.0, port_value: 10000 } }
    filter_chains:
    - filters:
      - name: envoy.filters.network.http_connection_manager
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
          stat_prefix: ingress_http
          http_filters:
          - name: envoy.filters.http.rbac
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.filters.http.rbac.v3.RBAC
              rules:
                action: ALLOW
                policies:
                  "pass_with_header":
                    permissions:
                    - any: true
                    principals:
                    - header:
                        name: x-rbac-pass
                        string_match: { exact: "yes" }
          - name: envoy.filters.http.router
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
          route_config:
            name: local
            virtual_hosts:
            - name: default
              domains: ["*"]
              routes:
              - match: { prefix: "/" }
                direct_response: { status: 200, body: { inline_string: "ok\n" } }
```

- [ ] **Step 3: Author `tests/fixtures/0017-http-filter-rbac/envoy-rust.yaml`.**

Identical to `envoy.yaml` (phase-10 introduces no envoy-rust-vs-envoy config divergence;
the `admin` block may differ if envoy-rust's admin shape differs, but the static_resources
block is identical).

- [ ] **Step 4: Author `tests/fixtures/0017-http-filter-rbac/expectations.yaml`.**

```yaml
driver:
  kind: http1_probe_list
  probes:
    - name: probe-1-deny-no-header
      method: get
      path: /
      host: envoy-rust.test
      request_headers: []
      expected_status: 403
      expected_body: { kind: byte_exact, body: "RBAC: access denied" }
      expected_headers: set_equal_modulo_allow_list
    - name: probe-2-allow-with-header
      method: get
      path: /
      host: envoy-rust.test
      request_headers:
        - [x-rbac-pass, "yes"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "ok\n" }
      expected_headers: set_equal_modulo_allow_list
    - name: probe-3-deny-wrong-value
      method: get
      path: /
      host: envoy-rust.test
      request_headers:
        - [x-rbac-pass, "no"]
      expected_status: 403
      expected_body: { kind: byte_exact, body: "RBAC: access denied" }
      expected_headers: set_equal_modulo_allow_list
    - name: probe-4-allow-with-header-again
      method: get
      path: /
      host: envoy-rust.test
      request_headers:
        - [x-rbac-pass, "yes"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "ok\n" }
      expected_headers: set_equal_modulo_allow_list
```

- [ ] **Step 5: Author `tests/fixtures/0017-http-filter-rbac/README.md`.**

```markdown
# Fixture 0017 — `envoy.filters.http.rbac` (phase 10)

Bilateral RBAC filter regression. 4 sequential GET probes against the HCM with
[envoy.filters.http.rbac, envoy.filters.http.router] under `action: ALLOW`
with a single policy `pass_with_header` requiring the `x-rbac-pass: yes`
header on the request. Probes alternate header presence to exercise both the
Allow-match (200) and the Allow-no-match-default-Deny (403) paths.

Per ADR-0034 (phase-10 state-2 §6.2 empirical-verification revision): the
403 response body is `"RBAC: access denied"` (19 bytes, NO trailing newline);
upstream Envoy v1.33's `envoy.extensions.filters.http.rbac.v3.RBAC`
source-hardcodes this body. envoy-rust matches via
`Bytes::from_static(b"RBAC: access denied")` per phase-10 PLAN lock-in #14.

The 5 standard HTTP/1.1 response headers (`server`, `date`, `content-length`,
`content-type`, `connection`) are decorated onto the filter-synth 403 by the
existing H1 HCM `decorate_filter_synth_response` helper (landed at phase-09
ADR-0033 Commit C `ae2cef0`). This fixture is the **first non-LocalRateLimit
consumer** of the helper, validating its filter-agnostic design.
```

- [ ] **Step 6: Author `tests/differential/tests/http_filter_rbac.rs` (Docker-gated wrapper).**

Mirror `tests/differential/tests/http_filter_local_rate_limit.rs` shape (phase-09
precedent). Single `#[tokio::test]` invoking `run_fixture("0017-http-filter-rbac").await`
per the harness convention.

```rust
//! Docker-gated differential fixture for `envoy.filters.http.rbac` (phase 10).
//!
//! Gated behind the `differential_docker` cfg per the 04.1+ harness convention;
//! skipped in non-Docker CI environments.

#[cfg(differential_docker)]
mod docker {
    use differential::run_fixture;

    #[tokio::test]
    async fn http_filter_rbac_fixture() {
        run_fixture("0017-http-filter-rbac")
            .await
            .expect("fixture 0017 green");
    }
}
```

- [ ] **Step 7: Run the fixture locally.**

```
cargo test -p differential --test http_filter_rbac --features differential_docker -- --nocapture
```

Expected: PASS. If FAIL, inspect Docker logs + the harness's diff output for header /
body byte divergences.

- [ ] **Step 8: Run the workspace test bucket + 5 gates.**

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo deny check
```

Expected: all 5 PASS.

- [ ] **Step 9: Append to PROGRESS.md.**

Append `### Task 5 — D8.1 fixture 0017 + Docker-gated wrapper` subsection.

- [ ] **Step 10: Commit.**

```bash
git add tests/fixtures/0017-http-filter-rbac/ tests/differential/tests/http_filter_rbac.rs \
        tests/differential/src/lib.rs \
        docs/envoy-rust/phases/10-http-filter-rbac/PROGRESS.md
git commit -m "phase 10: task 5 — D8.1 fixture 0017 + Docker-gated wrapper"
```

---

## Task 6: D8.2 fuzz corpus seed

**Goal.** Land a new fuzz corpus seed `hcm_rbac_filter.yaml` for the
`parse_bootstrap` fuzz target; extend the `.gitignore` allow-list AND the in-source
`fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS array per the phase-09 Task 6
follow-up lesson (BOTH edits in ONE commit, not a follow-up).

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_rbac_filter.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` (1-line allow-list extension)
- Modify: `crates/envoy-config/src/bootstrap.rs` (extend `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array)

### Steps

- [ ] **Step 1: Create the fuzz corpus seed.**

Author `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_rbac_filter.yaml`
containing the same bootstrap shape as the fixture 0017 envoy-rust.yaml (or a
minimal variant — the goal is parse-acceptance, not behavior verification).

```yaml
admin:
  address:
    socket_address: { address: 0.0.0.0, port_value: 9901 }
static_resources:
  listeners:
  - name: ingress_http
    address: { socket_address: { address: 0.0.0.0, port_value: 10000 } }
    filter_chains:
    - filters:
      - name: envoy.filters.network.http_connection_manager
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
          stat_prefix: ingress_http
          http_filters:
          - name: envoy.filters.http.rbac
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.filters.http.rbac.v3.RBAC
              rules:
                action: ALLOW
                policies:
                  "pass_with_header":
                    permissions:
                    - any: true
                    principals:
                    - header:
                        name: x-rbac-pass
                        string_match: { exact: "yes" }
          - name: envoy.filters.http.router
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
          route_config:
            name: local
            virtual_hosts:
            - name: default
              domains: ["*"]
              routes:
              - match: { prefix: "/" }
                direct_response: { status: 200, body: { inline_string: "ok\n" } }
```

- [ ] **Step 2: Extend the `.gitignore` allow-list.**

In `crates/envoy-config/fuzz/.gitignore`, find the existing `!corpus/parse_bootstrap/*.yaml`
allow-list block. Verify the new file is covered. If the allow-list uses per-file lines
(e.g., `!corpus/parse_bootstrap/hcm_local_rate_limit_filter.yaml`), add the matching
1-line entry:

```
!corpus/parse_bootstrap/hcm_rbac_filter.yaml
```

- [ ] **Step 3: Extend the in-source SUCCESS array.**

In `crates/envoy-config/src/bootstrap.rs`, find the
`fuzz_corpus_seeds_parse_or_reject_cleanly` test function (or whatever its current
name is — search for `fuzz_corpus`). Find the SUCCESS-array literal (the array of
filenames the test expects to parse cleanly). Add the new filename:

```rust
"hcm_rbac_filter.yaml",
```

In alphabetical position within the existing SUCCESS array.

- [ ] **Step 4: Run the parse_bootstrap fuzz test to verify the seed parses.**

```
cargo test -p envoy-config --lib fuzz_corpus_seeds_parse_or_reject_cleanly
```

Expected: PASS — the new seed parses cleanly through the parser + validator.

- [ ] **Step 5: Run the 5 stable-toolchain gates.**

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo deny check
```

Expected: all 5 PASS.

- [ ] **Step 6: Append to PROGRESS.md.**

Append `### Task 6 — D8.2 fuzz corpus seed` subsection.

- [ ] **Step 7: Commit.**

```bash
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_rbac_filter.yaml \
        crates/envoy-config/fuzz/.gitignore \
        crates/envoy-config/src/bootstrap.rs \
        docs/envoy-rust/phases/10-http-filter-rbac/PROGRESS.md
git commit -m "phase 10: task 6 — D8.2 fuzz corpus seed (hcm_rbac_filter.yaml)"
```

---

## Task 7: D8.3 in-process backstop (closes 09 REVIEW M3 via kill_on_drop discipline)

**Goal.** Land the in-process backstop test `crates/envoy-bin/tests/http_filter_rbac.rs`
that exercises the RBAC filter end-to-end against a real envoy-bin subprocess (no
Docker). Per SPEC §6.4 + 09 REVIEW M3 disposition: **use `tokio::process::Command +
.kill_on_drop(true) + Stdio::null()`** (NOT `std::process::Command`). Closes 09 REVIEW
M3 at the named close site.

**Direct code-spot-check required before writing** (per SPEC §6.4 + the awareness-only
doctrine note in phase-09 REVIEW M3's Process note): read
`crates/envoy-bin/tests/admin_drain_listeners.rs` (08.2 backstop precedent) +
`crates/envoy-bin/tests/http_filter_header_mutation.rs` (07.2 backstop precedent)
directly via `Read` tool to verify the established `tokio::process::Command +
kill_on_drop(true)` shape before authoring this test. Do NOT rely on the prior phase's
PROGRESS narrative claim.

**Files:**
- Create: `crates/envoy-bin/tests/http_filter_rbac.rs`

### Steps

- [ ] **Step 1: Direct code-spot-check the precedent backstops.**

```
cat crates/envoy-bin/tests/admin_drain_listeners.rs | head -100
cat crates/envoy-bin/tests/http_filter_header_mutation.rs | head -100
```

Verify the established shape:
- `use tokio::process::Command;`
- `Command::new("cargo").args(...).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true).spawn()`
- (or similar — confirm exact pattern at task time)

- [ ] **Step 2: Author `crates/envoy-bin/tests/http_filter_rbac.rs`.**

```rust
//! Phase-10 in-process backstop: end-to-end RBAC filter exercise against a real
//! envoy-bin subprocess (no Docker).
//!
//! Per phase-09 REVIEW M3 disposition + phase-10 SPEC §6.4: uses
//! `tokio::process::Command + .kill_on_drop(true) + Stdio::null()` discipline
//! adopted directly from the 07.2 + 08.2 backstop precedents — NO regression
//! to `std::process::Command`. CLOSES 09 REVIEW M3 at this named site.
//!
//! Bootstrap shape: HCM + [envoy.filters.http.rbac, envoy.filters.http.router]
//! with action: ALLOW + one policy `pass_with_header` requiring x-rbac-pass: yes
//! on the request. 4 sequential GET probes alternate header presence:
//!   probe 1 (no header)         → 403, body "RBAC: access denied"
//!   probe 2 (x-rbac-pass: yes)  → 200, body "ok\n"
//!   probe 3 (x-rbac-pass: no)   → 403, body "RBAC: access denied"
//!   probe 4 (x-rbac-pass: yes)  → 200, body "ok\n"

use std::io::Write;
use std::net::SocketAddr;
use std::process::Stdio;
use std::time::Duration;
use tempfile::NamedTempFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::time::{sleep, timeout};

/// Reserve a free TCP port on 127.0.0.1.
fn reserve_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("local_addr").port();
    drop(listener);
    port
}

#[tokio::test]
async fn http_filter_rbac_in_process_backstop() {
    let admin_port = reserve_port();
    let listener_port = reserve_port();

    let bootstrap_yaml = format!(
        r#"admin:
  address:
    socket_address: {{ address: 127.0.0.1, port_value: {admin_port} }}
static_resources:
  listeners:
  - name: ingress_http
    address: {{ socket_address: {{ address: 127.0.0.1, port_value: {listener_port} }} }}
    filter_chains:
    - filters:
      - name: envoy.filters.network.http_connection_manager
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
          stat_prefix: ingress_http
          http_filters:
          - name: envoy.filters.http.rbac
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.filters.http.rbac.v3.RBAC
              rules:
                action: ALLOW
                policies:
                  "pass_with_header":
                    permissions:
                    - any: true
                    principals:
                    - header:
                        name: x-rbac-pass
                        string_match: {{ exact: "yes" }}
          - name: envoy.filters.http.router
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
          route_config:
            name: local
            virtual_hosts:
            - name: default
              domains: ["*"]
              routes:
              - match: {{ prefix: "/" }}
                direct_response: {{ status: 200, body: {{ inline_string: "ok\n" }} }}
"#
    );

    let mut bootstrap_file = NamedTempFile::new().expect("tempfile");
    bootstrap_file
        .write_all(bootstrap_yaml.as_bytes())
        .expect("write bootstrap");
    let bootstrap_path = bootstrap_file.path().to_path_buf();

    // Per phase-09 REVIEW M3 + phase-10 SPEC §6.4 + 07.2/08.2 precedent:
    // tokio::process::Command + .kill_on_drop(true) + Stdio::null() on stdout.
    let mut child = Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .args(["-c", bootstrap_path.to_str().expect("utf8 path")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin");

    // Wait for the listener to come up.
    let listener_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    for attempt in 0..50 {
        match TcpStream::connect(&listener_addr).await {
            Ok(_) => break,
            Err(_) if attempt == 49 => panic!("envoy-bin listener never came up"),
            Err(_) => sleep(Duration::from_millis(100)).await,
        }
    }

    async fn probe(addr: SocketAddr, extra_header: Option<(&str, &str)>) -> (u16, Vec<u8>) {
        let mut stream = TcpStream::connect(addr).await.expect("connect");
        let mut req = format!("GET / HTTP/1.1\r\nHost: envoy-rust.test\r\nConnection: close\r\n");
        if let Some((n, v)) = extra_header {
            req.push_str(&format!("{n}: {v}\r\n"));
        }
        req.push_str("\r\n");
        stream.write_all(req.as_bytes()).await.expect("write req");
        let mut buf = Vec::new();
        timeout(Duration::from_secs(5), stream.read_to_end(&mut buf))
            .await
            .expect("read timeout")
            .expect("read");
        let head_end = buf
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("header terminator");
        let head = &buf[..head_end];
        let body = buf[head_end + 4..].to_vec();
        let head_str = std::str::from_utf8(head).expect("ascii head");
        let status_line = head_str.lines().next().expect("status line");
        // e.g., "HTTP/1.1 403 Forbidden"
        let status: u16 = status_line
            .split_whitespace()
            .nth(1)
            .expect("status code")
            .parse()
            .expect("parse status");
        (status, body)
    }

    let (s1, b1) = probe(listener_addr, None).await;
    assert_eq!(s1, 403, "probe-1 (no header) → 403");
    assert_eq!(&b1[..], b"RBAC: access denied", "probe-1 body");

    let (s2, b2) = probe(listener_addr, Some(("x-rbac-pass", "yes"))).await;
    assert_eq!(s2, 200, "probe-2 (x-rbac-pass: yes) → 200");
    assert_eq!(&b2[..], b"ok\n", "probe-2 body");

    let (s3, b3) = probe(listener_addr, Some(("x-rbac-pass", "no"))).await;
    assert_eq!(s3, 403, "probe-3 (x-rbac-pass: no) → 403");
    assert_eq!(&b3[..], b"RBAC: access denied", "probe-3 body");

    let (s4, b4) = probe(listener_addr, Some(("x-rbac-pass", "yes"))).await;
    assert_eq!(s4, 200, "probe-4 (x-rbac-pass: yes) → 200");
    assert_eq!(&b4[..], b"ok\n", "probe-4 body");

    // kill_on_drop(true) ensures the child terminates when `child` drops at scope exit.
    drop(child);
}
```

(Note: the precise `tokio::process::Command` API call shape + the `tempfile` /
`reserve_port` helper imports may need slight adjustments to match the exact 07.2 /
08.2 backstop conventions. The Task 7 implementer reads those precedents directly via
`Read` tool first and adapts the boilerplate.)

- [ ] **Step 3: Run the backstop.**

```
cargo test -p envoy-bin --test http_filter_rbac -- --nocapture
```

Expected: PASS — all 4 probes match the expected status + body.

- [ ] **Step 4: Run the 5 stable-toolchain gates.**

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo deny check
```

Expected: all 5 PASS.

- [ ] **Step 5: Append to PROGRESS.md.**

Append `### Task 7 — D8.3 in-process backstop (closes 09 REVIEW M3)` subsection. Record
the closure attribution + the direct code-spot-check evidence + the kill_on_drop
discipline adoption.

- [ ] **Step 6: Commit.**

```bash
git add crates/envoy-bin/tests/http_filter_rbac.rs \
        docs/envoy-rust/phases/10-http-filter-rbac/PROGRESS.md
git commit -m "phase 10: task 7 — D8.3 in-process backstop (closes 09 REVIEW M3)"
```

---

## Task 8: state-4 phase-done verification + STATE advance to state-5-next

**Goal.** Land the state-4 evidence anchor in PROGRESS.md (17-fixture green
simultaneously + per-gate quoted output + CI run URL + HEAD SHA + completion timestamp)
+ advance STATE.md to `state 4-complete / state-5-next` per the established 06.x /
07.x / 08.x / 09 cadence.

**Files:**
- Modify: `docs/envoy-rust/phases/10-http-filter-rbac/PROGRESS.md` (state-4 evidence anchor subsection).
- Modify: `docs/envoy-rust/STATE.md` (advance Active phase status + Next expected skill).

### Steps

- [ ] **Step 1: Push the prior task commits + wait for CI green.**

```bash
git push origin main
gh run watch
```

Confirm CI run completes `success` on all jobs: `build + test + lint (Linux + macOS)`,
`fuzz (parse_bootstrap, 30s)`, `differential (Docker)`. Record the CI run URL + HEAD
SHA + completion timestamp for the PROGRESS state-4 evidence anchor.

- [ ] **Step 2: Locally re-run all 5 stable-toolchain gates + record quoted output.**

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo deny check
```

Record exit codes + meaningful output (e.g., "729 passed, 0 failed, 2 ignored" for
`cargo test`). Per the 09 Task 8 precedent.

- [ ] **Step 3: Verify all 17 Docker-gated fixtures green simultaneously.**

```
cargo test -p differential --features differential_docker -- --nocapture
```

Expected: all 17 fixtures (`0001-tcp-echo` through `0017-http-filter-rbac`) green.

- [ ] **Step 4: Verify the in-process backstop.**

```
cargo test -p envoy-bin --test http_filter_rbac
```

Expected: PASS.

- [ ] **Step 5: Verify the parse_bootstrap fuzz target on the 17-seed corpus.**

```
cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30 -runs=100000
```

Expected: clean exit; no crashes; corpus extends from 16 to 17 seeds verified at
parse-or-reject-cleanly.

- [ ] **Step 6: Write the state-4 evidence anchor in PROGRESS.md.**

Append `### Task 8 — state-4 phase-done verification + STATE advance to state-5-next`
subsection mirroring the phase-09 Task 8 commit `a5ebddd` shape. Include per-gate
quoted output + CI run URL + HEAD SHA + completion timestamp + the 17-fixture
green-simultaneously evidence + the closure attributions for 09 REVIEW M2 (D5) + 09
REVIEW M3 (D8.3) + ADR-0034 (state-2 commit).

- [ ] **Step 7: Advance STATE.md Active phase status.**

In `docs/envoy-rust/STATE.md`, update the `**status:**` line to:

```markdown
**status:** phase 10 lifecycle state 4-complete / state-5-next (PLAN.md + PROGRESS.md + state-4 evidence landed; REVIEW.md pending).
```

Rewrite "Next expected skill" to `superpowers:requesting-code-review` scoped to the
phase-10 arc. Rewrite "Last commit" + "Last updated". Append a new "Phase-10 state-3
execution arc" subsection in Notes recording the per-task commit chain + the
ADR-0034 landing + the M2/M3 closures + the lock-in voidings if any. Preserve all
prior subsections verbatim per D-3.5 + D-3.4.

- [ ] **Step 8: Commit.**

```bash
git add docs/envoy-rust/phases/10-http-filter-rbac/PROGRESS.md \
        docs/envoy-rust/STATE.md
git commit -m "phase 10: task 8 — state-4 phase-done verification + STATE advance to state-5-next"
git push origin main
```

Verify CI green on the push.

---

*End of PLAN. Phase 10 state-2 lifecycle complete on landing of this PLAN.md + the
PROGRESS.md skeleton + Task 1 preamble + ADR-0034 + the inline SPEC §2.2 + §3 D8.1 +
§5.9 revisions, in a single standalone pre-Task-1 commit. The next session enters
state 3 — dispatches Task 1 per `superpowers:subagent-driven-development` per the
`feedback_execution_style` standing preference.*
