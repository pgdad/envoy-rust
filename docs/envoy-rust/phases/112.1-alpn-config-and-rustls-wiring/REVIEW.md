# Sub-phase 112.1 — ALPN config surface + `rustls` wiring on both sides — CODE REVIEW

> **§5 state 5.** This document is the state-5 output for `112.1` and it **CLOSES §7.5 gate (f)**,
> the only gate the state-4 session left open. Written by a THIRD context: the state-3 session
> implemented, the state-4 session graded, and neither may review (§5.1; `ADR-0127`).
>
> **Verdict: APPROVED.** §2 (Issues — Must Fix) is **EMPTY**, so the state machine advances to
> **state 6**, not back to state 3.
>
> **This review wrote no code, no test and no fixture.** The CI identity must therefore stay at
> `binaries=167 passed=2265 failed=0`. Every finding below is **BANKED** as a carry-forward, not
> fixed (§6.3; `ADR-0165`: a phase banks, it never clears — and a REVIEW banks its own findings too).
>
> **One fresh measurement contradicts three landed artifacts, so `ADR-0188` fires** (see M-1).

---

## §0 — How this review was conducted

### §0.1 — Scope

The review surface is the **549 net code lines** between base `3a2cf93e40b653d33bacbf5504206a1d5a5c0142`
(the parent of Task 1) and HEAD `8518d4ddd147c5c13e8161d750f94eaede8e3552`. Re-derived at this
session with `git diff --numstat <base> HEAD -- . ':(exclude)docs/**'`:

| file | + | − | net | claimed |
|---|---|---|---|---|
| `crates/envoy-config/src/bootstrap.rs` | 228 | 9 | **219** | 219 ✓ |
| `crates/envoy-tls/src/tests.rs` | 156 | 0 | **156** | 156 ✓ |
| `crates/envoy-tls/src/lib.rs` | 122 | 6 | **116** | 116 ✓ |
| `crates/envoy-config/fuzz/` (2 files) | 44 | 0 | **44** | 44 ✓ |
| `crates/envoy-config/src/lib.rs` | 13 | 0 | **13** | 13 ✓ |
| `crates/envoy-tcp/src/lib.rs` | 1 | 0 | **1** | 1 ✓ |
| **TOTAL** | **564** | **15** | **549** | **549 ✓** |

All six rows and the total match `PROGRESS.md` and `ADR-0186` exactly. **The range is stated because
a numstat citation goes stale at the carrying commit**: `<base> HEAD` is correct only while the
`':(exclude)docs/**'` filter is applied, since three docs-only commits have landed since `c86afd5`.

Out of scope by design and deliberately not reviewed: `tests/` and any differential fixture (sibling
`112.2`), `ROADMAP.md` (state 6), and every landed artifact (`SPEC.md`, `PLAN.md`, and the §5 state-3
and state-4 sections of `PROGRESS.md` are UNEDITABLE).

### §0.2 — Method

Five read-only reviewers were fanned out over the partition the phase-111 review established, minus
its fixture slice, which is empty here: **(1)** the D6′ accept-path rewrite, **(2)** the config
layer, **(3)** the test suite, **(4)** the fuzz seed + gitignore, **(5)** artifact consistency. Each
was given full zero-context instructions (D-3.4), forbidden to write or to run `cargo` (the workspace
lock serializes), told that `~/.cargo/registry/src/` is readable evidence **with the pin resolved
from `Cargo.lock` first**, and required to run a positive control before reporting any zero.

**Every subagent finding was re-verified on disk by this session, and the ones that did not survive
were downgraded.** §5 records each dissent. The main session additionally ran the one measurement
none of them could: a networking-free `--mode validate` probe against the pinned upstream image,
which is what turned the review's largest finding from a suspicion into a fact.

**Pins resolved from `Cargo.lock` first**, and stated because this host's registry cache holds
several versions and the `rustls-*` glob also matches `rustls-native-certs`, `rustls-pemfile`,
`rustls-pki-types` and `rustls-webpki`: **`rustls 0.23.39`** (`Cargo.lock:1672-1673`) and
**`tokio-rustls 0.26.4`** (`:2204-2205`), read at
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`.

### §0.3 — The §7.5 gate was NOT re-run

Re-running it is state 4's job and it is done. This review takes the state-4 record as given and adds
gate (f) only. What this session DID re-derive independently, because a landed figure is a claim:

- **The net-549 table above** — all six rows and the total, at this commit.
- **`ADR-0187`'s `+13` CI-identity delta, by an INDEPENDENT source census** rather than by re-reading
  the CI log. Counting `#\[(tokio::)?test` **without a closing bracket** across each crate's `src/`
  at base and at HEAD: `envoy-config` **710 → 717 (+7)**, `envoy-tls` **16 → 22 (+6)**, total
  **+13** — which is exactly `ADR-0187`'s correction and NOT the `+14` that `PLAN.md` M-R13,
  `ADR-0185` and `ADR-0186` shared. **`ADR-0187` survives independent re-derivation.**
  Two method notes: the source census reads exactly **one high** on `envoy-config` in both directions
  (709/716 at runtime), and the extra match is located — `bootstrap.rs:20038` is a COMMENT containing
  the literal `` `#[test]` ``; because it is present at both ends it cancels out of the delta.
  The documented trap reproduces: the bracket-anchored form `#\[test\]` reads **1** against **22**
  on `crates/envoy-tls/src/tests.rs`.
- **The `112.1` artifacts contain no fabricated SHA.** Seven distinct 40-char hex strings appear
  across the phase directory and `STATE.md`; **six resolve** under `git cat-file -e`
  (`2a9712b3…`, `3a2cf93e…`, `6b7aba0d…`, `9f2010a5…`, `c86afd54…`, `d94922be…`). The seventh,
  `56da5afd7df364350ff92de4fb49a9b09957c172`, is **not a git SHA and not a defect** — it is the
  first 40 characters of the 64-character Docker digest
  `sha256:56da5afd…c28770c2` from `ENVOY_TARGET.md`, truncated by the audit regex itself.
  **Recorded because it is exactly the believable near-miss the audit is supposed to survive**: a
  `MISSING` line that reads as a fabrication and is in fact an artifact of the method. The genuinely
  fabricated SHA this audit exists for was in `STATE.md`'s CI-record line and was already corrected
  by `8518d4d` before this session began.


---

## §1 — Strengths

**The D6′ accept path is correct, and the ALPN-free twin is a genuine twin.** This was the one piece
of real engineering in the sub-phase and it holds up under the closest reading the review could give
it. `finish_server_config` (`crates/envoy-tls/src/lib.rs:83-100`) clones the builder's output
**before** `alpn_protocols` is assigned (`:97-98`), so the twin is not a clone-and-clear that could
drift — it is the same object with one field never written. Enumerating every field
`ConfigBuilder::with_cert_resolver` sets (`rustls-0.23.39/src/server/builder.rs:103-127`): the
non-`Copy` fields — `provider`, `verifier`, `cert_resolver`, `session_storage`, `ticketer`,
`key_log`, `time_provider`, `cert_compression_cache`, `cert_compressors`, `cert_decompressors` — are
all `Arc`s or `&'static`, so the twin **shares the same objects**, not equivalents; and the scalars
(`max_early_data_size: 0`, `send_half_rtt_data: false`, `send_tls13_tickets: 2`,
`enable_secret_extraction: false`, `max_fragment_size: None`, `ignore_client_order: false`) are
identical by construction. `alpn_protocols` is the **only** differing field. rustls' own
"sharing resumption storage between `ServerConfig`s" warning (`server_conn.rs:269-292`) names exactly
two fields to check, `verifier` and `cert_resolver`, and both are the *same* `Arc` here.

**The escape hatch was located in the dependency, not asserted about it.** `NoApplicationProtocol` is
produced in exactly one place — `ExtensionProcessing::process_common`,
`rustls-0.23.39/src/server/hs.rs:113-118` — whose only two call sites (`server/tls13.rs:679`,
`server/tls12.rs:336`) are both reached from `Accepted::into_connection`
(`server_conn.rs:1046`), **never** from `Acceptor::accept` (`server_conn.rs:861-889`), which runs
only `hs::process_client_hello` and never touches a `ServerConfig`. That asymmetry is the entire
reason D6′ is expressible at zero dependency cost, and `SPEC.md` §2.1 identified it correctly.

**The comparison is byte-correct.** `ClientHello::alpn()` returns
`Option<impl Iterator<Item = &'a [u8]>>` (`rustls-0.23.39/src/server/server_conn.rs:187-193`) —
bytes, not `&str`, as the SPEC said. `lib.rs:90-93` builds the wire list with `p.as_bytes().to_vec()`
and `lib.rs:262` compares `&[u8]` to `&[u8]`. **No lossy UTF-8 conversion, no case folding, no
trimming.** The intersection test is logically identical to rustls' own `find`/`any` at
`hs.rs:101-104`, and `self.alpn == self.config.alpn_protocols` holds by construction.

**The borrow shape is right and self-documenting.** `advertise` is reduced to an owned `bool` inside
a block (`lib.rs:255-273`) so `hello` and the collected `Vec<&[u8]>` are dead before
`into_stream(self, …)` consumes the `StartHandshake`. Under NLL the explicit block is technically
redundant, but it makes the `E0505` hazard visible to the next reader, which is worth the two braces.

**D6′.1's safety argument for the 90 pre-existing fixtures is a design property, and it was verified
empirically rather than trusted.** `git ls-files '*.yaml' '*.yml' | xargs grep -n '^ *alpn_protocols:'`
returns **exactly one** hit repo-wide, and it is the new fuzz corpus seed — no runtime config
anywhere configures ALPN (positive control, identical method: `^ *tls_certificates:` returns 9
files). So every one of the 90 fixtures has an empty list, `alpn_free_config` is `None`, and
`accept()` takes the `TlsAcceptor` route at `lib.rs:228-234` that is byte-identical to the
pre-change code. Gate (b) is not passing by luck.

**The ALPN matrix is complete and every cell is right.** Traced through the pinned rustls source:

| server list | client offers | path taken | outcome |
|---|---|---|---|
| empty | nothing | `TlsAcceptor`, unchanged | block skipped; nothing selected, no alert |
| empty | `["h2"]` | `TlsAcceptor`, unchanged | `our_protocols` empty ⇒ alert branch not taken. Envoy parity for free |
| `["h2"]` | nothing | `LazyConfigAcceptor`, `advertise = true` | `hello.protocols == None` ⇒ block skipped; nothing selected, no alert |
| `["h2","http/1.1"]` | `["http/1.1","h2"]` | `LazyConfigAcceptor`, `advertise = true` | selected by **server** preference (`hs.rs:101-104` iterates ours) ⇒ `h2` |
| `["h2"]` | `["spdy/3"]` | `LazyConfigAcceptor`, `advertise = false` ⇒ twin | **handshake completes, nothing selected, no alert. This is D6′, and it works.** |
| `["h2"]` | an *empty* ALPN extension | never reaches the peek | rustls rejects at decode (`IllegalEmptyList("ProtocolNames")`, `handshake.rs:415-416`) — same disposition as the old path |

**The mutation proof is the strongest part of the state-3 record, and it is method-complete.** All
five rules were applied to all four mutations (`PROGRESS.md` Task 8): the target was asserted to
occur **exactly once**, a rebuild was forced and confirmed, the verdict gates on the `test result`
line's existence rather than the exit code, an unmutated control was taken from the same tree **both
before and after**, and everything ran in a scratch worktree. Every mutation reddened **exactly** the
tests asserting the deleted behaviour and no others — which is the outcome that proves the mutations
were aimed correctly, not merely that something went red.

**D6′ is genuinely observed end to end, not asserted.**
`alpn_mismatch_completes_handshake_with_no_protocol` (`crates/envoy-tls/src/tests.rs:1203`) drives a
real loopback handshake with disagreeing lists and asserts the client `connect` returns **`Ok`** —
which is a sound pin on "no alert", because a fatal `no_application_protocol` would make it `Err` —
plus `None` on both sides. No test in the set settles for asserting `ServerConfig.alpn_protocols ==
vec![…]`. The helpers do not weaken this: `ds_context_with_alpn` (`:1115`) builds a real
`envoy_config::DownstreamTlsContext` and `alpn_handshake` (`:1125`) feeds it to the **production**
`DownstreamTls::from_context` and `DownstreamTls::accept`; nothing is hand-rolled on the server side.

**The reject tests are structural, and the rename strengthened one of them.** All three
`InvalidAlpnProtocol` assertions use `matches!(err, ConfigError::InvalidAlpnProtocol { side, index,
len })` with all three fields bound to literals, and they discriminate both `side` ("listener" vs
"cluster") and `index` (0 vs 1), so a hardcoded-field mutant dies. **There are zero
`contains(...)` assertions in the new tests** — the column-0 YAML-key trap does not apply — and the
Task-1 rename actually *replaced* a weak `msg.contains("alpn_protocols") || msg.contains("unknown
field")` with an exact `assert_eq`. The boundary is doubly pinned: 255 must pass, 256 must fail, so
both `>=255` and `>256` die.

**The fuzz seed is real, tracked, and non-vacuous.** `git ls-files` prints the path and its blob
matches the diff; the `.gitignore` negation at `crates/envoy-config/fuzz/.gitignore:68` is a
**literal filename**, and `grep -c '^!.*\*'` over that file is **0** against a positive control of
67 `^!` lines — so no glob hole was opened, the ~13k untracked generated corpus files stay ignored,
and `git ls-files 'crates/*/fuzz/artifacts/*'` is 0. The seed differs from the pre-existing
`tls_downstream_single_cert.yaml` by exactly two hunks, so the only new risk surface is the ALPN
block, and both of its elements (`h2`, `http/1.1`, 2 and 8 bytes) drive
`validate_alpn_protocols`'s loop body at `bootstrap.rs:3793`. The ledger's "no `ci.yml` step was
owed" is **correct**: `.github/workflows/ci.yml:77-134` enumerates its five targets by hand with no
discovery loop, and `parse_bootstrap` is among them, so the seed rides the existing step.

**No parses-then-silently-ignores state exists** (`ADR-0049`, `ADR-0176`). There are exactly three
non-test rustls config builders in the workspace — `crates/envoy-tls/src/lib.rs:118`, `:206`, `:364`
— and **all three read `alpn_protocols`**. `envoy-bin/src/main.rs:921` builds an `UpstreamTls` for
every `all_clusters()` entry, static and dynamic alike. Both halves of D2 really did land together.

**Zero flakiness risk in the new tests.** Both new loopback sites bind `("127.0.0.1", 0)`; every
`tokio::spawn` handle is awaited; there is no `sleep`, no fixed port, no host-dependent `unwrap`;
readiness is structural. None of this host's known startup-race or port-reuse flake shapes applies.

**`BEHAVIOR_CONTRACT.md` is correctly untouched and carries no claim this change falsifies.**
It was not modified in the range, and it contains **zero** occurrences of `alpn`, `TlsAcceptor`,
`accept path` or `DownstreamTls`. The ALPN contract section belongs to `112.2` per `ADR-0184`, and
the phase correctly did not pre-empt it. This slice returned no finding.

---

## §2 — Issues (Must Fix)

**NONE.**

Stated plainly, because it is the load-bearing sentence of the review: an Issue here would send the
work back to §5 **state 3**, not state 4.

Every deliverable in `SPEC.md` §1 landed and works. The field exists on the one shared struct; it is
honored on both the `ServerConfig` and the `ClientConfig` side in the same landing; absent and empty
both mean "do not advertise"; the validator rejects at the declared boundary; and D6′ — the one cell
that would otherwise have been a claim — completes the handshake with nothing selected and no alert,
proven by a real handshake and by a mutation whose RED set is exactly one test.

**Four findings below are graded Important and every one is BANKED, not fixed** (§6.3; `ADR-0165`).
None of them makes a `112.1` deliverable un-done; each either bounds a claim the artifacts state too
broadly, or names a reachable input the slice does not yet handle. The precedent is exact: the
phase-111 review banked 28 findings and the phase closed at state 6.

**One of the four is a MEASURED divergence that falsifies a landed sentence, so `ADR-0188` fires.**
That is a reconciliation ADR, not a re-scoping: it changes no deliverable and no §6.1 verdict.


---

## §3 — Important

### M-1 — the `>255` rule is per comma-separated SEGMENT upstream and per ELEMENT here. **MEASURED at this review.** The landed sentence "the two reject sets therefore coincide" is FALSE, and this is a REJECT-direction divergence — the exact class D4′ was created to avoid

**This is the review's largest finding, and it is a measurement, not a reading.**

`crates/envoy-config/src/bootstrap.rs:5958` applies `proto.len() > 255` to each **list element**.
`crates/envoy-config/src/lib.rs:87` states as fact: *"The two reject sets therefore coincide."*
`112.1/SPEC.md` §3 D4′ says the same, and `ADR-0184`'s PV-3 records the measurement it rests on.

None of the four ADRs, the SPEC, the PLAN or the PROGRESS considers a **comma**. Confirmed by census:
`grep -rni 'comma' docs/envoy-rust/phases/112*/` returns zero ALPN-related hits (every match is the
substring inside "command"), against a positive control of 3 files matching `alpn`.

**The probe.** `--mode validate` against the `ENVOY_TARGET.md` pin, run at this session. It is
**networking-free**, so it needed no port and could not collide with the foreign workstream's
18080-18090 / 18443-18463 reservations. Image provenance asserted before use:
`docker image inspect` reports `RepoDigests[0]` =
`envoyproxy/envoy@sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`, matching
`ENVOY_TARGET.md:9` exactly. The config carries a cluster-side `UpstreamTlsContext` with a
`validation_context.trusted_ca` pointing at the image's own CA bundle, which avoids needing a private
key. **The probe was proved non-vacuous first**: the identical config with `alpn_protocolz` is
rejected with `no such field: 'alpn_protocolz'` at
`…common_tls_context: message envoy.extensions.transport_sockets.tls.v3.CommonTlsContext`, so the
validator genuinely parses this exact struct.

| `alpn_protocols` element | length | comma segments | upstream Envoy v1.33.0 | envoy-rust |
|---|---|---|---|---|
| `["h2","http/1.1"]` | 2, 8 | — | exit 0, `configuration OK` | accepts |
| `"a"×255` | 255 | [255] | exit 0, **accepted** | accepts |
| `"a"×256` | 256 | [256] | exit 1, **`Invalid ALPN protocol string`** | rejects |
| `"a"×150 + "," + "b"×149` | **300** | [150, 149] | exit 0, **ACCEPTED** | **REJECTS** |
| `"a"×255 + "," + "b"×255` | **511** | [255, 255] | exit 0, **ACCEPTED** | **REJECTS** |
| `("c"×200) × 3, comma-joined` | **602** | [200, 200, 200] | exit 0, **ACCEPTED** | **REJECTS** |
| `"a"×256 + ",h2"` | 259 | [**256**, 2] | exit 1, **`Invalid ALPN protocol string`** | rejects |
| `"h2," + "a"×256` | 259 | [2, **256**] | exit 1, **`Invalid ALPN protocol string`** | rejects |

The last two are the decisive pair. A **259**-byte element is REJECTED when one of its segments is
256, while a **602**-byte element is ACCEPTED when every segment is ≤255. **The bound is applied per
comma-separated segment, and total element length is irrelevant.** The 255/256 rows reproduce
`SPEC.md` §2.3 exactly, so the probe agrees with the landed measurement everywhere the landed
measurement looked — it simply looked at no string containing a comma.

**Consequence 1 — a measured reject-direction divergence, live today.**
`alpn_protocols: ["<255 a's>,<255 b's>"]` boots upstream Envoy and is rejected by envoy-rust with
`ConfigError::InvalidAlpnProtocol { side, index: 0, len: 511 }`. `SPEC.md` §3 D4′ chose the >255 rule
in order to make the reject sets coincide, on the reasoning that *"rejecting where upstream accepts
would manufacture a reject-direction divergence this phase does not need."* That divergence exists
anyway, on a config shape nobody thought to probe.

**Consequence 2 — an accept-direction divergence, inferred and NOT yet measured on the wire.** If the
segment is the unit of the length check, the segment is almost certainly the unit of the wire
encoding too, which means `alpn_protocols: ["h2,http/1.1"]` — accepted by both proxies — offers
**two** protocols upstream and **one** 11-byte protocol here. `SPEC.md` §2.3's independently measured
oddity is consistent with this and hard to explain without it: a server list of `["", "h2"]` makes
upstream negotiate **nothing at all, not even `h2`**, which is exactly what a comma-joined buffer
beginning with a zero-length segment would produce. **I am labelling this INFERRED, not measured:**
the validate-level probe proves only that the *length check* is per-segment. Proving the wire claim
needs a handshake, which is `112.2`'s harness, not this review's.

**Grade rationale.** Important, not Critical: no crash, no security consequence, and the divergent
configs are unusual. But it falsifies a sentence three landed artifacts assert as fact, and
Consequence 2 lands squarely on the surface `112.2` is about to witness — so it is much cheaper to
know now than after `112.2`'s fixture is written.

**Banked as CF-112-8. `ADR-0188` fires** to record the measurement and to correct
`crates/envoy-config/src/lib.rs:87`, `SPEC.md` §3 D4′ and `ADR-0184` PV-3 as **incomplete** (their
positive findings all stand; only the "reject sets coincide" generalisation fails). **Not fixed
here** (§6.3; `ADR-0165`), and deliberately so: the right fix is to split on `,` before applying the
bound, and it should land with the wire measurement that confirms Consequence 2, not ahead of it.

---

### M-2 — an empty `alpn_protocols` element, which D4′ DELIBERATELY accepts, makes the upstream TLS connect hit a `debug_assert!` inside `rustls` — a panic in every debug and test build

`crates/envoy-tls/src/lib.rs:369-374` (the assignment) and `crates/envoy-config/src/bootstrap.rs:5958`
(the validator that lets it through).

**Concrete input:** a cluster whose `UpstreamTlsContext.common_tls_context.alpn_protocols` is
`["h2", ""]` — precisely the shape blessed by `accepts_empty_and_duplicate_alpn_elements`
(`bootstrap.rs:9027`), because `validate_alpn_protocols` rejects only `len > 255` and D4′ accepts the
empty element on measured upstream parity.

**The chain, every hop located in the pinned `rustls 0.23.39` source:**

1. `crates/envoy-tls/src/lib.rs:369` copies the list verbatim into `ClientConfig::alpn_protocols`.
2. On the first upstream connect: `ClientConnection::new` → `ClientExtensionsInput::from_alpn`
   (`rustls-0.23.39/src/msgs/handshake.rs:783-791`), which maps **every** entry through
   `ProtocolName::from`.
3. `ProtocolName` is `wrapped_payload!(… PayloadU8<NonEmpty>)` (`handshake.rs:394-396`), whose
   `From<Vec<u8>>` (`handshake.rs:48-52`) calls `PayloadU8::new`.
4. `PayloadU8::new` is `debug_assert!(bytes.len() >= C::MIN)` (`msgs/base.rs:169-172`) with
   `NonEmpty::MIN = 1` (`base.rs:222-227`). **`debug_assert!(0 >= 1)` ⇒ panic.**

`Cargo.toml` declares `[profile.release]` only — there is **no `[profile.dev]` override** — so
`debug-assertions` is ON for `cargo test` and for every debug build, **which is what the differential
harness runs**. In release the assertion compiles out and `PayloadU8::encode` (`base.rs:181-186`)
instead emits a zero-length `ProtocolName`, malformed against RFC 7301's
`opaque ProtocolName<1..2^8-1>` — rustls itself rejects such a name on decode with
`IllegalEmptyValue` (`base.rs:188-191`), so a rustls peer would fail the handshake.

**The downstream side is safe, and safe for a reason worth recording rather than by luck of testing:**
`finish_server_config` never converts to a `ProtocolName`, and an empty server entry can never be
*selected*, because client-offered names are decoded through the same `NonEmpty` bound and a
zero-length offer is rejected at decode. So `hs.rs:107`'s `ProtocolName::from` is never reached with
an empty name. The empty downstream element is merely inert.

**Why this is not already banked.** CF-112-6 (`SPEC.md` §6) banks that envoy-rust's runtime behaviour
under an empty element is *"not specified and not tested — rustls would place a zero-length name in
the extension."* That sentence predicts the **release** consequence and is correct about it. It does
not predict a panic, and a panic on a config the validator deliberately accepts is a different class
of defect from a malformed extension. §8 lists this cell as NOT MEASURED; this review measured it by
reading the pinned source, and the answer is worse than the SPEC assumed.

**Grade rationale.** Important, not Critical: it needs an odd config (`""` as a protocol name), the
panic is confined to the connection task rather than the process, and release builds do not panic at
all. But it is reachable from configuration alone, on a path the phase's own validator was written to
permit. **Banked as CF-112-9, refining CF-112-6.** The fix is one line — skip empty elements when
building `ClientConfig::alpn_protocols` — but it interacts with M-1's comma question and with
CF-112-6's "one empty element poisons the whole list" upstream behaviour, so it should be decided
together with them, not patched in isolation.

---

### M-3 — `DownstreamTls::from_listener`'s entire ALPN plumbing is untested; the mutant that deletes it survives the whole suite. It is the production constructor for every multi-chain and SNI listener

`crates/envoy-tls/src/lib.rs:156`, `:168-183` and `:210`.

**Surviving mutant, concrete:** replace `finish_server_config(config, alpn_protocols.unwrap_or(&[]))`
at `lib.rs:210` with `finish_server_config(config, &[])`. All 6 new `envoy-tls` tests and all 8 new
`envoy-config` tests stay green, as do the 16 pre-existing `envoy-tls` tests. The first-chain-wins
selection at `:168-170` and the CF-112-4 disagreement `tracing::warn!` at `:173-181` are equally
unpinned — swapping first-chain-wins for last-chain-wins, or deleting the warning outright, is
invisible to the suite.

**Verified on disk:** `grep -n 'from_listener' crates/envoy-tls/src/tests.rs` returns exactly one
test, `from_listener_builds_multi_cert_config` (`tests.rs:914`), and it configures no ALPN. No
fixture covers it either — `grep -rn alpn tests/fixtures/` is **0** (positive control: 9 fixtures
match `transport_socket`), and fixtures are `112.2`'s anyway.

**Why it matters more than a coverage nit.** `crates/envoy-bin/src/main.rs:1096-1113` chooses
`from_context` **only** when the listener has exactly one filter chain and no `server_names`;
**every** multi-chain or SNI listener goes through `from_listener`. That is precisely the deployment
shape in which a real operator configures ALPN — terminate several SNIs, offer `h2` on them — so the
untested constructor is the one production ALPN will actually use. Compare the sibling
`from_context`, whose ALPN threading is pinned by four separate tests plus mutation M2.

**Grade rationale.** Important. The code is, on reading, correct; nothing here is a live wrong answer.
But an entire production constructor's handling of the phase's one new field has zero regression
guard, and it is the constructor that matters most. **Banked as CF-112-10.**

---

### M-4 — a CDS-supplied `type: EDS` cluster's `UpstreamTlsContext` reaches NO transport-socket validation at all, so D4′ never runs on that path. Pre-existing and structural, but it bounds the SPEC's D2b claim

`crates/envoy-config/src/bootstrap.rs:4161-4163` and `:3752`.

**The trace, verified on disk:**

- `crates/envoy-config/src/cds.rs:69` runs `validate_cluster` per CDS resource. For a `type: EDS`
  cluster, `load_assignment` is `None` pre-merge, so `validate_cluster` hits
  `let Some(la) = cluster.load_assignment.as_ref() else { return Ok(()) }` at `bootstrap.rs:4161-4163`
  and returns **before** the transport-socket block at `:4180-4211`, where `validate_alpn_protocols`
  lives (`:4188`).
- The EDS pass then populates `load_assignment`, and the post-merge `validate()`
  (`crates/envoy-config/src/lib.rs:1387`) re-runs `validate_cluster` — but its loop at
  `bootstrap.rs:3752` iterates **`bootstrap.static_resources.clusters` only**. The `effective_clusters`
  collection built four lines earlier at `:3745` *does* chain `dynamic_clusters`, and the validation
  loop does not use it.
- There are exactly two non-test call sites of `validate_cluster` (`cds.rs:69` and
  `bootstrap.rs:3753`), so there is no third path that could catch it.

`envoy-bin/src/main.rs:921` then builds an `UpstreamTls` for every `all_clusters()` entry, dynamic
included, and `envoy-tls/src/lib.rs:369` assigns the unvalidated list to `ClientConfig`.

**Consequence:** a CDS `type: EDS` cluster carrying `alpn_protocols: ["<256 a's>"]` is accepted at
boot and reaches rustls, where `PayloadU8::encode`'s `(slice.len() as u8)` (`base.rs:184`) **silently
truncates the length byte to 0**, so every upstream TLS connection fails at handshake instead of the
config being rejected at load. It is the same path that would surface M-2's empty-element panic
without any validator having had a chance to see the config.

**This hole is PRE-EXISTING and 112.1 did not create it.** The same early return equally skips
`EmptyUpstreamSni`, `MissingValidationContext`, `EmptyTlsCertificates` and the direction check on
that path. What 112.1 did is add a new field whose validator inherits the hole while `SPEC.md` §3 D2b
claims the field is *"honored on both sides"* and `lib.rs:87` claims the reject sets coincide — and
on this one path neither statement holds.

**Grade rationale.** Important as a **bound on a landed claim**, explicitly not as a defect this
phase introduced. Fixing it means changing `bootstrap.rs:3752` to iterate `effective_clusters`, which
is a change to shared validation semantics affecting five other error variants and well outside a
review's remit. **Banked as CF-112-11.**

---

### M-5 — eighteen `file:line` citations across landed artifacts were TRUE at base and are FALSE at HEAD; two of them point FORWARD into the unstarted sibling, and one is a DANGLING SYMBOL this phase renamed away

This phase inserted 228 lines into `bootstrap.rs`, 156 into `tests.rs`, 122 into `envoy-tls/src/lib.rs`,
13 into `envoy-config/src/lib.rs` and 27 into `DECISIONS.md`. **A phase invalidates citations it does
not own**, and the full census — every `file:line` across all `.md` under `docs/envoy-rust/` except
the `STATE_HISTORY.md` archive, each re-read at base and at HEAD — finds 18 that this change broke.
No ADR records the breakage; all five of `ADR-0183`…`0187`'s correction lists are silent on it.

| citing artifact | target | claim | at HEAD |
|---|---|---|---|
| `112/SPEC.md:111`, `DECISIONS.md:2548` (ADR-0183) | `bootstrap.rs:8853` | `rejects_unknown_field_in_common_tls_context` | **symbol deleted** (renamed by Task 1) |
| `112/SPEC.md:170`, `:438`, `DECISIONS.md:2522`, `:2566` | `bootstrap.rs:3663` | merged-listener cap | now `:3669` |
| `112/SPEC.md:379`, `112.1/SPEC.md:356`, `112.2/SPEC.md:199`, `DECISIONS.md:2570`, `:2576` | `bootstrap.rs:4267` | `Http2OverTlsNotSupported` | now `:4275` |
| `112/SPEC.md:387` | `bootstrap.rs:4256` | `validate_hcm` codec match | now `:4264` |
| `112.1/SPEC.md:222`, `112.1/PLAN.md:62` | `tests.rs:240` | E0063 construction site | now `:241` |
| `112.1/SPEC.md:223`, `112.1/PLAN.md:63` | `tests.rs:454` | E0063 construction site | now `:456` |
| `112.1/SPEC.md:448` | `tests.rs:339` | `accept_returns_handshake_error_on_garbage_input` | now `:341` (line 339 is blank) |
| `STATE.md:28` | `bootstrap.rs:2923-2926` | `direct_response.body` mandatory | now `:2929` |

**Five were re-verified line-by-line by this session** by reading line *N* of the target at both
`3a2cf93e` and HEAD; all five reproduce exactly. The dangling-symbol case was additionally checked by
name: `rejects_unknown_field_in_common_tls_context` occurs **1** time at base and **0** at HEAD, so
`112/SPEC.md:111`'s *"the rejection is pinned"* evidence row cannot be re-resolved by text either —
which is the failure mode "locate by TEXT, not by line number" does not save you from.

**Two sub-classes carry the weight, and are why the aggregate is Important rather than Minor:**

1. **`112.2/SPEC.md:199` and `DECISIONS.md:2576` point forward into the UNSTARTED sibling.**
   `DECISIONS.md:2576` is **CF-112-1's own definition**, and `112/SPEC.md:438` instructs a future
   session, in so many words, to *"check `bootstrap.rs:3663` first — it may forbid this"* while
   adjudicating the cell-6 fixture shape. **`112.2`'s state-2 session is the next consumer of exactly
   these citations**, and it will follow them into the wrong lines.
2. **`STATE.md:28`** is the Standing-traps line, re-read at every cold start.

Individually most rows are Minor line drift. In aggregate, and given that the next session downstream
is the one they misdirect, they are worth one carry-forward. **The precedent is exact:** the
phase-111 review found that three banked citation breaks were a sample and 29 more existed. **Banked
as CF-112-12**, with the explicit note that the landed artifacts carrying them are UNEDITABLE, so the
remedy is a re-anchoring pass in a future phase, not an edit here.


---

## §4 — Minor

### N-1 — `from_listener`'s first-chain-wins makes ALPN depend on filter-chain ORDER, and the disagreement warning fires in only one of the two directions

`crates/envoy-tls/src/lib.rs:168-183`. The guard is
`if !this.is_empty() && this.as_slice() != first`. Two asymmetric cases follow, and the SPEC and
`ADR-0185` discuss neither:

- **Chain A declares no ALPN, chain B declares `["h2"]`.** Chain A's **empty** list is what seeds
  `alpn_protocols` at `:169`, so `first` is `[]`. Chain B warns and is **dropped**: the listener
  advertises nothing at all, and the `honored = ?first` field of the warning prints `[]`. A
  deployment whose ALPN works or does not work depending on the order two filter chains appear in
  the YAML is a surprising property, and honoring the first **non-empty** list would remove it.
- **Chain A declares `["h2"]`, chain B declares none.** `this.is_empty()` is true, so **no warning
  fires at all** and chain B is silently given `["h2"]` it did not ask for. This is the direction the
  code comment at `:148-155` explicitly promises against — *"warned about rather than silently
  dropped"* — and it is the genuinely silent one.

CF-112-4 already banks that upstream's per-chain-vs-per-listener semantics are unmeasured, so the
divergence itself is owned. What is not owned is the order-dependence and the one-directional
silence. **Minor rather than Important because the honest trade-off cuts both ways** — honoring the
first non-empty list would apply chain B's ALPN to chain A, which is also wrong — and because no
config in the tree has more than one TLS chain with a non-empty list. It is a warning-message and
documentation defect, not a wrong answer. **Note it interacts with M-3: none of this is tested.**
Banked with M-3 under CF-112-10.

### N-2 — D6′ is defeated across a HelloRetryRequest, because the peek reads ClientHello1 and rustls decides on ClientHello2

`crates/envoy-tls/src/lib.rs:251-273`. Verified in the pinned rustls source, both halves:

- The HRR branch in `server/tls13.rs` emits the retry and **returns** a fresh
  `hs::ExpectClientHello { done_retry: true, config }` (visible at `tls13.rs:220-245`) — this is
  **before** `process_common`, whose only TLS 1.3 call site is `tls13.rs:679`. So the ALPN decision
  has not yet been made when the retry is sent.
- CH2 is then processed at `hs.rs:671`, and `process_common` runs against **CH2's** ALPN list, using
  the `ServerConfig` our peek chose from **CH1**.
- rustls pins **only SNI** across a retry — `hs.rs:738-744`,
  `PeerMisbehaved::ServerNameDifferedOnRetry`, guarded on `done_retry`. Grepping `done_retry` across
  `hs.rs` finds no ALPN analogue.

So a client that offers `["h2"]` in CH1 (our peek sees an intersection, hands over the ALPN-carrying
config) and `["spdy/3"]` in CH2 receives the fatal `no_application_protocol` alert D6′ exists to
eliminate. **Minor**: RFC 8446 §4.1.2 forbids a client changing its extensions across a retry, so no
conforming client reaches it, and no differential fixture can express it (the harness drives real
clients). It is recorded because `SPEC.md` states D6′ unqualifiedly at §1 row 6 and §3, and this is
a residual hole in that statement — an ADR line, not code.

### N-3 — the Task-1 rename left `#[serde(deny_unknown_fields)]` on `CommonTlsContext` with no test pinning it

`crates/envoy-config/src/bootstrap.rs:1185` (the attribute) and `:9063` (the renamed test).
`rejects_unknown_field_in_common_tls_context` was the sole in-tree pin on that attribute for this
struct, and inverting it to `accepts_alpn_protocols_in_common_tls_context` left nothing behind.
Verified: no `.rs` or `.yaml` anywhere in the repo places an unknown key under `common_tls_context`
(the sibling `rejects_unknown_field_in_downstream_tls_context` at `:8847` uses
`require_client_certificate` at the **outer** `DownstreamTlsContext` level, so it pins that struct
only). Deleting the attribute at `:1185` leaves the whole workspace green.

**Downgraded from Important, as two reviewers graded it.** The attribute is present and correct; the
behaviour is right today; nothing diverges. The loss is a regression guard, restorable with a
three-line `alpn_protocolz` reject test that would also mirror `SPEC.md` §2.3's own negative control.
Recording it also notes something in the phase's favour: the rename *strengthened* the test it
replaced, swapping a weak `contains(...)` for an exact `assert_eq`.

### N-4 — D6′.1 has no unit-level pin, and `PLAN.md` claims it does

`crates/envoy-tls/src/lib.rs:94-96`. Surviving mutant: delete the `wire.is_empty()` early return so
the twin is always `Some`, routing **every** config in the tree through `LazyConfigAcceptor`. All 22
`envoy-tls` tests stay green, `accept_returns_handshake_error_on_garbage_input` included.
`PLAN.md` D-PLAN-3 asserts that `alpn_empty_server_list_does_not_advertise` "is the D6′.1 guard test
… the in-process proxy for the differential gate"; it is not — it asserts only that nothing is
negotiated, which is true down **both** paths.

**Minor, deliberately.** The mutant is outcome-equivalent over the wire; only the code path and the
ClientHello-buffering point change. The finding is about the **state of the evidence**: the §7.5
gate (b) safety argument for the 90 pre-existing fixtures rests on those fixtures alone, with no
unit-level pin, and the PLAN's sentence makes it sound otherwise. §1 records that the *design*
property is real and was verified here by a repo-wide census; it is the *guard against regression*
that is missing.

### N-5 — the byte-length semantics of the validator are asserted only with ASCII data

`crates/envoy-config/src/bootstrap.rs:5958`, `crates/envoy-tls/src/lib.rs:92` and `:262`. Both
boundary tests use `"a".repeat(n)`, so `proto.len()` → `proto.chars().count()` survives everywhere,
as does a lossy UTF-8 comparison in the accept path. `grep -cP '[^\x00-\x7F]'` over the new
`bootstrap.rs` test region returns **0** against a positive control of 707 over the whole file.

**The code is correct** — `String::len()` is bytes, which is what RFC 7301's single-octet length
prefix requires, and matches upstream's `std::string::size()`. Only the assertion is weak. One extra
element (`"é".repeat(128)` — 256 bytes, 128 chars) would close it. Note this becomes more than
cosmetic if M-1 is acted on, since a comma-splitting validator has to keep counting bytes per
segment.

### N-6 — `SPEC.md` §9 gate (b) says fixtures `0004`/`0005`/`0006` "exercise the rewritten accept path". They do not, and `0005` builds no `DownstreamTls` at all

The claim appears in `SPEC.md` §4 and §9(b), is repeated in `PROGRESS.md`'s gate (b) section, in
`STATE.md`, and in `ADR-0187`'s headline. Two things are wrong with it:

- **By D6′.1's design they take the UNCHANGED path.** None configures ALPN, so `alpn_free_config` is
  `None` and `accept()` returns at `lib.rs:228-234`. `SPEC.md` §3 D6′.1 and the code comment at
  `lib.rs:226-227` both say this correctly — §4 and §9(b) contradict them.
- **`0005-tls-upstream` carries no `tls_certificates` at all**, so it constructs no `DownstreamTls`
  and cannot exercise `accept()` in either direction. Verified: `grep -rln '^ *tls_certificates:'
  tests/fixtures/` matches **only** `0004-tls-downstream` and `0006-tls-sni` (both sides), against a
  positive control of 9 fixtures matching `transport_socket`.

**Gate (b) is still met and this review does not reopen it.** 90 of 90 fixtures ran green in CI and
that genuinely proves the shared crate did not regress. It is the *stated justification* that is
wrong, and it is wrong in the direction that overstates the evidence — which matters because §4
leans on it to argue gate (b) "cannot pass vacuously". The accurate version: gate (b) proves the
unchanged path stayed unchanged, and **nothing in the differential suite exercises the rewritten
path at all**. That is exactly why `112.2` exists, and it is a point in the split's favour.

### N-7 — the new fuzz seed is not registered in `fuzz_corpus_seeds_parse_or_reject_cleanly`

`crates/envoy-config/src/bootstrap.rs:8657` carries a hand-maintained list of seed paths; the new
`tls_downstream_alpn.yaml` is not in it (`grep -c` = 0, against a positive control of 2 for
`route_redirect_action.yaml`). The phase's only proof that the seed parses was a **throwaway** test
deleted before commit (`PROGRESS.md` Task 7), and libFuzzer silently ignores a seed that merely
returns `Err`, so future seed rot would be invisible.

**Minor because the convention lapsed long before this phase.** Measured: the list names **40** seeds
while **67** are tracked, so **27** are unregistered, and none is missing from disk. The seed's shape
is pinned anyway by the landed inline-YAML tests at `bootstrap.rs:9063` and `:9013`/`:9019`/`:9027`/
`:9042`. A consistency nit against a dead convention.

### N-8 — `ConfigError::InvalidAlpnProtocol` cannot name the offending listener or cluster

`crates/envoy-config/src/lib.rs:88-96` carries `side: &'static str` ("listener"/"cluster"), `index`
and `len`. With ten TLS listeners the operator gets `element 0 is 256 bytes` and no name. Neighbouring
variants that *can* name the resource do (`NetworkFilterChainNotTerminated { listener, … }`,
`EmptyClusterEndpoints(cluster)`), though the immediate precedent `EmptyTlsCertificates { side }` has
the identical limitation, which is why this is Minor and not more. Consistency note: `index` is
0-based here while `NetworkFilterNotTerminated` reports `position: idx + 1`.

### N-9 — `PROGRESS.md` exculpates the PLAN of an arithmetic slip the PLAN contains, and `ADR-0187` attributes to the PLAN a number the PLAN never states

Two attribution errors pointing in opposite directions, both in the correction record itself.

- `PROGRESS.md:477-479` reads *"One further arithmetic slip, in the `STATE.md` handoff **rather than
  the plan**."* But `PLAN.md:340` independently states *"Expect `+2252 → 2252 + 6 config-layer tests
  + 6 envoy-tls tests ≈ **2264**`"* — the same wrong figure, from a **different** (6+6) decomposition
  than M-R13's (708+8). A reader trusting `PROGRESS.md:477` will believe the landed PLAN is clean on
  this; it is not.
- `ADR-0187` charges *"`PLAN.md` M-R13, `ADR-0185` and `ADR-0186`"* with predicting **2266**.
  `grep -c '2266' PLAN.md` is **0**; `grep -c '2264'` is **1**. The *substance* of the charge is
  correct — `PLAN.md:164` does carry the "716 = 708 pre-existing + 8 new" decomposition that implies
  2266, and that decomposition is the genuine root cause — but the PLAN's one explicit identity
  prediction is 2264, so the attribution as worded is imprecise.

**Minor**: `ADR-0187`'s conclusion is right and its root-cause analysis survived independent
re-derivation in §0.3. This is about where the errors are recorded as living, which matters only
because the next reader will go looking.

### N-10 — three internal contradictions inside the landed, uneditable `PLAN.md`, none of them among `ADR-0186`'s five

- `PLAN.md:231` says *"507 of the **550** lines below are not estimated"*, while `:241`'s own
  subtotal row reads **551** with the basis cell *"507 measured, 44 projected"* — and 507 + 44 = 551.
  `ADR-0185` uses 551 throughout, and the sub-phase landed at 549, so **551 is the right figure and
  `:231`'s "550" is the typo.**
- `PLAN.md:338` reads *"**Mutation proof (Task 9).** Re-run M-R13's **two** mutations"*, against
  `:247` *"Task count: **8**"*, `:347` *"Eight tasks"*, and Task 8 at `:1364`, which specifies
  **four** mutations. Four is what ran. Both the task number and the mutation count in that one
  sentence are wrong.
- `PLAN.md:300`'s file-structure table assigns `bootstrap.rs` to tasks *"1, 2"*, but Task 3
  (`2949ccf`, numstat `62 0`) touches **only** `bootstrap.rs`.

**Minor, and none affects scope, design, the §6.1 verdict or what landed.** Recorded because
`ADR-0186` set out to enumerate the PLAN's defects and these three are outside its list of five — so
the list is a sample, not a census, and should not be read as one.

### N-11 — the stop-condition's `tokio` positive control does not reproduce: it reads 11 of 15, not 12

`PROGRESS.md:832-833` claims `tokio` = **12** of 15 manifests, obtained *"with the identical
invocation"* as the zero-returning forbidden-token greps. Measured at this session with that
invocation: `grep -l '^tokio' crates/*/Cargo.toml Cargo.toml` → **11**. The four non-matchers are
`crates/envoy-config`, `crates/envoy-jwt`, `crates/envoy-stats` and the root manifest, each checked
individually for any `tokio` mention (none has one). This figure is inherited verbatim into
`STATE.md:28`, and this review's own leg-(ii) measurement reproduced 11 independently before reading
the claim.

**The conclusion is unaffected and leg (ii) is genuinely FALSE** — `quinn`, `wasmtime`, `tonic`,
`opentelemetry` and `prost` are all 0, and an 11-of-15 control still proves the method finds what is
there. But a positive control whose own number is wrong is worth exactly as much scrutiny as the zero
it is vouching for, and this project's standing rule is that a carried-forward method warning is
itself a claim.

### N-12 — `PROGRESS.md`'s gate (e) transcripts are abridged without an ellipsis, against the counts asserted beside them

`PROGRESS.md:591-599` quotes a build block containing **3** `Compiling` lines and then asserts
*"`Compiling` lines = 5"*; `:602-608` quotes **1** `Checking` line and asserts *"zero findings over
**13** `Checking` lines"*. The counts are plausible and this review cannot falsify them (state 5 does
not re-run the gate, and running `cargo` would serialize the workspace lock). The defect is
presentational: the section's stated contract is that it *"records what the gate actually printed"*,
and nothing marks the blocks as trimmed. **Minor**, and worth one convention: an elided transcript
should carry a `…` so a later reader can tell a short block from a small number.


---

## §5 — Severity dissent, and subagent findings DOWNGRADED on re-verification

Five reviewers returned **eight** findings graded Important. **Three were downgraded by this session**
after re-verification on disk; two were **upgraded in confidence** by a measurement the reviewers
could not run; and the rest survived as graded. The standing expectation held — the phase-111 review
downgraded seven of seven — though less severely here, because three of the four surviving Importants
are backed by a traced execution path rather than by a reading.

**DOWNGRADED — "the Task-1 rename deleted the only pin on `deny_unknown_fields`" (two reviewers,
both Important) → Minor (N-3).** Both are factually right: no test anywhere places an unknown key
under `common_tls_context`, and deleting the attribute leaves the workspace green. But the attribute
is present at `bootstrap.rs:1185`, the behaviour is correct, nothing diverges from upstream, and 207
other structs carry the same attribute. The loss is a regression guard, not a defect. Additionally,
reading the *pre-rename* test shows its own comment conceded that it pinned `alpn_protocols`
specifically as an unknown field — *"alpn_protocols is a phase-04 surface"* — rather than the
attribute generically, so what was lost is narrower than "the only pin" suggests.

**DOWNGRADED — "D6′.1 has no unit pin" (Important) → Minor (N-4).** The surviving mutant is real, but
it is **outcome-equivalent over the wire**: routing every config through `LazyConfigAcceptor` changes
the code path and the ClientHello-buffering point, not a single byte a peer observes. The reviewer's
own note said as much. What remains is a documentation defect — `PLAN.md` D-PLAN-3 claims a guard
test that does not guard — plus an accurate statement of where the evidence actually sits.

**DOWNGRADED — the fuzz slice's two findings arrived correctly graded Minor and stay Minor**, with
credit: that reviewer measured the convention it was tempted to charge (40 seeds listed, 67 tracked,
27 unregistered) and graded itself down accordingly rather than reporting a phase defect. That is the
behaviour this partition is for.

**SURVIVED as Important — the empty-element `debug_assert!` panic (M-2).** Two reviewers found it
independently, and this session derived the same chain a third time from the pinned source before
reading either. Every hop is located, not asserted: `handshake.rs:783-791` → `handshake.rs:48-52` →
`base.rs:169-172` with `NonEmpty::MIN = 1` at `base.rs:222-227`.

**SURVIVED as Important — `from_listener`'s ALPN plumbing is untested (M-3)**, re-verified: exactly
one test names `from_listener` and it configures no ALPN; `grep -rn alpn tests/fixtures/` is 0
against a 9-file positive control.

**SURVIVED as Important, with the finding's own framing corrected — the CDS/EDS validation gap
(M-4).** The reviewer graded it Important and traced it correctly, and this session re-verified both
halves on disk (`bootstrap.rs:4161-4163`'s early return; `:3752` iterating `static_resources.clusters`
while `:3745` built an `effective_clusters` collection that chains the dynamic ones). What this
session changed is the **attribution**: it is pre-existing and structural, equally skipping five
other error variants, and 112.1 inherited rather than created it. The finding is a bound on a landed
claim, not a defect introduced here, and M-4 says so.

**UPGRADED IN CONFIDENCE — the comma-splitting hypothesis (M-1).** The config reviewer graded this
Important while stating plainly that it could not verify it: there is no vendored Envoy source on
this host, so it reasoned from `SPEC.md` §2.3's measured `["", "h2"]` oddity to a join-on-comma
encoder. **That inference was suggestive but not sound** — a strictly per-element encoder that
concatenates length-prefixed names produces the same observable, because a zero-length prefix
corrupts the rest of the buffer for the peer's parser either way. So the reviewer's conclusion was
right for a reason that did not carry.

This session therefore ran the experiment the reviewer could not: eight `--mode validate`
configurations against the pinned image, with a non-vacuity control and an asserted image digest.
**The 259-vs-602-byte pair settles it**: a 602-byte element with three 200-byte segments is accepted
and a 259-byte element containing one 256-byte segment is rejected. The bound is per segment. **A
hypothesis became a measurement, and that is the difference between a note and `ADR-0188`.** Recorded
because it is the general lesson of this review: **the reviewers that could not run the decisive
experiment still located exactly where it needed to be run.**

---

## §6 — Deliberate decisions verified, not re-litigated

- **D3 (absent or empty ⇒ do not advertise).** Matches `rustls`' own documented contract for
  `alpn_protocols` and upstream Envoy's measured behaviour. `#[serde(default)]` reproduces the
  pre-phase behaviour for every existing config exactly.
- **D4′ (reject only `len > 255`).** The *direction* of the decision is right and this review upholds
  it: `ADR-0184` PV-3 correctly refuted the parent's D4, and rejecting the empty element or the
  duplicate would have manufactured a reject-direction divergence. M-1 does not reopen D4′; it shows
  the rule's **unit** is wrong, not its shape.
- **D5 (server preference).** Verified in the pinned source — `hs.rs:101-104`'s `find` iterates
  `our_protocols` — and pinned by `alpn_selection_follows_server_preference`, whose mutation RED set
  is exact. Parity by construction, not by hope.
- **D6′.1 (confine the rewrite to listeners that configure ALPN).** The right call, and the reason
  gate (b) is a design property. §1 records the repo-wide census proving the confinement is total
  today. N-4 concerns only the absence of a regression guard for it.
- **D-PLAN-2 / `ADR-0185` DECISION 7 (`from_listener` warns rather than rejects).** Upheld. The
  divergence **is** deferred rather than avoided — a `warn!` changes neither the accept set nor the
  reject set, so a listener whose chains disagree diverges on the wire exactly as it would have
  silently, and `112.2` §5 non-goal 5 plans no witness. But the alternative is worse: boot-fatal
  rejection would manufacture a **reject-direction** divergence on unmeasured upstream semantics,
  which is strictly worse than an accept-direction one because it turns a working config into a boot
  failure — and it is the identical error `ADR-0184` had to correct in D4. **CF-112-4 is the right
  place for it.** N-1 records only that CF-112-4's wording does not name the order-dependence a
  reader would need in order to reproduce the case.
- **The split seam itself (`ADR-0184`).** Landing both sides of D2 together was correct and is what
  kept the sub-phase out of a parses-then-silently-ignores state. Verified: all three non-test rustls
  config builders read the field.
- **Shipping no fixture (§4).** Correct, and the scope boundary is clean in both directions —
  `git diff --numstat 3a2cf93e HEAD -- tests/` is **empty** (positive control: 508 tracked files
  under `tests/`), `112.1` §4 assigns the witness away, `112.2` §1 claims exactly that set, no
  deliverable is double-claimed, and the two carry-forward lists agree. N-6 corrects only the
  *stated justification* for gate (b), not the decision.

---

## §7 — Carry-forwards for the state-6 close-out to bank

**Opened by this review:** **M-1 … M-5** and **N-1 … N-12** above, plus the five new numbered
carry-forwards they name:

- **CF-112-8** — the `alpn_protocols` length bound is per comma-separated **segment** upstream and
  per **element** here (**MEASURED**, M-1). A live reject-direction divergence, plus an inferred
  accept-direction one on the wire that `112.2` is positioned to witness cheaply.
- **CF-112-9** — an empty `alpn_protocols` element, accepted by D4′, panics a debug build on the
  upstream connect via `rustls`' `PayloadU8<NonEmpty>` `debug_assert!` (M-2). **Refines CF-112-6**,
  which predicted only the release-build wire consequence.
- **CF-112-10** — `DownstreamTls::from_listener`'s ALPN plumbing, first-chain-wins selection and
  disagreement warning are entirely untested (M-3), and the selection is order-dependent with a
  one-directional warning (N-1).
- **CF-112-11** — a CDS-supplied `type: EDS` cluster reaches no transport-socket validation at all,
  so D4′ (and five pre-existing checks) never run on that path (M-4). **Pre-existing and structural.**
- **CF-112-12** — 18 landed-artifact `file:line` citations broken by this phase's insertions, two of
  them pointing forward into the unstarted sibling and one a dangling symbol (M-5).

**Opened by the sub-phase and carried UNCONSUMED** (§6.3; `ADR-0165` — a phase banks, it never
clears): **CF-112-1** (`Http2OverTlsNotSupported` not lifted), **CF-112-2** (the upstream ALPN offer
is unit-tested but not differentially witnessed), **CF-112-3** (ALPN × SNI filter-chain selection
unmeasured), **CF-112-4** (per-chain vs per-listener ALPN unmeasured upstream), **CF-112-6** (empty
and duplicate elements — see CF-112-9), **CF-112-7** (ALPN over the io_uring H1 path).
**CF-112-5 stays CLOSED** (`ADR-0184` measured it).

**Carried forward from earlier phases, INTACT and unconsumed:** phase 111's REVIEW M-1…M-15 +
N-1…N-13; CF-111-1 (explicitly NOT this family's to consume), CF-111-2, CF-111-3, CF-111-4,
CF-111-5, CF-111-6 (LIVE), CF-111-7/8/9; the `110.2` REVIEW's M-1…M-8 + N-1…N-12; the `110.1`
REVIEW's M-1…M-9 + N-1…N-10; CF-110-1…CF-110-9; CF-109-1/2/3; CF-108-1/2/3; CF-76-1;
CF-75-2/3/4/5/6; CF-72-2/CF-75-1; M71-6; CF-74-1/2/3/4/6; CF-73-1; the `109.2`, `109.1` and `108.2`
REVIEW sets; and the HTTP-filters-family (1)–(4).

**Nothing was fixed by this session.** No code file, no test, no fixture, no landed artifact
(`SPEC.md` and `PLAN.md` included), no `ROADMAP.md` row, and no `stop` file.

**If a follow-up wants a natural first task:** M-1's comma split plus M-2's empty-element skip are
roughly ten lines across two files and should land **together**, with the wire measurement that
confirms M-1's Consequence 2 — and `112.2`'s harness is the cheapest place that measurement will ever
be. N-3's three-line reject test and N-5's one extra multi-byte element are another ten minutes.
M-3's gap is the largest single piece and wants a `from_listener` ALPN test per N-1 case.


---

## §8 — Assessment

**`112.1` did the hard thing well, and it did it because it read the dependency instead of trusting
the SPEC that described it.**

The parent phase declared ALPN a pass-through: set a field on `ServerConfig`, set a field on
`ClientConfig`, done. That reading would have shipped a divergence, because `rustls 0.23.39` sends a
fatal `no_application_protocol` alert exactly where upstream Envoy completes the handshake with
nothing selected. Thirty lines of `src/server/hs.rs` turned a declared pass-through into an
accept-path rewrite — **at zero dependency cost**, because the same source also revealed that the
alert fires in `Accepted::into_connection` and not in `Acceptor::accept`, which is the entire reason
a `LazyConfigAcceptor` can get between the ClientHello and the decision. That is the phase's best
moment, and it is worth naming precisely: the escape hatch was **located**, not hoped for.

**The twin is the detail that shows the care.** The obvious implementation is to clone the finished
config and clear `alpn_protocols`; the one that shipped clones *before* the field is ever written, so
the two configs cannot drift and every heavyweight field is the same `Arc` rather than an equivalent.
Checking that against rustls' own "sharing resumption storage between `ServerConfig`s" warning — which
names `verifier` and `cert_resolver` as the two fields that must match — the implementation satisfies
it by construction. A reviewer looking for a subtle TLS behaviour change between the two paths finds
none, and finds none for a structural reason rather than a lucky one.

**D6′.1 is the other structurally-right decision.** Confining the rewrite to listeners that actually
configure ALPN makes §7.5 gate (b) a design property instead of a hope, and this review verified the
confinement is total: exactly one file in the repo sets `alpn_protocols`, and it is a fuzz seed. The
90 pre-existing fixtures take code that is byte-identical to the pre-phase accept path. That is the
version of "no regression" that cannot rot.

**The sizing deserves its own sentence, because it is the strongest datapoint the project has.** The
state-2 session built and mutation-proved a working prototype before pricing anything, and the
sub-phase landed at **549** against a projection of **551** — 0.4%, with five of six per-file rows
exact. Set against the calibration mean of 1.47×, the lesson is not that estimates got better; it is
that **a prototype-priced estimate and a judgement-priced estimate are different kinds of object**,
and the §6.1 gate should probably know the difference. That belongs in `DRAFT-ADR-split-thresholds`,
which already records it as datapoint (e).

**Where the phase is weakest is where it was least curious, and the pattern is consistent.** Four of
this review's five Important findings sit on paths the phase reasoned about correctly but never
*probed*: the comma inside an element (M-1), the empty string inside the list (M-2), the second
filter chain (M-3, N-1), and the dynamic cluster (M-4). Each was measured or traced somewhere in the
artifacts — §2.3 measured the empty element, §5 non-goal 7 named per-chain ALPN, CF-112-6 predicted
the zero-length wire name — and in each case the phase stopped one step before the consequence. The
sharpest instance is M-1: §2.3 ran a seven-row `--mode validate` table over element *lengths* and got
every row right, and one more row containing a comma would have caught a divergence that three landed
artifacts now assert does not exist. **The probe was the right idea; its input space was one
character too small.**

That is a mild criticism of a strong slice, and it is the criticism worth making, because the phase's
own method is what makes it fixable: `112.2` is about to stand up an ALPN differential harness, and
every one of M-1, M-2 and CF-112-6 becomes a cheap fixture the moment it exists.

**Verdict: APPROVED. §7.5 gate (f) is CLOSED.** §2 is empty; the state machine advances to state 6.
Seventeen findings are banked and none is fixed, per §6.3 and `ADR-0165`. One reconciliation ADR
(`ADR-0188`) fires, for a measurement taken at this review that contradicts a sentence in
`crates/envoy-config/src/lib.rs:87`, in `112.1/SPEC.md` §3 D4′ and in `ADR-0184` PV-3 — correcting
none of their positive findings, only the generalisation drawn from them.

---

### STOP CONDITION — re-derived from disk at this review. ALL THREE LEGS FALSE

The **eighty-first** consecutive evaluation by the ledger's running count. All three legs measured
independently and freshly at this commit; all three FALSE; **no `stop` file was created**, and
`ls stop` returns `No such file or directory`.

- **LEG (i) FALSE.** `ROADMAP.md` census, status = field **4** on a `' | '` split driven from the
  `^\| [0-9]` prefix: **120 rows / 117 `done` / 1 `in-progress` / 2 `planned`**, and the buckets sum
  to the row count (117 + 1 + 2 = 120 ✓). Parent `112` is `in-progress`; `112.1` and `112.2` are
  `planned`. **It did not move at this review and was not supposed to** — a state-5 review does not
  touch `ROADMAP.md`; row `112.1` flips at its state-6 close-out. The forbidden `NF == 6` filter
  reproduces its documented near-miss exactly: it reads **118**, dropping row 38 (NF=7) and row 39
  (NF=10), the two carrying unescaped in-cell pipes. **Not "fixed."**
- **LEG (ii) FALSE.** **14** crates, none of them `envoy-http3`/`envoy-grpc`/`envoy-wasm`/
  `envoy-protos`/`envoy-runtime`. `quinn`, `wasmtime`, `tonic`, `opentelemetry` and `prost` are each
  **0** across all **15** manifests (`crates/*/Cargo.toml` + the root). **Positive control, same
  `grep -l '^<dep>'` invocation: `tokio` = 11 of 15** — non-zero, so the method finds what is there,
  and see N-11, because the inherited figure was 12. `tests/conformance/` holds only `h2spec/`.
- **LEG (iii) FALSE and unmoved.** **11** `### ` family headings, censused from a single `/^### /`
  rule to avoid the documented `next`-swallowing `awk` bug: **10 / 5 / 3 / 14 / 3 / 4 / 6 / 29 / 6 /
  0 / 13**, with **27** rows before the first heading, summing to **120** ✓. Exactly **one** family
  carries zero rows — `### WASM host family`. The known filing defect (seven `Observability family:`
  rows physically under `### Deprecated / edge features`) is present and **deliberately not repaired**.

**ALL THREE MUST HOLD; ZERO DO.** A human asking for a conditional `stop` file is an instruction to
evaluate the condition, not evidence that the answer changed.

---

### Next state

**§5 state 6 — the close-out for `112.1` — is a SEPARATE session** (§5.1; `ADR-0127`: a reviewer must
not close out what it reviewed). It flips `ROADMAP.md` row `112.1` to `done` — **that cell only**,
parent `112` stays `in-progress` — banks §7's carry-forward set, and advances `STATE.md` to `112.2`
at §5 state 2. **`112.2` may not start until `112.1` has closed**, because every `112.2` fixture
configures `alpn_protocols`.

**The close-out is the one state that does not pay for parallelism:** small, indivisible, and every
action a state-ledger write.
