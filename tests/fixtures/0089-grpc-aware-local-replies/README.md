# 0089 — gRPC-aware local replies (HTTP/1.1)

Sub-phase **110.2** (`docs/envoy-rust/phases/110.2-grpc-local-reply-fixture-and-contract/`).
Relevant ADRs: **ADR-0178** (the phase-110 split), **ADR-0179** (sibling 110.1's
plan), **ADR-0180** (this sub-phase's plan and the measurements behind every cell
below). Reference proxy: `envoyproxy/envoy:v1.33.0`, digest
`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`,
verified with `docker image inspect` before probing.

## What it witnesses

Sibling **110.1** built the gRPC-aware LOCAL REPLY transform for HTTP/1.1 and
proved it only IN-PROCESS. **This fixture is the cross-proxy witness.**

A request whose `content-type` is EXACTLY `application/grpc` or begins with
`application/grpc+` turns any LOCALLY GENERATED reply into HTTP `200` +
`content-type: application/grpc` + `content-length: 0`, body DROPPED, with a
`grpc-status` header carrying a mapped code and — only when the original body
was non-empty — a `grpc-message` header carrying that body percent-encoded.
Any `location` header SURVIVES.

Backend-free and CLUSTER-FREE (`clusters: []`): every response is a local
reply, which is the whole surface under test. **32 probes** through the
`http1_probe_list` driver against 24 routes, each at its own distinct path with
its own distinct body (the `BEHAVIOR_CONTRACT.md` §G one-path-per-probe
attribution rule).

| # | probe | path | sent `content-type` | status | body | rule witnessed |
|---:|---|---|---|---:|---|---|
| 1 | `g-200-maps-to-unknown` | `/m-200` | `application/grpc` | 200 | `""` | 2xx → `grpc-status: 2` |
| 2 | `g-400-maps-to-13` | `/m-400` | `application/grpc` | 200 | `""` | `400` → **13** |
| 3 | `g-401-maps-to-16` | `/m-401` | `application/grpc` | 200 | `""` | `401` → **16** |
| 4 | `g-403-maps-to-7` | `/m-403` | `application/grpc` | 200 | `""` | `403` → **7** |
| 5 | `g-404-maps-to-12` | `/m-404` | `application/grpc` | 200 | `""` | `404` → **12** |
| 6 | `g-405-falls-to-unknown` | `/m-405` | `application/grpc` | 200 | `""` | counter-intuitive default arm |
| 7 | `g-429-maps-to-14` | `/m-429` | `application/grpc` | 200 | `""` | `429` → **14** |
| 8 | `g-500-falls-to-unknown` | `/m-500` | `application/grpc` | 200 | `""` | a 5xx that is **not** 14 |
| 9 | `g-502-maps-to-14` | `/m-502` | `application/grpc` | 200 | `""` | `502` → **14** |
| 10 | `g-503-maps-to-14` | `/m-503` | `application/grpc` | 200 | `""` | `503` → **14** |
| 11 | `g-504-maps-to-14` | `/m-504` | `application/grpc` | 200 | `""` | `504` → **14** |
| 12 | `c-200-untransformed` | `/m-200` | — | 200 | `"B200"` | control: status + body survive |
| 13 | `c-400-untransformed` | `/m-400` | — | 400 | `"B400"` | control |
| 14 | `c-404-untransformed` | `/m-404` | — | 404 | `"B404"` | control |
| 15 | `c-503-untransformed` | `/m-503` | — | 503 | `"B503"` | control |
| 16 | `d-exact-positive` | `/d-exact` | `application/grpc` | 200 | `""` | exact match detects |
| 17 | `d-plus-proto-positive` | `/d-plus-proto` | `application/grpc+proto` | 200 | `""` | `+` suffix detects |
| 18 | `d-plus-bare-positive` | `/d-plus-bare` | `application/grpc+` | 200 | `""` | bare `+` detects |
| 19 | `d-param-negative` | `/d-param` | `application/grpc; charset=utf-8` | 404 | `"DPARAM"` | a parameter DEFEATS it |
| 20 | `d-upper-negative` | `/d-upper` | `APPLICATION/GRPC` | 404 | `"DUPPER"` | CASE-SENSITIVE on the value |
| 21 | `d-web-negative` | `/d-web` | `application/grpc-web` | 404 | `"DWEB"` | naive-prefix trap 1 |
| 22 | `d-foo-negative` | `/d-foo` | `application/grpcfoo` | 404 | `"DFOO"` | naive-prefix trap 2 |
| 23 | `d-absent-negative` | `/d-absent` | — | 404 | `"DABSENT"` | header absent |
| 24 | `x-post-method-insensitive` | `/x-post` | `application/grpc` (**post**) | 200 | `""` | detection is METHOD-INSENSITIVE |
| 25 | `e-empty-no-grpc-message` | `/e-empty` | `application/grpc` | 200 | `""` | `grpc-message` ABSENT ENTIRELY |
| 26 | `nomatch-404-no-grpc-message` | `/no-such-route` | `application/grpc` | 200 | `""` | the HCM's OWN route-not-found 404 |
| 27 | `enc-main-percent-encoded` | `/enc-main` | `application/grpc` | 200 | `""` | `%0A` `%09` `%C3%A9` `%2525` |
| 28 | `enc-main-control` | `/enc-main` | — | 400 | `"a b\ncontrol\ttab é %25 end"` | the untransformed original |
| 29 | `enc-edge-tilde-escaped` | `/enc-edge` | `application/grpc` | 200 | `""` | **`~` → `%7E`**; `"` and `\` pass |
| 30 | `enc-edge-control` | `/enc-edge` | — | 400 | `"q\"b s\\l t~t dd"` | the untransformed original |
| 31 | `r-redirect-grpc-keeps-location` | `/r-redir` | `application/grpc` | 200 | `""` | `location` SURVIVES + `grpc-status: 2` |
| 32 | `r-redirect-control` | `/r-redir` | — | 301 | `""` | control: `301` + `location` |

Every probe carries `host: "envoy-rust.test"` and
`expected_headers: set_equal_modulo_allow_list`.

## What actually pins what

`expected_status` and `expected_body` are **ABSOLUTE** — asserted against BOTH
proxies independently.

`expected_headers: set_equal_modulo_allow_list` is **CROSS-PROXY**. `diff_headers`
takes only the two proxies' header vectors — there is no fixture-declared
expected header VALUE anywhere in the harness (`Http1HeaderRule` is a unit
variant carrying no data). It compares the lower-cased header NAME SET, then the
VALUE of every name outside the 3-entry `HEADER_ALLOW_LIST` (`server`, `date`,
`x-envoy-upstream-service-time`).

**`grpc-status`, `grpc-message`, `content-type`, `content-length` and `location`
are all OUTSIDE that allow-list, so all five are compared byte-exact between the
two proxies. THAT comparison is the entire mapping and encoding witness.**

> **NEVER add `location` — or `content-type` — to `HEADER_ALLOW_LIST` to make a
> probe pass.** Allow-listing a name vacates that name's assertion across the
> whole corpus while leaving every fixture green. It is the most dangerous
> failure mode available here precisely because it looks like success.

Two harness properties to design around: the name comparison is **set-based, not
multiset-based**, so a DUPLICATED `grpc-status` would be invisible and only the
FIRST occurrence's value is compared; and **header ORDER is never read**. The
measured wire order is recorded in `BEHAVIOR_CONTRACT.md` `## gRPC` §D and is
pinned by 110.1's in-process unit tests, not by this fixture.

## Three cells are deliberately ABSENT

Each is a **MEASURED** divergence unrelated to gRPC. Including any of them would
turn this fixture RED for the wrong reason — and a fixture that reds for the
wrong reason teaches the next reader the wrong lesson.

1. **No `201` or `3xx` `direct_response` cell (CF-110-3).** Upstream emits
   `location: <scheme>://<authority><path>` on a `direct_response` of `201`,
   `301` AND `302` — in BOTH the gRPC and the control direction — and
   envoy-rust's `synth_direct_response` emits none. (`204` gets none on either
   side.) The `redirect:` route at probes 31–32 is the safe way to get a
   `location` header in: `synth_redirect` already emits it, and both proxies
   agree on its value.
2. **No empty-body CONTROL probe (CF-110-6).** envoy-rust's `synth_with` emits
   `content-type` on an empty-body local reply where upstream emits none.
   `BEHAVIOR_CONTRACT.md` (ADR-0059) records the upstream rule, but the
   decorator implementing it covers only FILTER local replies. This has never
   been caught because no other fixture in the corpus uses an empty-body
   `direct_response`. **Both empty-body cells are probed in the gRPC direction
   (probes 25–26), where the two proxies emit `content-type: application/grpc`
   and AGREE** — so the contract cell is witnessed twice over; only the two
   control twins are dropped.
3. **No `header_mutation`-injected `grpc-status` cell (CF-110-8).** With a
   chain-level `header_mutation` adding `grpc-status: 99`, upstream STILL
   transforms and merely lets the operator's `99` win over the mapped `12`,
   while envoy-rust's idempotence sentinel
   (`crates/envoy-http1/src/grpc.rs:158-160`) returns early and emits no
   transform at all. A genuine divergence, not a fixture-shape accident.

A fourth constraint shapes the config rather than the probe set:
**every `direct_response` carries an explicit `body:` (CF-110-7)** — the field
is MANDATORY in envoy-rust (`crates/envoy-config/src/bootstrap.rs:2923-2926`
declares `body: DataSource`, with no `#[serde(default)]` and not `Option`) and
OPTIONAL upstream, so a bodiless `direct_response` is boot-fatal here and
accepted there. The empty cell is spelled `body: { inline_string: "" }`.

## Two shape decisions

**No `node:` block.** Upstream parses YAML 1.1 and booleanizes an unquoted
`y`/`n`/`on`/`off` scalar; `serde_yaml` parses YAML 1.2 and does not. Omitting
`node:` entirely is what lets ONE file serve both proxies. The precise rule is
*never write an unquoted `y`/`n`/`on`/`off` scalar* — not "omit `node:`" as
such.

**`admin.socket_address.port_value` is a LITERAL `0`, never `{{ADMIN_PORT}}`.**
That substitution is driver-gated to `AdminScrape` / `Http1KeepAlive` /
`Http2KeepAlive` / `TcpWithStats`; `Http1ProbeList` is NOT among them, and
`render_yaml` leaves an unmatched token UNTOUCHED — so a literal
`{{ADMIN_PORT}}` would reach the parser and fail as an address. `{{PORT}}` IS
substituted for this driver.

**`envoy.yaml` and `envoy-rust.yaml` are BYTE-IDENTICAL:**

```
216e712c14b1ca1dd8fcd0a4c277f8ab  envoy.yaml
216e712c14b1ca1dd8fcd0a4c277f8ab  envoy-rust.yaml
 6561 envoy.yaml
 6561 envoy-rust.yaml
```

Both the md5 AND the byte count are asserted, because a uniform md5 can be the
empty-file md5 `d41d8cd98f00b204e9800998ecf8427e`. **This is a PER-FIXTURE
claim to be re-derived, never a tree property** — most fixtures in the corpus
have genuinely divergent pairs.

## Running it

```bash
cargo build -p envoy-bin          # REQUIRED FIRST
cargo test -p differential --test grpc_aware_local_replies
```

The harness runs the **DEBUG** binary; a stale one fails with `unknown field`
errors that look like real divergences. This fixture is **backend-free** (no
`{{BACKEND_IP}}` marker, so no backend container spawns), and is therefore
fully verifiable on a developer host rather than CI-authoritative — unlike the
backend-routing fixtures, which red locally on the `192.168.65.2` bridge.

It completes in **~1 second**, which is NORMAL for a backend-free fixture and
is not evidence of a silent skip. To prove the containers really ran, poll with
a **VALID** format field while it runs:

```bash
cargo test -p differential --test grpc_aware_local_replies &
for i in $(seq 1 40); do docker ps --format '{{.ID}} {{.Image}} {{.Names}}'; sleep 0.2; done | sort -u
wait
```

Expect a line naming `envoyproxy/envoy:v1.33.0`. **`{{.ImageID}}` is an INVALID
field** — it turns every poll line into a template error that reads as "no
containers ran".

## Proof it is not vacuous

Sibling 110.1 already landed the behaviour, so **this fixture passes on its
first run**: it is a CHARACTERIZATION PIN, and a green alone proves only that
it executes. Four in-place mutations supply the RED evidence. Each was guarded
by asserting its anchor string occurs EXACTLY ONCE before mutating, reverted
byte-exactly, adjudicated by **md5** rather than by eye, and paired with an
unmutated control run from the same tree.

| mutation | change | probe it REDs | what that proves |
|---|---|---|---|
| **V1** | `/m-403` status `403`→`500`, **`envoy.yaml` only** | `g-403-maps-to-7` | `header 'grpc-status': envoy='2' envoy-rust='7'` — the `grpc-status` VALUE is genuinely compared cross-proxy |
| **V2** | `APPLICATION/GRPC` → `application/grpc` in `expectations.yaml` | `d-upper-negative` | `upstream status 200 != expected 404` — the case-sensitivity negative cell is live |
| **V3** | `/e-empty` body `""`→`"EMPTYNOW"`, **`envoy.yaml` only** | `e-empty-no-grpc-message` | `header name sets differ: only-in-envoy=["grpc-message"]` — `grpc-message` ABSENCE, not just its value, is pinned |
| **V4** | `/enc-main` body tail `end`→`END`, **`envoy.yaml` only** | `enc-main-percent-encoded` | `header 'grpc-message': envoy='…%2525 END' envoy-rust='…%2525 end'` — the `grpc-message` VALUE, i.e. the whole encoding rule, is compared |

**Three of the four mutate ONE SIDE only, and that is essential.** `diff_headers`
is purely cross-proxy: a mutation applied to BOTH yamls moves both proxies in
lockstep and returns a GREEN that reads as "these cells are vacuous". V1 was
originally specified as a two-sided mutation and had to be corrected to the
one-sided form for exactly this reason.

> **The probe-list driver ABORTS AT THE FIRST FAILING PROBE**, so one red run
> names exactly ONE probe. Never infer a second cell's state from a single red
> run.
