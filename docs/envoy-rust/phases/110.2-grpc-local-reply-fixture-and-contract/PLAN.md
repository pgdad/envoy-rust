# Sub-phase 110.2 — the differential witness: fixture `0089` + the `BEHAVIOR_CONTRACT.md` `## gRPC` section — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the cross-proxy differential witness for the gRPC-aware local-reply transform that sibling `110.1` built and proved only in-process — a NEW cluster-free, backend-free fixture `0089-grpc-aware-local-replies` driving **32 HTTP/1.1 probes** through the EXISTING `http1_probe_list` driver — and record the measured contract as a new `## gRPC` section in `BEHAVIOR_CONTRACT.md`.

**Architecture:** No crate source changes. One HCM listener, `clusters: []`, 24 `direct_response`/`redirect` routes each at its own distinct path, and a probe list that sends `content-type: application/grpc` (and its negatives) at them. The harness compares status, byte-exact body, and header **name-set equality plus value-exact equality** for every name outside the 3-entry `HEADER_ALLOW_LIST` — so `grpc-status`, `grpc-message`, `content-type`, `content-length` and `location` are all pinned value-exact. `envoy.yaml` and `envoy-rust.yaml` are BYTE-IDENTICAL.

**Tech Stack:** Rust 2024 workspace; `tests/differential` harness crate (`testcontainers` + raw-socket HTTP/1.1); upstream reference `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`; YAML fixtures.

**Spec:** `docs/envoy-rust/phases/110.2-grpc-local-reply-fixture-and-contract/SPEC.md` (LANDED and UNEDITABLE). The landed foundation it witnesses is `docs/envoy-rust/phases/110.1-grpc-local-reply-transform/` (`SPEC.md` + `PLAN.md` + `PROGRESS.md` + `REVIEW.md`, all LANDED and UNEDITABLE).

---

## §0 — What this state-2 session MEASURED, and what it changes

Every figure below was re-derived FRESH at this PLAN-write. **Nothing here is inherited.** Three results change the plan relative to what `110.2/SPEC.md` §3 anticipated, and all three came out of the X-2 end-to-end dry-run — which is exactly why the SPEC mandates it.

### §0.1 — the X-item ledger (SPEC §6), all eight CLOSED

| item | verdict | evidence |
|---|---|---|
| **X-1** re-run §1.1 + §1.2 against the pinned image | **CONFIRMED, every cell** | digest verified by `docker image inspect` BEFORE probing: `envoyproxy/envoy@sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2` == `ENVOY_TARGET.md`. See §0.2. |
| **X-2** dry-run the EXACT `0089` YAML against BOTH proxies | **DONE — 34 probes run, 32 GREEN, 2 RED; the 2 REDs diagnosed and REMOVED; the frozen 32-probe set is 32/32 GREEN** | §0.3 |
| **X-3** byte-identical `envoy.yaml` ≡ `envoy-rust.yaml` | **CONFIRMED** — md5 `4a58299d1084c85d37e5a0b77ba1dbc2`, **5144 bytes** each. Non-empty, so the uniform hash is real byte-identity, not the empty-file md5 `d41d8cd98f00b204e9800998ecf8427e`. | one template renders both sides |
| **X-4** fixture census, no `0089` | **CONFIRMED** — `git ls-files 'tests/fixtures/**' \| cut -d/ -f3 \| sort -u \| wc -l` = **88**; highest `0088-runtime-fraction-route-gating`; `ls -d tests/fixtures/0089*` → `No such file or directory`; **88** differential test files. |
| **X-5** the §2 harness facts on disk | **CONFIRMED, all four** | §0.4 |
| **X-6** CF-110-3 still holds | **CONFIRMED and WIDENED** — `location` on `201`, `301` **AND** `302`, in BOTH directions. `204` gets none. No chosen cell trips it. | §0.5 |
| **X-7** audit a suspiciously fast green | **DONE** — `docker ps --format '{{.ID}} {{.Image}} {{.Names}}'` (a VALID field set) showed the reference containers genuinely running; negative control on an unbound port returned `ConnectionRefusedError [Errno 111]`. |
| **X-8** rebuild the DEBUG `envoy-bin` first | **DONE** — `cargo build -p envoy-bin` exit 0 before any probe; the dry-run used `target/debug/envoy-bin`. |

### §0.2 — X-1: the measured matrices, re-confirmed cell by cell

Probe harness: the exact `0089` config below, one `direct_response` per status at its own path, raw-socket HTTP/1.1 client, one connection per probe, `Connection: close`.

**§1.1 mapping — CONFIRMED, all eleven probed cells.** `200`→**2**, `400`→**13**, `401`→**16**, `403`→**7**, `404`→**12**, `405`→**2**, `429`→**14**, `500`→**2**, `502`→**14**, `503`→**14**, `504`→**14**. The two counter-intuitive default-arm witnesses (`500`, `405`) both returned `grpc-status: 2`.

**§1.2 detection — CONFIRMED, all eight probed cells.** Positive: `application/grpc`, `application/grpc+proto`, `application/grpc+` (bare). Negative: `application/grpc; charset=utf-8`, `APPLICATION/GRPC`, `application/grpc-web`, `application/grpcfoo`, header absent. Method-insensitivity confirmed with a `post` probe (`403`→`grpc-status: 7`).

**§1.3 encoding — CONFIRMED, including both boundary cells the parent SPEC got wrong.**
`a b\ncontrol\ttab é %25 end` → `a b%0Acontrol%09tab %C3%A9 %2525 end`, and `q"b s\l t~t dd` → `q"b s\l t%7Et dd`. **`~` (0x7E) IS escaped**; `"` and `\` pass through; `%25` renders `%2525`.

**§1.4 wire shape and ORDER — CONFIRMED byte for byte.** Measured order on every transformed reply:

```
[location,] content-type, grpc-status, [grpc-message,] date, server, connection, content-length
```

`grpc-message` is **ABSENT ENTIRELY** (not empty) when the original body was empty — confirmed on both the `inline_string: ""` route and the HCM's own unmatched-path 404. `location` SURVIVES the transform and stays FIRST.

### §0.3 — X-2: what the dry-run found, and the two probes it killed

The dry-run ran **34** probes against both proxies and compared them under the harness's exact semantics (status exact; body byte-exact; header name-set equality lower-cased; value-exact for every name outside `{server, date, x-envoy-upstream-service-time}`). **32 green, 2 red.**

Both REDs were **non-gRPC CONTROL probes on an EMPTY-body local reply**, and both were the same divergence:

```
e-empty-ctl  NAMESET  only-in-envoy-rust=['content-type']
nomatch-ctl  NAMESET  only-in-envoy-rust=['content-type']

envoy : HTTP/1.1 404 | date, server, connection, content-length
rust  : HTTP/1.1 404 | server, date, content-length, content-type, connection
```

This is **PRE-EXISTING and ORTHOGONAL to gRPC**, and it is banked here as **CF-110-6** (§0.6). `BEHAVIOR_CONTRACT.md:1131-1137` (ADR-0059) already records the upstream rule — *"Upstream Envoy v1.33 does NOT emit `content-type` on an empty-body local reply"* — but the decorator that implements it (`decorate_filter_synth_response`) covers only FILTER local replies; `synth_with` emits `content-type` unconditionally. **No existing fixture uses an empty-body `direct_response`** (`grep -rn 'inline_string: ""' tests/fixtures/` = **0** across the 40 `direct_response` fixtures), which is exactly why it has never been caught.

**Both cells are GREEN in the gRPC direction** — there both proxies emit `content-type: application/grpc` — so SPEC §3 F3's empty-body requirement is met in full. **Only the non-gRPC twins are removed.**

> **BINDING ON THE IMPLEMENTATION: fixture `0089` must carry NO empty-body probe in the non-gRPC control direction.** Same class as CF-110-3: a RED for a reason unrelated to gRPC.

The dry-run also found a **second, independent divergence before the config would even boot**: `direct_response: { status: 404 }` with no `body:` is accepted by upstream and **rejected by envoy-rust**:

```
parsing bootstrap YAML: static_resources.listeners[0].filter_chains[0].filters[0]:
missing field `body` at line 8 column 15
```

`crates/envoy-config/src/bootstrap.rs:2923-2926` declares `pub struct DirectResponse { pub status: u16, pub body: DataSource }` — `body` is MANDATORY, with no `#[serde(default)]` and no `Option`. Banked as **CF-110-7**.

> **BINDING ON THE IMPLEMENTATION: every `direct_response` in `0089` must carry an explicit `body:`.** The empty-body cell is spelled `body: { inline_string: "" }`.

### §0.4 — X-5: the four harness facts, verified on disk

| fact | verdict |
|---|---|
| `HEADER_ALLOW_LIST` is **3 entries** and `location` is ABSENT | **CONFIRMED** — `tests/differential/src/lib.rs:1189-1193`: `server`, `date`, `x-envoy-upstream-service-time`, all `AllowMode::NameRequired`. `location` count over the constant = **0**. **NEVER add it.** |
| `Http1BodyRule::ByteExact { body: String }` is the ONLY variant | **CONFIRMED** — `lib.rs:1064-1079`. |
| `Http1Method` has **three** variants | **CONFIRMED** — `lib.rs:1038-1050`: `Get`, `Options`, `Post`. **There is no `put` and no `delete`.** |
| `drive_http1` interpolates `extra_headers` **RAW** | **CONFIRMED** — `lib.rs:2212-2214`: `req.push_str(&format!("{n}: {v}\r\n"))`. No lower-casing, no `HeaderName` normalisation, no validation. **This is what makes the `APPLICATION/GRPC` case-sensitivity cell a real witness.** It also emits `Host:` and `Connection: close` itself and auto-adds `Content-Length` when `body:` is set — so `host`, `connection` and `content-length` must NEVER appear in `extra_headers`. |

Two further driver facts measured this session and load-bearing on the design:

- **The driver NEVER compares the HTTP reason phrase.** `drive_http1` reads `resp.code` only and never touches `httparse`'s `resp.reason`; `DriveHttp1Result.status` is a bare `u16`. A `200 OK` and a `200 Totally Fine` are equivalent to this harness. **This is why REVIEW finding M-1 is NOT scheduled into `110.2` — see §0.7.**
- **`diff_headers` compares a SET, not a multiset** (`lib.rs:1211-1215`), and on a duplicated name only the FIRST occurrence's value is compared (`lib.rs:1234`/`:1239`). A duplicated `grpc-status` would be invisible. **Header ORDER is never read.**

### §0.5 — X-6: CF-110-3 re-confirmed and WIDENED

Measured this session on a throwaway config:

| `direct_response` status | control direction | gRPC direction |
|---|---|---|
| `201` | `201` + **`location: http://envoy-rust.test/cf-201`** | `200` + **`location`** + `grpc-status: 2` |
| `301` | `301` + **`location`** | `200` + **`location`** + `grpc-status: 2` |
| `302` | `302` + **`location`** | `200` + **`location`** + `grpc-status: 2` |
| `204` | `204`, **no `location`** | `200`, **no `location`**, `grpc-status: 2` |

envoy-rust's `synth_direct_response` emits no `location` on any of them, so **any `201`/`3xx` `direct_response` cell REDs the name-set check for a reason unrelated to gRPC.** CF-110-3 stands, now with `302` and the `204` non-case added. **`0089` uses NO `201` and NO `3xx` `direct_response` status.** The `redirect:` route is safe and is included — `synth_redirect` already emits `location`, and the dry-run confirms both proxies agree on it value-exact in both directions.

### §0.6 — REVIEW finding M-2: MEASURED, and it is a real DIVERGENCE

`110.1/REVIEW.md` M-2 found that the idempotence sentinel's soundness argument surveys the wrong file set, and left the *direction* of the divergence UNMEASURED. **This session measured it on both proxies.** Config: a chain-level `envoy.filters.http.header_mutation` whose `mutations.response_mutations` adds `grpc-status: 99`, in front of a `direct_response` 404 route.

| | control direction | **gRPC direction** |
|---|---|---|
| **upstream** | `404`, body `MUTGS`, `grpc-status: 99` | **`200`**, `content-type: application/grpc`, **body DROPPED**, `content-length: 0`, `grpc-message: MUTGS`, **`grpc-status: 99`** |
| **envoy-rust** | `404`, body `MUTGS`, `grpc-status: 99` | **`404`**, body `MUTGS`, `content-type: text/plain`, `grpc-status: 99` — **the transform is SUPPRESSED ENTIRELY** |

**Upstream still transforms; it simply lets the operator's `grpc-status` value win instead of overwriting it with the mapped `12`.** envoy-rust's sentinel (`crates/envoy-http1/src/grpc.rs:158-160`) returns early and emits no transform at all. Banked as **CF-110-8**.

> **BINDING ON THE IMPLEMENTATION: `0089` must carry NO `header_mutation`-injected `grpc-status` cell.** It would RED — this is a genuine divergence, not a fixture-shape accident. The route-scoped form is unreachable anyway: envoy-rust's `typed_per_filter_config` accepts only `CorsPolicy`, `CsrfPolicy` and `BufferPerRoute`, so `HeaderMutationPerRoute` is a hard parse error.

This satisfies `110.1/REVIEW.md` §9 item 2 in its **stronger** form: the `## gRPC` contract section states the cell as **MEASURED**, not as unmeasured.

### §0.7 — Decisions this PLAN makes, and what it deliberately does NOT do

**DECIDED-OUT — `0089` carries NO access-log arm**, and the reason is structural rather than a matter of taste. `110.1/REVIEW.md` §9 item 3 names `110.2` as the natural home for a `%RESPONSE_CODE%`/`%BYTES_SENT%` witness of the seam-PLACEMENT finding. **The two witnesses cannot share a fixture**, measured on disk: a fixture has exactly ONE `driver`, `Driver::Http1ProbeList` never reads an access log, and the byte-exact access-log driver's probe struct `AccessLogByteExactProbe` (`tests/differential/src/lib.rs:1116-1132`) carries `expected_status` but **no `expected_headers` and no `expected_body`** — so it cannot pin `grpc-status`, which is the whole point of `0089`. An access-log witness therefore requires a SECOND fixture with a different driver, which is outside `110.2/SPEC.md` §3's F1–F6 and outside its §7 size estimate. **Banked as CF-110-9.** Placement remains witnessed by `110.1`'s two in-process observability tests and its state-3 placement mutation.

**DECIDED-OUT — no banked finding from `110.1/REVIEW.md` is scheduled into `110.2`.** Per §6.3 and ADR-0165 a phase banks, it does not clear. Two additional measured reasons: (a) **M-1 is not witnessable by this deliverable at all** — its cell is the HTTP reason phrase, and §0.4 measures that this driver never compares it, so "fixing" M-1 here would be unwitnessed by the phase's own fixture; (b) M-3/M-4/M-5/M-6 are all in-process test edits inside `crates/envoy-http1/`, and `110.2/SPEC.md` §5 states plainly that **no crate source change is planned here**. M-1…M-9 and N-1…N-10 all stay OPEN and are carried forward untouched.

**DECIDED-IN — the empty-body cell is spelled `body: { inline_string: "" }` and is probed in the gRPC direction only**, plus a second, free empty-body witness at the HCM's own unmatched-path 404 (no route matches `/no-such-route`, since `0089` has no catch-all). That second cell costs nothing and drives a local-reply site — `synth_404` via `build_response_in` — that `110.1`'s test list never drove, which narrows M-3 by one site as a side effect rather than as scheduled work.

### §0.8 — the §6.1 split gate: **DOES NOT FIRE**

Bottom-up, docs-excluded, `added − deleted` — the metric every landed calibration phase was measured under. `BEHAVIOR_CONTRACT.md` lives under `docs/` and is EXCLUDED from the gate metric.

| bucket | measured basis | estimate |
|---|---|---|
| `envoy.yaml` + `envoy-rust.yaml` | the dry-run template is **85 lines**; +~10 for the house header comment; ×2 byte-identical files | **≈ 190** |
| `expectations.yaml` | 32 probes × ~6 lines + a ~35-line header comment block (`0088`'s is ~30; `0086` = 140 lines for 19 probes) | **≈ 230** |
| `README.md` | `0088` = 111, `0086` = 113; this one carries three binding constraints + the M-2 measurement + two mutation proofs | **≈ 150** |
| `tests/differential/tests/grpc_aware_local_replies.rs` | `0088`'s = 40, `0086`'s = 47 | **≈ 45** |
| **Total** | | **≈ 615 (range 550–700)** |

**Task count: 7.** Threshold ~25. **Does not fire.**

**LoC: ≈615.** Threshold ~1500. **Does not fire**, and it survives the worst overrun in the record. Calibration re-measured from git this session across eight landed phases (net, docs-excluded, `added − deleted`):

| phase | PLAN estimate | actual net | ratio |
|---|---:|---:|---:|
| `110.1` | ≈912 | **1290** | 1.41 |
| `109.2` | ≈745 | 562 | 0.75 |
| `109.1` | ≈1180 | 1726 | 1.46 |
| `108.2` | ≈905 | 854 | 0.94 |
| `108.1` | ≈1215 | 1128 | 0.93 |
| `76.2` | ≈1312 | 1568 | 1.20 |
| `76.1` | ≈515 | 774 | 1.50 |
| `75.2` | ~760 | 897 | 1.18 |

Median ratio **1.19**, worst observed **1.50**. At the worst observed overrun this sub-phase lands at **≈923** — still 38% under the gate.

> **`110.1`'s calibration figure is 1290, NOT the 1165 `PROGRESS.md` reports** (REVIEW M-7). Independently re-derived from git this session: `git diff --numstat c54bf83 29d25e5 -- . ':(exclude)docs/'` returns **seven** files summing `added=1305 deleted=15` → **net 1290**; the 1165 figure is `added − deleted` over `crates/envoy-http1/` **alone**, omitting `crates/envoy-http2/src/hcm.rs` (+125) and `Cargo.lock` (6/6, net 0). The table above uses 1290.

---

## Global Constraints

Every task's requirements implicitly include this section. Values are copied verbatim from the SPEC or measured on disk this session.

1. **NO crate source change.** `110.2/SPEC.md` §5: *"no crate source change is planned here."* If a task finds itself editing anything under `crates/`, the scope is wrong — stop and record it as a finding. The three divergences in §0.3/§0.6 are **banked, not fixed**.
2. **NO `201` and NO `3xx` `direct_response` status anywhere in `0089`** (CF-110-3, X-6). A `redirect:` route is fine and is included.
3. **NO empty-body probe in the non-gRPC CONTROL direction** (CF-110-6, §0.3).
4. **Every `direct_response` carries an explicit `body:`** (CF-110-7, §0.3). Empty is `body: { inline_string: "" }`.
5. **NO `header_mutation`-injected `grpc-status` cell** (CF-110-8, §0.6).
6. **NEVER add `location` to `HEADER_ALLOW_LIST`.** It is 3 entries — `server`, `date`, `x-envoy-upstream-service-time` (`tests/differential/src/lib.rs:1189-1193`). Adding `location` would silently vacate every `location` assertion in the corpus while leaving fixtures green.
7. **No `node:` block, and no unquoted `y`/`n`/`on`/`off` scalar anywhere.** Upstream parses YAML 1.1 and booleanizes them; `serde_yaml` parses YAML 1.2 and does not. Omitting `node:` is what lets ONE file serve both proxies.
8. **`admin.socket_address.port_value` is a LITERAL `0`, never `{{ADMIN_PORT}}`.** That substitution is driver-gated to `AdminScrape` / `Http1KeepAlive` / `Http2KeepAlive` / `TcpWithStats` (`tests/differential/src/lib.rs:3066-3073`); `Http1ProbeList` is not among them, and `render_yaml` leaves an unmatched token UNTOUCHED, so a literal `{{ADMIN_PORT}}` would reach the parser and fail as an address. **`{{PORT}}` IS substituted for this driver** (`lib.rs:3011`).
9. **`envoy.yaml` and `envoy-rust.yaml` must stay BYTE-IDENTICAL.** Assert BOTH the md5 AND the byte count at every task that touches them — a uniform md5 can be the empty-file md5 (`d41d8cd98f00b204e9800998ecf8427e`).
10. **`deny_unknown_fields` is on `Expectations`, `Driver`, `Http1Probe`, `Http1Method`, `Http1BodyRule` and `Http1HeaderRule`.** Any typo'd key is a HARD PARSE ERROR, not a silent ignore.
11. **Never put `host`, `connection` or `content-length` in `extra_headers`** — `drive_http1` emits all three itself.
12. **The probe-list driver ABORTS AT THE FIRST FAILING PROBE.** One red run names exactly ONE probe; never infer a second cell's state from a single red run.
13. **Rebuild the DEBUG `envoy-bin` before every fixture run**: `cargo build -p envoy-bin`. The harness runs the debug binary; a stale one fails with `unknown field` errors that look like real divergences.
14. **Registration is cargo auto-discovery.** Dropping `tests/differential/tests/<slug>.rs` in IS the registration. **No `Cargo.toml` edit, no registry list, no macro.**

---

## File Structure

| file | disposition | responsibility |
|---|---|---|
| `tests/fixtures/0089-grpc-aware-local-replies/envoy.yaml` | **Create** | The reference config. One HCM listener on `{{PORT}}`, `clusters: []`, 24 routes, admin `port_value: 0`, no `node:`. |
| `tests/fixtures/0089-grpc-aware-local-replies/envoy-rust.yaml` | **Create** | BYTE-IDENTICAL copy of the above. |
| `tests/fixtures/0089-grpc-aware-local-replies/expectations.yaml` | **Create** | `kind: http1_probe_list` with 32 probes + `equivalence`. |
| `tests/fixtures/0089-grpc-aware-local-replies/README.md` | **Create** | What it witnesses, the three binding constraints, the mutation proofs. |
| `tests/differential/tests/grpc_aware_local_replies.rs` | **Create** | The cargo-auto-discovered entrypoint. ~45 lines, mostly a `//!` doc block. |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | **Modify: insert at line 643** | The new `## gRPC` section, between the `---` at 642 and `## Header allow-list` at 644. |
| `docs/envoy-rust/phases/110.2-.../PROGRESS.md` | **Create/append** | The running log, one entry per task. |

**Nothing else is touched.** No `crates/` file, no `Cargo.toml`, no `Cargo.lock`, no `ci.yml`, no `deny.toml`, no `ROADMAP.md` (its rows flip at state 6, not here), no landed phase artifact.

---

## The frozen probe set — 32 probes, ALL dry-run GREEN

This table is the contract between the tasks. Every row was run end-to-end against both proxies at this PLAN-write. `ct` is the `content-type` request header; `—` means the header is absent.

| # | probe name | task | method | path | `ct` | expected status | expected body | witnesses |
|---:|---|:--:|---|---|---|---:|---|---|
| 1 | `g-200-maps-to-unknown` | 1 | get | `/m-200` | `application/grpc` | 200 | `""` | 2xx → `grpc-status: 2` |
| 2 | `g-400-maps-to-13` | 1 | get | `/m-400` | `application/grpc` | 200 | `""` | `400` → **13** |
| 3 | `g-401-maps-to-16` | 1 | get | `/m-401` | `application/grpc` | 200 | `""` | `401` → **16** |
| 4 | `g-403-maps-to-7` | 1 | get | `/m-403` | `application/grpc` | 200 | `""` | `403` → **7** |
| 5 | `g-404-maps-to-12` | 1 | get | `/m-404` | `application/grpc` | 200 | `""` | `404` → **12** |
| 6 | `g-405-falls-to-unknown` | 1 | get | `/m-405` | `application/grpc` | 200 | `""` | counter-intuitive default arm |
| 7 | `g-429-maps-to-14` | 1 | get | `/m-429` | `application/grpc` | 200 | `""` | `429` → **14** |
| 8 | `g-500-falls-to-unknown` | 1 | get | `/m-500` | `application/grpc` | 200 | `""` | counter-intuitive default arm |
| 9 | `g-502-maps-to-14` | 1 | get | `/m-502` | `application/grpc` | 200 | `""` | `502` → **14** |
| 10 | `g-503-maps-to-14` | 1 | get | `/m-503` | `application/grpc` | 200 | `""` | `503` → **14** |
| 11 | `g-504-maps-to-14` | 1 | get | `/m-504` | `application/grpc` | 200 | `""` | `504` → **14** |
| 12 | `c-200-untransformed` | 1 | get | `/m-200` | — | 200 | `"B200"` | control: status + body survive |
| 13 | `c-400-untransformed` | 1 | get | `/m-400` | — | 400 | `"B400"` | control |
| 14 | `c-404-untransformed` | 1 | get | `/m-404` | — | 404 | `"B404"` | control |
| 15 | `c-503-untransformed` | 1 | get | `/m-503` | — | 503 | `"B503"` | control |
| 16 | `d-exact-positive` | 2 | get | `/d-exact` | `application/grpc` | 200 | `""` | exact match detects |
| 17 | `d-plus-proto-positive` | 2 | get | `/d-plus-proto` | `application/grpc+proto` | 200 | `""` | `+` suffix detects |
| 18 | `d-plus-bare-positive` | 2 | get | `/d-plus-bare` | `application/grpc+` | 200 | `""` | bare `+` detects |
| 19 | `d-param-negative` | 2 | get | `/d-param` | `application/grpc; charset=utf-8` | 404 | `"DPARAM"` | a parameter DEFEATS it |
| 20 | `d-upper-negative` | 2 | get | `/d-upper` | `APPLICATION/GRPC` | 404 | `"DUPPER"` | **CASE-SENSITIVE on the value** |
| 21 | `d-web-negative` | 2 | get | `/d-web` | `application/grpc-web` | 404 | `"DWEB"` | not a bare prefix match |
| 22 | `d-foo-negative` | 2 | get | `/d-foo` | `application/grpcfoo` | 404 | `"DFOO"` | not a bare prefix match |
| 23 | `d-absent-negative` | 2 | get | `/d-absent` | — | 404 | `"DABSENT"` | header absent |
| 24 | `x-post-method-insensitive` | 3 | **post** | `/x-post` | `application/grpc` | 200 | `""` | detection is METHOD-INSENSITIVE |
| 25 | `e-empty-no-grpc-message` | 3 | get | `/e-empty` | `application/grpc` | 200 | `""` | `grpc-message` **ABSENT ENTIRELY** |
| 26 | `nomatch-404-no-grpc-message` | 3 | get | `/no-such-route` | `application/grpc` | 200 | `""` | the HCM's OWN route-not-found 404 |
| 27 | `enc-main-percent-encoded` | 4 | get | `/enc-main` | `application/grpc` | 200 | `""` | `%0A` `%09` `%C3%A9` `%2525` |
| 28 | `enc-main-control` | 4 | get | `/enc-main` | — | 400 | `"a b\ncontrol\ttab é %25 end"` | the untransformed original |
| 29 | `enc-edge-tilde-escaped` | 4 | get | `/enc-edge` | `application/grpc` | 200 | `""` | **`~` → `%7E`**; `"` and `\` pass |
| 30 | `enc-edge-control` | 4 | get | `/enc-edge` | — | 400 | `"q\"b s\\l t~t dd"` | the untransformed original |
| 31 | `r-redirect-grpc-keeps-location` | 5 | get | `/r-redir` | `application/grpc` | 200 | `""` | `location` SURVIVES + `grpc-status: 2` |
| 32 | `r-redirect-control` | 5 | get | `/r-redir` | — | 301 | `""` | control: `301` + `location` |

Every probe carries `host: "envoy-rust.test"` and `expected_headers: set_equal_modulo_allow_list`.

**What actually pins the mapping.** `expected_status` and `expected_body` are absolute (asserted against BOTH sides); `expected_headers` is CROSS-PROXY. Because `grpc-status` and `grpc-message` are not on the allow-list, `diff_headers` compares their VALUES byte-exact between the two proxies — **that comparison is the entire mapping and encoding witness.** A wrong code, a wrong encoding, a missing header or a spurious one each go RED.

---

## Task 1: the fixture skeleton and the §1.1 mapping cells

**Files:**
- Create: `tests/fixtures/0089-grpc-aware-local-replies/envoy.yaml`
- Create: `tests/fixtures/0089-grpc-aware-local-replies/envoy-rust.yaml`
- Create: `tests/fixtures/0089-grpc-aware-local-replies/expectations.yaml`
- Create: `tests/differential/tests/grpc_aware_local_replies.rs`

**Interfaces:**
- Consumes: nothing — this is the first task.
- Produces: the two config files (byte-identical, all 24 routes present from the start so later tasks add probes only, never routes); `expectations.yaml` with probes 1–15; the entrypoint `async fn grpc_aware_local_replies()` calling `differential::run_fixture(&dir)`.

- [ ] **Step 1: Write the config — both sides, byte-identical**

Write this to `tests/fixtures/0089-grpc-aware-local-replies/envoy.yaml`. **All 24 routes land now**, so no later task edits this file.

```yaml
# Sub-phase 110.2: the differential witness for gRPC-aware LOCAL REPLIES over
# HTTP/1.1 (the transform sibling 110.1 landed and proved only in-process).
# Backend-free and CLUSTER-FREE (`clusters: []`) — every response is a LOCAL
# reply, which is the whole surface under test. No `node:` block: upstream
# parses YAML 1.1, so an unquoted `y`/`n`/`on`/`off` scalar would booleanize
# there and not here — omitting it is what lets ONE file serve both proxies
# byte-identically. Admin `port_value` is a LITERAL 0: `{{ADMIN_PORT}}` is
# driver-gated and `Http1ProbeList` is not one of the four drivers that get it.
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address: { address: 0.0.0.0, port_value: {{PORT}} }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        # --- §1.1 mapping: one route per configured status, each
                        # --- at its OWN path with its OWN body (the §G attribution
                        # --- rule). NO 201 and NO 3xx: upstream emits `location`
                        # --- on those and envoy-rust does not (CF-110-3).
                        - match: { prefix: "/m-200" }
                          direct_response: { status: 200, body: { inline_string: "B200" } }
                        - match: { prefix: "/m-400" }
                          direct_response: { status: 400, body: { inline_string: "B400" } }
                        - match: { prefix: "/m-401" }
                          direct_response: { status: 401, body: { inline_string: "B401" } }
                        - match: { prefix: "/m-403" }
                          direct_response: { status: 403, body: { inline_string: "B403" } }
                        - match: { prefix: "/m-404" }
                          direct_response: { status: 404, body: { inline_string: "B404" } }
                        - match: { prefix: "/m-405" }
                          direct_response: { status: 405, body: { inline_string: "B405" } }
                        - match: { prefix: "/m-429" }
                          direct_response: { status: 429, body: { inline_string: "B429" } }
                        - match: { prefix: "/m-500" }
                          direct_response: { status: 500, body: { inline_string: "B500" } }
                        - match: { prefix: "/m-502" }
                          direct_response: { status: 502, body: { inline_string: "B502" } }
                        - match: { prefix: "/m-503" }
                          direct_response: { status: 503, body: { inline_string: "B503" } }
                        - match: { prefix: "/m-504" }
                          direct_response: { status: 504, body: { inline_string: "B504" } }
                        # --- §1.2 detection: all land on 404 (grpc-status 12) so a
                        # --- transform is discriminable from a non-transform by
                        # --- BOTH the status and the body.
                        - match: { prefix: "/d-exact" }
                          direct_response: { status: 404, body: { inline_string: "DEXACT" } }
                        - match: { prefix: "/d-plus-proto" }
                          direct_response: { status: 404, body: { inline_string: "DPROTO" } }
                        - match: { prefix: "/d-plus-bare" }
                          direct_response: { status: 404, body: { inline_string: "DBARE" } }
                        - match: { prefix: "/d-param" }
                          direct_response: { status: 404, body: { inline_string: "DPARAM" } }
                        - match: { prefix: "/d-upper" }
                          direct_response: { status: 404, body: { inline_string: "DUPPER" } }
                        - match: { prefix: "/d-web" }
                          direct_response: { status: 404, body: { inline_string: "DWEB" } }
                        - match: { prefix: "/d-foo" }
                          direct_response: { status: 404, body: { inline_string: "DFOO" } }
                        - match: { prefix: "/d-absent" }
                          direct_response: { status: 404, body: { inline_string: "DABSENT" } }
                        # --- detection is METHOD-INSENSITIVE
                        - match: { prefix: "/x-post" }
                          direct_response: { status: 403, body: { inline_string: "XPOST" } }
                        # --- empty body -> `grpc-message` ABSENT ENTIRELY. `body:`
                        # --- is MANDATORY in envoy-rust (CF-110-7), so the empty
                        # --- body is spelled explicitly.
                        - match: { prefix: "/e-empty" }
                          direct_response: { status: 404, body: { inline_string: "" } }
                        # --- §1.3 percent-encoding
                        - match: { prefix: "/enc-main" }
                          direct_response:
                            status: 400
                            body: { inline_string: "a b\ncontrol\ttab é %25 end" }
                        - match: { prefix: "/enc-edge" }
                          direct_response:
                            status: 400
                            body: { inline_string: "q\"b s\\l t~t dd" }
                        # --- `location` must SURVIVE the transform. A `redirect:`
                        # --- route is safe where a 3xx `direct_response` is not:
                        # --- `synth_redirect` already emits `location`.
                        - match: { prefix: "/r-redir" }
                          redirect: { host_redirect: "h", path_redirect: "/x" }
                        # --- NO catch-all: `/no-such-route` must reach the HCM's
                        # --- OWN route-not-found 404, a second empty-body cell.
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
admin:
  address:
    socket_address: { address: 0.0.0.0, port_value: 0 }
```

Then copy it byte-for-byte:

```bash
cd tests/fixtures/0089-grpc-aware-local-replies
cp envoy.yaml envoy-rust.yaml
md5sum envoy.yaml envoy-rust.yaml && wc -c envoy.yaml envoy-rust.yaml
```

Expected: two identical md5s AND two identical non-zero byte counts. **Assert both** — a uniform md5 can be the empty-file md5 `d41d8cd98f00b204e9800998ecf8427e`.

- [ ] **Step 2: Write `expectations.yaml` with probes 1–15**

```yaml
# Sub-phase 110.2: the FIRST differential witness of gRPC-aware local replies
# in the corpus, and the first fixture in the tree that sends
# `content-type: application/grpc` at all.
#
# Sibling 110.1 landed the transform: a request whose `content-type` is
# EXACTLY `application/grpc` or begins with `application/grpc+` turns any
# LOCALLY GENERATED reply into HTTP 200 + `content-type: application/grpc`
# + `content-length: 0`, body DROPPED, with a `grpc-status` header and — only
# when the original body was non-empty — a `grpc-message` header carrying that
# body percent-encoded. 110.1 proved it IN-PROCESS; this fixture proves it
# CROSS-PROXY.
#
# WHAT PINS WHAT. `expected_status` and `expected_body` are ABSOLUTE (asserted
# against BOTH proxies). `expected_headers: set_equal_modulo_allow_list` is
# CROSS-PROXY: `diff_headers` compares the lower-cased header NAME SET and then
# the VALUE of every name outside the 3-entry HEADER_ALLOW_LIST
# (`server`, `date`, `x-envoy-upstream-service-time`). `grpc-status`,
# `grpc-message`, `content-type`, `content-length` and `location` are NOT on
# that list, so all five are compared byte-exact. THAT comparison is the entire
# mapping and encoding witness.
#
# THREE CELLS ARE DELIBERATELY ABSENT, each a MEASURED divergence unrelated to
# gRPC that would RED this fixture for the wrong reason:
#   * no 201/3xx `direct_response` — upstream emits `location` on those and
#     envoy-rust does not (CF-110-3, re-measured at the 110.2 PLAN-write on
#     201/301/302; 204 gets none).
#   * no empty-body CONTROL probe — envoy-rust's `synth_with` emits
#     `content-type` on an empty-body local reply and upstream does not
#     (CF-110-6, found by the PLAN-write dry-run). Both empty-body cells are
#     probed in the gRPC direction, where the two proxies AGREE.
#   * no `header_mutation`-injected `grpc-status` cell — envoy-rust's
#     idempotence sentinel SUPPRESSES the whole transform there while upstream
#     transforms anyway and lets the operator's value win (CF-110-8, MEASURED
#     on both proxies at the 110.2 PLAN-write).
#
# The driver ABORTS AT THE FIRST FAILING PROBE, so one red run names exactly
# ONE probe; never infer a second cell's state from a single red run.
driver:
  kind: http1_probe_list
  probes:
    # ---- §1.1 the SPARSE EIGHT-ENTRY mapping over a DEFAULT of 2 -----------
    - name: g-200-maps-to-unknown
      method: get
      path: "/m-200"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: g-400-maps-to-13
      method: get
      path: "/m-400"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: g-401-maps-to-16
      method: get
      path: "/m-401"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: g-403-maps-to-7
      method: get
      path: "/m-403"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: g-404-maps-to-12
      method: get
      path: "/m-404"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    # `405` is one of the two counter-intuitive default-arm witnesses: it looks
    # like it should be special and is NOT.
    - name: g-405-falls-to-unknown
      method: get
      path: "/m-405"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: g-429-maps-to-14
      method: get
      path: "/m-429"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    # `500` is the sharpest default-arm cell — a 5xx that is NOT 14.
    - name: g-500-falls-to-unknown
      method: get
      path: "/m-500"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: g-502-maps-to-14
      method: get
      path: "/m-502"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: g-503-maps-to-14
      method: get
      path: "/m-503"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: g-504-maps-to-14
      method: get
      path: "/m-504"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    # ---- paired non-gRPC CONTROLS: the transform must NOT fire -------------
    - name: c-200-untransformed
      method: get
      path: "/m-200"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "B200" }
      expected_headers: set_equal_modulo_allow_list
    - name: c-400-untransformed
      method: get
      path: "/m-400"
      host: "envoy-rust.test"
      expected_status: 400
      expected_body: { kind: byte_exact, body: "B400" }
      expected_headers: set_equal_modulo_allow_list
    - name: c-404-untransformed
      method: get
      path: "/m-404"
      host: "envoy-rust.test"
      expected_status: 404
      expected_body: { kind: byte_exact, body: "B404" }
      expected_headers: set_equal_modulo_allow_list
    - name: c-503-untransformed
      method: get
      path: "/m-503"
      host: "envoy-rust.test"
      expected_status: 503
      expected_body: { kind: byte_exact, body: "B503" }
      expected_headers: set_equal_modulo_allow_list
equivalence:
  response_status: exact
  response_body: { kind: byte_exact }
```

- [ ] **Step 3: Write the entrypoint**

`tests/differential/tests/grpc_aware_local_replies.rs`:

```rust
//! Sub-phase 110.2 differential acceptance test: gRPC-aware LOCAL REPLIES over
//! HTTP/1.1 — the cross-proxy witness for the transform sibling 110.1 landed
//! and proved only in-process.
//!
//! 32 HTTP/1.1 probes at a backend-free, CLUSTER-FREE HCM listener
//! (`clusters: []`, `direct_response` + one `redirect:` route). A request whose
//! `content-type` is EXACTLY `application/grpc` or begins with
//! `application/grpc+` turns any LOCALLY GENERATED reply into HTTP 200 +
//! `content-type: application/grpc` + `content-length: 0`, body DROPPED, with a
//! `grpc-status` header carrying a mapped code and — only when the original
//! body was non-empty — a `grpc-message` header carrying that body
//! percent-encoded.
//!
//! The mapping is SPARSE: `400`→13, `401`→16, `403`→7, `404`→12, `429`→14,
//! `502`→14, `503`→14, `504`→14, and EVERYTHING ELSE → 2 (UNKNOWN) — including
//! the whole 2xx/3xx range and, counter-intuitively, `500` and `405`, both of
//! which this fixture probes. Detection is byte-exact and CASE-SENSITIVE on the
//! VALUE: `APPLICATION/GRPC` does not match, a `; charset=utf-8` parameter
//! DEFEATS it, and neither `application/grpc-web` nor `application/grpcfoo` is
//! a match — the two traps a naive `starts_with` falls into.
//!
//! `grpc-status`, `grpc-message`, `content-type`, `content-length` and
//! `location` are all OUTSIDE the harness's 3-entry `HEADER_ALLOW_LIST`, so
//! `diff_headers` compares every one of them VALUE-EXACT across the two
//! proxies. That comparison is this fixture's entire witness.
//!
//! `envoy.yaml` and `envoy-rust.yaml` are BYTE-IDENTICAL. Backend-free (no
//! `{{BACKEND_IP}}` marker, so no backend container spawns), therefore fully
//! verifiable on a developer host rather than CI-authoritative.

use std::path::PathBuf;

#[tokio::test]
async fn grpc_aware_local_replies() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0089-grpc-aware-local-replies");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 4: Rebuild the debug binary and run the fixture**

```bash
cargo build -p envoy-bin
cargo test -p differential --test grpc_aware_local_replies -- --nocapture 2>&1 | tee /tmp/t1.log
```

Expected: **PASS**, `test result: ok. 1 passed; 0 failed`. Do NOT pipe through `tail` — it truncates the `failures:` block; redirect to a file and read it.

> This test passes on the FIRST run: `110.1` already landed the behaviour, so the fixture is a CHARACTERIZATION PIN. **The mutation in Step 5 IS the RED evidence** — without it the run proves only that the fixture executes.

- [ ] **Step 5: Prove it is not vacuous — mutation V1 (a mapped code)**

Change `/m-403`'s configured status from `403` to `500` in **BOTH** yamls. `403` maps to `7`; `500` falls to the default `2`. Upstream will then answer `grpc-status: 2` while `expected_status`/`expected_body` still pass — the RED must come from `diff_headers`, proving the `grpc-status` VALUE is genuinely compared.

```bash
cd tests/fixtures/0089-grpc-aware-local-replies
md5sum envoy.yaml envoy-rust.yaml > /tmp/v1.md5
# Guard BEFORE mutating: the anchor must occur EXACTLY ONCE per file. If it
# occurs twice the sed would move an implementation and its own witness in
# lockstep and return a GREEN that reads as "these cells are vacuous".
grep -c 'status: 403, body: { inline_string: "B403" }' envoy.yaml       # must be 1
grep -c 'status: 403, body: { inline_string: "B403" }' envoy-rust.yaml  # must be 1
sed -i 's|direct_response: { status: 403, body: { inline_string: "B403" } }|direct_response: { status: 500, body: { inline_string: "B403" } }|' envoy.yaml envoy-rust.yaml
cd - && cargo test -p differential --test grpc_aware_local_replies 2>&1 | tee /tmp/v1.log
```

Expected: **FAIL**, naming probe `g-403-maps-to-7`. Confirm the failure names that probe and no other (the driver aborts at the first failure).

Revert and verify byte-exactly:

```bash
cd tests/fixtures/0089-grpc-aware-local-replies
sed -i 's|direct_response: { status: 500, body: { inline_string: "B403" } }|direct_response: { status: 403, body: { inline_string: "B403" } }|' envoy.yaml envoy-rust.yaml
md5sum -c /tmp/v1.md5 && md5sum envoy.yaml envoy-rust.yaml && wc -c envoy.yaml envoy-rust.yaml
cd - && cargo test -p differential --test grpc_aware_local_replies
```

Expected: `envoy.yaml: OK` **and** `envoy-rust.yaml: OK` from `md5sum -c`, both files identical again with equal non-zero byte counts, and the test GREEN.

- [ ] **Step 6: Commit**

```bash
git add tests/fixtures/0089-grpc-aware-local-replies tests/differential/tests/grpc_aware_local_replies.rs
git commit -m "phase 110.2 task 1: fixture 0089 skeleton + the 11 mapping cells and 4 controls, byte-identical yamls, mutation V1 proves the grpc-status value is compared"
```

---

## Task 2: the §1.2 detection cells

**Files:**
- Modify: `tests/fixtures/0089-grpc-aware-local-replies/expectations.yaml` (append probes 16–23 after probe 15, before `equivalence:`)

**Interfaces:**
- Consumes: Task 1's two config files (routes `/d-exact` … `/d-absent` already exist — **do not edit the yamls**) and the `expectations.yaml` probe list.
- Produces: probes 16–23. No new interface.

- [ ] **Step 1: Append the eight detection probes**

Insert immediately after `c-503-untransformed` and before the `equivalence:` key:

```yaml
    # ---- §1.2 detection: EXACT `application/grpc` or the prefix
    # ---- `application/grpc+`, and NOTHING else. Header VALUE case matters;
    # ---- `drive_http1` interpolates `extra_headers` RAW (no lower-casing),
    # ---- which is what makes the `APPLICATION/GRPC` cell a real witness.
    - name: d-exact-positive
      method: get
      path: "/d-exact"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: d-plus-proto-positive
      method: get
      path: "/d-plus-proto"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc+proto"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: d-plus-bare-positive
      method: get
      path: "/d-plus-bare"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc+"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    # A PARAMETER defeats detection — with or without the space.
    - name: d-param-negative
      method: get
      path: "/d-param"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc; charset=utf-8"]
      expected_status: 404
      expected_body: { kind: byte_exact, body: "DPARAM" }
      expected_headers: set_equal_modulo_allow_list
    # CASE-SENSITIVE on the VALUE.
    - name: d-upper-negative
      method: get
      path: "/d-upper"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "APPLICATION/GRPC"]
      expected_status: 404
      expected_body: { kind: byte_exact, body: "DUPPER" }
      expected_headers: set_equal_modulo_allow_list
    # Trap 1 for a naive `starts_with("application/grpc")`.
    - name: d-web-negative
      method: get
      path: "/d-web"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc-web"]
      expected_status: 404
      expected_body: { kind: byte_exact, body: "DWEB" }
      expected_headers: set_equal_modulo_allow_list
    # Trap 2 for the same naive prefix match.
    - name: d-foo-negative
      method: get
      path: "/d-foo"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpcfoo"]
      expected_status: 404
      expected_body: { kind: byte_exact, body: "DFOO" }
      expected_headers: set_equal_modulo_allow_list
    - name: d-absent-negative
      method: get
      path: "/d-absent"
      host: "envoy-rust.test"
      expected_status: 404
      expected_body: { kind: byte_exact, body: "DABSENT" }
      expected_headers: set_equal_modulo_allow_list
```

- [ ] **Step 2: Run the fixture**

```bash
cargo build -p envoy-bin
cargo test -p differential --test grpc_aware_local_replies 2>&1 | tee /tmp/t2.log
```

Expected: **PASS**.

- [ ] **Step 3: Prove the negative cells are not vacuous — mutation V2**

Turn the `APPLICATION/GRPC` negative cell POSITIVE by lower-casing the sent value. Upstream will then transform (`200` + `grpc-status: 12`), so `expected_status: 404` must fail.

```bash
cd tests/fixtures/0089-grpc-aware-local-replies
md5sum expectations.yaml > /tmp/v2.md5
grep -c '"APPLICATION/GRPC"' expectations.yaml    # must be exactly 1
sed -i 's|"APPLICATION/GRPC"|"application/grpc"|' expectations.yaml
cd - && cargo test -p differential --test grpc_aware_local_replies 2>&1 | tee /tmp/v2.log
```

Expected: **FAIL**, naming probe `d-upper-negative`.

> The `grep -c` guard is not decoration. If the anchor string occurred more than once the `sed` would move an implementation and its own witness in lockstep and return a GREEN that reads as "these cells are vacuous". **Refuse the mutation unless the count is exactly 1.**

Revert and verify:

```bash
cd tests/fixtures/0089-grpc-aware-local-replies
git checkout -- expectations.yaml   # SAFE: the file is TRACKED as of Task 1
md5sum -c /tmp/v2.md5
grep -c '"APPLICATION/GRPC"' expectations.yaml   # must be back to 1
cd - && cargo test -p differential --test grpc_aware_local_replies
```

Expected: `expectations.yaml: OK` and the test GREEN.

> `git checkout --` is a no-op on an UNTRACKED file and would leave the mutation in place while looking clean. It is safe here **only because Task 1 committed the file**. The `md5sum -c` is what actually adjudicates it.

- [ ] **Step 4: Commit**

```bash
git add tests/fixtures/0089-grpc-aware-local-replies/expectations.yaml
git commit -m "phase 110.2 task 2: the 8 detection cells — exact/+proto/+bare positive, param/case/grpc-web/grpcfoo/absent negative; mutation V2 proves the negatives are live"
```

---

## Task 3: method-insensitivity and the two empty-body cells

**Files:**
- Modify: `tests/fixtures/0089-grpc-aware-local-replies/expectations.yaml` (append probes 24–26)

**Interfaces:**
- Consumes: Task 2's probe list; routes `/x-post` and `/e-empty` from Task 1. `/no-such-route` matches NO route by design.
- Produces: probes 24–26.

- [ ] **Step 1: Append the three probes**

```yaml
    # ---- detection is METHOD-INSENSITIVE. `Http1Method` offers only
    # ---- get/options/post — there is no `put` or `delete` — so `post` is the
    # ---- available second method.
    - name: x-post-method-insensitive
      method: post
      path: "/x-post"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    # ---- an EMPTY original body means `grpc-message` is ABSENT ENTIRELY, not
    # ---- empty. The name-set half of `diff_headers` is what catches a
    # ---- spurious empty header, so this cell needs no value assertion.
    # ---- NOTE: there is deliberately no non-gRPC twin — envoy-rust's
    # ---- `synth_with` emits `content-type` on an empty-body local reply where
    # ---- upstream emits none (CF-110-6). In the gRPC direction both proxies
    # ---- emit `content-type: application/grpc` and AGREE.
    - name: e-empty-no-grpc-message
      method: get
      path: "/e-empty"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    # ---- the SECOND empty-body cell, and a different local-reply SITE: this
    # ---- path matches no route, so the reply is the HCM's OWN
    # ---- route-not-found 404 (`synth_404` via `build_response_in`) rather
    # ---- than a `direct_response`.
    - name: nomatch-404-no-grpc-message
      method: get
      path: "/no-such-route"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
```

- [ ] **Step 2: Run the fixture**

```bash
cargo build -p envoy-bin
cargo test -p differential --test grpc_aware_local_replies 2>&1 | tee /tmp/t3.log
```

Expected: **PASS**.

- [ ] **Step 3: Prove the empty-body cell pins `grpc-message` ABSENCE — mutation V3**

Give `/e-empty` a non-empty body in **both** yamls. Upstream then emits `grpc-message: EMPTYNOW`, changing the header NAME SET on the upstream side while envoy-rust does the same — so this mutation must **NOT** red on its own. To make it a genuine witness, mutate only the **upstream** side:

```bash
cd tests/fixtures/0089-grpc-aware-local-replies
md5sum envoy.yaml envoy-rust.yaml > /tmp/v3.md5
grep -c 'inline_string: ""' envoy.yaml    # must be exactly 1
sed -i 's|direct_response: { status: 404, body: { inline_string: "" } }|direct_response: { status: 404, body: { inline_string: "EMPTYNOW" } }|' envoy.yaml
cd - && cargo test -p differential --test grpc_aware_local_replies 2>&1 | tee /tmp/v3.log
```

Expected: **FAIL**, naming probe `e-empty-no-grpc-message`, with a header **name-set** difference reporting `grpc-message` present only on the envoy side. That is the direct proof that absence — not just value — is pinned.

> This mutation deliberately breaks byte-identity for the duration of the run. Restore it and re-assert identity:

```bash
cd tests/fixtures/0089-grpc-aware-local-replies
git checkout -- envoy.yaml
md5sum -c /tmp/v3.md5 && md5sum envoy.yaml envoy-rust.yaml && wc -c envoy.yaml envoy-rust.yaml
cd - && cargo test -p differential --test grpc_aware_local_replies
```

Expected: both md5s `OK`, the two files identical with equal non-zero byte counts, test GREEN.

- [ ] **Step 4: Commit**

```bash
git add tests/fixtures/0089-grpc-aware-local-replies/expectations.yaml
git commit -m "phase 110.2 task 3: method-insensitivity + both empty-body cells (direct_response and the HCM route-not-found 404); mutation V3 proves grpc-message ABSENCE is pinned"
```

---

## Task 4: the §1.3 percent-encoding cells

**Files:**
- Modify: `tests/fixtures/0089-grpc-aware-local-replies/expectations.yaml` (append probes 27–30)

**Interfaces:**
- Consumes: routes `/enc-main` and `/enc-edge` from Task 1.
- Produces: probes 27–30.

- [ ] **Step 1: Append the four probes**

The control probes assert the ORIGINAL body byte-exactly, which is what makes the encoded `grpc-message` meaningful — without them a wrong encoding and a wrong source body are indistinguishable.

```yaml
    # ---- §1.3 percent-encoding. A byte passes through UNCHANGED iff it is in
    # ---- `0x20..=0x7D` AND is not `%` (0x25). Everything else becomes `%` plus
    # ---- TWO UPPERCASE hex digits; multi-byte UTF-8 is encoded PER BYTE.
    # ---- MEASURED: `a b\ncontrol\ttab é %25 end`
    # ----        -> `a b%0Acontrol%09tab %C3%A9 %2525 end`
    # ---- The `%25` -> `%2525` cell is the discriminating one for a
    # ---- hand-rolled encoder that forgets to escape `%` itself.
    - name: enc-main-percent-encoded
      method: get
      path: "/enc-main"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: enc-main-control
      method: get
      path: "/enc-main"
      host: "envoy-rust.test"
      expected_status: 400
      expected_body: { kind: byte_exact, body: "a b\ncontrol\ttab é %25 end" }
      expected_headers: set_equal_modulo_allow_list
    # ---- the BOUNDARY cell the parent SPEC got wrong: `~` (0x7E) IS ESCAPED
    # ---- (`%7E`), while `"` (0x22) and `\` (0x5C) pass through unchanged.
    # ---- MEASURED: `q"b s\l t~t dd` -> `q"b s\l t%7Et dd`
    - name: enc-edge-tilde-escaped
      method: get
      path: "/enc-edge"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: enc-edge-control
      method: get
      path: "/enc-edge"
      host: "envoy-rust.test"
      expected_status: 400
      expected_body: { kind: byte_exact, body: "q\"b s\\l t~t dd" }
      expected_headers: set_equal_modulo_allow_list
```

- [ ] **Step 2: Run the fixture**

```bash
cargo build -p envoy-bin
cargo test -p differential --test grpc_aware_local_replies 2>&1 | tee /tmp/t4.log
```

Expected: **PASS**. If `enc-main-control` fails on the body, the YAML escape handling diverged between the two parsers — that is a finding, not something to paper over by relaxing the assertion.

- [ ] **Step 3: Commit**

```bash
git add tests/fixtures/0089-grpc-aware-local-replies/expectations.yaml
git commit -m "phase 110.2 task 4: the percent-encoding cells — %0A/%09/%C3%A9/%2525 and the ~ -> %7E boundary, each with its byte-exact untransformed control"
```

---

## Task 5: the redirect cells — `location` survives the transform

**Files:**
- Modify: `tests/fixtures/0089-grpc-aware-local-replies/expectations.yaml` (append probes 31–32)

**Interfaces:**
- Consumes: the `/r-redir` `redirect:` route from Task 1.
- Produces: probes 31–32; the complete 32-probe list.

- [ ] **Step 1: Append the two probes**

```yaml
    # ---- `location` SURVIVES the transform and stays FIRST in wire order.
    # ---- A `redirect:` route is the ONLY safe way to get a `location` header
    # ---- into this fixture: upstream also emits `location` on a 201/3xx
    # ---- `direct_response` and envoy-rust does not (CF-110-3), so no such
    # ---- cell may appear here. `synth_redirect` already emits it.
    # ---- `location` is NOT on the HEADER_ALLOW_LIST, so its VALUE is
    # ---- compared byte-exact — and that comparison IS this cell's witness.
    - name: r-redirect-grpc-keeps-location
      method: get
      path: "/r-redir"
      host: "envoy-rust.test"
      extra_headers:
        - ["content-type", "application/grpc"]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
    - name: r-redirect-control
      method: get
      path: "/r-redir"
      host: "envoy-rust.test"
      expected_status: 301
      expected_body: { kind: byte_exact, body: "" }
      expected_headers: set_equal_modulo_allow_list
```

- [ ] **Step 2: Run the fixture — the full 32-probe set**

```bash
cargo build -p envoy-bin
cargo test -p differential --test grpc_aware_local_replies -- --nocapture 2>&1 | tee /tmp/t5.log
grep -c 'name:' tests/fixtures/0089-grpc-aware-local-replies/expectations.yaml
```

Expected: **PASS**, and the probe count is **32**.

- [ ] **Step 3: Assert byte-identity one last time, and audit the run**

```bash
cd tests/fixtures/0089-grpc-aware-local-replies
md5sum envoy.yaml envoy-rust.yaml && wc -c envoy.yaml envoy-rust.yaml
```

Expected: identical md5s AND identical non-zero byte counts.

Then audit the green, because a backend-free fixture finishes in ~1–3 s and that is NORMAL rather than a silent skip. Prove the containers really ran, with a **valid** `docker ps` format field:

```bash
cargo test -p differential --test grpc_aware_local_replies &
for i in $(seq 1 30); do docker ps --format '{{.ID}} {{.Image}} {{.Names}}'; sleep 1; done | sort -u
wait
```

Expected: at least one line naming `envoyproxy/envoy:v1.33.0`. **`{{.ImageID}}` is an INVALID field** — it turns every poll line into a template error that reads as "no containers ran".

- [ ] **Step 4: Commit**

```bash
git add tests/fixtures/0089-grpc-aware-local-replies/expectations.yaml
git commit -m "phase 110.2 task 5: the redirect cells — location survives the transform alongside grpc-status 2; the 32-probe set is complete and green"
```

---

## Task 6: the fixture README

**Files:**
- Create: `tests/fixtures/0089-grpc-aware-local-replies/README.md`

**Interfaces:**
- Consumes: the finished fixture and the mutation results from Tasks 1–5.
- Produces: the prose record. No code reads it (85 of 88 fixtures carry one).

- [ ] **Step 1: Write the README**

It must contain, at minimum:

1. **Title and provenance** — `# 0089 — gRPC-aware local replies (HTTP/1.1)`, sub-phase 110.2, the ADR numbers, and the pinned image `envoyproxy/envoy:v1.33.0` with its digest.
2. **What it witnesses** — a probe table with one row per cell, mirroring "The frozen probe set" above (32 rows: probe name, path, sent `content-type`, expected status/body, and the rule witnessed).
3. **What actually pins what** — `expected_status`/`expected_body` are absolute; `expected_headers` is cross-proxy; `grpc-status`, `grpc-message`, `content-type`, `content-length` and `location` are all outside the 3-entry `HEADER_ALLOW_LIST` and therefore value-compared byte-exact. **Never add `location` to that list.**
4. **The three deliberately-absent cells, each with its measurement** — CF-110-3 (`location` on 201/301/302 `direct_response`), CF-110-6 (empty-body `content-type`), CF-110-8 (the `header_mutation` `grpc-status` suppression). State plainly that each is a MEASURED divergence unrelated to gRPC and that including it would RED the fixture for the wrong reason.
5. **The two shape decisions** — no `node:` block (YAML 1.1 booleanization) and the literal `admin` `port_value: 0` (the `{{ADMIN_PORT}}` substitution is driver-gated and `Http1ProbeList` is not one of the four). Note that `envoy.yaml` and `envoy-rust.yaml` are BYTE-IDENTICAL and that this is a PER-FIXTURE claim to be re-derived, never a tree property.
6. **Running it** — `cargo build -p envoy-bin` first (the harness runs the DEBUG binary; a stale one fails with `unknown field` errors that look like real divergences), then `cargo test -p differential --test grpc_aware_local_replies`. Note that it is backend-free and therefore fully verifiable on a developer host.
7. **Proof it is not vacuous** — a table of mutations V1/V2/V3 with the exact probe each REDs, and the note that the driver **aborts at the first failing probe**, so one red run names exactly ONE probe.

- [ ] **Step 2: Verify no stray claim**

```bash
grep -n 'byte-identical\|BYTE-IDENTICAL' tests/fixtures/0089-grpc-aware-local-replies/README.md
md5sum tests/fixtures/0089-grpc-aware-local-replies/envoy*.yaml
wc -c tests/fixtures/0089-grpc-aware-local-replies/envoy*.yaml
```

Every byte-identity claim in the prose must be backed by the md5 **and** the byte count printed here.

- [ ] **Step 3: Commit**

```bash
git add tests/fixtures/0089-grpc-aware-local-replies/README.md
git commit -m "phase 110.2 task 6: fixture 0089 README — the 32-cell witness table, the three deliberately-absent divergent cells, and the V1/V2/V3 mutation proofs"
```

---

## Task 7: the `BEHAVIOR_CONTRACT.md` `## gRPC` section

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — **INSERT at line 643**, between the `---` at 642 and `## Header allow-list` at 644.

**Interfaces:**
- Consumes: every measurement in §0 of this plan.
- Produces: the canonical contract record. Nothing downstream reads it programmatically.

- [ ] **Step 1: Re-derive the insertion point BY TEXT, not by the line number above**

```bash
grep -n '^## Active gRPC health check\|^## Header allow-list' docs/envoy-rust/BEHAVIOR_CONTRACT.md
grep -c '^## gRPC' docs/envoy-rust/BEHAVIOR_CONTRACT.md
```

Expected at this PLAN-write: `574` and `644`, and a `## gRPC` count of **0**. **Line numbers drift — locate by text.** The new section goes immediately BEFORE `## Header allow-list`, keeping all gRPC content contiguous and leaving the data-plane block (lines 27–641) intact.

> Sections in this file are **INSERTED topically, never appended at EOF** — verified across every `##`-adding commit in the file's history. `## Timing tolerances` has been last since bootstrap.

- [ ] **Step 2: Write the section**

Follow the house anatomy exactly: a `>` blockquote naming the phase, the ADRs, the pinned image and the witness fixture; then measured tables; then the derived rules; then a header-set table; then the `HEADER_ALLOW_LIST` disposition; then a closing **NOT MEASURED** list. Open and close with `---` separators.

Required content:

- **§A — the HTTP→`grpc-status` mapping.** The sparse eight-entry table (`400`→13, `401`→16, `403`→7, `404`→12, `429`→14, `502`→14, `503`→14, `504`→14) over an explicit default of **2 (UNKNOWN)**, and the plain statement that everything else — the whole 2xx/3xx range and, counter-intuitively, `500`, `501`, `405`, `408`, `409`, `412`, `413`, `499` — maps to 2.
- **§B — detection.** Detected iff the request `content-type` is EXACTLY `application/grpc` or begins with `application/grpc+`. CASE-SENSITIVE on the VALUE; a parameter defeats it; `application/grpc-web` and `application/grpcfoo` do not match. METHOD-INSENSITIVE and independent of `te: trailers`. Record that the trailing-space cell is the codec's OWS handling, not matcher tolerance.
- **§C — `grpc-message` percent-encoding.** A byte passes through UNCHANGED iff it is in `0x20..=0x7D` AND is not `%` (0x25); every other byte becomes `%` + TWO UPPERCASE hex digits; UTF-8 per byte. **`~` (0x7E) IS escaped** — state explicitly that the parent SPEC's `0x20..=0x7E` rule was measured FALSE.
- **§D — the wire shape and the header ORDER.** Status `200`; `content-type: application/grpc`; body DROPPED; `content-length: 0`; `grpc-status` per §A; `grpc-message` per §C **only when the original body was non-empty — ABSENT ENTIRELY otherwise**; `location` SURVIVES. Measured order:
  `[location,] content-type, grpc-status, [grpc-message,] date, server, connection, content-length`.
  **State that the harness does NOT compare order** — it is recorded for the contract, not as a fixture gate — and that a wrong order fails an in-process unit test, not the fixture.
- **§E — the harness disposition.** `grpc-status`, `grpc-message`, `content-type`, `content-length` and `location` are all OUTSIDE the 3-entry `HEADER_ALLOW_LIST` and are therefore compared VALUE-EXACT. **Never add `location`.** Note that `diff_headers` compares a SET, so a duplicated `grpc-status` would be invisible.
- **§F — scope.** HTTP/1.1 LOCAL replies only. H2 is unbuilt (CF-110-1, shape measured: same transform, headers-only, `content-length` OMITTED rather than `0`). Proxied responses are untransformed (CF-110-2).
- **§G — the pre-existing `grpc-status` response header: MEASURED, and a DIVERGENCE.** This is REVIEW finding M-2, and it must be stated as measured rather than unmeasured. Record both sides exactly as §0.6 measures them: upstream still transforms and lets the operator's `grpc-status` value win; envoy-rust's idempotence sentinel suppresses the transform entirely. Name it **CF-110-8** and state that fixture `0089` deliberately carries no such cell.
- **§H — NOT MEASURED — do not treat these as settled.** At minimum: the transform over TLS; the interaction with a response `grpc-message` header injected by an operator; whether upstream's `grpc-status` preservation in §G is an add-if-absent rule or a filter-ordering artifact (measured as an observable, mechanism NOT established); and the H2 cells not covered by `110.1` §1.7.
- **Fixture pointer** — `tests/fixtures/0089-grpc-aware-local-replies`, 32 probes, backend-free, `envoy.yaml` ≡ `envoy-rust.yaml` byte-identical.
- **The two other recorded divergences this surface sits next to** — CF-110-6 (empty-body `content-type`) and CF-110-7 (`direct_response.body` mandatory here, optional upstream), each with the note that it is orthogonal to gRPC.

- [ ] **Step 3: Verify the insertion structurally**

```bash
grep -c '^## gRPC' docs/envoy-rust/BEHAVIOR_CONTRACT.md          # must be 1
grep -n '^## ' docs/envoy-rust/BEHAVIOR_CONTRACT.md | head -20   # ## gRPC between the HC section and Header allow-list
sed -n "$(grep -n '^## gRPC' docs/envoy-rust/BEHAVIOR_CONTRACT.md | cut -d: -f1),+4p" docs/envoy-rust/BEHAVIOR_CONTRACT.md
git diff --numstat -- docs/envoy-rust/BEHAVIOR_CONTRACT.md
```

Expected: exactly one `## gRPC`; `## ` count **15 → 16**; the numstat shows a PURE INSERTION (`N 0`).

> Use the full path `docs/envoy-rust/BEHAVIOR_CONTRACT.md` in every pathspec. A bare `-- BEHAVIOR_CONTRACT.md` matches nothing and returns a believable EMPTY forever (`110.1/REVIEW.md` N-5).

- [ ] **Step 4: Confirm no fixture went red**

```bash
cargo test -p differential --test grpc_aware_local_replies
```

Expected: **PASS**. (A docs edit cannot move it; this is the cheap confirmation that the tree is still green before the state-4 gate runs the full sweep.)

- [ ] **Step 5: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 110.2 task 7: BEHAVIOR_CONTRACT ## gRPC section — the mapping/detection/encoding/wire-shape matrices, the harness disposition, and M-2 recorded as a MEASURED divergence (CF-110-8)"
```

---

## What state 4 will verify (NOT this state's work)

Recorded so the executor does not attempt it here. Per §7.5, instantiated by `110.2/SPEC.md` §8:

- (a) Fixture `0089` green cross-proxy on EVERY probe.
- (b) All **88** pre-existing fixtures still green (CI-authoritative for the backend-routing ones).
- (c) Conformance unchanged — h2spec threshold untouched, `known-failures.txt` untouched at **21** lines / ONE real entry.
- (d) **No new fuzz target** — no parser, codec, filter or config surface is added.
- (e) `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` — all clean at WORKSPACE scope.
- (f) `REVIEW.md` APPROVED.

**The test-count identity moves by exactly ONE** — `0089`'s single new test binary. The workspace identity is **`binaries=165 passed=2227 failed=0`** today (CI runs the plain `cargo test --workspace`; `--all-targets` yields 149, the 16-binary gap being the doc-test harnesses). Expect **2228** after this sub-phase. **Any other movement is a signal.**

---

## Carry-forwards

**Opened by this PLAN-write, all MEASURED this session:**

- **CF-110-6** — envoy-rust's `synth_with` family emits `content-type` on an **empty-body** local reply where upstream emits none. Pre-existing, orthogonal to gRPC, and unwitnessed by any existing fixture (`grep -rn 'inline_string: ""' tests/fixtures/` = 0 across the 40 `direct_response` fixtures). `BEHAVIOR_CONTRACT.md:1131-1137` (ADR-0059) records the upstream rule, but the decorator implementing it covers only FILTER local replies. *Unblocked by* a slice that moves the empty-body `content-type` suppression into `synth_with`.
- **CF-110-7** — `direct_response.body` is MANDATORY in envoy-rust (`crates/envoy-config/src/bootstrap.rs:2923-2926`: `pub struct DirectResponse { pub status: u16, pub body: DataSource }`) and OPTIONAL upstream. A bodiless `direct_response` is boot-fatal here and accepted there — a reject-direction, config-load divergence. *Unblocked by* making the field `#[serde(default)]` with an empty-`DataSource` default.
- **CF-110-8** — a pre-existing `grpc-status` response header SUPPRESSES envoy-rust's entire transform via the idempotence sentinel (`crates/envoy-http1/src/grpc.rs:158-160`), while upstream transforms anyway and preserves the operator's value. MEASURED on both proxies (§0.6). This is `110.1/REVIEW.md` M-2 promoted from unmeasured to measured. *Unblocked by* narrowing the sentinel to the actual upstream rule.
- **CF-110-9** — the seam-PLACEMENT finding has no differential witness, and cannot get one inside `0089`: a fixture has ONE driver, `Http1ProbeList` never reads an access log, and `AccessLogByteExactProbe` carries no `expected_headers`. A `%RESPONSE_CODE%`/`%BYTES_SENT%` witness needs a SECOND fixture (§0.7). *Unblocked by* a slice that adds one.

**Carried unchanged, and NONE is fixed here** (§6.3; ADR-0165): CF-110-1 (NARROWED), CF-110-2, CF-110-3 (REASSIGNED, re-measured and WIDENED to `302` this session), CF-110-4, CF-110-5, CF-109-1 (WIDENED)/2/3, CF-108-1/2/3, CF-76-1, CF-75-2/3/4/5/6, CF-72-2/CF-75-1, M71-6, CF-74-1/2/3/4/6, CF-73-1, the `110.1` REVIEW's **M-1…M-9 + N-1…N-10**, the `109.2` REVIEW's M-1…M-8 + N-1…N-11, the `109.1` M-5 + N-1…N-6 set, the `108.2` M-2 + N-1…N-6 set, and the HTTP-filters-family (1)-(4).

## Next state

§5 **state 3** — the implementation, a SEPARATE session per §5.1 and ADR-0127. **That session is the first one that writes code.** It executes Tasks 1–7 with `superpowers:subagent-driven-development` or `superpowers:executing-plans`, appending to `PROGRESS.md` on each task completion. `ROADMAP.md` is NOT touched until state 6, where rows `110.2` **and parent `110`** flip `done` TOGETHER (the `76.2`/`108.2`/`109.2` two-row precedent, unlike the `110.1` close-out which flipped one row only).
