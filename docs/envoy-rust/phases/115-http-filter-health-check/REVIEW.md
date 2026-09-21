# Phase 115 — `envoy.filters.http.health_check` (non-pass-through mode) — §5 STATE-5 CODE REVIEW

> **Verdict: NOT APPROVED — re-enter §5.2 STATE 3** (not state 4). §2 carries **two MUST-FIX
> wire divergences on the phase's OWN reply** plus the landed contract claim that hides them.
> Both were reproduced BY THIS SESSION against `envoyproxy/envoy:v1.33.0` (image digest
> `sha256:56da5afd…770c2`, the `ENVOY_TARGET.md` pin, checked per container by `docker inspect`)
> and the landed DEBUG `envoy-bin`, on fixture `0095`'s own config with only `codec_type` varied.
> Everything else in the phase is sound: the filter, the `FilterResponse::details` seam, the
> `node.cluster` stamping, the `downstream_rq_Nxx` exclusion and both fixtures were verified
> correct, most of them by mutation.

This document is written for a reader with no prior context (D-3.4). The phase, its scope and its
seven SPEC corrections are defined in `SPEC.md`, `PLAN.md` and `ADR-0200`/`ADR-0201`; where
`SPEC.md` and `PLAN.md` disagree, `PLAN.md` wins (`ADR-0201`). Code citations are to HEAD `845bb23`,
whose code tree is identical to the reviewed head `9aa367c` (every commit after it is docs-only).

---

## §0 — How this review was conducted

### §0.1 Scope

The code under review is the arc `04661b7..9aa367c`: eight task commits plus the state-3 advance,
**1454 insertions / 55 deletions = 1399 net over 23 files excluding `docs/`** (re-derived:
`git diff --numstat 04661b7..9aa367c -- . ':(exclude)docs/'`), plus the `BEHAVIOR_CONTRACT.md`
section Task 8 added (`63d9445`, `72 0`). The largest files: `crates/envoy-config/src/bootstrap.rs`
(`283 0`), `crates/envoy-filter/src/health_check.rs` (`274 0`, created),
`crates/envoy-http1/src/hcm.rs` (`182 19`), `crates/envoy-http2/src/hcm.rs` (`115 20`).

### §0.2 Method

This is a THIRD context: state 3 implemented and state 4 graded, and neither may review (§5.1,
`ADR-0127`). Five read-only reviewers were dispatched over independent dimensions, each in its own
scratch worktree with its own `CARGO_TARGET_DIR` (all removed afterwards; the main tree stayed
clean): (1) the config grammar, validator and `node.cluster` stamping; (2) the filter, its
instance arm and the `details` seam; (3) the H1/H2 HCM wiring, the Nxx exclusion and the access-log
detail; (4) fixtures `0095`/`0096`, their runners, the contract section and the numeric record;
(5) an UNTESTED-COMPOSITION probe run against BOTH proxies. **Every finding charged in §2 and §3
was re-verified by this session on disk before being written down**; findings reported by a
reviewer but NOT re-verified here are labelled as such (§3 F-8) and are banked as leads, not as
facts.

### §0.3 The §7.5 gate was NOT re-run here

Legs (a)–(e) were recorded by the state-4 gate (`476e70f`, `PROGRESS.md` `# §5 STATE 4`) and
confirmed by CI run `35581009301` on `476e70f` (`binaries=172 passed=2345 failed=0`). This review
re-derived the figures it relies on (the 1399 net; the 30 added `#[test]`/`#[tokio::test]`
attributes against 0 removed; 13 production `HttpFilterInstance` arms + 2 `test-util` arms;
byte-identical fixture configs by `cmp`) and treats the gate's legs as valid FOR THE REVIEWED TREE.
Because §2 sends the phase back to state 3, the gate must be re-run in full at the next state 4.

---

## §1 — Strengths (verified, not assumed)

- **The `:path` view is correct and complete.** `HealthCheckFilter::matches`
  (`crates/envoy-filter/src/health_check.rs`, `fn matches`) routes a `:path` matcher to a one-entry
  `[(":path", req.path)]` view and every other matcher to `req.headers`, AND-folded with
  `Iterator::all` (empty list ⇒ match-all). `HeaderMatcher::matches`
  (`crates/envoy-config/src/matcher.rs`) looks the name up case-insensitively, so every mode —
  `present_match`, `invert_match`, prefix/suffix/contains with `ignore_case`, `safe_regex` —
  composes with the view. The composition probe confirmed `invert_match` on a `:path` prefix,
  `present_match`, an upper-case `:PATH`, a mixed-case header name and a `host` matcher all behave
  sanely.
- **Build-time regex compilation closes the `CF-115-9` trap for this filter.** `build_from_config`
  compiles every matcher's `SafeRegex` on an owned clone, so a bad pattern is build-fatal
  (`bad_regex_is_build_fatal`) and a good one cannot hit the `validator ensured … compiled` panic
  the `fault` filter hits.
- **The `details` seam is minimal and loss-free.** Exactly two production reads of `.details`
  exist, both on the decode-side `StopAndSend` arm (`envoy-http1/src/hcm.rs` and
  `envoy-http2/src/hcm.rs`, the `SynthFromDecode(…, filter_resp.details)` construction).
  `FilterPipeline::decode_headers` moves the response, never rebuilds it. The only production
  producer of a non-`None` value is `HEALTH_CHECK_OK` in `health_check.rs`. Mutating either codec's
  `response_code_details_for_log = details.map(str::to_owned)` to `None` turns both of that codec's
  new tests RED (`d=-` against `d=seam_probe` / `d=health_check_ok`).
- **Declaring `response_code_details_for_log` uninitialised is an improvement, not a risk.** The
  compiler's definite-assignment check now forces every arm to set it; the diff adds exactly one
  assignment per codec, and it compiles, so no pre-existing arm relied on the old `None` default.
- **The Nxx exclusion keys on the right thing and is mutation-proved on BOTH codecs.** Breaking the
  gate in H1 alone and in H2 alone (forced rebuilds, unmutated control green) fails each codec's
  "the intercept is not counted" assertion. The key string has one producer, and nothing writes the
  variable between the `SynthFromDecode` arm and the Nxx site. `downstream_rq_completed` and
  `downstream_rq_http1_total` do not exist in envoy-rust (re-derived: `rq_completed` = 0 hits in
  `crates/`; every `http1_total` hit is the upstream-connection `upstream_cx_http1_total`), so the
  divergence `ADR-0201` DECISION 4 warns about cannot occur on a stat envoy-rust emits.
- **The `node.cluster` stamp reaches every production path.** `validate_hcm` has ONE production
  caller; `validate()` runs at `parse_bootstrap` and again post-merge in `load_dynamic_resources`;
  every `HCMConfig::from_config` site in `envoy-bin` reads listeners after that merge; the RDS
  watcher swaps route tables only and never rebuilds a pipeline. A probe with an LDS-file listener
  under `node.cluster: lds-node-cluster` came back stamped.
- **Both fixtures are non-vacuous and green.** `envoy.yaml` ≡ `envoy-rust.yaml` by `cmp` for both.
  `0095`'s driver compares status and body per side against the expectation and the header NAME SET
  plus every non-allow-listed VALUE across sides, so the stamped header's presence on an intercept,
  its absence on a fall-through, `content-type` and `content-length` all discriminate. The fixture
  reviewer re-ran both green (~1.4 s, normal for backend-free) and reproduced mutation V3 (`all` →
  `any`, exit 101 at `p9-and-path-alone-falls-through`). `0096`'s non-zero entries are real
  witnesses and its `value: 0` entry is honestly disclosed as NOT a presence witness.
- **Chain composition matches upstream** (probe, both proxies): header_mutation adding the matched
  header BEFORE the filter; `local_ratelimit` before the filter; response-header mutation placed
  before AND after the filter (upstream ALSO decorates the intercept from a filter AFTER it — so the
  encode-all-filters behaviour of `FilterPipeline::encode_headers` is parity here); `cors` before
  and after; H1 keep-alive pipelining of two intercepts; a `Content-Length` POST body drained
  before the reply; the default (non-normalising) path handling of `//healthz`, `/a/../healthz`,
  `/%68ealthz`, `/./healthz`; a `node`-less bootstrap's empty header value.
- **The Task-1 rider (`CF-114-6`) landed correctly** at numstat `3 3`: the
  `build_access_log_record` doc block now sits directly above its `fn`.
- **The record is accurate.** Every `PROGRESS.md` figure spot-checked reproduced: 1399 net over 23
  files, the per-task numstats `3/3, 124/24, 376/16, 265/0, 185/12, 304/0, 197/0`, 30 added test
  attributes (config 13, filter 11, http1 2, http2 2, differential 2), `health_check.rs` 274 lines.

---

## §2 — Issues (MUST FIX) — the phase re-enters §5.2 STATE 3

### I-1 — On H2 an intercepted probe carries `content-length: 0`; upstream sends NO `content-length`

**Measured this session** (fixture `0095`'s config, `codec_type: HTTP2`, `curl --http2-prior-knowledge`,
`date` stripped):

| request | upstream v1.33.0 | envoy-rust (landed) |
|---|---|---|
| `GET /healthz` | `:status 200`, `x-envoy-upstream-healthchecked-cluster: hc-fixture-cluster`, `server` | the same **plus `content-length: 0`** |
| `HEAD /healthz` | same as GET — no `content-length` | the same **plus `content-length: 0`** |
| `GET /other` (control, `direct_response`) | `content-length: 4`, `content-type: text/plain`, `server` | identical set |

**Why it is a MUST-FIX.** Response headers are compared set-equal modulo the allow-list
(`BEHAVIOR_CONTRACT.md` §7.2 matrix; the allow-list is `server`, `date`,
`x-envoy-upstream-service-time`). This is an extra header on EVERY H2 intercept — the phase's
primary output on a codec the phase explicitly enabled and pinned in-process
(`h2_health_check_filter_intercepts_and_logs`, which asserts `content-type` absent but never
`content-length`). The `direct_response` control shows the framing path is otherwise at parity;
the divergence is specific to the filter-synth reply. **`CF-115-4` (no H2 differential witness)
was hiding a real header-set divergence, not merely a coverage gap.**

**Cause.** Both codecs decorate a decode-side filter reply with
`decorate_filter_synth_response` (`crates/envoy-http1/src/hcm.rs`, `pub fn
decorate_filter_synth_response`; H2 reaches it through `decorate_filter_synth_response_h2` in
`crates/envoy-http2/src/response.rs`), which ALWAYS overwrites `content-length` from `body.len()`.
Upstream's health-check reply is headers-only.

### I-2 — On H1 a `HEAD` intercept carries `content-length: 0`; upstream sends `transfer-encoding: chunked` and no `content-length`

**Measured this session** (`codec_type: HTTP1`, keep-alive requests):

| request | upstream v1.33.0 | envoy-rust (landed) |
|---|---|---|
| `GET /healthz` | `x-envoy-upstream-healthchecked-cluster`, `server`, `content-length: 0` | same names (+ `connection: keep-alive`, pre-existing, see note) |
| `HEAD /healthz` | `x-envoy-upstream-healthchecked-cluster`, `server`, **`transfer-encoding: chunked`**, NO `content-length` | `content-length: 0`, no `transfer-encoding` |
| `HEAD /other` (control, `direct_response`) | `content-length: 4`, `content-type`, `server` | identical set |

**Why it is a MUST-FIX.** HEAD is an ordinary health-probe method and the filter is
method-agnostic by design, so this is reachable by the phase's own in-scope configs on the H1 codec
the phase claims as its witnessed surface. The `direct_response` HEAD control is at parity, so the
divergence is again the filter-synth framing. Fixture `0095` cannot see it: its ten probes are GET
and POST only, and **the H1 probe driver cannot express HEAD at all** —
`tests/differential/src/lib.rs` `pub enum Http1Method` has exactly `Get`, `Options`, `Post`.

*Note — not charged:* envoy-rust's `connection: keep-alive` on a keep-alive H1 reply (upstream sends
no `connection` header) appears on the `direct_response` control too; it is pre-existing
(ADR-0033 decoration) and is not this phase's. The fixture driver always sends `Connection: close`,
under which both proxies send `connection: close`.

### I-3 — The landed contract section states the divergent cells as MEASURED fact

`BEHAVIOR_CONTRACT.md`, section **"health_check intercept wire shape (MEASURED)"** (the bullet
beginning "Status **200**; body EMPTY (`content-length: 0`)"), states `content-length: 0` for the
intercept without a codec or method qualifier. It is true for H1 non-HEAD only; it is false for H2
(I-1) and for H1 HEAD (I-2). The section's lead also says the behaviour is "differentially proven
by fixtures `0095` … and `0096`", which no fixture can prove for either cell.

### Required disposition for the §5.2 STATE-3 re-entry

1. Make a health-check intercept a HEADERS-ONLY reply on both codecs so that the wire matches the
   measured cells above: H2 — no `content-length`; H1 non-HEAD — `content-length: 0` (unchanged);
   H1 HEAD — `transfer-encoding: chunked`, no `content-length`, no body bytes. How to express
   "headers-only" (a `FilterResponse` flag, a dedicated decoration path, or otherwise) is the
   state-3 session's design call; if it is a decision a later reader needs, record it in an ADR.
   ⚠ Do NOT widen the change to other filters' local replies — their upstream framing is
   unmeasured.
2. Pin each cell in-process: extend `h2_health_check_filter_intercepts_and_logs` to assert
   `content-length` ABSENT (GET and HEAD), and add an H1 HEAD intercept assertion. Each pin's RED
   evidence is a mutation (restore the landed decoration) run against a forced rebuild.
3. Witness I-2 differentially if the driver can be widened safely (add `Http1Method::Head` and a
   body-less response read for HEAD), with a new probe in `0095`; otherwise bank the differential
   HEAD witness as a carry-forward with its reason. I-1's differential witness remains `CF-115-4`.
4. Correct the `BEHAVIOR_CONTRACT.md` wire-shape bullet to state the three cells per codec/method
   and to say which bullets are witnessed differentially and which only in-process (see also N-10).
5. Re-run the full §7.5 gate at the next state 4, and review the fix in a fresh state-5 session that
   writes `REVIEW-2.md` (this `REVIEW.md` is landed and is never edited).

---

## §3 — Important (banked, NOT required for approval)

### F-1 — A chunked-body request answered by a local reply advertises keep-alive and then drops the connection (pre-existing; phase 115 widens its reach) → `CF-115-11`

**Measured this session on envoy-rust:** `POST /healthz` with `Transfer-Encoding: chunked` and a
5-byte body, followed on the same connection by `GET /other`, gets ONE response — `200` with
`connection: keep-alive` — then the unread chunk bytes are parsed as a request line (`WARN
connection task failed error=malformed request line`) and the connection closes; the pipelined GET
is never answered. The identical sequence against `POST /other` gets `501 Not Implemented`
(`Transfer-Encoding: chunked not supported`) with `connection: keep-alive` and the SAME drop, so the
root cause (a local reply that neither drains the chunked body nor sends `connection: close`)
predates this phase. What phase 115 changes is only that `/healthz` now answers 200 instead of 501,
because decode filters run before the 501 check. The probe reviewer measured upstream draining the
body and serving the next request. Health checkers rarely send bodies; banked.

### F-2 — The LDS / post-merge stamping path has no test

`ADR-0201` DECISION 3 and the contract section say "LDS-delivered listeners carry it too". The code
does (§1), but every stamping test uses a static listener through `parse_bootstrap`. A future change
that gated the post-merge `validate` differently would silently send an empty header on LDS
listeners with the suite green. A ~25-line test (LDS file → `load_dynamic_resources` → assert the
stamp) closes it.

### F-3 — `:path` case-insensitivity is unpinned at BOTH the validator and the filter

Both sites use `eq_ignore_ascii_case(":path")` (`validate_health_check_config` in `bootstrap.rs`;
`fn matches` in `health_check.rs`). The reviewers' mutations replacing each with `== ":path"` leave
their crate's suite green, and `git grep` finds no `:PATH`/`:Path` in either file's tests
(re-derived: 0). Under the filter mutation a `:PATH` matcher would be evaluated against
`req.headers` and never fire; under the validator mutation a `:PATH` config would be rejected at
load. One cell in each test module closes both.

### F-4 — `absent_node_stamps_the_empty_string` passes whether or not stamping happens

`local_cluster` is `#[serde(skip)]` and defaults to `""` — exactly what the test asserts. With the
stamp loop deleted, only `validation_stamps_node_cluster_into_the_filter` fails. The test is a
legitimate parity cell but its name claims a stamping witness it cannot give alone.

### F-5 — `counters[7]` / `counters[6]` bind `request_total` / `ok` by POSITION, and the test cannot tell them apart

`build_from_config` takes the two handles by index into `STAT_NAMES`; the test asserts
`(request_total, ok) == (2, 2)`, and the two always tick together, so swapping the indices passes
all eleven tests (reviewer mutation). Invisible today; it becomes a silent mis-count the moment a
path ticks `request_total` without `ok` (`CF-115-1`/`-2`/`-5`). Bind by name.

### F-6 — An H1 absolute-form request target is never normalised (pre-existing) → `CF-115-12`

`GET http://x/healthz HTTP/1.1` leaves `FilterRequest::path` = `http://x/healthz`
(`Http1Codec::parse_request` keeps the raw target), so the filter and the route walker both miss —
consistently with each other — while the probe reviewer measured upstream rewriting it to
`/healthz` and intercepting. H2 is unaffected (`uri.path_and_query()`). It affects every `:path`
consumer, not just this filter; banked.

### F-7 — The encode path runs a decode-side reply through filters AFTER the one that sent it — NOT CHARGED

The codec reviewer flagged `FilterPipeline::encode_headers` iterating ALL filters as a possible
divergence. **The composition probe refutes it for this filter:** a `header_mutation` response
header configured AFTER `health_check` decorates the intercept on upstream too. Recorded here so the
question is not re-opened for this filter; other filters' local replies remain unmeasured.

### F-8 — Pre-existing divergences reported by the probe reviewer and NOT re-verified by this session → lead list `CF-115-13`

Each was reported as measured on both proxies by the composition probe; this session did not
re-run them, so they are LEADS for a future pick, not facts: (a) HTTP/1.0 requests — upstream
answers `426` (`low_version`), envoy-rust serves them; (b) a route `typed_per_filter_config` with
`envoy.config.route.v3.FilterConfig{disabled: true}` — upstream accepts and disables the filter on
that route, envoy-rust rejects at load (affects every filter); (c) HCM `merge_slashes` /
`normalize_path` — rejected at load by envoy-rust, accepted upstream; (d) `Expect: 100-continue` —
envoy-rust never sends `100 Continue`; (e) upstream `cors` does not decorate `direct_response`
routes, envoy-rust does; (f) `local_ratelimit`'s 429 logs `local_rate_limited` upstream and `-` in
envoy-rust (an instance of `CF-115-8`); (g) `codec_type: AUTO` does not detect H2 prior knowledge.

---

## §4 — Minor (banked)

- **N-1** `types.rs`'s doc on `FilterResponse::details` says it is read only to fill the access-log
  record; the same value also gates both codecs' `downstream_rq_Nxx` tick. A future filter that
  reused `health_check_ok` would silently suppress the counters. Name the second use.
- **N-2** Matcher modes on `:path` other than case-sensitive `exact` (invert, present, prefix/suffix/
  contains with `ignore_case`, a VALID `safe_regex`) have no filter-level test; the shared engine is
  tested in `envoy-config` and the probe found them sane, so this is coverage, not correctness.
- **N-3** `path_matcher_sees_the_query_string` and `exact_path_is_exact_and_case_sensitive` assert
  only fall-through, so each would pass alone against a filter that never intercepts; the module as
  a whole is not vacuous. Pair each with a positive cell.
- **N-4** `h2_health_check_filter_intercepts_and_logs` does not assert `downstream_rq_total`; the
  "intercept IS counted in total" half of `ADR-0201` DECISION 4 is pinned on H1 only.
- **N-5** An ENCODE-side `StopAndSend` replacing an intercept keeps the decode-side
  `health_check_ok` (so the replacement is logged and Nxx-excluded as a health check) and discards
  the replacement's own `details`. Unreachable today — no production filter short-circuits on
  encode — but the first one will hit it.
- **N-6** A health-check reply to a request carrying `content-type: application/grpc` passes through
  `apply_grpc_local_reply` (`outgoing_local` stays `true` on the `SynthFromDecode` arm), gaining
  `grpc-status: 0`; upstream's behaviour for this cell is unmeasured and no test covers it.
- **N-7** Grammar edges unmeasured upstream: `cache_time: 0s` with `pass_through_mode: false` and
  `cluster_min_healthy_percentages: {}` are REJECTED by envoy-rust (upstream plausibly treats both
  as unset and accepts); `headers: null` is a serde error. Worth one `--mode validate` probe.
- **N-8** `ConfigError::UnsupportedHealthCheckField`'s message ends "only pass_through_mode: false is
  supported" even for configs that already set `false` (`cache_time`,
  `cluster_min_healthy_percentages`); its doc also files the `false` + `cache_time` cell under
  "REJECT-direction divergence" though `SPEC.md` §2.1 measured upstream rejecting it too.
- **N-9** Fixture `0095`'s README and `expectations.yaml` header comment say the stamped header
  "must be the bootstrap `node.cluster`"; the driver compares the two proxies' values with each
  other, never with the literal `hc-fixture-cluster`, and compares only the first occurrence of a
  name. True via upstream's measured behaviour, not via an assertion.
- **N-10** The contract section says the behaviour is "differentially proven by 0095/0096", but the
  chain-order bullet (`[health_check, fault]` vs `[fault, health_check]`) has NO in-tree witness of
  any kind, and the access-log values and the `node`-absent cell are in-process only. Fold into I-3's
  correction.
- **N-11** `downstream_rq_completed` (upstream: intercept excluded) and `downstream_rq_http1_total`
  (upstream: intercept counted) are ABSENT from envoy-rust altogether; the contract section mentions
  the first only as an upstream fact. Pre-existing stat-surface gap; record it where a reader looks.
- **N-12** `PROGRESS.md`'s "2 new differential runners and 30 new test functions" reads as 32; the
  30 already includes the 2 runners (28 + 2). The 2345 arithmetic is correct.

---

## §5 — Reviewer findings adjudicated

- **Charged as MUST-FIX after re-verification (2):** the probe reviewer's H2 `content-length` and
  H1 HEAD framing findings, both reproduced here against both proxies WITH `direct_response`
  controls that show parity on the non-filter path (§2 I-1, I-2). The probe reviewer had rated them
  Important; they are raised to MUST-FIX because they are header-set divergences on the phase's own
  reply, reachable by in-scope configs, which §7.2 requires to be set-equal.
- **Charged as Important after re-verification:** the chunked-body drop (re-run here on envoy-rust
  including the pre-existing 501 control, F-1); the LDS, `:path`-case and absent-node test gaps
  (re-checked by reading the tests and by `git grep`, F-2 … F-4); the position-bound counters
  (F-5, by reading the test's only assertion).
- **NOT CHARGED (1):** the encode-all-filters concern (F-7) — refuted for this filter by the
  probe's two-proxy measurement.
- **Banked as UNVERIFIED leads (7):** F-8 (a)–(g).
- **Absolute-form target (F-6):** the filter reviewer's code trace and the probe's envoy-rust run
  agree on envoy-rust's behaviour; upstream's rewrite is the probe's measurement, not this session's.

---

## §6 — Deliberate decisions verified, not re-litigated

- The one-entry `:path` view instead of injecting pseudo-headers at the codecs (`ADR-0201`
  DECISION 2).
- Stamping `node.cluster` in `validate_hcm` instead of threading the bootstrap node (DECISION 3).
- The stat surface and the Nxx exclusion keyed on `health_check_ok` (DECISION 4).
- The Task-3 clippy deferral with no suppression (DECISION 5); the landed file carries 0 `allow(`.
- The REJECT-direction divergences — `pass_through_mode: true`, `cache_time`,
  `cluster_min_healthy_percentages`, non-`:path` pseudo-headers — each fail-loud and banked
  (`CF-115-1`/`-5`/`-6`).
- The nine local differential reds of the state-4 gate — adjudicated there by an interleaved
  pre-phase control; not re-litigated.

---

## §7 — Carry-forwards

| id | status | what |
|---|---|---|
| `CF-115-1` … `CF-115-3`, `CF-115-5` … `CF-115-10` | unchanged | as recorded in `PLAN.md` "Carry-forwards this phase opens or amends" |
| `CF-115-4` | **AMENDED** | the missing H2 differential witness now hides a MEASURED divergence (§2 I-1), which the state-3 re-entry fixes in-process; the differential witness itself stays banked |
| `CF-115-11` | **NEW** | chunked-body local replies advertise keep-alive and drop the connection (§3 F-1) |
| `CF-115-12` | **NEW** | H1 absolute-form request target is never normalised to a path (§3 F-6) |
| `CF-115-13` | **NEW (leads, unverified)** | the seven pre-existing divergences of §3 F-8 |
| F-2 … F-5, N-1 … N-12 | banked | the state-3 re-entry MAY take any of them as riders in files it already touches; none is required |

`CF-114-6` stays CONSUMED; `CF-75-5` and the phase-112 ALPN rider stand and are not re-costed.

---

## §8 — Assessment

**Ready to close? NO — re-enter §5.2 STATE 3.** The filter logic, the details seam, the stamping,
the stat exclusion and both fixtures are correct and well tested; the defect is in the FRAMING of
the reply the filter produces, where the landed code reuses a decoration that always writes
`content-length` and upstream's health-check reply is headers-only. It diverges on every H2
intercept and on every H1 HEAD intercept, and the contract section records the divergent cells as
measured fact.

### The §7.5 gate

| leg | status at this review |
|---|---|
| (a) new fixtures green | PASS on the reviewed tree (state 4; re-confirmed by the fixture reviewer) — must be re-run after the fix |
| (b) pre-existing fixtures green | PASS on the reviewed tree (state 4, CI `35581009301` `failed=0`) — must be re-run |
| (c) conformance | PASS, CI-authoritative — must be re-run |
| (d) fuzz | PASS (no new target) — must be re-run |
| (e) build / clippy / fmt / test / deny | PASS on the reviewed tree — must be re-run |
| (f) `REVIEW.md` approved | **NOT APPROVED** |

### Stop condition — re-derived from disk at this review; ALL THREE LEGS FALSE

- **(i)** `ROADMAP.md`: **123** rows, **122** `done`, **1** `planned`; not-done set exactly `{115}` at
  file line 78 (status = field 4 of a `' | '` split; field-count histogram `{6: 121, 7: 1, 10: 1}`).
- **(ii)** **14** crates; `envoy-{http3,grpc,wasm,protos,runtime,xds}` absent by `test -d`;
  `quinn`/`wasmtime`/`tonic`/`opentelemetry`/`prost` = **0** of the **28** manifests from
  `git ls-files '*Cargo.toml'` against `tokio` = **19/28**; `histogram` = **0** over `crates/`
  against `gauge` = **365** (`grep -rIo`, `crates/`) / **352** (`crates/*/src/`).
- **(iii)** **11** `### ` family headings reading 11/5/3/14/3/4/6/31/6/**0**/13 with **27**
  pre-heading rows (sum 123); `### WASM host family` has zero rows.

No `stop` file exists and none was created.

### Next state

**§5.2 STATE 3 — re-implementation of §2's required disposition**, in a SEPARATE session
(§5.1; `ADR-0127`), TDD per fix, appending to `PROGRESS.md`. Then state 4 (full gate) and a fresh
state-5 re-review writing `REVIEW-2.md`. `ROADMAP.md` row `115` stays `planned` throughout.
