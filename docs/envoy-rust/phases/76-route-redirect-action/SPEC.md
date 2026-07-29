# Phase 76 — SPEC

**Title:** HTTP route `redirect` action (`envoy.config.route.v3.RedirectAction`) — the THIRD `Route.action` arm after `route:` and `direct_response:`, landing the `location`-header construction rule byte-exact cross-proxy on a backend-free fixture.

**ROADMAP row:** `76` (status `in-progress`).
**Pick ADR:** `ADR-0168`.
**Reserved-for-split ADR:** `ADR-0169` (projected NOT to fire — see §8).
**Depends on:** `04` (HCM + route match + `direct_response`), `05` (HTTP/2 — it reuses H1's resolver), `32` (access-log command operators), `42` (`%RESPONSE_CODE_DETAILS%`).

---

## 0. How to read this document

This SPEC is written for a session with **zero prior context** (doctrine D-3.4). Every
behavioural claim about upstream Envoy below was **MEASURED at this phase's §5 state-0/1
recon** against the pinned reference image `envoyproxy/envoy:v1.33.0`
(`docs/envoy-rust/ENVOY_TARGET.md`), not read from documentation and not inherited from a
previous phase. Where a claim was *not* measured, it says so explicitly.

Terminology used throughout:

- **upstream** / **Envoy** — the reference proxy, the pinned Docker image.
- **envoy-rust** — the Rust implementation in this repository, the subject under test.
- **differential fixture** — a directory under `tests/fixtures/` holding a config pair
  (`envoy.yaml` for upstream, `envoy-rust.yaml` for the subject) plus `expectations.yaml`;
  the harness in `tests/differential/` runs both proxies against identical inputs and
  diffs the results under `docs/envoy-rust/BEHAVIOR_CONTRACT.md`.
- **boot-fatal** — the config is rejected at load and the process exits non-zero.

---

## 1. The gap, in one paragraph

Upstream Envoy's `Route` message carries a three-way `oneof action`: `route:` (proxy to a
cluster), `direct_response:` (synthesize a reply), and **`redirect:`** (synthesize a 3xx
reply carrying a `location:` header). envoy-rust implements the first two and **wholly
lacks the third**. Concretely, `crates/envoy-config/src/bootstrap.rs:2178` declares

```rust
pub enum RouteAction {
    DirectResponse(DirectResponse),
    Route(RouteAction_Route),
}
```

and the hand-written `Route` deserializer at `bootstrap.rs:2416-2527` accepts exactly the
five keys `name`, `match`, `direct_response`, `route`, `typed_per_filter_config`
(`bootstrap.rs:2483-2494`), rejecting anything else. So a config that upstream Envoy loads
and serves is **boot-fatal in envoy-rust today**. MEASURED: `grep -rn` over `crates/` for
`RedirectAction`, `host_redirect`, `path_redirect`, `port_redirect`, `https_redirect`,
`scheme_redirect`, `strip_query` returns **zero** implementation hits.

This phase closes that gap for the measured subset defined in §4, and banks the
`location`-construction rule in `BEHAVIOR_CONTRACT.md`.

---

## 2. Why this surface (the cheapest-strong-differential argument)

Four properties make this the cheapest strong differential surface currently available.
They are the reason for the pick and are recorded here so a reviewer can check the
reasoning rather than re-derive it.

1. **It is backend-free.** A redirect is synthesized by the proxy; no cluster, no upstream
   connection, no backend helper process. MEASURED: the §5 recon config that exercised all
   eight redirect sub-fields declared **zero clusters** and validated OK. This matters
   concretely on the development host, where backend-routing fixtures go RED locally
   because the host routes the backend via `192.168.65.2` rather than an allow-listed
   address, making CI the only authority. A backend-free fixture is **fully verifiable
   locally**, like the existing `direct_response` fixtures.

2. **The witness needs ZERO new harness machinery.** The `location` header is **not** on
   the response-header allow-list — MEASURED: `HEADER_ALLOW_LIST`
   (`tests/differential/src/lib.rs:1177-1181`) has exactly three entries (`server`, `date`,
   `x-envoy-upstream-service-time`), and `diff_headers` (`lib.rs:1193`) applies
   **value-exact** comparison to every name not on that list. So the existing
   `Driver::Http1ProbeList` (`tests/differential/src/lib.rs:119`), whose per-probe
   `Http1Probe` already carries `expected_status`, `expected_body` and `expected_headers`
   (`lib.rs:1144-1165`), witnesses the `location` string **byte-exact cross-proxy** with no
   new driver, no new rule variant, and no new expectation kind.

3. **The runtime insertion point is a single clean seam.** Route-action dispatch is one
   `match &route.action` at `crates/envoy-http1/src/hcm.rs:2110`, and its
   `DirectResponse` arm already returns
   `BuildOutcome::Synth(Response, Option<&'static str>)` where the second field is the
   `%RESPONSE_CODE_DETAILS%` access-log string (`hcm.rs:1924-1929`). HTTP/2 does not have
   its own resolver — it calls H1's (`crates/envoy-http2/src/hcm.rs:475` and `:518`), so a
   third arm serves **both codecs at once**.

4. **The measured behaviour is rich but fully deterministic.** Fifteen distinct runtime
   cells and five reject-direction rules (§3), all byte-exact, none timing-dependent, none
   requiring concurrency.

**Rejected alternatives** are recorded in `ADR-0168`, with the measurement that killed
each one.

---

## 3. MEASURED upstream behaviour

All measurements below were taken against `envoyproxy/envoy:v1.33.0` at this phase's
state-0/1 recon. Runtime probes ran the image port-mapped (`docker -p`) with a
`text_format` access log to `/dev/stdout`; wire-shape probes used `--mode validate`, which
binds no sockets.

### 3.1 Runtime — status line and `location` header

Request authority in every row below is `orig.test:18000` (i.e. the client sent
`Host: orig.test:18000`), and the listener is plaintext HTTP/1.1.

| # | route config | request target | measured status | measured `location` |
|---|---|---|---|---|
| R1 | `redirect: { host_redirect: "example.com" }` | `/host` | `301 Moved Permanently` | `http://example.com/host` |
| R2 | `redirect: { host_redirect: "example.com" }` | `/host/deep?a=b` | `301` | `http://example.com/host/deep?a=b` |
| R3 | `redirect: { path_redirect: "/newpath" }` | `/path/sub` | `301` | `http://orig.test:18000/newpath` |
| R4 | `redirect: { path_redirect: "/newpath" }` | `/path/x?k=v` | `301` | `http://orig.test:18000/newpath?k=v` |
| R5 | `redirect: { prefix_rewrite: "/replaced" }` | `/pfx/sub` | `301` | `http://orig.test:18000/replaced/sub` |
| R6 | `redirect: { https_redirect: true }` | `/https/x` | `301` | `https://orig.test:18000/https/x` |
| R7 | `redirect: { host_redirect: "example.com", response_code: TEMPORARY_REDIRECT }` | `/code` | `307 Temporary Redirect` | `http://example.com/code` |
| R8 | `redirect: { host_redirect: "example.com", strip_query: true }` | `/strip/a?q=1&z=2` | `301` | `http://example.com/strip/a` |
| R9 | `redirect: { host_redirect: "example.com", port_redirect: 8443 }` | `/port` | `301` | `http://example.com:8443/port` |
| R10 | `redirect: {}` (bare, all defaults) | `/bare/deep` | `301` | `http://orig.test:18000/bare/deep` |
| R11 | `redirect: { scheme_redirect: "ftp" }` | `/scheme/x` | `301` | `ftp://orig.test:18000/scheme/x` |
| R12 | `redirect: { regex_rewrite: { pattern: { regex: "^/rgx/(.*)$" }, substitution: "/deep/\1/end" } }` | `/rgx/mid?q=1` | `301` | `http://orig.test:18000/deep/mid/end?q=1` |
| R13 | `redirect: { scheme_redirect: "https", host_redirect: "e.com" }` | `/aa/y` | `301` | `https://e.com/aa/y` |
| R14 | `redirect: { host_redirect: "e.com", strip_query: true, response_code: SEE_OTHER }` | `/bb/y?q=1` | `303 See Other` | `http://e.com/bb/y` |
| R15 | `redirect: { https_redirect: true, port_redirect: 443 }` | `/cc/y` | `301` | `https://orig.test:443/cc/y` |

Response body on every row: **empty**, with `content-length: 0`. `server: envoy` present
(allow-listed, name-required). No `x-envoy-upstream-service-time` (no upstream was
contacted) — consistent with the existing contract row for that header, which says it is
absent on non-proxied paths.

### 3.2 The derived rules (this is what the implementation must encode)

Read off rows R1–R15, not from documentation:

**(a) Scheme.** Default is the scheme the request arrived on (`http` for a plaintext
listener — R1/R10). `https_redirect: true` forces `https` (R6). `scheme_redirect: "<s>"`
forces the literal `<s>`, and it is **not validated against a scheme allow-list** — the
literal `ftp` was accepted and emitted verbatim (R11).

**(b) Authority — the asymmetry, and the trap.** This is the one rule a from-scratch
implementation is most likely to get wrong:

- If `host_redirect` **is** set, the authority becomes that host and **the request's
  original port is DROPPED** (R1: `orig.test:18000` → `example.com`, no port).
- If `host_redirect` is **not** set, the request's original authority is preserved
  **including its port** (R6: stays `orig.test:18000`; R10 likewise).
- `port_redirect` overrides the port in both cases, and is rendered as `:<n>` (R9 with
  `host_redirect`; R15 without).
- Changing only the scheme does **not** normalise or drop a now-redundant port: R15
  produced the literal `https://orig.test:443/cc/y` (an explicit `:443` on an `https`
  URL), and R6 kept `:18000` on an `https` URL.

**(c) Path.** Exactly one of three mutually-exclusive rewrites, or none:

- none → the request path is used as-is (R1/R10).
- `path_redirect: "/p"` → the path becomes the literal `/p` (R3).
- `prefix_rewrite: "/p"` → the portion of the path that the route's `prefix:` matcher
  matched is replaced by `/p`, and the remainder is appended (R5: route `prefix: "/pfx"`,
  request `/pfx/sub`, rewrite `/replaced` → `/replaced/sub`).
- `regex_rewrite: { pattern, substitution }` → capture-group substitution over the whole
  path (R12). **This sub-field is a NON-GOAL of this phase — see §5.**

**(d) Query.** By default the request's query string is **preserved and re-appended**, and
this holds even when the path was replaced wholesale: R4 shows `path_redirect: "/newpath"`
against `/path/x?k=v` yielding `/newpath?k=v`. `strip_query: true` drops it (R8/R14).

**(e) Status code.** Default `301`. The `response_code` enum has five values and the
measured reason phrases are: `MOVED_PERMANENTLY` → `301 Moved Permanently`, `FOUND` → 302,
`SEE_OTHER` → `303 See Other` (R14), `TEMPORARY_REDIRECT` → `307 Temporary Redirect` (R7),
`PERMANENT_REDIRECT` → 308. (302 and 308 were validated as accepted config values; their
reason phrases were not separately captured on the wire — see §7 NOT MEASURED.)

### 3.3 Runtime — access-log observables

MEASURED with `text_format: "PROBE path=%REQ(:PATH)% status=%RESPONSE_CODE% flags=%RESPONSE_FLAGS% details=%RESPONSE_CODE_DETAILS% route=%ROUTE_NAME%\n"`:

- **`%RESPONSE_CODE_DETAILS%` is `direct_response`** on every redirect row — the *same
  string* upstream uses for a `direct_response:` route. This is a genuinely useful
  measurement: envoy-rust already emits exactly that string from the `DirectResponse` arm
  (`crates/envoy-http1/src/hcm.rs:2112`, `Some("direct_response")`), so the redirect arm
  **reuses the existing constant verbatim** and needs no new detail string, no new
  `Op`, and no `AccessLogRecord` field.
- **`%RESPONSE_FLAGS%` is `-`** on every redirect row.
- **`prefix_rewrite` MUTATES the logged `:path`.** Request `/pfx/sub` was logged as
  `PATH=/replaced/sub`. By contrast `path_redirect` did **not** rewrite the logged path
  (`/path/sub` logged as `/path/sub`). This is a real discriminating observable and a
  parity trap: the rewrite is applied to the request's `:path` header in place for
  `prefix_rewrite`, but `path_redirect` affects only the `location` string.

### 3.4 Reject-direction (config-load) rules

MEASURED via `--mode validate`. All five are rejections upstream, so envoy-rust must
reject them too for load parity:

| # | config | upstream verdict |
|---|---|---|
| J1 | `path_redirect` **+** `prefix_rewrite` together | REJECT (`oneof path_rewrite_specifier`) |
| J2 | `path_redirect` **+** `regex_rewrite` together | REJECT (same oneof) |
| J3 | `redirect` **+** `route` on one `Route` | REJECT (`oneof action`) |
| J4 | `redirect` **+** `direct_response` on one `Route` | REJECT (same oneof) |
| J5 | `scheme_redirect` **+** `https_redirect` together | REJECT (`oneof scheme_rewrite_specifier`) |
| J6 | `response_code: BOGUS` (unknown enum name) | REJECT |
| J7 | `response_code: 302` (numeric, out of enum range) | REJECT (PGV `defined_only`) |

And two ACCEPTANCES worth recording because they are surprising:

- `port_redirect: 0` validates OK.
- `port_redirect: 70000` validates OK — **there is no PGV upper bound**, and at runtime it
  is rendered verbatim: MEASURED `location: http://e.com:70000/hostport/z`. envoy-rust must
  therefore **not** add a `1..=65535` bound, or it would introduce a reject-direction
  divergence.

---

## 4. Scope — what this phase builds

### 4.1 Config schema (`crates/envoy-config`)

1. A new `RedirectAction` struct with `#[serde(deny_unknown_fields)]`, carrying:
   `host_redirect: Option<String>`, `port_redirect: Option<u32>`,
   `strip_query: bool` (default `false`), `response_code: RedirectResponseCode`
   (default `MOVED_PERMANENTLY`), plus the two oneof groups modelled the way the house
   already models oneofs — as `Option` fields validated for exclusivity, matching how
   `Route`'s own action oneof is handled at `bootstrap.rs:2499-2514`:
   - `path_rewrite_specifier`: `path_redirect` | `prefix_rewrite` (`regex_rewrite` is a
     non-goal, §5).
   - `scheme_rewrite_specifier`: `https_redirect` | `scheme_redirect`.
2. A `RedirectResponseCode` enum with all **five** upstream values, so J6/J7 reject.
3. A third `RouteAction::Redirect(RedirectAction)` variant, the matching arm in the
   hand-written `Route` visitor, `"redirect"` added to the accepted-key list at
   `bootstrap.rs:2486-2492`, and the cardinality error text at `bootstrap.rs:2499-2514`
   widened from *"exactly one of `direct_response` or `route`"* to the three-way form —
   which is what makes J3 and J4 reject.
4. `Serialize` support, because `/config_dump` round-trips the bootstrap
   (`bootstrap.rs:2544` and `:2565` are the existing `RouteAction` serialize arms).
5. New `ConfigError` variants for the two oneof violations (J1/J2 and J5). `ConfigError`
   lives at `crates/envoy-config/src/lib.rs:73` and currently has ~123 variants.

### 4.2 Runtime (`crates/envoy-http1`, serving both codecs)

6. A pure function that builds the redirect from (request authority, request target,
   matched route's `prefix:`, `RedirectAction`) → `(status, reason, location)`, encoding
   rules (a)–(e) of §3.2. Pure and total, so it is exhaustively unit-testable without a
   socket.
7. A third arm at the `match &route.action` dispatch (`crates/envoy-http1/src/hcm.rs:2110`)
   returning `BuildOutcome::Synth(<the 3xx response>, Some("direct_response"))` — reusing
   the measured detail string of §3.3.
8. The `prefix_rewrite` in-place `:path` mutation of §3.3, so `%REQ(:PATH)%` matches.

### 4.3 Differential fixture

9. **Fixture `0086-route-redirect-action`** (next free id — MEASURED: `tests/fixtures/`
   holds **85** directories, highest `0085`; `tests/differential/tests/` holds **85** test
   files). Backend-free, `Driver::Http1ProbeList`, one probe per in-scope row of §3.1,
   each asserting `expected_status` **and** `expected_headers: set_equal_modulo_allow_list`
   so the `location` value is compared byte-exact on both sides.
   - **Fixture-authoring rule, MANDATORY:** every probe carries a **distinct `path:`**.
     This is the standing rule at `BEHAVIOR_CONTRACT.md` Phase 75 §G. It applies here for
     an additional reason specific to this phase: each probe must select a *different
     route*, so distinct paths are load-bearing for correctness, not just attribution.
   - **Known driver limitation to design around:** `Http1ProbeList` **aborts at the first
     failing probe**, so one red run names one probe. A regression touching several cells
     will be reported as a single failure; that is expected and must not be read as
     "only one cell broke".

### 4.4 Contract + fuzz

10. A `BEHAVIOR_CONTRACT.md` **Phase 76** section banking §3.2 (a)–(e), §3.3, §3.4, the
    R1–R15 table, and an explicit statement that `location` is **not** allow-listed and is
    therefore value-exact.
11. A `parse_bootstrap` fuzz corpus seed exercising a `redirect:` route, with the
    **explicit `!`-un-ignore line** in `crates/envoy-config/fuzz/.gitignore` — without it
    the seed is silently untracked and invisible to CI. MEASURED: that file currently
    carries 63 un-ignore lines at `:2-64`. **No new fuzz target**, so §7.5 gate (d) is
    satisfied by the existing target and **no `ci.yml` edit is needed**.

---

## 5. Non-goals (explicit — do NOT widen into these)

Each is deferred deliberately, and each will therefore be **boot-fatal** in envoy-rust
while upstream accepts it. That is the established house pattern for narrow scope — the
same posture already taken for `hash_policy` sub-modes, `retry_policy` sub-fields and
`domains` wildcards — and each is recorded rather than silently dropped (§6.3 forbids
vague deferral).

1. **`regex_rewrite` inside `redirect`** (R12 measured, and it validates OK upstream). It
   needs capture-group substitution machinery this phase would otherwise not touch.
   Deferred to keep the phase under the §6.1 LoC gate. **Boot-fatal here.**
2. **`RouteAction.prefix_rewrite` / `regex_rewrite` / `host_rewrite_*` on the `route:`
   (proxying) arm.** This phase touches the **redirect** arm only. The proxying arm's
   rewrite family is a separate, larger surface.
3. **`internal_redirect_policy`, `non_forwarding_action`, `weighted_clusters`,
   `cluster_header`, route-level `timeout`, route/vhost/route-config-level header
   mutation.** All measured ABSENT from envoy-rust and all out of scope.
4. **An HTTP/2 differential fixture for redirect.** H2 reuses H1's `build_response`
   (`crates/envoy-http2/src/hcm.rs:518`), so H2 is covered **in-process** plus by the H1
   differential fixture — the same disposition phases 68 and 69 took for their surfaces.
5. **The `%RESPONSE_CODE_DETAILS%` strings of the *error* synth paths** (400/404/501),
   still `None` in envoy-rust. Unrelated to redirect.
6. **`CF-76-1` (opened by this recon — see §6.1).** Do NOT fix it here.
7. **`CF-75-2`, `CF-75-3`, `CF-75-4`, `CF-75-5`, `CF-75-6`** and every other open
   carry-forward. This phase consumes none of them and must not widen into them.

---

## 6. Carry-forwards

### 6.1 OPENED by this phase's recon — `CF-76-1` (NEW, MEASURED)

**The query string is not stripped before route path matching.** Upstream Envoy strips the
query before matching a route's `path:`/`prefix:`; envoy-rust matches the raw request
target byte-for-byte, so an exact `path:` route **fails to match** a request that carries a
query string.

MEASURED upstream (backend-free `direct_response` routes, `envoyproxy/envoy:v1.33.0`):

| request target | route | upstream result |
|---|---|---|
| `/exact` | `match: { path: "/exact" }` | `201` (match) |
| `/exact?q=1` | `match: { path: "/exact" }` | **`201` (match — query stripped)** |
| `/exact?` | `match: { path: "/exact" }` | `201` (match) |
| `/exact%3Fq=1` | `match: { path: "/exact" }` | `404` (no match — a percent-encoded `?` is **not** a delimiter, so it stays part of the path) |

envoy-rust side, verified on disk: `route_matches` compares `path == p`
(`crates/envoy-http1/src/hcm.rs:2155-2166`) and is called with the **raw** target at
`hcm.rs:2028` and `hcm.rs:2094`; `crates/envoy-http1/src/codec.rs:26-28` states the
request-target is matched "byte-for-byte (no normalization)". H2 inherits it via
`crates/envoy-http2/src/hcm.rs:475`. So `/exact?q=1` compares `"/exact?q=1" == "/exact"` →
false. **This is a real, silent, pre-existing divergence on the most-exercised surface in
the project, and no existing fixture pins it** (no fixture sends a query against an exact
`path:` route).

**It is NOT fixed in this phase** (§6.3). It is a strong candidate for its own small phase:
the fix site is one function, but it interacts with `%REQ(:PATH)%` logging, with the
upstream-forwarded target, and with this phase's own query-preservation rule (§3.2 (d)) —
so it deserves its own measured scope. This phase deliberately keeps its boundary clean by
using a **`prefix:`-matched** route wherever a probe carries a query string, which matches
regardless of the divergence.

### 6.2 MEASUREMENT CONTRIBUTED to an existing carry-forward — `CF-75-2`

`CF-75-2` is the recorded open divergence that upstream comma-joins duplicate header values
before value matching while envoy-rust matches only the first occurrence. Its record
explicitly flagged the **join rule itself as unmeasured**. This recon measured it, so a
future phase does not have to. Against `envoyproxy/envoy:v1.33.0`, sending `x-a: v` twice:

- `exact_match: "v,v"` → **MATCH**; `exact_match: "v, v"` → **NO MATCH**. The delimiter is
  a single comma with **no** following space.
- Individual values are **OWS-trimmed before joining**: sending `x-a:␠␠␠v␠␠` twice still
  matched `exact_match: "v,v"`.
- `safe_regex_match: "^v,v$"` → MATCH, and `suffix_match: ",v"` → MATCH: the joined string
  is what every value mode sees.
- `range_match: { start: 1, end: 100 }` with `x-b: 1` and `x-b: 2` → **NO MATCH** (the
  joined `"1,2"` is not an integer), while a single `x-b: 5` matches. So joining happens
  *before* the numeric parse.
- `cookie` is **not** special-cased in this direction: `cookie: a=1` + `cookie: b=2`
  matched `exact_match: "a=1,b=2"`.

Also relevant and recorded here: the fix is **single-site**. All five consumers pass the
full header slice into one function and the first-occurrence collapse happens exactly once,
at `crates/envoy-config/src/matcher.rs:40-43`. The one wrinkle a future phase must handle
is that HTTP/2 pre-collapses `host` at `crates/envoy-http2/src/request.rs:65-74`.

**Nothing about `CF-75-2` is changed by this phase.** It remains OPEN and out of scope.

### 6.3 Consumed by this phase

**None.** This phase consumes no carry-forward. It is a new-surface phase.

---

## 7. NOT MEASURED (stated explicitly, per D-3.4)

A later session must not treat any of the following as settled:

1. The wire reason phrases for `FOUND` (302) and `PERMANENT_REDIRECT` (308). Both were
   confirmed as **accepted config values** via `--mode validate`, but only 301/303/307 had
   their status lines captured on the wire. The state-2 PLAN-write should measure the other
   two before pinning them in a fixture.
2. Redirect behaviour on a **TLS** listener — i.e. whether the default scheme becomes
   `https` when the request arrived over TLS. Every runtime probe used a plaintext
   listener. Rule (a) of §3.2 says "the scheme the request arrived on", which is the
   natural reading, but the `https` case was **not** measured.
3. Redirect behaviour over **HTTP/2**. All runtime probes were HTTP/1.1. H2 shares H1's
   resolver in envoy-rust, but upstream's H2 `:scheme`/`:authority` handling was not probed.
4. Whether a request with **no `Host` header** (HTTP/1.0-style) reaches a redirect route at
   all, and what authority the `location` then carries.
5. `port_redirect` values above 65535 at runtime beyond the single `70000` probe; no
   boundary sweep was done.
6. The interaction of `redirect` with `typed_per_filter_config` on the same `Route`.
7. Whether upstream emits any response header other than `location`, `date`, `server`,
   `content-length` on a redirect. The probe grepped for those four plus status; it did not
   dump the full header set. The fixture's `set_equal_modulo_allow_list` rule will catch
   any difference here automatically, but the PLAN should not *assume* the set.

One probe-design error is recorded so it is not repeated: an early recon config placed a
`prefix: "/scheme"` route before a `prefix: "/schemehost"` route, so `/schemehost/y` was
shadowed by the earlier route and returned that route's answer. The affected cell was
re-measured with non-overlapping prefixes and is row R13. **Prefix-overlap in a probe set
is a live hazard for this phase's fixture**, since every probe must select a different
route.

---

## 8. Size estimate and the §6.1 split gate

The §6.1 gate fires at the state-2 PLAN-write if the plan exceeds **~25 numbered tasks** or
**~1500 net LoC**.

Projected here, by component: schema + enum + validators ~280; `RouteAction` variant,
visitor arm, cardinality-text widening, `Serialize` ~200; `ConfigError` variants ~40;
the pure location-builder ~180; dispatch arm + `prefix_rewrite` `:path` mutation ~60;
in-process unit tests (the bulk — rules (a)–(e), all five enum values, both oneofs,
J1–J7) ~450; fixture (3 files + test entrypoint) ~120; fuzz seed + un-ignore ~10.
**Total ≈ 1340 net LoC across ≈ 16–20 tasks.**

That is **under both thresholds, but within ~11% of the LoC gate** — close enough that the
estimate must be **re-derived at the state-2 PLAN-write**, which owns the split decision.
Stating this plainly, as required: the split is **projected NOT to fire**, and
**`ADR-0169` is RESERVED-UNFIRED** for it. If the re-derived estimate does cross, the split
line is already chosen and should be used rather than invented:

- **`76.1`** — the config surface: `RedirectAction` schema, `RedirectResponseCode`, the
  third `RouteAction` variant, the visitor arm, the three-way cardinality error, the oneof
  validators and their `ConfigError` variants, `Serialize`, the fuzz seed. No new fixture;
  regression-equivalence proven by the 85 existing fixtures staying green (the
  foundation-slice pattern used by phases 05.1 / 07.1 / 12.1 / 14.1).
- **`76.2`** — the runtime: the location-builder, the dispatch arm, the `prefix_rewrite`
  `:path` mutation, fixture `0086`, the `BEHAVIOR_CONTRACT.md` Phase 76 section, and the
  parent-76 close.

This mirrors the phase-68/69 precedent, where a split ADR was reserved, the estimate was
re-derived at the PLAN-write, and the split did not fire.

---

## 9. Definition of done (the §7.5 gate, instantiated)

- **(a)** Fixture `0086-route-redirect-action` green — both proxies return identical
  status, identical byte-exact `location`, and an empty body on every probe.
- **(b)** All **85** pre-existing fixtures still green. The redirect arm is inert unless a
  route configures `redirect:`, so no existing fixture changes behaviour; this is a
  regression assertion, not a re-baseline.
- **(c)** Conformance: `h2spec` unchanged at its existing threshold. **Do not trim
  `known-failures.txt`** (21 lines) — the development host scores h2spec 3.5/2 as PASS
  where CI does not, so trimming on local evidence would break CI.
- **(d)** No new fuzz target, so the existing `parse_bootstrap` short-budget CI run
  satisfies this gate; the new corpus seed must be confirmed **tracked** via
  `git ls-files`.
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets
  --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`,
  `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

---

## 10. Next state

The next session is the **§5 state-2 PLAN-write** (`superpowers:writing-plans`), and it is
a **SEPARATE session** — §5.1 permits exactly one state per session and forbids the context
that wrote this SPEC from also writing the PLAN. It must:

1. **Re-verify this SPEC's citations on disk before transcribing them.** Line numbers in a
   large file drift; every `file:line` above was measured at this commit and must be
   re-anchored on text, not trusted.
2. **Measure the two items in §7 that the fixture depends on** — the 302/308 reason
   phrases (§7 item 1) and the full redirect response-header set (§7 item 7).
3. **Re-derive the LoC estimate** and own the §6.1 split decision (§8).
4. Write `PLAN.md` as TDD-ordered numbered tasks (D-3.1: tests first, no exceptions), and
   pre-flight the plan's own literal Rust against `cargo fmt --check` and
   `clippy -D warnings` — a recurring failure mode is a PLAN whose example code trips the
   plan's own gate.
