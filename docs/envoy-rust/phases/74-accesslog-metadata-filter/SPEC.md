# Phase 74 — Observability family: the access-log FILTER subsystem — arm #6, `metadata_filter` (the DYNAMIC-METADATA emission gate)

> **State-0/1 pick session** (per `BOOTSTRAP_PROMPT.md` §5 state-0/1 + memory
> `closeout-and-pick-are-separate-sessions`). This SPEC is the brainstorm output
> for a NEW phase `74`. Every §0 wire/behavior claim below was **MEASURED this
> session against `envoyproxy/envoy:v1.33.0`** (`docs/envoy-rust/ENVOY_TARGET.md`
> pin, digest `sha256:56da5afd…770c2`) via four read-only recon fan-outs (two
> in-tree change-surface surveys, one live `--mode validate` wire-shape sweep, one
> in-tree `HeaderMatcher`-parity costing) PLUS six port-mapped **live runtime
> probes run by the main session** with graceful-stop flush.

**Pick in one line:** the `AccessLogFilter` oneof has FIVE arms
(`status_code_filter` / `response_flag_filter` / `header_filter` +
the recursive `and_filter` / `or_filter`); add the SIXTH,
**`metadata_filter`** — a per-sink predicate gating emission on the request's
**dynamic metadata**. This is the arm the phase-70/71/72/73 SPECs each deferred as
"needs dynamic-metadata plumbing" — **and that plumbing already landed**
(phases 33/34/35/36). The record ALREADY carries the metadata store, and the
matcher types + value engine ALREADY exist; the arm is now the cheapest remaining
leaf, and it opens a genuinely NEW gating axis.

---

## §0. State-0 recon — evidence (MEASURED this session against `envoyproxy/envoy:v1.33.0`)

### R-0.1 — the pick's central finding: the dynamic-metadata plumbing ALREADY EXISTS

The three prior arm picks each rejected `metadata_filter` on the grounds that it
"needs dynamic-metadata plumbing". That premise is now **false** — MEASURED
in-tree:

- **`AccessLogRecord` ALREADY carries the whole store.**
  `crates/envoy-accesslog/src/record.rs:111-112`:
  `pub dynamic_metadata: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>`,
  populated at the HCM record-build site and rendered today by
  `%DYNAMIC_METADATA(ns:key)%` (phase 33). **No new record field, no new request-data
  threading.**
- **The store is IN SCOPE at BOTH emit gates.** H1
  `crates/envoy-http1/src/hcm.rs:1508-1531` (gate at `:1515`) and H2
  `crates/envoy-http2/src/hcm.rs:1131-1157` (gate at `:1138`) both hold the fully
  built `record` — so `record.dynamic_metadata` is available at the gate on both
  codecs with **zero** new plumbing.
- **The matcher CONFIG TYPES already exist**, built for the phase-35/36 RBAC
  `metadata` condition and re-exported from `envoy-config`:
  `MetadataMatcher { filter: String, path: Vec<MetadataPathSegment>, value: ValueMatcher }`
  (`crates/envoy-config/src/bootstrap.rs:1628-1638`), `MetadataPathSegment { key: String }`
  (`:1640-1644`), `ValueMatcher::{StringMatch, PresentMatch}` (`:1658-1670`),
  `StringMatcher` 5-mode (`:2915-2944`). Upstream's `metadata_filter.matcher` is
  **the same `type.matcher.v3.MetadataMatcher` message** (R-0.2) → the config type
  is reused verbatim.
- **The value ENGINE is `pub` and store-agnostic.**
  `crates/envoy-config/src/matcher.rs:125-130`:
  `pub fn ValueMatcher::matches(&self, value: &str) -> bool`. It takes a bare
  `&str`, so it composes with any resolution strategy.
- **The producers are landed and fixture-proven.** `envoy.filters.http.set_metadata`
  (phase 33) and `envoy.filters.http.header_to_metadata` (phase 34), witnessed by
  differential fixtures `0041`/`0042`/`0043`/`0044`.
- **The differential driver needs ZERO change.** `Driver::Http1AccessLogByteExact`
  + `AccessLogByteExactProbe` (`tests/differential/src/lib.rs:159-165`, `:1104-1119`)
  already carry `extra_headers` + `expect_logged`, and
  `expected_logged_count` (`:1134-1136`) computes the line target — exactly as
  fixtures `0078`/`0079`/`0080` use them.

**The ONE genuinely new mechanical cost** (R-0.6): `LogFilter::should_log` has a
fixed 3-arg signature and must be widened to see the metadata.

### R-0.2 — LIVE-ENVOY (`--mode validate`, networking-free): the `metadata_filter` wire shape

Probes against `envoyproxy/envoy:v1.33.0 --mode validate` (an H1 HCM
`AccessLog.filter`), MEASURED:

- **Shape confirmed exactly:**
  `metadata_filter: { matcher: <type.matcher.v3.MetadataMatcher>, match_if_key_not_found: <BoolValue> }`,
  a mutually-exclusive `AccessLogFilter` oneof arm. Minimal positive
  (`matcher: { filter: "com.example", path: [{key: k}], value: { string_match: { exact: "v" } } }`)
  → `configuration OK`.
- **`matcher` is OPTIONAL — `metadata_filter: {}` VALIDATES.** Both
  `metadata_filter: {}` and `metadata_filter: { match_if_key_not_found: true }`
  report `configuration OK`. `MetadataFilter.matcher` carries **no**
  `(validate.rules).message.required`. **A load-parity trap: envoy-rust must NOT
  reject a matcher-less `metadata_filter`.**
- **Inside `matcher`, three fields ARE required** (PGV):
  - `filter` — `min_len: 1` (omitted → `MetadataMatcherValidationError.Filter: value length must be at least 1 characters`).
  - `path` — `min_items: 1` (`path: []` and an omitted `path` are the SAME wire
    message → `MetadataMatcherValidationError.Path: value must contain at least 1 item(s)`).
    **No max_items** — multi-segment (`[{key: a}, {key: b}]`) validates.
    Each segment is a `required` oneof named `segment`; its `key` is `min_len: 1`.
  - `value` — `message.required` (omitted → `Value: value is required`); `value: {}`
    fails differently, on the inner oneof (`field: "match_pattern", reason: is required`).
- **`value` accepts far more `ValueMatcher` kinds than envoy-rust models** — all of
  `string_match{exact,prefix,suffix,contains,safe_regex,ignore_case}`, `bool_match`,
  `present_match`, `double_match{exact,range}`, `list_match{one_of}`, `null_match`,
  `or_match` validate. (`safe_regex.google_re2` still parses but emits a
  deprecation warning; `safe_regex: { regex: … }` is the clean v1.33 spelling.)
- **`match_if_key_not_found` is a `google.protobuf.BoolValue` WRAPPER** — decisive
  probe: `{ value: true }` is accepted alongside bare `true`/`false`. A plain
  `bool` field would reject the wrapped form. So absent and explicit-`false` are
  **distinct on the wire** (`None` vs `Some(false)`); in YAML you write a bare
  `true`/`false`.
- **`matcher.invert` is a plain `bool`, accepted** (`invert: true` and
  `invert: false` both `configuration OK`). See R-0.5 — it is accepted but
  **INERT** on this path.
- **oneof mutual exclusivity** — setting BOTH `metadata_filter` and `header_filter`
  is REJECTED one layer ABOVE PGV, in the JSON→proto parser:
  `'header_filter' has already been set (either directly or as part of a oneof)`.
- **The message is CLOSED** — an unknown key inside `metadata_filter` is a hard
  error (`no such field: 'bogus_field'`).
- **NO producer cross-check.** A `metadata_filter` naming namespace `com.example`
  validates with a router-only `http_filters` chain and no `set_metadata` anywhere.
  `filter:` is an opaque, unvalidated namespace string — envoy-rust must likewise
  attempt no linkage check.

### R-0.3 — LIVE-ENVOY (runtime, port-mapped, no backend, graceful-stop flush): the keep/drop decision is deterministic and byte-exact

A port-mapped H1 HCM, `header_to_metadata` mapping request header `x-a` →
`com.example:k`, file access log
`text_format_source "S=%RESPONSE_CODE% P=%REQ(:PATH)% M=%DYNAMIC_METADATA(com.example:k)%\n"`,
`filter: { metadata_filter: { matcher: { filter: com.example, path: [{key: k}], value: { string_match: { exact: "1" } } } } }`,
one `direct_response` `/x → 200`, **no cluster / no upstream**. Three requests
(`x-a: 1`, `x-a: 2`, no header), then `docker stop` (SIGTERM graceful flush).
Access-log file (MEASURED):

```
S=200 P=/x M=1        # x-a:1 → metadata k="1" → exact "1" MATCHES → KEPT
S=200 P=/x M=-        # no x-a → key NOT FOUND  → match_if_key_not_found → KEPT
```

`x-a: 2` (metadata `k="2"`, value mismatch) → **DROPPED**. A clean three-way
observable on a single deterministic backend-free config.

### R-0.4 — LIVE-ENVOY (runtime): `match_if_key_not_found` — the DEFAULT is `true`; explicit `false` drops the absent case

Same fixture, adding `match_if_key_not_found: false`. Access log (MEASURED):

```
S=200 P=/x M=1        # x-a:1 → value matches → KEPT
```

Both `x-a: 2` (value mismatch) and the **no-header** probe → **DROPPED**. Compare
R-0.3, where the identical no-header probe was KEPT. This **resolves the wrapper
default that `--mode validate` provably cannot measure** (R-0.2): absent
`match_if_key_not_found` behaves as `true`, explicit `false` as `false`. The
runtime rule is therefore:

```
resolved = dynamic_metadata[matcher.filter][matcher.path[0].key]
match resolved {
    None    => match_if_key_not_found,     // default true
    Some(v) => matcher.value.matches(v),
}
```

Corroborated independently: pointing `filter:` at a namespace no filter ever writes
(`com.nonexistent`) made every record "key not found" → **all three KEPT** under
the default.

### R-0.5 — LIVE-ENVOY (runtime): `matcher.invert` is ACCEPTED but **INERT** on the access-log path (a NEW divergence trap)

`invert: true` added to the R-0.3 matcher, run **twice**. Both runs produced a
byte-identical keep/drop set to the NON-inverted R-0.3 run:

```
S=200 P=/x M=1        # value MATCHES exact "1" → still KEPT
S=200 P=/x M=-        # key absent            → still KEPT
```

with `x-a: 2` still DROPPED. Had `invert` been honored, `M=2` would be KEPT and
`M=1` DROPPED — the exact opposite. **Control probe:** a sibling bogus field
(`invertBOGUS: true`) is REJECTED
(`message envoy.type.matcher.v3.MetadataMatcher … no such field: 'invertBOGUS'`),
proving `invert` is a genuine, recognised field of the message — accepted by the
parser and then ignored by this evaluation path. Per D-3.3 the MEASURED behavior
is the contract; envoy-rust must not "implement" `invert` here. §2.2 scopes it out
and §10 opens a carry-forward.

### R-0.6 — the ONE genuinely new mechanical cost: the `should_log` signature

`crates/envoy-accesslog/src/filter.rs:71-112`:
`pub fn should_log(&self, status: u16, response_flags: &str, headers: &[(String, String)]) -> bool`.
A metadata arm needs the store as a fourth input. MEASURED call-site census
(`grep -rn 'should_log' crates/ tests/`): **102 occurrences** —
`envoy-accesslog/src/filter.rs` 42, `envoy-http1/src/hcm.rs` 49,
`envoy-accesslog/src/file_sink.rs` 7, `envoy-http2/src/hcm.rs` 4. The large
majority are in-process **test** call sites taking literals like `(200, "-", &[])`;
the production sites are exactly four (`filter.rs:71` definition,
`file_sink.rs:102` wrapper, H1 `hcm.rs:1515`, H2 `hcm.rs:1138`). The edit is
mechanical (one added argument per site) but is the phase's single largest LoC
line item (§8). PV-5 weighs it against the record-based consolidation alternative
(§4).

### R-0.7 — the ADR-0150 cycle seam applies unchanged, and the trait can be cleaner here

`crates/envoy-accesslog/Cargo.toml` has **ZERO workspace dependencies**;
`crates/envoy-config/Cargo.toml` depends on `envoy-accesslog` — so the reverse edge
is a hard Cargo cycle (ADR-0150). Phase 72's precedent is exact:
`pub trait HeaderMatch` in `envoy-accesslog` (`filter.rs:32-35`), the sole impl in
`envoy-config` (`matcher.rs:63-70`), boxed into `LogFilter::Header` by the
`envoy-http1` compile step (`hcm.rs:1747-1786`), which depends on both.

A metadata equivalent must do the **resolution** inside the trait impl —
`envoy-accesslog` cannot see `MetadataMatcher`'s `filter`/`path` fields. The
`match_if_key_not_found` policy, however, lives on the FILTER (not the matcher),
so the natural split is a trait returning **`Option<bool>`** (`None` = the path did
not resolve) and letting `LogFilter` apply the not-found default. That keeps R-0.4's
rule expressed exactly once, in the crate that owns the field. PV-4 confirms.

The existing RBAC path resolution (`eval_metadata`,
`crates/envoy-filter/src/rbac.rs:77-88`) is **NOT reusable**: it is private and
takes `&FilterRequest`, not the store.

### R-0.8 — the in-tree `MetadataMatcher` is STRICTER than upstream in three inherited ways

MEASURED in-tree, all pre-existing phase-35/36 boundaries (NOT introduced by this
phase):

1. **No `invert` field.** `MetadataMatcher` (`bootstrap.rs:1628-1638`) has exactly
   `{filter, path, value}` under `#[serde(deny_unknown_fields)]` → a config carrying
   `invert:` is boot-fatal here, while upstream accepts it (R-0.2) and ignores it
   (R-0.5).
2. **Single-segment `path` only.** `validate_metadata_matcher`
   (`bootstrap.rs:4836-4858`) rejects `path.len() != 1`; upstream accepts
   multi-segment (R-0.2). This is *forced* by the data model — the record's store is
   a FLAT `BTreeMap<String, BTreeMap<String, String>>` (R-0.1), in which a
   two-level path is unrepresentable.
3. **`ValueMatcher` models 2 of upstream's 7+ arms** (`string_match`,
   `present_match` — `bootstrap.rs:1658-1670`); `bool_match`/`double_match`/
   `list_match`/`null_match`/`or_match` are boot-fatal here, accepted upstream.

All three are fail-loud (envoy-rust refuses to BOOT; runtime behavior never
silently differs), consistent with the ADR-0049 posture and the `BEHAVIOR_CONTRACT.md`
§E.1 "stricter than upstream" precedent. §2.2 keeps them; §10 carries them forward.

Note that `validate_metadata_matcher` is **RBAC-scoped** — it is a private fn taking
`listener_name`/`policy_name` and producing `ConfigError::RbacMetadataMatcherInvalid`
(`lib.rs:675`) — so it is **not** directly callable from the access-log validator
(PV-2).

### R-0.9 — the rejected-alternative costing was MEASURED too (CF-72-1 / CF-72-2)

The `HeaderMatcher`-parity candidate was costed in-tree rather than assumed. The
CF-72-1 fix is **one expression** (`crates/envoy-config/src/matcher.rs:51`,
`mode_result ^ invert_match`) — but that expression is read by **five call sites
across four subsystems**: H1+H2 route matching (`envoy-http1/src/hcm.rs:2141`, and
H2 delegates via `resolve_route`), HTTP RBAC (`envoy-filter/src/rbac.rs:60`), the
fault filter's header gate (`fault.rs:76`), JWT authn (`jwt_authn.rs:185`), and the
access-log `header_filter` (`filter.rs:100`). **Zero** differential fixtures use
`invert_match` anywhere (grep over all fixture YAML → no hits), so a fix carries no
existing coverage and would need NEW fixtures on the route path AND the access-log
path. CF-72-2's `treat_missing_header_as_empty` half additionally interacts with
CF-72-1's absence handling, and phase 72 already recorded a **mutation check**
(`72-.../PROGRESS.md:355-360`) proving the naive uniform fix breaks the
`present_match` parity pin. §4 rejects it on this measured basis.

### R-0.10 — numbering

Next ROADMAP id **74**; next fixture ids **0081** + **0082** (`0080` landed; 80
fixture dirs exist); next ADR **ADR-0154** (ledger head ADR-0153; ADR-0154 is FREE
— the phase-73 §6.1 split reservation expired UNFIRED).

---

## §1. Goal

Land **`metadata_filter`**, the SIXTH `envoy.config.accesslog.v3.AccessLogFilter`
oneof arm, end-to-end over the EXISTING phase-70/71/72/73 `filter` seam. A sink
whose `filter` is a `metadata_filter` emits a record iff the request's **dynamic
metadata**, resolved at `matcher.filter` → `matcher.path[0].key`, matches
`matcher.value` — or, when the path does not resolve, iff `match_if_key_not_found`
(default **`true`**, R-0.4).

This lights up a genuinely NEW gating axis — **dynamic metadata**, the first
cross-subsystem link between the phase-33/34 metadata PRODUCERS and the phase-70+
access-log FILTER subsystem — at low new-logic cost, because every input it needs
already exists (R-0.1): the record carries the store, both emit gates hold it, the
matcher config types and value engine are landed, the producers are fixture-proven,
and the differential driver needs no change.

Concretely add: (i) the `MetadataFilter { matcher: Option<MetadataMatcher>,
match_if_key_not_found: Option<bool> }` config struct + the `metadata_filter` oneof
arm (compiler-forced 6-arm destructuring + 6-arm compile match); (ii) an
access-log-scoped fail-loud validation of the reused `MetadataMatcher`; (iii) a new
`MetadataMatch` trait-object seam in `envoy-accesslog` (ADR-0150 pattern, R-0.7)
with its sole impl in `envoy-config` reusing `ValueMatcher::matches` verbatim;
(iv) the `LogFilter::Metadata` runtime variant + the `should_log` widening to carry
the store; (v) TWO NEW backend-free byte-exact fixtures `0081` (value keep/drop)
and `0082` (`match_if_key_not_found: false` absent-drop).

---

## §2. Scope

### 2.1 In scope

1. **`MetadataFilter` config struct** (`envoy-config`) —
   `{ matcher: Option<MetadataMatcher>, match_if_key_not_found: Option<bool> }`,
   `#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]`
   `#[serde(default, deny_unknown_fields)]`. **`matcher` is `Option`** — upstream
   accepts a matcher-less `metadata_filter` (R-0.2), so rejecting it would break
   load parity. **`match_if_key_not_found` is `Option<bool>`, NOT `bool`** — it is a
   `BoolValue` wrapper, so absent and explicit-`false` are distinct on the wire
   (R-0.2); `None` means "default", resolved to `true` at compile (R-0.4).
   `MetadataMatcher` / `MetadataPathSegment` / `ValueMatcher` / `StringMatcher` are
   **reused verbatim** from phase 35/36 — no new matcher types.
2. **The `metadata_filter` oneof arm** on `AccessLogFilter` (a sixth `Option` field)
   + the `bootstrap::{MetadataFilter}` re-export.
3. **Fail-loud validation** in `validate_access_log_filter`: (a) the compiler-forced
   **6-arm** destructuring + 6-entry `set_arms` cardinality; (b) when `matcher` is
   present, an **access-log-scoped** matcher check mirroring upstream's PGV bounds
   (R-0.2) — `filter` non-empty, `path` non-empty, every segment `key` non-empty,
   `path.len() == 1` (the inherited R-0.8 boundary, forced by the flat store) —
   emitting ONE new `ConfigError` variant (the existing
   `RbacMetadataMatcherInvalid` is RBAC-scoped and carries `listener`/`policy_name`,
   R-0.8); (c) compile the `value`'s SafeRegex in place via the existing
   `ValueMatcher::compile_safe_regexes` (`bootstrap.rs:5541`), matching how the
   `header_filter` arm compiles its matcher (`validate_access_log_filter` is already
   `&mut`-taking); (d) a matcher-less `metadata_filter` must **pass** validation.
4. **`MetadataMatch` trait** (`envoy-accesslog`, ADR-0150 seam, R-0.7):
   `fn matches(&self, dynamic_metadata: &BTreeMap<String, BTreeMap<String, String>>) -> Option<bool>`
   — `None` iff the path did not resolve, so the not-found policy stays in
   `LogFilter` where the field lives. `Debug + Send + Sync`, mirroring `HeaderMatch`.
   **Sole impl** in `envoy-config` over `MetadataMatcher`, reusing
   `ValueMatcher::matches` verbatim.
5. **`LogFilter::Metadata { matcher: Option<Arc<dyn MetadataMatch>>, match_if_key_not_found: bool }`**
   runtime variant + its `should_log` arm implementing R-0.4's rule exactly
   (`None` matcher, or `Some` matcher resolving to `None`, → `match_if_key_not_found`).
   Introduces no `Eq`/`PartialEq` and no `envoy-config` dependency → **ADR-0150 holds**.
6. **The `should_log` signature widening** (R-0.6) — add
   `dynamic_metadata: &BTreeMap<String, BTreeMap<String, String>>` to
   `LogFilter::should_log` and `FileSink::should_log`, and pass
   `&record.dynamic_metadata` at BOTH emit gates (H1 `hcm.rs:1515`, H2
   `hcm.rs:1138` — already in scope, R-0.1). Mechanical update of the ~100 test
   call sites.
7. **The 6-arm `compile_access_log_filter`** (`envoy-http1/src/hcm.rs`): the tuple
   widens to 6; the new arm boxes `matcher.clone()` into the `MetadataMatch` seam
   and resolves `match_if_key_not_found.unwrap_or(true)`.
8. **NEW fixture `0081-accesslog-metadata-filter`** — H1 HCM,
   `header_to_metadata` mapping `x-a` → `com.example:k`, a file access log with
   `metadata_filter { matcher: { filter: com.example, path: [{key: k}], value: { string_match: { exact: "1" } } } }`,
   one `direct_response` `/x → 200`, **NO backend**. Probes ordered **kept-LAST**
   (ADR-0147 convention): a DROPPED probe (`x-a: 2`, value mismatch) then a KEPT
   probe (`x-a: 1`). Format carries `%DYNAMIC_METADATA(com.example:k)%` so the line
   itself witnesses the metadata.
9. **NEW fixture `0082-accesslog-metadata-filter-key-not-found`** — the same shape
   with `match_if_key_not_found: false`, probing the **absent-key** arm: a DROPPED
   probe (no `x-a` → key not found → dropped) then a KEPT probe (`x-a: 1`). This is
   the R-0.4 observable that fixture `0081` cannot carry, because
   `match_if_key_not_found` is per-sink and the driver asserts over ONE log file
   per side.
10. **In-process tests:** `should_log` for the `Metadata` arm across
    {value-match KEEP, value-mismatch DROP, key-absent × `match_if_key_not_found`
    true/false, namespace-absent, matcher-absent}; the `MetadataMatch` impl
    (resolution + `Option<bool>` contract, incl. `present_match` and each
    `StringMatcher` mode); the 6-arm compile (incl. the `unwrap_or(true)` default);
    the validator negatives (empty `filter`, empty `path`, empty segment `key`,
    multi-segment `path`, and — the load-parity pin — a **matcher-less
    `metadata_filter` ACCEPTS**); the 6-arm oneof cardinality (a `metadata_filter`
    alongside any other arm → ambiguous); the inherited-strictness pins (`invert`
    rejected, non-`string_match`/`present_match` `ValueMatcher` rejected); the
    no-`filter`-still-logs and five-existing-arm regressions.
11. **`BEHAVIOR_CONTRACT.md`** — a `metadata_filter` subsection under the access-log
    filter section (§6), recording R-0.2–R-0.5 and R-0.8.
12. **N73-R1 folded** — `crates/envoy-config/src/bootstrap.rs:714` still documents
    `AccessLogFilter` as "THREE oneof arms" (it has FIVE, and will have SIX). This
    phase edits exactly that struct, so the one-line doc fix rides along free.

### 2.2 Out of scope (deliberate, with rationale)

- **`matcher.invert`.** MEASURED accepted-but-**INERT** on this path (R-0.5), and
  absent from the in-tree `MetadataMatcher` (R-0.8). Implementing it would
  *introduce* a divergence; adding it as a parse-and-ignore field would touch the
  RBAC-shared type, whose own `invert` semantics are unmeasured. Kept boot-fatal
  (the ADR-0049 / CF-72-2 posture: stricter, fail-loud, never silently different).
  Documented; carry-forward §10.
- **Multi-segment `path`** (upstream accepts; R-0.2). Structurally unrepresentable
  against the record's FLAT two-level store (R-0.1/R-0.8) — supporting it means
  re-typing the metadata store across `envoy-filter`, both HCMs and
  `envoy-accesslog`, far beyond this arm. Inherited phase-35 boundary, kept
  fail-loud. Carry-forward §10.
- **The unmodelled `ValueMatcher` arms** (`bool_match`, `double_match`,
  `list_match`, `null_match`, `or_match` — R-0.2/R-0.8). The store is string-only,
  so the non-string arms have no representable value to match. Inherited phase-35/36
  boundary. Carry-forward §10.
- **`grpc_status_filter` / `duration_filter` / `runtime_filter`** — the remaining
  leaf arms (§4).
- **The standalone H2 access-log-filter differential** (M71-6). The gate is
  codec-agnostic; both fixtures are H1 (the 0076–0080 precedent). The H2 gate IS
  wired and unit-tested. M71-6 stays live.
- **Any change to the differential driver.** R-0.1 confirmed ZERO driver change;
  both fixtures are pure config + probes + a ~12-line test each.
- **CF-72-1 / CF-72-2** — rejected on measured cost (R-0.9, §4); this phase does not
  touch the header-match engine.

### 2.3 §7.4 fuzz disposition

`metadata_filter` adds a config sub-message on the already-fuzz-reachable
`AccessLogFilter` path; it introduces **no new byte-parser**. **Default
projection:** extend the existing `parse_bootstrap` corpus with a
`metadata_filter.yaml` seed — **no new fuzz target** (the phase-68/69/70/71/72/73
precedent, ADR-0137; `crates/envoy-config/fuzz/fuzz_targets/` holds exactly one
target). **Confirm at the state-2 PLAN-write** (PV-7). The new seed MUST get an
explicit `!`-un-ignore line in `crates/envoy-config/fuzz/.gitignore` (which
`*`-ignores the corpus dir) or it is silently untracked and invisible to CI —
memory `fuzz-corpus-seed-gitignored-by-default`; verify with `git ls-files`.

---

## §3. PLAN-VERIFY items (re-confirm against the live tree at the state-2 PLAN-write)

- **PV-1 — serde model.** Confirm `MetadataFilter { matcher: Option<MetadataMatcher>,
  match_if_key_not_found: Option<bool> }` deserializes with
  `#[serde(default, deny_unknown_fields)]`; that `metadata_filter: {}` parses (R-0.2
  load parity); that `MetadataMatcher`'s hand-rolled `ValueMatcher` deserializer
  composes here unchanged. Re-confirm `AccessLogFilter` derives (no `Clone`) and the
  exact `bootstrap.rs` line spans (`AccessLogFilter` ~723-742; `MetadataMatcher`
  ~1628-1638) — they drift.
- **PV-2 — the access-log-scoped matcher validator.** Re-confirm
  `validate_metadata_matcher` (`bootstrap.rs:4836-4858`) is RBAC-scoped (its
  `listener`/`policy_name` args + `RbacMetadataMatcherInvalid`) and decide:
  (a) a new access-log-scoped validator + ONE new `ConfigError` variant
  [recommended], vs (b) refactoring the RBAC one to be caller-agnostic [wider blast
  radius — an RBAC error-shape change]. Confirm the SafeRegex in-place compile via
  `ValueMatcher::compile_safe_regexes` (`bootstrap.rs:5541`) works from the `&mut`
  `validate_access_log_filter` (note the RBAC path canNOT compile in place —
  immutable borrow — and defers to filter-lowering; the access-log path can).
- **PV-3 — the 6-arm cardinality.** Confirm the destructuring at
  `validate_access_log_filter` (~`bootstrap.rs:5254-5260`) is compiler-forced (no
  `..`) so the new field breaks the build until handled, AND note the `set_arms`
  array (~`:5266-5276`) is **NOT** length-checked by the compiler — it must be grown
  by hand to 6 and pinned by a cardinality test.
- **PV-4 — the `MetadataMatch` seam shape.** Confirm the `Option<bool>` return
  contract (R-0.7) compiles cleanly and expresses R-0.4 exactly once; confirm the
  sole impl in `envoy-config` reuses `ValueMatcher::matches` (`matcher.rs:125-130`)
  verbatim; confirm no `Eq`/`PartialEq` is introduced on `LogFilter` and no
  `envoy-accesslog` → `envoy-config` edge appears (ADR-0150). Weigh the alternative
  of a `bool`-returning trait that takes the not-found default as a second argument.
- **PV-5 — the `should_log` widening.** Re-run the call-site census (R-0.6 measured
  102) and confirm the 4th-parameter widening is preferred over the record-based
  consolidation `should_log(&AccessLogRecord, &[(String,String)])` (§4). Confirm
  `&record.dynamic_metadata` is in scope at H1 `hcm.rs:1515` and H2 `hcm.rs:1138`.
- **PV-6 — driver reuse + fixture shape.** Re-confirm `Http1AccessLogByteExact` +
  `AccessLogByteExactProbe` need ZERO change; re-confirm the kept-LAST ordering and
  the CF-71-1 suppression settle handle a dropped-then-kept sequence; confirm
  `header_to_metadata`'s in-tree config surface matches fixture `0042`'s shape and
  that `%DYNAMIC_METADATA(com.example:k)%` renders `-` on the absent path in
  envoy-rust as it does upstream (R-0.3).
- **PV-7 — fuzz.** Re-confirm §2.3 (seed only; no new target; the `!`-un-ignore line
  + `git ls-files` check).
- **PV-8 — split gate.** Re-run the §8 estimate against the live tree; the
  signature-widening churn (R-0.6) is the item most likely to move it.

---

## §4. Rejected / deferred alternatives (what this pick was chosen over)

- **A `HeaderMatcher`-parity phase (CF-72-1 + CF-72-2).** REJECTED on **measured**
  cost (R-0.9), not on merit — it is a real divergence and deserves a phase. But
  the one-expression fix at `matcher.rs:51` is read by five call sites across four
  subsystems (H1+H2 route matching, RBAC, fault, JWT authn, access-log filtering);
  **zero** existing fixtures exercise `invert_match`, so it would need new
  differential fixtures on the route path AND the access-log path; the fix must be
  mode-scoped (a naive uniform DROP breaks the `present_match` parity pin — phase 72
  already proved this by mutation check); and CF-72-2's
  `treat_missing_header_as_empty` half interacts with the same absence handling.
  That is a cross-cutting correctness phase with a route-differential blast radius —
  strictly above the cheapest-strong bar that `metadata_filter` clears. It remains
  the strongest *next* candidate.
- **`duration_filter`.** Its wire shape is trivially cheap (MEASURED R-0.2: the
  SAME `ComparisonFilter`/`RuntimeUInt32` the landed `status_code_filter` already
  models — `runtime_key` `min_len 1`, ops exactly `{EQ,GE,LE}`). But the *predicate*
  is request DURATION: a cross-proxy differential would have to assert a latency
  comparison, which is exactly the timing-flakiness `BEHAVIOR_CONTRACT.md` excludes
  by default ("Timing: not compared by default"). Deferred to a timing-tolerant
  phase — the same rationale as at the phase-70/71/72/73 picks.
- **`grpc_status_filter`.** Wire shape MEASURED (R-0.2: `statuses` has no
  `min_items` and no uniqueness bound; `exclude` is a plain bool; the enum spelling
  is **`CANCELED`**, one L — `CANCELLED` is rejected; numeric tokens accepted). But
  the predicate reads the gRPC response **trailer** status, and envoy-rust has no
  gRPC data plane — there is nothing to produce a non-trivial value, so the
  differential would be vacuous. Needs the gRPC family to open first.
- **`runtime_filter`** — needs RTDS; envoy-rust has no runtime subsystem (the same
  reason `runtime_key` is inert throughout the filter family).
- **M73-R2 / M71-3 / M71-6 as phases of their own** (a CI-pin fixture for the
  already-measured mixed-leaf/depth-3 compositions; a dedicated all-drop
  `expected_logged_count == 0` fixture; the standalone H2 filter differential).
  Each is fixture-only and lights up **no new observable** — they pin parity already
  measured. Below the bar as phases; weigh at state-2 for cheap folding (§10).
- **Splitting `metadata_filter` across two phases** (matcher first, then
  `match_if_key_not_found`). REJECTED: the two are one message, share the entire
  seam, and differ by one `unwrap_or(true)`; splitting would strand the trait seam
  half-wired. §8 sits well under the §6.1 gate as one phase.
- **Re-weighed and still rejected:** each §9 family opener (network-filter payload
  codecs, `sni_cluster`, non-deterministic LB, HTTP/3+QUIC, gRPC bridge/transcoding,
  observability SINKS [gRPC ALS, OTLP], runtime/RTDS, hot-restart, WASM host) —
  each a LARGE new subsystem far above the cheapest-strong-differential bar.

**`metadata_filter` wins:** it is the only remaining `AccessLogFilter` leaf whose
data axis is **already fully plumbed** (R-0.1 — the record carries the store, both
gates hold it, the matcher types and value engine are landed, the producers are
fixture-proven, the driver needs no change), and it is the first arm to link the
phase-33/34 metadata producers to the phase-70+ filter subsystem. It yields fully
deterministic BACKEND-FREE byte-exact observables on a NEW axis, measured live
three ways (value match / value mismatch / key-not-found, R-0.3) plus the
`match_if_key_not_found` polarity flip (R-0.4). It also banks a NEW measured
divergence trap (`invert` accepted-but-inert, R-0.5) that no reader of the proto
could have predicted.

---

## §5. Differential surface at phase end

- **NEW fixture `0081-accesslog-metadata-filter`** — green cross-proxy: an H1 HCM
  listener with an `envoy.filters.http.header_to_metadata` filter mapping request
  header `x-a` → dynamic metadata `com.example:k`, a file access log
  (`text_format_source` `STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)% M=%DYNAMIC_METADATA(com.example:k)%\n`)
  gated by
  `metadata_filter { matcher: { filter: com.example, path: [{ key: k }], value: { string_match: { exact: "1" } } } }`,
  and one `direct_response` route (`/x → 200`). Probes ordered kept-LAST:
  `GET /x` with `x-a: 2` (`expect_logged: false` — value mismatch → DROP) then
  `GET /x` with `x-a: 1` (`expect_logged: true` → KEEP). The access-log file across
  BOTH proxies is asserted the SAME single **byte-identical** line, via the EXISTING
  `Http1AccessLogByteExact` driver + the CF-71-1 suppression settle. `clusters: []`;
  no backend spawns.
- **NEW fixture `0082-accesslog-metadata-filter-key-not-found`** — green
  cross-proxy: the same shape with `match_if_key_not_found: false`. Probes ordered
  kept-LAST: `GET /x` with NO `x-a` (`expect_logged: false` — key not found →
  DROP, the polarity flip MEASURED at R-0.4) then `GET /x` with `x-a: 1`
  (`expect_logged: true` → KEEP). Witnesses the wrapper-default semantics that
  `--mode validate` provably cannot reach (R-0.2/R-0.4).
- **All pre-existing fixtures `0001`–`0080` stay green** — a sink with no `filter`,
  or any of the five landed arms, is byte-unchanged; no existing fixture sets a
  `metadata_filter` (§7.5 (b)). The `should_log` widening (R-0.6) is
  signature-only — every existing arm ignores the new argument.
- **In-process:** the `Metadata` `should_log` matrix (value match / mismatch /
  key-absent × `match_if_key_not_found` true+false / namespace-absent /
  matcher-absent) + the `MetadataMatch` `Option<bool>` resolution contract + the
  6-arm compile incl. the `unwrap_or(true)` default + the validator negatives + the
  **matcher-less-accepts** load-parity pin + the 6-arm oneof cardinality + the
  inherited-strictness pins (`invert`, unmodelled `ValueMatcher` arms) + the
  no-`filter`-still-logs and five-existing-arm regressions.

**Why the differential needs no backend:** the strong deterministic byte-exact
observable comes from a `direct_response` 200 whose emission is gated on dynamic
metadata derived purely from a client-supplied request header — no cluster, no
upstream (R-0.3/R-0.4), mirroring fixtures `0076`–`0080` and reusing fixture
`0042`'s proven `header_to_metadata` shape.

---

## §6. `BEHAVIOR_CONTRACT.md` additions

A `metadata_filter` subsection under the access-log filter section (sibling to the
phase-70 `status_code_filter`, phase-71 `response_flag_filter`, phase-72
`header_filter`, and phase-73 `and_filter`/`or_filter` subsections), recording the
MEASURED facts:

- **§A the schema** —
  `filter: { metadata_filter: { matcher: { filter, path: [{key}], value }, match_if_key_not_found } }`;
  `matcher` is OPTIONAL upstream and here (R-0.2); inside it `filter` (`min_len 1`),
  `path` (`min_items 1`) and `value` are REQUIRED; `match_if_key_not_found` is a
  `BoolValue` wrapper written as a bare `true`/`false` (R-0.2).
- **§B the decision** — resolve `dynamic_metadata[filter][path[0].key]`; unresolved
  → `match_if_key_not_found`; resolved → `value.matches(v)`. **The default is
  `true`** (R-0.4, measured live — `--mode validate` cannot reach it). A missing
  NAMESPACE behaves identically to a missing KEY. A matcher-less `metadata_filter`
  keeps every record (R-0.4 corroboration).
- **§C `invert` is accepted-but-INERT upstream on this path** (R-0.5, reproduced
  twice, with the `invertBOGUS` control proving the field is genuine) — and is
  boot-fatal in envoy-rust (R-0.8). Both halves recorded, with the warning that
  "implementing" `invert` here would CREATE a divergence.
- **§D where envoy-rust is STRICTER** (the §E.1 precedent) — no `invert` field;
  single-segment `path` only (forced by the flat string-only store); `ValueMatcher`
  limited to `string_match`/`present_match`. All fail-loud, never silent.
- **§E mutual exclusion** — `metadata_filter` joins the six-arm oneof; zero arms and
  more-than-one arm are each `ConfigError::AmbiguousAccessLogFilter`.
- **§F no producer cross-check** — the `filter:` namespace is an opaque string;
  neither proxy verifies any filter ever writes it (R-0.2).
- **§G the authoritative fixture-0081/0082 files.**

---

## §7. ADR reservations

- **ADR-0154 (FIRED this session):** the phase-74 pick + scope + rejected
  alternatives (this SPEC's decisions), including the R-0.5 `invert`-inert finding
  and the R-0.9 measured basis for deferring CF-72-1/CF-72-2.
- **ADR-0155 (reserved):** the §6.2 empirical-verification reconciliation at the
  state-2 PLAN-write (PV-1..PV-8 resolutions — the `MetadataFilter` serde model, the
  access-log-scoped matcher validator + its new `ConfigError`, the `MetadataMatch`
  seam shape, the `should_log` widening decision, the driver-reuse confirmation, the
  fuzz disposition).
- **ADR-0156 (reserved):** the §6.1 split, if PV-8 fires it (unlikely — §8).

---

## §8. Estimated size (for the §6.1 split gate at state-2)

| Area | Net LoC (rough) |
|---|---|
| `envoy-config`: `MetadataFilter` struct + the oneof arm + re-exports + the N73-R1 doc fix | ~35 |
| `envoy-config`: access-log-scoped matcher validation + 1 new `ConfigError` variant + 6-arm cardinality | ~70 |
| `envoy-config`: `impl MetadataMatch for MetadataMatcher` (resolution + `ValueMatcher::matches` reuse) | ~30 |
| `envoy-accesslog`: `MetadataMatch` trait + `LogFilter::Metadata` + the `should_log` arm | ~50 |
| `should_log` signature widening across `filter.rs` / `file_sink.rs` / both HCM gates + ~100 test call sites (R-0.6) | ~115 |
| `envoy-http1`: the 6-arm `compile_access_log_filter` + the `unwrap_or(true)` default | ~30 |
| fixtures `0081` + `0082` (2× config-pair + expectations + README) — reuses the driver | ~280 |
| differential test entrypoints (2× ~12 LoC cloned from `access_log_header_filter.rs`) | ~25 |
| in-process tests (the `should_log` matrix + the seam contract + compile + validator negatives + the matcher-less load-parity pin + cardinality + strictness pins + regressions) | ~230 |
| `BEHAVIOR_CONTRACT.md` + ROADMAP/docs + the fuzz seed & `!`-un-ignore | ~70 |
| **Total** | **~935 net LoC / ~12–15 tasks** |

Comfortably UNDER the ~1500 LoC / ~25 task gate — a **single phase**, no split
projected. Larger than phases 70–73 (~630–670) for exactly one reason: the
`should_log` signature widening's ~100 mechanical test call-site edits (R-0.6),
which carry no design risk. PV-8 re-derives at the state-2 PLAN-write; ADR-0156 is
held in reserve as a formality.

---

## §10. Carry-forwards

**CHEAPLY FOLDED** (this phase touches `AccessLogFilter` + `validate_access_log_filter`
+ the access-log filter surface):

- **N73-R1** — the stale "THREE oneof arms" doc comment at
  `crates/envoy-config/src/bootstrap.rs:714` (five arms exist; six after this
  phase). Folded, §2.1 item 12.
- **M71-3** (the all-suppressed `expected_logged_count == 0` driver shape untested)
  and **M73-R2** (no committed fixture pins the mixed-leaf / depth-3 /
  `response_flag_filter`-child compositions the phase-73 state-5 LIVE-PROBES
  measured at parity) — weigh at state-2 whether either is cheaply added as an
  extra probe group or in-process assertion alongside the two new fixtures; else
  carry forward.
- **M70-R4** (`"filter": null` serialization) + **M70-R9** (provenance note) —
  access-log-adjacent; weigh at state-2, else carry forward.

**OPENED by this pick** (owner = whatever future phase touches the surface):

- **CF-74-1** — `matcher.invert` is accepted-but-INERT upstream on the access-log
  path (R-0.5) and boot-fatal in envoy-rust (R-0.8). A load-parity gap in the
  REJECT direction (ADR-0049 posture). Owner = a future `MetadataMatcher`-parity
  phase, which must ALSO measure whether `invert` is honored on the **RBAC** path
  before adding the field to the shared type.
- **CF-74-2** — multi-segment `path` (upstream accepts, R-0.2; envoy-rust rejects,
  R-0.8) is blocked on the FLAT string-only metadata store shared by
  `envoy-filter`, both HCMs and `envoy-accesslog`. Owner = a future
  metadata-store-typing phase.
- **CF-74-3** — the unmodelled `ValueMatcher` arms (`bool_match`, `double_match`,
  `list_match`, `null_match`, `or_match`; R-0.2/R-0.8), blocked on the same
  string-only store. Owner = the same future phase as CF-74-2.

**OPENED LATER IN THE PHASE** (recorded here so this §10 stays the phase's single
carry-forward ledger — see `PLAN.md`'s carry-forward section and `REVIEW.md`):

- **CF-74-4** *(opened at the state-2 PLAN-write)* — the RBAC-scoped
  `validate_metadata_matcher` does NOT check that a path segment's `key` is
  non-empty, though upstream PGV enforces `min_len 1`. The access-log validator
  added by this phase DOES. Fixing the RBAC side means touching
  `RbacMetadataMatcherInvalid`'s six coupled tests — out of scope. Owner = the
  next RBAC-matcher phase.
- **CF-74-5** *(opened at the state-2 PLAN-write)* — **CLOSED at the §5.2 state-3
  re-entry.** `present_match` on the RESOLVED branch was DERIVED from the measured
  rule rather than separately live-probed. The §5 state-5 code-review MEASURED it
  cross-proxy in BOTH polarities (probe group 1 sinks S4/S5 — exact complements
  over seven requests, both proxies agreeing on every cell, per-side `md5sum`
  `380b58e471f8c0c545d02a5e8b7b9df3`). `BEHAVIOR_CONTRACT.md` §G was upgraded from
  "derived, not separately measured" to MEASURED and carries the table.
- **CF-74-6** *(opened by the state-5 code-review, `REVIEW.md` I-1)* — the wrapped
  `google.protobuf.BoolValue` spelling `match_if_key_not_found: { value: <bool> }`
  is ACCEPTED and HONORED upstream (MEASURED, with a `{ bogus: false }` control
  that upstream rejects naming `google.protobuf.BoolValue`, proving the field is
  genuinely wrapper-typed) but is BOOT-FATAL in envoy-rust
  (`invalid type: map, expected a boolean`). A load-parity gap in the REJECT
  direction — fail-loud, never a silent runtime difference. **The `Option<bool>`
  model is CORRECT and must not be "fixed" in isolation**: it is what preserves
  the absent-vs-explicit-`false` distinction the wrapper carries, and the house
  precedent for wrapper fields is bare-only (`UInt32Value`, ADR-0063). Recorded in
  `BEHAVIOR_CONTRACT.md` §D and pinned by
  `metadata_filter_deserialize_round_trip_and_defaults`. Owner = a future
  wrapper-spelling-parity phase, which should ALSO survey the other
  `Option<bool>` / `Option<u32>` wrapper-typed fields rather than closing this one
  field alone.

**NOT consumed** (owner = whatever future phase touches their surface):

- **CF-72-1** (the shared-engine value-matcher `absent+invert` divergence —
  MODE-SCOPED; a fixer MUST preserve the `present_match` KEEP, memory
  `envoy-headermatcher-invert-absent-is-mode-dependent`) + **CF-72-2** (name-only
  `{name}` → `PresentMatch(true)` + `treat_missing_header_as_empty`). Costed but
  deferred (R-0.9, §4); this phase does not touch the header-match engine. The
  strongest *next* candidate.
- **CF-73-1** (arbitrary `and_filter`/`or_filter` nesting depth, no stack guard —
  parity with upstream) + **N73-R2** (no depth guard) — composition-specific,
  untouched.
- **M71-6** (H2 access-log-filter differential) + **M71-7** (latent multi-flag
  membership) + **M71-8** (`""`/whitespace token rejection) — response-flag /
  H2-specific, untouched.
- **M73-R1** (composition surface narrower than upstream — by-design ADR-0049),
  **M69-A..I, CF-69-1/2/3/5, M68-1, M-1, CF-67-3/5/6/7**, the older Minors in
  `67.3/SPEC.md` §10, and the HTTP-filters-family (1)–(4) in `STATE_HISTORY.md` —
  all untouched by this arm; carry forward.
