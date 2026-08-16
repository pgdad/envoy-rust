# Sub-phase 110.2 — the differential witness: NEW cluster-free fixture `0089` + the `BEHAVIOR_CONTRACT.md` `## gRPC` section + the parent-110 close

> Redistributed from `docs/envoy-rust/phases/110-grpc-aware-local-replies/SPEC.md`
> at the §5 state-2 PLAN-write, when the §6.1 split FIRED (**ADR-0178**).
> Written for a reader with ZERO prior context (D-3.4). Every upstream
> behaviour cited as MEASURED was probed at the split session against the
> `ENVOY_TARGET.md`-pinned `envoyproxy/envoy:v1.33.0` (digest
> `sha256:56da5afd…70c2`, verified by `docker image inspect` BEFORE any probe).
> Every figure is a CLAIM this sub-phase's state-2 PLAN-write must re-derive.

## §0. What this sub-phase is

Sibling **`110.1`** builds gRPC-aware local replies for HTTP/1.1 and proves
them IN-PROCESS: a request whose `content-type` is `application/grpc` (or
`application/grpc+…`) turns any locally generated reply into HTTP **`200`** +
`content-type: application/grpc` + `content-length: 0`, body DROPPED, with a
**`grpc-status`** header and — only when the original body was non-empty — a
**`grpc-message`** header carrying that body percent-encoded.

**This sub-phase adds the CROSS-PROXY WITNESS and the canonical contract
record.** It creates the NEW cluster-free, backend-free differential fixture
**`0089`**, adds a `## gRPC` section to `BEHAVIOR_CONTRACT.md`, and closes the
parent phase `110`. **It ships no crate source change** unless a divergence
surfaces at the fixture, in which case that divergence is the finding and the
fix belongs here.

**`110.1` MUST be `done` before this sub-phase enters `in-progress`.** There is
nothing to witness until the transform exists.

## §1. What the fixture must witness — the measured matrices

These are the cells `110.1` implements. They are restated here in full so this
document stands alone (D-3.4); `110.1/SPEC.md` §§1.1–1.4 is the fuller record
including the probe methodology.

### §1.1 The HTTP→`grpc-status` mapping — SPARSE EIGHT entries over a DEFAULT of 2

`400`→**13**, `401`→**16**, `403`→**7**, `404`→**12**, `429`→**14**,
`502`→**14**, `503`→**14**, `504`→**14**. **Everything else → 2 (UNKNOWN)** —
including the whole 2xx/3xx range and, counter-intuitively, `500`, `501`,
`405`, `408`, `409`, `412`, `413` and `499`.

### §1.2 Detection — EXACT `application/grpc` or the prefix `application/grpc+`

Positive: `application/grpc`, `application/grpc+proto`, `application/grpc+json`,
bare `application/grpc+`. Negative: `application/grpc; charset=utf-8` and
`application/grpc;charset=utf-8` (a parameter DEFEATS it), `APPLICATION/GRPC`
and `Application/Grpc` (CASE-SENSITIVE), `application/grpc-web`,
`application/grpc-web+proto`, `application/grpcfoo`, `application/json`,
header absent. METHOD-INSENSITIVE and INDEPENDENT of `te: trailers`.

### §1.3 `grpc-message` percent-encoding — the boundary rule

**A byte passes through UNCHANGED iff it is in `0x20..=0x7D` AND is not `%`
(0x25). Every other byte becomes `%` + TWO UPPERCASE hex digits.** UTF-8 is
encoded PER BYTE. Measured cells: `a b\ncontrol\ttab é %25 end` →
`a b%0Acontrol%09tab %C3%A9 %2525 end`; `~`→`%7E`; `0x7F`→`%7F`; `"` and `\`
pass through; `` +,/:;=?@[]{}|^`<>#&*() `` pass through.

> The parent `110/SPEC.md` claimed `0x20..=0x7E` passes through. **That is
> MEASURED FALSE — `~` (0x7E) IS escaped.** Do not re-inherit the old rule.

### §1.4 The wire shape and the header order

Status → `200`; `content-type` → `application/grpc`; body DROPPED;
`content-length` → `0`; `grpc-status` per §1.1; `grpc-message` per §1.3 **only
when the original body was non-empty — ABSENT ENTIRELY, not empty, otherwise**.
Any `location` header SURVIVES.

MEASURED order:
`[location,] content-type, grpc-status, [grpc-message,] date, server, connection, content-length`.

**But the harness does NOT compare order** (§2.2) — this is recorded for the
contract, not as a fixture gate.

## §2. The harness needs NOTHING new — verified FIELD BY FIELD

### §2.1 The driver and the probe struct

- **`Driver::Http1ProbeList { probes: Vec<Http1Probe> }`**,
  `tests/differential/src/lib.rs:119-121`. The enclosing enum carries
  `#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]`
  (`:38`), so the fixture-YAML spelling is **`kind: http1_probe_list`** — used
  by **14** fixtures today. ⚠ `deny_unknown_fields` means any typo'd key in the
  `driver:` block is a HARD PARSE ERROR, not a silent ignore.
- **`Http1Probe`** (`:1154-1177`) carries `name: String` (MANDATORY),
  `method: Http1Method` (MANDATORY), `path: String` (MANDATORY),
  `host: String` (MANDATORY), and — each `#[serde(default)]` —
  **`extra_headers: Vec<(String, String)>`**, `body: Option<String>`,
  `expected_status: Option<u16>`, `expected_body: Option<Http1BodyRule>`,
  `expected_headers: Option<Http1HeaderRule>`. The struct is
  `deny_unknown_fields`.
- **`Http1Method`** (`:1036-1050`) is `snake_case` and offers only **`get`,
  `options`, `post`** — there is no `put` or `delete`. §1.2's
  method-insensitivity is therefore witnessable with `get` and `post` only.
- **`extra_headers` is what makes this fixture possible at all** — it is how a
  probe SENDS `content-type: application/grpc`.

### §2.2 **Header CASE is preserved — the `APPLICATION/GRPC` negative cell is NOT vacuous**

`drive_http1` (`tests/differential/src/lib.rs:2194-2222`) builds the request
text with a raw interpolation:

```rust
for (n, v) in extra_headers {
    req.push_str(&format!("{n}: {v}\r\n"));
}
```

**No `to_ascii_lowercase()`, no `HeaderName` normalisation, no validation.** So
`("content-type", "APPLICATION/GRPC")` reaches BOTH proxies with the value case
intact, and the §1.2 case-sensitivity negative cell is a real witness. (Note
the contrast: `drive_http2` uses `builder.header(...)`, which DOES lower-case
names — header-name case is controllable only on the H1 path. Irrelevant here,
since the case that matters is the VALUE.)

⚠ `drive_http1` unconditionally emits `Host:` and `Connection: close` itself,
and auto-adds `Content-Length` when `body:` is set. **Do not put `host`,
`connection` or `content-length` in `extra_headers`** — duplicates are sent
verbatim.

### §2.3 The comparison — set-equality of names, exact equality of values

`run_http1_probe_list_arm` compares, per probe: response status (envoy ↔
envoy-rust under `response_status: exact`, then each against
`expected_status`), then body via `assert_body_rule` and `expected_body`, then
— when `expected_headers: set_equal_modulo_allow_list` is set — `diff_headers`.

`diff_headers` (`:1202-1263`) (1) builds a `BTreeSet` of LOWER-CASED header
names from each side and bails if the SETS differ, then (2) for every name NOT
in the allow-list compares the VALUES with exact `!=`.

**`HEADER_ALLOW_LIST` (`:1184-1193`) holds exactly THREE entries — `server`,
`date`, `x-envoy-upstream-service-time`** — all `AllowMode::NameRequired`.
**Never add `location`.** ⚠ Despite its name, `NameRequired` does NOT enforce
presence; presence is enforced symmetrically for ALL headers by the name-set
equality check. `NameRequired` ONLY suppresses the value comparison.

**Therefore `grpc-status`, `grpc-message`, `content-type`, `content-length` and
`location` are ALL value-compared EXACTLY.** A wrong code, a wrong encoding, a
missing header or a spurious one each go RED.

Two properties to design around: the name comparison is **set-based, not
multiset-based**, so a DUPLICATED `grpc-status` would be invisible and only the
FIRST occurrence's value is compared; and response header-name CASE is
normalised away, so `Grpc-Status` vs `grpc-status` passes. **Header ORDER is
never read** — see §1.4.

### §2.4 The body rule

`Http1BodyRule` (`:1062-1079`) has exactly ONE variant,
**`ByteExact { body: String }`**, spelled
`expected_body: { kind: byte_exact, body: "..." }`. The empty body is expressed
as `body: ""` (fixture `0086` does this throughout).

⚠ The field is a `String`, deserialized from YAML text — **a non-UTF-8 or
gRPC-framed body is NOT expressible today.** That is fine here: every cell in
this fixture asserts an EMPTY body on the gRPC side. If a future slice needs a
framed body, this enum must be extended.

### §2.5 The template and the registration mechanism

- **Template: fixture `0088-runtime-fraction-route-gating`.** `clusters: []`
  (`envoy.yaml:109`), `envoy.yaml` and `envoy-rust.yaml` **BYTE-IDENTICAL**
  (md5 `d205936b0390260855f19258dd02f51a`, **6006 bytes** each — non-empty, so
  the uniform hash is real byte-identity and not the empty-file md5), sole
  template token `{{PORT}}` at line 5 of each. Four files:
  `envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml`, `README.md`.
- **Two shape rules inherited from `0088`, both load-bearing.**
  (i) **No `node:` block** — upstream parses YAML 1.1 and booleanizes the
  unquoted `y` in `cluster: y`, rejecting the config; omitting `node:` entirely
  is what lets both proxies accept ONE file and makes the two YAMLs
  byte-identical. (ii) **`admin.socket_address.port_value` is a LITERAL `0`,
  not `{{ADMIN_PORT}}`** — the `{{ADMIN_PORT}}` substitution is DRIVER-GATED
  and `Http1ProbeList` is NOT among the four drivers that get it, so a literal
  `{{ADMIN_PORT}}` would reach the parser untouched and fail as an address.
  `{{PORT}}` IS substituted for this driver.
- **Registration is cargo auto-discovery.** `tests/differential/Cargo.toml` has
  NO `[[test]]` sections, so every `.rs` directly under
  `tests/differential/tests/` becomes its own test binary. There are **88** such
  files, 1:1 with the 88 fixture directories. Naming: the fixture directory
  name with the `NNNN-` prefix stripped and hyphens → underscores.
- **Exactly what must be created**: `tests/fixtures/0089-<slug>/envoy.yaml`,
  `…/envoy-rust.yaml`, `…/expectations.yaml` (all three names are hard-coded in
  `run_fixture`), a `tests/differential/tests/<slug_underscored>.rs` entrypoint
  (~10 executable lines under a dense `//!` doc block, the house style), and a
  `README.md` (convention — 85 of 88 fixtures have one; no code reads it).
  **No `Cargo.toml` edit, no registry list, no macro.**
- **Fixture numbering**: `git ls-files 'tests/fixtures/**' | cut -d/ -f3 |
  sort -u | wc -l` = **88**, highest `0088`, no `0089` exists → this fixture is
  **`0089`**. (The naive `git ls-files 'tests/fixtures/*/'` is a vacuous glob
  returning a clean-looking ZERO — do not use it.)

## §3. Scope — the fixture's shape

**F1 — `0089-grpc-aware-local-replies`**, cloned from `0088`'s shape:
cluster-free, backend-free, `clusters: []`, no `node:` block, admin
`port_value: 0`, sole token `{{PORT}}`, `envoy.yaml` ≡ `envoy-rust.yaml`
BYTE-IDENTICAL, `kind: http1_probe_list`, `codec_type: HTTP1`.

**F2 — the route table.** One HCM listener; a `direct_response` route per cell,
**each with its OWN distinct path** (the `BEHAVIOR_CONTRACT.md` §G
one-path-per-probe attribution rule) and its own distinct body; plus one
`redirect:` route. **⚠ NO route may use a `201` or `3xx` `direct_response`
status** — upstream emits a `location` header on those and envoy-rust does not
(CF-110-3, `110.1/SPEC.md` §5 non-goal 9), which would RED the name-set check
for a reason unrelated to gRPC. The `redirect:` route is safe: envoy-rust's
`synth_redirect` already emits `location`.

**F3 — the probe set**, one per equivalence class, all deterministic:

- **Mapping**: the eight special statuses (`400`, `401`, `403`, `404`, `429`,
  `502`, `503`, `504`) plus at least two default-arm witnesses drawn from the
  counter-intuitive set (`500` and `405` are the sharpest; `200` covers the 2xx
  arm).
- **Paired non-gRPC controls** on a subset, proving the transform does NOT fire
  and pinning the untransformed status and body.
- **Detection edges**: `application/grpc+proto` positive;
  `application/grpc; charset=utf-8`, `APPLICATION/GRPC`,
  `application/grpc-web` and `application/grpcfoo` negative. (Case preservation
  is proven in §2.2, so the `APPLICATION/GRPC` cell is a real witness.)
- **Method insensitivity**: one `post` probe with a gRPC content-type.
- **Empty body**: one probe proving `grpc-message` is **ABSENT ENTIRELY**, not
  empty — the name-set check is what catches a spurious empty header.
- **Encoding**: one probe whose route body is the §1.3 string, asserting the
  exact `grpc-message` value. Optionally a second carrying `~` and `%25` — the
  two cells the parent SPEC's rule got wrong.
- **Redirect**: the `redirect:` route with and without a gRPC content-type,
  proving `location` survives alongside `grpc-status: 2`.

Every gRPC probe sets `expected_status: 200`,
`expected_body: { kind: byte_exact, body: "" }` and
`expected_headers: set_equal_modulo_allow_list`.

**F4 — `BEHAVIOR_CONTRACT.md` gains a `## gRPC` section** recording §1.1–§1.4
as the canonical contract, including the MEASURED header order and the explicit
note that the harness does not compare order.

**F5 — parent close.** ROADMAP rows `110.2` **and parent `110`** both flip
`done` in the same close-out, per the `76.2` / `108.2` / `109.2` precedent.

**F6 — proof the fixture is not vacuous.** At least two in-place mutations,
each reverted byte-exactly and md5-verified, per the `0088` precedent — for
example flipping one mapped code in the expectations, and turning one detection
NEGATIVE cell positive. ⚠ The probe-list driver **ABORTS AT THE FIRST FAILING
PROBE**, so one red run names exactly ONE probe; never infer a second cell's
state from a single red run.

## §4. Differential surface at phase end

- NEW fixture `0089-grpc-aware-local-replies` green cross-proxy
  (`http1_probe_list`, backend-free — **locally runnable on this development
  host**, unlike backend-routing fixtures which RED on the `192.168.65.2`
  bridge and are CI-authoritative).
- All 88 pre-existing fixtures still green.
- The CI identity `binaries=165 passed=2194 failed=0` moves by exactly this
  sub-phase's new test binary and `110.1`'s new tests — any OTHER movement is a
  signal.
- No conformance change: h2spec threshold untouched, `known-failures.txt`
  untouched at 21 lines / ONE real entry. No `tests/conformance/grpc/` is
  created — interop conformance needs a data path first.

## §5. Non-goals

Identical to `110.1/SPEC.md` §5 and unchanged by this sub-phase: no HTTP/2
(CF-110-1), no trailers of any kind, no proxied-response transform (CF-110-2),
no gRPC-Web / bridge / JSON-transcoding / `grpc_stats`, no
`grpc_status_filter`, no `fault.grpc_status`, no new config surface, and no fix
for the `location`-on-`direct_response` divergence (CF-110-3) — which this
sub-phase must merely AVOID tripping over, per F2.

Additionally: **no crate source change is planned here.** If the fixture
surfaces a divergence from `110.1`'s implementation, that divergence is this
sub-phase's finding and its fix lands here; but the slice is not a licence to
widen the behaviour.

## §6. PLAN-VERIFY items — re-confirm FRESH at this sub-phase's state-2

- **X-1** — re-run the §1.1 and §1.2 cells that the fixture will transcribe,
  against the pinned image, with the digest verified BEFORE probing. Do not
  transcribe an inherited cell.
- **X-2** — **dry-run the EXACT `0089` YAML end-to-end against BOTH proxies**
  before freezing the expectations (the `108.2`/`109.2` precedent: a dry-run is
  itself a CLAIM state 3 re-establishes, but skipping it is how fixtures land
  RED).
- **X-3** — re-derive that `envoy.yaml` and `envoy-rust.yaml` can be
  BYTE-IDENTICAL for this shape (assert BOTH the md5 AND the byte count — a
  uniform md5 can be the empty-file md5).
- **X-4** — re-derive the fixture census (88, highest `0088`) and confirm no
  `0089` exists.
- **X-5** — re-confirm the §2 harness facts on disk: the 3-entry
  `HEADER_ALLOW_LIST`, `Http1BodyRule::ByteExact { body: String }` as the only
  variant, `Http1Method`'s three variants, and that `drive_http1` still
  interpolates `extra_headers` raw (§2.2) — the whole case-sensitivity cell
  rests on that.
- **X-6** — re-confirm CF-110-3 (`location` on a `201`/`3xx`
  `direct_response`) still holds, and that no chosen fixture cell trips it.
- **X-7** — audit a suspiciously fast green: a backend-free fixture completing
  in ~1–3 s is NORMAL, but prove the containers really ran with a `docker ps`
  poll using a VALID format field (an invalid `{{.ImageID}}` turns every poll
  line into a template error that reads as "no containers ran") plus a negative
  control.
- **X-8** — rebuild the DEBUG `envoy-bin` before running the fixture; the
  harness runs the debug binary and a stale one fails with `unknown field`
  errors that look like real divergences.

## §7. Size estimate

Bottom-up, docs-excluded, `added − deleted`. `BEHAVIOR_CONTRACT.md` lives under
`docs/` and is EXCLUDED from the gate metric.

| bucket | estimate |
|---|---|
| `envoy.yaml` + `envoy-rust.yaml` (byte-identical, ~20 routes) | ≈ 260 |
| `expectations.yaml` (~26–30 probes plus the header comment) | ≈ 290 |
| `README.md` | ≈ 110 |
| `tests/differential/tests/<slug>.rs` entrypoint | ≈ 40 |
| **Total** | **≈ 700 (range 620–780)** |

Comfortably under the ~1500 gate. Calibrated against the three nearest landed
fixtures measured at the split session — `0087` = **316**, `0086` = **395**,
`0088` = **487** data lines, with entrypoints of 18 / 47 / 40 — this fixture
sits above all three because it carries roughly three times `0088`'s probe
count.

## §8. Definition of done — the §7.5 gate, instantiated

- (a) Fixture `0089` green cross-proxy on EVERY probe.
- (b) All 88 pre-existing fixtures green.
- (c) Conformance unchanged (§4).
- (d) **No new fuzz target** — no parser, codec, filter or config surface.
- (e) `cargo build --workspace --all-targets`, `cargo clippy --workspace
  --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`,
  `cargo test --workspace` and `cargo deny check` all clean at WORKSPACE scope.
- (f) `REVIEW.md` APPROVED.
- Plus: ROADMAP rows `110.2` **and parent `110`** both flip `done` at the
  close-out.

## §9. Carry-forwards

Unchanged from `110.1/SPEC.md` §9: **CF-110-1** (H2, shape MEASURED),
**CF-110-2** (proxied responses), **CF-110-3** (`location` on a `201`/`3xx`
`direct_response`), plus CF-109-1 (WIDENED)/2/3, CF-108-1/2/3, CF-76-1,
CF-75-2/3/4/5/6, CF-72-2/CF-75-1, M71-6, CF-74-1/2/3/4/6, CF-73-1, the `109.2`
REVIEW's M-1…M-8 + N-1…N-11, the `109.1` M-5 + N-1…N-6 set, the `108.2` M-2 +
N-1…N-6 set and the HTTP-filters-family (1)-(4). None is fixed here (§6.3;
ADR-0165).

## §10. Next state

This sub-phase sits at §5 **state 1 complete** (`SPEC.md` exists, no
`PLAN.md`), and is **BLOCKED until `110.1` is `done`**. `STATE.md` points at
`110.1`, not here. When `110.1` closes, the next session runs §5 state 2
(`superpowers:writing-plans`) for `110.2`, re-confirming X-1…X-8 fresh.
