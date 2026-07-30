# Sub-phase 76.2 — SPEC

**Title:** `Route.redirect` — the RUNTIME slice: the pure `location`-builder, the dedicated
redirect response synthesizer, the third arm at the single `match &route.action` dispatch
seam (serving BOTH codecs), the `prefix_rewrite` in-place `:path` mutation, the NEW
backend-free differential fixture `0086`, and the `BEHAVIOR_CONTRACT.md` Phase 76 bank.

**ROADMAP row:** `76.2` (status `planned` at this writing).
**Parent phase:** `76` (`docs/envoy-rust/phases/76-route-redirect-action/`, status `in-progress`).
**Split ADR:** `ADR-0169`.
**Parent pick ADR:** `ADR-0168`.
**Depends on:** `76.1` — **hard dependency.** This sub-phase consumes the `RedirectAction`
struct, the `RedirectResponseCode` enum and the `RouteAction::Redirect` variant that `76.1`
lands. It must not start before `76.1` is `done`.

---

## 0. How to read this document

Written for a session with **zero prior context** (doctrine D-3.4); self-contained.

Every behavioural claim about upstream Envoy below was **MEASURED** against the pinned
reference image `envoyproxy/envoy:v1.33.0` (`docs/envoy-rust/ENVOY_TARGET.md`) — at the
parent phase's §5 state-0/1 recon, or (marked **[76.2-NEW]**) at the §5 state-2 session
that fired the split. Nothing is read from documentation. Where a claim was *not* measured,
§8 says so explicitly.

**Every `file:line` citation was re-verified on disk at commit `f438cb9`** by anchoring on
TEXT. `crates/envoy-http1/src/hcm.rs` is ~10 000 lines and
`tests/differential/src/lib.rs` ~10 880 — re-anchor again before transcribing.

Terminology: **upstream** = the reference proxy; **envoy-rust** = the subject;
**boot-fatal** = rejected at load, process exits non-zero.

---

## 1. What `76.1` has already landed (do not rebuild it)

By the time this sub-phase starts, `crates/envoy-config` already carries:

- `RedirectAction` with `host_redirect: Option<String>`, `port_redirect: Option<u32>`,
  `path_redirect: Option<String>`, `prefix_rewrite: Option<String>`,
  `https_redirect: Option<bool>`, `scheme_redirect: Option<String>`,
  `strip_query: bool`, `response_code: RedirectResponseCode`.
  **The `Option`s are load-bearing** — upstream's oneofs are exclusive on field PRESENCE,
  not on value (`https_redirect: false` + `scheme_redirect` REJECTS; `https_redirect:
  false` alone ACCEPTS). Do not "simplify" any of them to a bare `bool`/`String`.
- `RedirectResponseCode` with the five values and a `-> u16` mapping.
- `RouteAction::Redirect(RedirectAction)`, the widened three-way `Route` cardinality check,
  the widened visitor key list, and both `Serialize` arms.
- **An inert placeholder** at the H1 runtime dispatch returning
  `BuildOutcome::Synth(synth_501(close), None)`, pinned by a `76.1` test. **Replacing that
  placeholder — and deliberately flipping its test — is this sub-phase's first job.**

---

## 2. MEASURED upstream runtime behaviour

### 2.1 The status line — all five response codes on the wire **[76.2-NEW]**

The parent SPEC had captured only 301/303/307 on the wire and listed the 302/308 reason
phrases as NOT MEASURED. All five are now measured:

| `response_code` | wire status line |
|---|---|
| `MOVED_PERMANENTLY` (default) | `HTTP/1.1 301 Moved Permanently` |
| `FOUND` | `HTTP/1.1 302 Found` |
| `SEE_OTHER` | `HTTP/1.1 303 See Other` |
| `TEMPORARY_REDIRECT` | `HTTP/1.1 307 Temporary Redirect` |
| `PERMANENT_REDIRECT` | `HTTP/1.1 308 Permanent Redirect` |

**envoy-rust's reason-phrase table is MISSING three of these.** MEASURED:
`canonical_reason` at `crates/envoy-http1/src/response.rs:188-215` contains
`301 => "Moved Permanently"` (`:195`) and `302 => "Found"` (`:196`), but **303, 307 and 308
are absent** and fall through to `_ => "OK"` (`:213`). A 303 redirect would emit
`HTTP/1.1 303 OK` today.

**This is a silent-wrong-answer hazard the differential fixture CANNOT catch.** The
harness's `drive_http1` parses the status *code* only, and the equivalence matrix compares
`response_status: exact` — the reason phrase is not part of it (the function's own doc
comment at `response.rs:184-187` says as much). So the three missing phrases must be pinned
by an **in-process unit test**, not by fixture `0086`. Call this out in `PLAN.md`.

### 2.2 The response header set **[76.2-NEW]** — the finding that reshapes the design

The parent SPEC listed the full header set as NOT MEASURED (its §7 item 7); the recon had
grepped only four names. Measured at the state-2 session under **the harness's exact
request shape** — a raw `GET <target> HTTP/1.1` with `Host:` and `Connection: close`, which
is what `drive_http1` sends (`tests/differential/src/lib.rs:2194-2206`):

| response | headers, in wire order |
|---|---|
| **redirect** | `location`, `date`, `server`, `connection`, `content-length` |
| `direct_response` (control) | `content-length`, `content-type`, `date`, `server`, `connection` |

**A redirect carries NO `content-type` header. A `direct_response` does.**

envoy-rust's shared synth skeleton `synth_with`
(`crates/envoy-http1/src/hcm.rs:2185-2204`) **always** emits five headers in the fixed
order `[server, date, content-length, content-type, connection]`:

```rust
fn synth_with(status: u16, body: Bytes, close: bool) -> Response {
    Response {
        status,
        reason: None,
        headers: vec![
            (headers::SERVER.to_string(), DEFAULT_SERVER_NAME.to_string()),
            (headers::DATE.to_string(), now_imf_fixdate()),
            (headers::CONTENT_LENGTH.to_string(), body.len().to_string()),
            (
                headers::CONTENT_TYPE.to_string(),
                DEFAULT_CONTENT_TYPE.to_string(),
            ),
            (
                headers::CONNECTION.to_string(),
                connection_value(close).to_string(),
            ),
        ],
        body,
    }
}
```

**Consequence: the redirect arm MUST NOT reuse `synth_with`.** If it did, envoy-rust would
emit a `content-type` upstream does not, and `diff_headers` would fail on its **first**
check — the name-set equality at `tests/differential/src/lib.rs:1209-1215` — with
`only-in-envoy-rust=["content-type"]`. A dedicated `synth_redirect` builder is required,
emitting exactly `location`, `date`, `server`, `connection`, `content-length` with an empty
body.

Two notes on ordering: `diff_headers` is **order-insensitive** (it compares a
`BTreeSet` of lowercased names, `lib.rs:1199-1215`), so order is not a *correctness* risk
for the fixture. But the `synth_with` doc comment at `hcm.rs:2183-2184` states header order
is load-bearing and byte-compared against upstream, and the sibling `synth_overflow`
(`hcm.rs:2242-2251`) is the established precedent for a synth path with its own header
list. Match upstream's measured order. The `connection` value continues to come from the
existing `connection_value(close)` helper (`hcm.rs:2172-2174`).

### 2.3 The `location` header — the construction rule

Request authority is whatever the client sent in `Host:`. Rows measured with
`Host: envoy-rust.test` unless the row says otherwise; the listener is plaintext HTTP/1.1;
every route below is `prefix:`-matched with a **non-overlapping** prefix.

| # | route `redirect:` config | request target | status | measured `location` |
|---|---|---|---|---|
| R1 | `host_redirect: "example.com"` | `/a-host` | 301 | `http://example.com/a-host` |
| R2 | `host_redirect: "example.com"` | `/b-query/deep?a=b` | 301 | `http://example.com/b-query/deep?a=b` |
| R3 | `path_redirect: "/newpath"` | `/c-pathr/sub` | 301 | `http://envoy-rust.test/newpath` |
| R4 | `path_redirect: "/newpath"` | `/d-pathq/x?k=v` | 301 | `http://envoy-rust.test/newpath?k=v` |
| R5 | `prefix_rewrite: "/replaced"` | `/e-pfx/sub` | 301 | `http://envoy-rust.test/replaced/sub` |
| R6 | `https_redirect: true` | `/f-https/x` | 301 | `https://envoy-rust.test/f-https/x` |
| R7 | `host_redirect: "example.com", response_code: TEMPORARY_REDIRECT` | `/g-c307` | **307** | `http://example.com/g-c307` |
| R8 | `host_redirect: "example.com", strip_query: true` | `/h-strip/a?q=1&z=2` | 301 | `http://example.com/h-strip/a` |
| R9 | `host_redirect: "example.com", port_redirect: 8443` | `/i-port` | 301 | `http://example.com:8443/i-port` |
| R10 | `{}` (bare, all defaults) | `/j-bare/deep` | 301 | `http://envoy-rust.test/j-bare/deep` |
| R11 | `scheme_redirect: "ftp"` | `/k-scheme/x` | 301 | `ftp://envoy-rust.test/k-scheme/x` |
| R12 | `scheme_redirect: "https", host_redirect: "e.com"` | `/l-both/y` | 301 | `https://e.com/l-both/y` |
| R13 | `host_redirect: "e.com", strip_query: true, response_code: SEE_OTHER` | `/m-see/y?q=1` | **303** | `http://e.com/m-see/y` |
| R14 | `https_redirect: true, port_redirect: 443` | `/n-hport/y` | 301 | `https://envoy-rust.test:443/n-hport/y` |
| R15 | `host_redirect: "example.com", response_code: FOUND` | `/o-found` | **302** | `http://example.com/o-found` |
| R16 | `host_redirect: "example.com", response_code: PERMANENT_REDIRECT` | `/p-perm` | **308** | `http://example.com/p-perm` |

**Authority rows, with an explicit port in `Host:` that does NOT match the listen port
[76.2-NEW].** These are what prove the rule is driven by the `Host` header rather than by
the socket, and they are what make fixture `0086` possible at all (§4.1):

| # | `Host:` sent | route config | target | measured `location` |
|---|---|---|---|---|
| Q1 | `envoy-rust.test:1234` | `https_redirect: true` | `/f-https/x` | `https://envoy-rust.test:1234/f-https/x` |
| Q2 | `envoy-rust.test:1234` | `host_redirect: "example.com"` | `/a-host` | `http://example.com/a-host` |
| Q3 | `envoy-rust.test:1234` | `{}` (bare) | `/j-bare/d` | `http://envoy-rust.test:1234/j-bare/d` |
| Q4 | `envoy-rust.test:1234` | `https_redirect: true, port_redirect: 443` | `/n-hport/y` | `https://envoy-rust.test:443/n-hport/y` |

Two further edge rows **[76.2-NEW]**:

| # | route config | target | measured `location` | reading |
|---|---|---|---|---|
| E1 | `https_redirect: false` (alone) | `/y-hfalse/z` | `http://envoy-rust.test/y-hfalse/z` | an explicit `false` behaves as the default scheme |
| E2 | `path_redirect: ""` | `/x-emptypath/z` | `http://envoy-rust.test/x-emptypath/z` | an EMPTY `path_redirect` performs **no** rewrite — the original path survives |

Body on every redirect row: **empty**, `content-length: 0`.

### 2.4 The derived rules — this is what the implementation must encode

Read off R1-R16, Q1-Q4, E1-E2. Not from documentation.

**(a) Scheme.** Default = the scheme the request arrived on (`http` on a plaintext
listener — R1/R10). `https_redirect: true` forces `https` (R6); an explicit
`https_redirect: false` is the default (E1). `scheme_redirect: "<s>"` forces the literal
`<s>` and is **not** validated against any scheme allow-list — the literal `ftp` was
accepted and emitted verbatim (R11).

**(b) Authority — the asymmetry, and the trap.** The one rule a from-scratch
implementation is most likely to get wrong:

- `host_redirect` **set** → the authority becomes that host and **the request's original
  port is DROPPED** (R1, and decisively Q2: `Host: envoy-rust.test:1234` →
  `http://example.com/a-host`, no port).
- `host_redirect` **unset** → the request's original authority is preserved **including its
  port** (Q1/Q3 keep `:1234`; R6/R10 have no port to keep).
- `port_redirect` overrides the port in **both** cases and renders as `:<n>` (R9 with
  `host_redirect`, R14/Q4 without).
- A scheme-only change does **not** normalise or drop a now-redundant port: R14/Q4 produce
  the literal `https://…:443/…`, and Q1 keeps `:1234` on an `https` URL.
- `port_redirect` is rendered **verbatim with no range clamp** — the parent recon measured
  `70000` rendering as `:70000`. `76.1` deliberately added no `1..=65535` bound.

**(c) Path.** Exactly one of three, or none:

- none → the request path is used as-is (R1/R10).
- `path_redirect: "/p"` → the path becomes the literal `/p` (R3) — **unless it is empty**,
  which performs no rewrite (E2).
- `prefix_rewrite: "/p"` → the portion of the path matched by the route's `prefix:` matcher
  is replaced by `/p` and the remainder appended (R5: route `prefix: "/e-pfx"`, request
  `/e-pfx/sub`, rewrite `/replaced` → `/replaced/sub`).
- `regex_rewrite` → **NON-GOAL** (§7); boot-fatal here via `deny_unknown_fields`.

**(d) Query.** By default the request's query string is **preserved and re-appended**, and
this holds even when the path is replaced wholesale — R4 shows `path_redirect: "/newpath"`
against `/d-pathq/x?k=v` yielding `/newpath?k=v`. `strip_query: true` drops it (R8/R13).

**(e) Status.** Default 301; the five `response_code` values map to the §2.1 table.

### 2.5 Access-log observables

MEASURED with
`text_format: "PROBE path=%REQ(:PATH)% status=%RESPONSE_CODE% flags=%RESPONSE_FLAGS% details=%RESPONSE_CODE_DETAILS% route=%ROUTE_NAME%\n"`, re-confirmed at the state-2 session:

```
PROBE path=/replaced/sub    status=301 flags=- details=direct_response route=-
PROBE path=/c-pathr/sub     status=301 flags=- details=direct_response route=-
PROBE path=/a-host          status=301 flags=- details=direct_response route=-
```

- **`%RESPONSE_CODE_DETAILS%` is `direct_response`** on every redirect row — the *same*
  string upstream uses for a `direct_response:` route. envoy-rust already emits exactly
  that string at `crates/envoy-http1/src/hcm.rs:2112` as a **bare `&'static str` literal**
  (MEASURED: there is no named constant for it anywhere in the repo; the sibling
  `"route_not_found"` at `hcm.rs:2086`/`:2105` follows the same bare-literal convention).
  So the redirect arm **reuses the literal verbatim** and needs **no new detail string, no
  new `Op`, and no new `AccessLogRecord` field**.
- **`%RESPONSE_FLAGS%` is `-`** on every redirect row.
- **`prefix_rewrite` MUTATES the logged `:path`** — request `/e-pfx/sub` logged as
  `path=/replaced/sub`. By contrast `path_redirect` does **not** (`/c-pathr/sub` logged
  unchanged). A real discriminating observable and a parity trap: the rewrite is applied to
  the request's `:path` in place for `prefix_rewrite`, while `path_redirect` affects only
  the `location` string.

---

## 3. The runtime insertion point (verified on disk)

**One seam serves both codecs.** Route-action dispatch is a single `match &route.action` at
`crates/envoy-http1/src/hcm.rs:2110`, inside `build_response_in`
(declared `:2051-2055`, returning `BuildOutcome`):

```rust
    // Hardcoded router-filter call site.
    match &route.action {
        RouteAction::DirectResponse(dr) => {
            BuildOutcome::Synth(synth_direct_response(dr, close), Some("direct_response"))
        }
        RouteAction::Route(ar) => BuildOutcome::Proxy { /* … 5 fields … */ },
    }
```

**HTTP/2 has no route-action dispatch of its own — CONFIRMED, not assumed.** It calls H1's
resolver at `crates/envoy-http2/src/hcm.rs:475` and H1's `build_response` at `:518`
(imported at `:18`). A `grep` over `crates/envoy-http2/` for `RouteAction` / `route.action`
returns 38 hits of which **zero** are a dispatch: 35 are `RouteAction::…` route-table
literals inside `#[cfg(test)]` fixtures, and 3 are comments/imports. There is no
`match … action` anywhere in the crate. **A third arm at `hcm.rs:2110` therefore serves
both codecs at once.**

Everything the redirect needs is **already in scope at the dispatch site**: the matched
`route` (so `route.r#match.prefix` for `prefix_rewrite`), `req.path` (the raw target),
`req.headers` (for `Host`), and `close`. `Route.r#match` is `RouteMatch`
(`bootstrap.rs:2572-2581`) whose `prefix: Option<String>` is at `:2576`. **No plumbing
through `ResolvedRoute` is required** and no signature change is needed for the
`location` build.

### 3.1 The `prefix_rewrite` `:path` mutation needs `&mut Request` — and that is cheap

The access-log record takes its path from `x_envoy_original_path_or_path(request.req)`
inside `build_access_log_record` (`hcm.rs:1601-1608`), which runs **after**
`build_response_in` returns. So for §2.5's `prefix_rewrite` mutation to be observable, the
rewrite must be applied to `req.path` itself — but `build_response_in` currently takes
`&Request`.

MEASURED: this is a small, contained change, because **every call site already holds a
mutable binding**:

- `crates/envoy-http1/src/hcm.rs:859` — `let mut req = req;`
- `crates/envoy-http1/src/uring.rs` — `req.body = Some(request_body);` at `:280`, so `req`
  is already `mut`; the call is at `:287`
- `crates/envoy-http2/src/hcm.rs:459` — `let mut envoy_req = http_to_envoy_request(…)?;`,
  and the `build_response` call is at `:518`

MEASURED total: **8** `build_response` / `build_response_in` call sites across the
workspace — 6 in `crates/envoy-http1/src/hcm.rs` (including two in-file unit tests at
`:9690` and `:9707`), 1 in `uring.rs`, 1 in `crates/envoy-http2/src/hcm.rs`. So the change
is two signature lines plus eight `&` → `&mut` edits.

One borrow-checker caveat to expect in H2: `envoy_req` is `mem::take`-emptied into a
`FilterRequest` at `hcm.rs:489` and written back before `:518`, and `matched_route`
borrows `config.inner` earlier. Take `&mut envoy_req` only at `:518`, after those borrows
end.

**Alternative considered and rejected:** widening `BuildOutcome::Synth` with a third
"rewritten path" field. MEASURED **9** `BuildOutcome::Synth(` construction sites, so it is
strictly more churn than the 8-site `&mut`, and it models the effect less honestly —
upstream really does rewrite the request's `:path` in place.

---

## 4. The differential fixture `0086`

**Id `0086` is the next free.** MEASURED via `git ls-files`: `tests/fixtures/` holds
exactly **85** directories, highest `0085-headermatcher-absence-accesslog-present-polarity`;
`tests/differential/tests/` holds exactly **85** `.rs` files (1:1, no subdirectories, no
non-`.rs` files); `git ls-files 'tests/fixtures/0086*'` returns **0**.

Proposed name: `0086-route-redirect-action`, entrypoint
`tests/differential/tests/route_redirect_action.rs`.

### 4.1 Why the fixture works with zero new harness machinery

Three measured facts combine:

1. **`location` is not allow-listed.** `HEADER_ALLOW_LIST`
   (`tests/differential/src/lib.rs:1177-1181`) has exactly three entries — `server`,
   `date`, `x-envoy-upstream-service-time`, all `AllowMode::NameRequired`. `diff_headers`
   (`lib.rs:1192-1247`) skips value comparison only for allow-listed names and compares
   every other name **byte-exact** (`lib.rs:1237`). So `location` and `content-length` are
   both compared value-exact, for free.
   **NEVER add `location` to the allow-list — it would silently vacate the entire witness.**
2. **The name-set check catches the `content-type` hazard of §2.2.** `diff_headers`
   compares lowercased name sets first (`lib.rs:1209-1215`) and bails with
   `only-in-envoy-rust=[…]`. So a redirect built on `synth_with` fails loudly.
3. **Both proxies receive an IDENTICAL `Host:` header.** `Http1Probe::host`
   (`lib.rs:1149`) is a required per-probe field and `drive_http1` writes it verbatim into
   the request line block (`lib.rs:2194-2199`). The two proxies listen on **different**
   ports — upstream on a testcontainers-mapped port, the subject on a reserved ephemeral
   port — but the authority in `location` comes from the `Host` header, **not** from the
   socket (proved by Q1-Q4 of §2.3, where the `Host` port deliberately differs from the
   listen port). So `location` is byte-comparable across the two sides.

No new driver, no new rule variant, no new expectation kind.

### 4.2 Fixture-authoring constraints — bake these into `PLAN.md`

- **Every probe carries a DISTINCT `path:`.** This is the standing rule at
  `BEHAVIOR_CONTRACT.md` Phase 75 §G. Here it is load-bearing for **correctness** as well
  as attribution, because each probe must select a *different* route.
- **PREFIX OVERLAP SILENTLY SHADOWS A PROBE.** A parent-recon cell was lost when
  `prefix: "/scheme"` preceded `prefix: "/schemehost"`, so `/schemehost/y` matched the
  earlier route and returned its answer. The §2.3 measurement set was re-run with
  deliberately non-overlapping prefixes (`/a-host`, `/b-query`, … `/p-perm`), where no
  prefix is a prefix of another. **Reuse that scheme; verify no probe can match an earlier
  route.**
- **Query-bearing probes MUST use `prefix:`-matched routes**, never `path:`. This keeps the
  fixture clean of **CF-76-1** (§7 item 5): upstream strips the query before route matching
  while envoy-rust matches the raw target, so an exact-`path:` route plus a query would
  diverge for reasons that have nothing to do with redirect. Every route in the §2.3 set is
  already `prefix:`-matched.
- **`Http1ProbeList` ABORTS AT THE FIRST FAILING PROBE.** MEASURED: every check in
  `run_http1_probe_list_arm` (`lib.rs:5421-5536`) uses `bail!`/`?` inside the
  `for probe in probes` loop, so the first failure returns immediately and later probes
  never run. One red run names ONE probe. A regression breaking several cells reports as a
  single failure — that is expected, and a review must not read it as "only one cell broke".
  If a mutation check needs to witness two cells, cite a second fixture rather than
  claiming two cells from one run.
- Fresh TCP connection per probe, `Connection: close` always appended; no keep-alive.

### 4.3 The probe set

One probe per row of §2.3 — R1-R16 plus Q1/Q3 (the authority-port cells, which need a
`host:` carrying an explicit `:1234`) — each asserting `expected_status` **and**
`expected_headers: set_equal_modulo_allow_list`, plus
`expected_body: { kind: byte_exact, body: "" }`.

Note Q2/Q4 are omitted from the fixture: Q2 duplicates R1's `location` and Q4 duplicates
R14's, so they add config lines without adding a distinguishable cell. Both remain pinned
in-process.

### 4.4 Exact fixture file shapes (verified templates)

The working template is `tests/fixtures/0007-http1-direct-response` — backend-free,
`clusters: []`, `Http1ProbeList`, per-probe `expected_headers`. The newest multi-probe
convention is `tests/fixtures/0083-headermatcher-absence-parity` (22 probes).

- `envoy.yaml` — HCM listener on `{{PORT}}`, `address: 0.0.0.0`, `codec_type: HTTP1`,
  `clusters: []`, and a trailing `admin:` block on `port_value: 0`.
- `envoy-rust.yaml` — **identical except three hunks**: a `node: { id: x, cluster: y }`
  block prepended, `0.0.0.0` → `127.0.0.1`, and the `admin:` block deleted.
  **YAML 1.1 trap:** an unquoted `cluster: y` under `node:` parses as boolean `true` — the
  existing fixtures write it exactly as `y` and it is fine there, but do not "improve" it.
- `expectations.yaml` — `driver: { kind: http1_probe_list, probes: [...] }` then
  `equivalence: { response_status: exact, response_body: { kind: byte_exact } }`.
- `README.md` — conventional (82 of 85 fixtures have one); `run_fixture` never reads it.

`{{PORT}}` is the only token this driver substitutes — `port_key_for` returns `"PORT"` and
`Http1ProbeList` is **not** in `driver_needs_admin_port`, so **`{{ADMIN_PORT}}` must not
appear**.

Exact per-probe schema (`Http1Probe`, `lib.rs:1144-1165`) — required `name`, `method`,
`path`, `host`; optional `extra_headers`, `body`, `expected_status`, `expected_body`,
`expected_headers`; `deny_unknown_fields`, so a typo'd key fails to deserialize:

```yaml
    - name: r01-host-drops-port
      method: get
      path: "/a-host"
      host: "envoy-rust.test"
      expected_status: 301
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
```

**`expected_headers` is a BARE SCALAR**, not a map — `Http1HeaderRule`
(`lib.rs:1069-1073`) is an externally-tagged unit-variant enum with
`rename_all = "snake_case"`. Do not confuse it with the sibling `HeaderRule`
(`lib.rs:1081-1085`), which is `#[serde(tag = "rule")]` and *is* spelled as a map — that
one belongs to `Driver::Http1WithAccessLog`.

### 4.5 Registration

**No registry file exists.** `tests/differential/Cargo.toml` has **zero** `[[test]]`
sections; cargo auto-discovers every `tests/differential/tests/*.rs`. Adding the file is
sufficient. Naming convention across all 85: fixture directory name minus the `NNNN-`
prefix, `-` → `_`, and the test fn is `<same>_fixture`.

**33 test-binary NAMES are duplicated between `tests/differential/tests/` and
`crates/envoy-bin/tests/`** — always pass `-p differential`, never a bare `--test <name>`.

---

## 5. Scope — what this sub-phase builds

1. **Replace the `76.1` placeholder arm** at `hcm.rs:2110` with the real redirect, and flip
   its `76.1` test deliberately (not silently).
2. **A pure `location`-builder**: `(request authority, raw request target, matched route's
   `prefix:`, &RedirectAction) → (status, location)`, encoding §2.4 (a)-(e). Pure and
   total, so it is exhaustively unit-testable without a socket.
3. **`synth_redirect`** — a dedicated response builder emitting exactly
   `location`, `date`, `server`, `connection`, `content-length` with an empty body (§2.2).
   **Not** `synth_with`.
4. **Three reason phrases** — add 303/307/308 to `canonical_reason`
   (`response.rs:188-215`), pinned in-process because the fixture cannot see them (§2.1).
5. **The `prefix_rewrite` in-place `:path` mutation** via `&mut Request` (§3.1).
6. **Fixture `0086`** (§4), and its entrypoint.
7. **`BEHAVIOR_CONTRACT.md` Phase 76 section** banking §2.1-§2.5: the R/Q/E tables, rules
   (a)-(e), the header set including the **no-`content-type`** rule, the
   `%RESPONSE_CODE_DETAILS%` = `direct_response` reuse, and an explicit statement that
   `location` is **not** allow-listed and is therefore compared value-exact.
8. **Close the parent phase `76`** — flip rows `76.2` and parent `76` to `done` at the
   state-6 close-out.

---

## 6. The test obligations

**In-process (the bulk — none needs Docker):** one unit test per §2.3 row R1-R16, Q1-Q4,
E1-E2; the header-set test asserting **exactly five names and NO `content-type`**; the
three reason phrases 303/307/308; the `prefix_rewrite` `:path` mutation and the
`path_redirect` non-mutation; the `%RESPONSE_CODE_DETAILS%` = `direct_response` reuse; and
**an HTTP/2 in-process redirect test** proving the shared seam (§3) really does serve H2.

**Differential:** fixture `0086` green; all **85** pre-existing fixtures still green.

---

## 7. Non-goals (explicit — do NOT widen into these)

1. **`regex_rewrite` inside `redirect`.** Measured working upstream; excluded to hold the
   LoC gate. Boot-fatal here via `deny_unknown_fields` — the intended posture.
2. **`RouteAction.prefix_rewrite` / `regex_rewrite` / `host_rewrite_*` on the `route:`
   (proxying) arm.** This sub-phase touches the **redirect** arm only.
3. **`internal_redirect_policy`, `non_forwarding_action`, `weighted_clusters`,
   `cluster_header`, route-level `timeout`, route/vhost/route-config-level header
   mutation.** All measured ABSENT from envoy-rust; all out of scope.
4. **An HTTP/2 differential fixture.** H2 reuses H1's `build_response` (§3), so it is
   covered in-process plus by the H1 fixture — the disposition phases 68 and 69 took.
5. **`CF-76-1`** — upstream strips the query before route path matching
   (`path: "/exact"` MATCHES `/exact?q=1`) while envoy-rust compares the raw target
   byte-for-byte (`route_matches` does `path == p` at `hcm.rs:2155-2166`, called with
   `req.path` at `:2028` and `:2094`; `codec.rs:26-28` documents "byte-for-byte (no
   normalization)"; H2 inherits via `envoy-http2/src/hcm.rs:475`). **Silent, pre-existing,
   on the most-exercised surface in the project, pinned by NO fixture.** Do NOT fix it
   opportunistically (§6.3) — it interacts with `%REQ(:PATH)%` logging, the
   upstream-forwarded target and this sub-phase's own query rule, so it needs its own
   measured phase. §4.2's `prefix:`-only rule keeps this fixture clean of it.
6. **`CF-75-2`, `CF-75-3`, `CF-75-4`, `CF-75-5`, `CF-75-6`** and every other open
   carry-forward.
7. **The `%RESPONSE_CODE_DETAILS%` strings of the error synth paths** (400/404/501), still
   `None` in envoy-rust. Unrelated.

---

## 8. NOT MEASURED — do not treat as settled

1. Redirect behaviour on a **TLS** listener — whether the default scheme becomes `https`
   when the request arrived over TLS. Every probe used a plaintext listener. Rule (a) says
   "the scheme the request arrived on", which is the natural reading, but the `https` case
   was **not** measured.
2. Redirect behaviour over **HTTP/2** *upstream-side*. All probes were HTTP/1.1. envoy-rust
   shares H1's resolver, but upstream's H2 `:scheme`/`:authority` handling was not probed.
   (The in-process H2 test of §6 covers envoy-rust's own seam, not upstream parity.)
3. A request with **no `Host` header** — whether it reaches a redirect route at all, and
   what authority `location` then carries. Note envoy-rust's `resolve_route_in` returns
   `None` on a missing/empty Host (`hcm.rs:2019`), yielding the existing 400 path.
4. `port_redirect` boundary behaviour above 65535 beyond the single `70000` probe.
5. The interaction of `redirect` with `typed_per_filter_config` on the same `Route`.
6. Whether `strip_port` (`hcm.rs:2138-2143`, `rfind(':')`-based) handles a **bracketed
   IPv6 literal** authority correctly. It is pre-existing and used for vhost matching; a
   redirect echoing the authority may surface it. Not probed.

---

## 9. Size estimate

| component | est. net LoC |
|---|---|
| pure `location`-builder + docs | 95 |
| `synth_redirect` (own header set) | 35 |
| dispatch arm (replacing the placeholder) | 15 |
| `canonical_reason` 303/307/308 | 3 |
| `&mut Request` signature + 8 call sites | 10 |
| in-process tests (R/Q/E rows, header set, reason phrases, path mutation, H2) | 420 |
| fixture `0086` (2 configs + expectations + README + entrypoint) | 415 |
| `BEHAVIOR_CONTRACT.md` Phase 76 section | 90 |
| **total** | **≈ 1083** |

Under both §6.1 thresholds. Projected **≈ 10-12 numbered tasks**. **No further split is
projected** — but the estimate must be re-derived at this sub-phase's own state-2
PLAN-write, which owns that decision.

---

## 10. Next state

`76.2` does not start until `76.1` is `done`. When it does, its next session is the
**§5 state-2 PLAN-write** (`superpowers:writing-plans`), a **SEPARATE session**. It must:

1. **Re-verify every `file:line` citation above on disk before transcribing it** — anchor
   on TEXT, never on a number. **`76.1` lands ahead of this sub-phase and WILL shift
   `bootstrap.rs` line numbers**, and it may shift `hcm.rs` (the placeholder arm).
2. Write `PLAN.md` as TDD-ordered numbered tasks (D-3.1: tests first, no exceptions).
3. **Pre-flight the PLAN's own literal Rust** against `cargo fmt --all -- --check` and
   `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
4. Note for state 3/4: **`cargo build -p envoy-bin` before any local differential run** —
   the harness runs `target/debug/envoy-bin`, and `76.1` added a config key, so a stale
   binary REDs with `unknown field`. A backend-free redirect fixture is **fully verifiable
   locally**, which is a deliberate property of this phase's pick.
