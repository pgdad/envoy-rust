# Sub-phase 112.2 — the ALPN differential witness + the contract section — CODE REVIEW

> **§5 state 5.** This document is the state-5 output for `112.2` and it **CLOSES §7.5 gate (f)**,
> the only gate the state-4 session left open. Written by a THIRD context: the state-3 session
> implemented, the state-4 session graded, and neither may review (§5.1; `ADR-0127`).
>
> **Verdict: APPROVED.** §2 (Issues — Must Fix) is **EMPTY**, so the state machine advances to
> **state 6**, not back to state 3 (§5.2).
>
> **This review wrote no code, no test and no fixture, and edited no landed artifact.** The CI
> identity must therefore stay at `binaries=168 passed=2274 failed=0`. Every finding below is
> **BANKED** as a carry-forward, not fixed (§6.3; `ADR-0165`: a phase banks, it never clears — and
> a REVIEW banks its own findings too).
>
> **Twenty-one findings: 2 Important, 19 Minor, opening `CF-112-13` … `CF-112-19`.** `ADR-0191`
> fires: the verdict and the banked set are decisions. **No landed figure was contradicted**, so
> unlike `ADR-0188` this ADR corrects nothing — but two landed *sentences* are false and one
> landed *deliverable* is one bullet short, and both are recorded here rather than repaired.

---

## §0 — How this review was conducted

### §0.1 — Scope

The review surface is the **652 net code lines** between base `00060ad` (the state-2 CI record,
parent of Task 1) and `5e51add` (the state-3 advance). Re-derived at this session with
`git diff --numstat 00060ad 5e51add -- . ':(exclude)docs/**'`:

| file | + | − | net | claimed |
|---|---|---|---|---|
| `tests/differential/src/lib.rs` | 331 | 11 | **320** | 320 ✓ |
| `tests/fixtures/0091-tls-alpn/` (5 files) | 166 | 0 | **166** | 166 ✓ |
| `tests/fixtures/0092-tls-alpn-server-preference/` (5 files) | 129 | 0 | **129** | 129 ✓ |
| `tests/differential/tests/tls_alpn.rs` | 32 | 0 | **32** | 32 ✓ |
| `tests/fixtures/0004-tls-downstream/expectations.yaml` | 5 | 0 | **5** | 5 ✓ |
| **TOTAL** | **663** | **11** | **652** | **652 ✓** |

Plus `docs/envoy-rust/BEHAVIOR_CONTRACT.md` at **`119 0`**. All five rows and the total match
`PROGRESS.md` F1 and `ADR-0190` exactly. **The range is stated because a numstat citation goes
stale at the carrying commit**: `<base> HEAD` is wrong here, since four docs-only commits have
landed since `5e51add`.

**`crates/`, `Cargo.toml` and `Cargo.lock` are UNTOUCHED.**
`git diff --stat 00060ad 5e51add -- crates Cargo.toml Cargo.lock` is **EMPTY** — which is
`SPEC.md` §5 non-goal 1 holding, verified rather than assumed.

The seven task commits, each touching exactly the files its subject line claims and summing to the
652 above: `a0b2908` (`174 1`), `3c28950` (`103 6`), `dabc2db` (`55 5`), `13d705f` (fixture `0091`
+ 25 lines of runner), `b8498d9` (fixture `0092` + 7 lines of runner), `55eb5ae` (`5 0` on `0004`),
`4d1a80d` (`119 0` on the contract).

Out of scope by design and deliberately not reviewed: `crates/` (sibling `112.1`, reviewed at
`1b6a81d`), `ROADMAP.md` (state 6), and every landed artifact — `112.2/SPEC.md`, `112.2/PLAN.md`
and **both sections** of `112.2/PROGRESS.md` are UNEDITABLE and were not edited.

### §0.2 — Method

Four read-only reviewers were fanned out over the partition the `STATE.md` handoff named:
**(1)** the `lib.rs` grammar, `check_alpn` and both driver paths; **(2)** the two fixtures, the
`0004` cell-6 edit and the runner; **(3)** the `BEHAVIOR_CONTRACT.md` section against the parent
SPEC cell table and CF-112-6/8/9; **(4)** the two `PROGRESS.md` sections against what the tree
actually holds. Each was given full zero-context instructions (D-3.4), forbidden to write or to
run `cargo` (the workspace lock serializes) or `docker`, told that `~/.cargo/registry/src/` is
readable evidence **with the pin resolved from `Cargo.lock` first**, and required to run a positive
control before reporting any zero.

**Every subagent finding was re-verified on disk by this session, and the ones that did not survive
were downgraded.** §5 records each dissent. Two findings in this review were derived by the main
session independently of any subagent and then corroborated by one (N-1, N-3); one subagent-
proposed Important was downgraded and one subagent-proposed Minor was upgraded.

**Pins resolved from `Cargo.lock` first**, and stated because this host's registry cache holds
several versions and the `rustls-*` glob also matches `rustls-native-certs`, `rustls-pemfile`,
`rustls-pki-types` and `rustls-webpki`: **`rustls 0.23.39`**, **`tokio-rustls 0.26.4`**,
**`serde 1.0.228`** and **`serde_derive 1.0.228`**, read at
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`. `serde_derive` is new to this phase's
evidence base and is the whole of M-1.

### §0.3 — The §7.5 gate was NOT re-run

Re-running it is state 4's job and it is done. This review takes the state-4 record as given and
adds gate (f) only. What this session DID re-derive independently, because a landed figure is a
claim:

- **The net-652 table above** — all five rows and the total, at this commit, plus the empty
  `crates/` diff.
- **`PROGRESS.md` F6's nine relocated citations and four new symbols**, each asserted to occur
  **exactly once** by `grep -cF` and each read back with `sed -n '<n>p'`: `Driver`'s serde tag
  `:38` (unmoved), `TlsTcp {` `:91`, `TlsTcpProbeList {` `:109`, `#[serde(deny_unknown_fields)]`
  above `TlsTcpProbe` `:748`, `pub struct TlsTcpProbe {` `:749`, `pub enum AlpnRule` `:774`,
  `pub async fn drive_tls(` `:1954`, `pub async fn drive_tls_probes(` `:2043`,
  `fn check_alpn(` `:2139`, `async fn run_tls_tcp_arm` `:5034`, the two `drive_tls(` call sites
  `:5071`/`:5082`, `async fn run_tls_tcp_probe_list_arm` `:5101`. `lib.rs` is **11549** lines.
  **F6 survives independent re-derivation in every coordinate.** The documented near-miss
  reproduces: `fn check_alpn` matches **twice** (`:2139` and its unit test at `:9104`), and the
  bare `#[serde(deny_unknown_fields)]` matches **16** unanchored / **14** at column 0.
- **`PROGRESS.md` F1's "13 of 14 fences verbatim" claim, by exact substring match.** The Tasks-1-3
  `rust` fences were extracted programmatically from `PLAN.md` (14 of them, between the `## Task 1`
  and `## Task 4` headings) and each tested with `in` against `lib.rs` at HEAD: **13 PASS**. The
  one exception is fence 4, and it is exactly the one F1 names — Task 1 Step 5's intermediate
  `Driver::TlsTcp { sni, expected_cn, .. }` dispatch, superseded by Task 2 Step 5's 17-line form,
  which IS verbatim (fence 11). **F1's byte-faithfulness claim survives.**
- **F1's root-cause arithmetic, measured on disk rather than restated.** The two late-added test
  blocks span `:9101-9123` (**23** lines) and `:9125-9156` (**32** lines); 23 + 32 + their two
  separator blanks = **57**, which is exactly the `274 → 331` shortfall F1 attributes to them.
  The deletions match exactly (11 = 11). **F1 is CONFIRMED, not merely plausible.**
- **The PLAN's test list against the tree.** All **7** new lib tests and both runner tests the
  PLAN names exist, none is missing and none is extra:
  `expectations_parse_tls_alpn_probe_list`, `…_probe_without_client_offer`,
  `expectations_parse_tls_tcp_with_alpn_fields`, `expectations_parse_pre_112_tls_fixtures_unchanged`,
  `expectations_reject_unknown_alpn_rule_field`, `check_alpn_adjudicates_all_four_outcomes`,
  `tls_probe_list_carries_distinct_per_probe_offers`, plus `tls_alpn_fixture` and
  `tls_alpn_server_preference_fixture`. 171 → **178** lib tests is +7, matching the CI delta.
- **The gate (c) and (d) inputs.** `known-failures.txt` is **21** lines with md5
  `19cd44d86a8b15d825f76c6e7b265e65` — exactly `SPEC.md` §9(c) — and untouched by this phase;
  `git diff --stat 00060ad 5e51add -- fuzz '*/fuzz/*' .github` is **EMPTY** against 5 fuzz targets.
- **The record's own account of itself**, which is where every remaining finding lives. Confirmed
  on disk: `grep -cF 'TlsTcp {'` = **5** and `'TlsTcpProbeList {'` = **8**, against the state-4 F6
  row's claim that every anchor was asserted unique (N-13); `PLAN.md:86`'s **596** against its own
  table's **595**, summed here (N-14); `13d705f`'s `tls_alpn.rs` at **25** against `PLAN.md:953`'s
  "(32)" (N-15); `SPEC.md` §2.4 spanning `:135-164` with two of F6's nine citations at `:168`/`:169`
  in §2.5 (N-16); `lib.rs:732` today being a doc comment while `PLAN.md:48` still cites it (N-17);
  and `PROGRESS.md:90`'s measured `M2 … passes` against `SPEC.md:330-332`'s "must turn the fixture
  RED" (N-19).
- **The SHA audit.** Five distinct 40-char hex strings appear across the phase directory and
  `STATE.md`; **four resolve** under `git cat-file -e` (`2a9712b3…`, `31169c2e…`, `5e51add1…`,
  `e9d0c3b5…`). The fifth, `56da5afd7df364350ff92de4fb49a9b09957c172`, is **not a git SHA and not
  a defect** — it is the first 40 characters of the 64-character Docker digest from
  `ENVOY_TARGET.md`, truncated by the audit regex itself. **No fabricated SHA.**
- **All 21 symbols the contract section names**, each resolved in the tree or in pinned dependency
  source, against a bogus-name positive control returning 0. The table is in §6.

---

## §1 — Strengths

**The assertion is read off the same completed handshake `expected_cn` already uses, and it is
placed before the payload write.** `drive_tls` reaches `tls.get_ref().1.alpn_protocol()` at
`lib.rs:2001-2003` and `drive_tls_probes` at `:2092-2093`, in both cases immediately after the
cert check and **before** `write_all`. So a mismatch fails on handshake evidence rather than on
downstream payload noise, and `SPEC.md` §2.4's "no new driver, no second connection" claim is
literally true — verified: the only callers of `drive_tls` in the whole repository are the two at
`:5071`/`:5082` inside `run_tls_tcp_arm`, and the only callers of `drive_tls_probes` are the two at
`:5132`/`:5135` inside `run_tls_tcp_probe_list_arm`, whose signature this phase did not touch
(confirmed: the symbol appears in no hunk of the `lib.rs` diff, against a control of 4 hits for
`run_tls_tcp_arm`).

**`AlpnRule`'s negative arm is the load-bearing design call of the phase and it is correct.**
Cells 3, 4 and 6 assert the *absence* of a protocol. A rule carrying only a positive value would
make them unwriteable; letting `expected_alpn: None` mean "negotiated nothing" would make them
silently vacuous, since `None` already means "do not check". The doc comment at `:759-766` states
that reasoning explicitly rather than leaving it implicit, and `check_alpn` (`:2139-2166`) has
exactly two branches with no wildcard — E2's deliberate inexpressibility of "any protocol" is
faithfully implemented, not merely asserted.

**`check_alpn` is a single pure adjudicator shared by both driver paths.** There is no second,
divergent copy of the comparison, and its unit test `check_alpn_adjudicates_all_four_outcomes`
(`:9101-9123`) covers all four outcomes — positive/match, positive/wrong, positive/nothing,
negative/match, negative/something — asserting on the message text, not merely on `is_err()`.

**The two new fixtures' upstream/subject divergence set is byte-identical to the established
`0004-tls-downstream` baseline.** This is the single most important check in the fixture slice and
it passes cleanly. `diff envoy.yaml envoy-rust.yaml` on each new fixture yields exactly the same
four hunks as the same diff on `0004`, differing only in line numbers (39 vs 38, 48 vs 47 — the one
extra `alpn_protocols` line): the upstream-only `admin:` block, the `0.0.0.0` vs `127.0.0.1`
listener bind, the upstream-only `dns_lookup_family: V4_ONLY`, and `{{BACKEND_HOST}}` vs literal
`127.0.0.1`. **No accidental semantic divergence**, and the `alpn_protocols` line itself — the one
field the phase adds — is byte-identical on both sides of both fixtures
(`0091` = `["h2", "http/1.1"]`, `0092` = `["http/1.1", "h2"]`). A green result here is meaningful.

**The cell set is not redundant, and each cell has a distinct failure mode it alone catches.**
Cells 1/2 catch "never advertises"; cell 3 catches `rustls`' default fatal `no_application_protocol`
alert — the one piece of real engineering, and the only cross-proxy witness of `112.1`'s D6′ accept
path; cell 5 catches a client-preference inversion; cell 6 catches a spurious default list. An
"always none" implementation reddens 1/2/5; a "spurious default list" implementation reddens 6.

**Cell 5's discriminating power is real, and its dependency claim verifies in pinned source.**
`0092`'s README asserts that `rustls` iterates the server's list in the outer position and only
scans the client's with `.any()`. Confirmed at `rustls-0.23.39/src/server/hs.rs:99-108`:
`our_protocols.iter().find(|ours| their_protocols.iter().any(|theirs| …))`. A client-preference
implementation returns `h2` and goes RED. The same function's `} else if !our_protocols.is_empty()`
branch at `:114-119` is exactly the escape hatch the contract section describes for cell 3.

**Empty `client_alpn` really does mean "send no ALPN extension", not "send an empty list".**
Verified at `rustls-0.23.39/src/msgs/handshake.rs:783-791`, where `ClientExtensionsInput::from_alpn`
maps `alpn_protocols.is_empty()` to `None` for the extension. The comment at `lib.rs:1970-1972` is
accurate, and cell 4 is therefore a genuine "client offers nothing" probe rather than a
"client offers an empty list" one.

**Non-UTF-8 negotiated ALPN cannot panic and cannot manufacture a false match, and the case is
unreachable through this harness for a stated reason.** `check_alpn`'s positive arm uses
`std::str::from_utf8(...).map_err(...)` → a clean `Err`; its negative arm uses `from_utf8_lossy`
**only inside the failure message**, never in a comparison. Beyond that,
`ClientConfig::check_selected_alpn` defaults to `true`
(`rustls-0.23.39/src/client/builder.rs:168`) and `process_alpn_protocol` (`client/hs.rs:589-604`)
fails the handshake with `SelectedUnofferedApplicationProtocol` if the server selects a protocol the
client did not offer — and the offer is always built from `Vec<String>`, hence always UTF-8. A rogue
server surfaces as a handshake error, not as a `check_alpn` outcome. Correct, and correctly ordered.

**CF-112-9 is actively avoided, and the avoidance is checkable.** No fixture in the tree configures
an empty ALPN element on either side: `grep` for an empty-string element across `0091`, `0092` and
`0004` returns **0** against a positive control of 26 quote characters in the same directories.

**The `0004` edit is placed where it costs nothing.** `0004`'s listener still configures no
`alpn_protocols` (0 hits in both config files), so `finish_server_config` returns
`alpn_free_config = None` and `accept()` takes the unchanged pre-112 `TlsAcceptor` path — the cell
is added without touching either side's code path. And `0004` is the one TLS fixture that *can*
carry it: `0005-tls-upstream` is `kind: tcp_echo`, plaintext downstream, with no downstream TLS
client to attach an offer to. The asymmetry the handoff asked about is principled, not an oversight.

**`#[serde(default)]` on all four new fields is present and pinned**, with
`expectations_parse_pre_112_tls_fixtures_unchanged` as the explicit regression pin for PV-8. The
struct-shaped surface is protected exactly as claimed; the one gap is M-1, on the unit arm.

**The task boundaries are clean and the sizes reconcile.** Task 1 is grammar only, Task 2 adds
`check_alpn` plus the `drive_tls` path, Task 3 the per-probe path, Tasks 4/5 the fixtures, Task 6
the `0004` cell, Task 7 the contract. No task touches a file its title does not name.

---

## §2 — Issues (Must Fix)

**EMPTY.**

Every deliverable in `SPEC.md` §1 landed: the client-side offer and expectation on both TLS drivers
(1), fixture `0091` with cells 1–4 (2), fixture `0092` with cell 5 (3), cell 6 on `0004` (4), the
runner (5), and the `BEHAVIOR_CONTRACT.md` ALPN section (6). Deliverable 7 — the parent-112
close-out — is state 6's by construction and is correctly absent.

No finding below makes a landed cell wrong, makes a fixture pass vacuously on the property it
exists to witness, or leaves the phase's own §5 non-goals violated. **M-1 and M-2 are both real and
both consequential, but neither invalidates a measured cell**: M-1 is a latent trap in the grammar
that no landed fixture triggers, and M-2 is a false sentence about a *neighbouring* behaviour
(multi-chain listeners) that this phase does not test and that the project already banks as
CF-112-10. Under §5.2 the state machine therefore advances to **state 6**, not back to state 3.

---

## §3 — Important

### M-1 — `#[serde(deny_unknown_fields)]` does NOT reach `AlpnRule::NoneSelected`, so `{ kind: none_selected, protocol: h2 }` parses silently as "nothing negotiated" — the exact inversion of what its author wrote. The phase's own pin tests only the other arm, and its comment claims the whole type is protected

`tests/differential/src/lib.rs:773` (the attribute), `:780` (the unit variant), `:9084-9085` (the
comment that over-claims), `:9087` (the test that under-tests).

**The mechanism, located in pinned source rather than asserted.** `AlpnRule` is an *internally
tagged* enum (`#[serde(tag = "kind", …, deny_unknown_fields)]`). In `serde_derive 1.0.228`,
`de/enum_internally.rs` dispatches on the variant's style:

```
Style::Unit => { … InternallyTaggedUnitVisitor::new(#type_name, #variant_name) … }
Style::Struct => struct_::deserialize(params, &variant.fields, cattrs, StructForm::InternallyTagged(variant_ident)),
```

The `Style::Struct` arm receives `cattrs` — the container attributes, which carry
`deny_unknown_fields`. **The `Style::Unit` arm does not.** And the visitor it uses discards
everything (`serde-1.0.228/src/private/de.rs:2990-2996`):

```rust
fn visit_map<M>(self, mut access: M) -> Result<(), M::Error> {
    while tri!(access.next_entry::<IgnoredAny, IgnoredAny>()).is_some() {}
    Ok(())
}
```

**Concrete input and concrete harm.** A fixture author copies cell 1's rule and edits only the kind:

```yaml
expected_alpn: { kind: none_selected, protocol: h2 }
```

This parses cleanly as `AlpnRule::NoneSelected`. The fixture now asserts *"nothing was negotiated"*
while its author believes it asserts `h2` — and on any listener that negotiates nothing it goes
**green**. The assertion is not weakened; it is **inverted**, and inverted in the silent direction.
`none_selected` is the arm **three of the six cells** use (3, 4 and 6), so this is not a corner of
the grammar.

**Why this is the phase's finding and not merely an inherited serde behaviour.** The hole itself is
repo-wide and pre-existing — the same shape affects `Driver::TcpEcho`, `Driver::TcpDirectResponse`
and `BodyRule::ByteExact`, which is the `equivalence` rule every fixture in the corpus declares.
What this phase newly contributes is a **claim to the contrary**, shipped as a passing test:

```rust
// 112.2 Task 1 RED: `AlpnRule` carries `deny_unknown_fields`, so a typo
// in the rule is a hard parse error rather than a silently ignored key.
#[test]
fn expectations_reject_unknown_alpn_rule_field() {
    …  expected_alpn: { kind: selected, protocol: h2, protocul: h3 }
```

The comment quantifies over the whole type (*"a typo in the rule"*); the test exercises only
`selected` — the one arm where the protection actually holds. A green test whose name and comment
assert a property it does not test is worse than no test, because it retires the question.
`SPEC.md` §2.5's PV-8 safety property — *"The fields must therefore exist in Rust before any fixture
YAML may name them"* — is true of the struct arm and false of the unit arm.

**Grade rationale.** Important, not Must-Fix: **no landed fixture triggers it.** All three
`none_selected` uses in the tree (`0091:24`, `0091:28`, `0004:8`) are bare, so every measured cell
is correct as written. But it is reachable from fixture data alone, it fails silently and in the
inverting direction, and the phase shipped a pin that says it cannot happen. **Banked as
CF-112-13.** The fix is one character-class change — `NoneSelected {}` routes the variant through
`struct_::deserialize` and *does* honour `deny_unknown_fields` — plus extending the existing test
with a `{ kind: none_selected, protocol: h2 }` case. It should be decided repo-wide rather than for
`AlpnRule` alone, because `BodyRule::ByteExact` carries the same hole across the whole corpus.

---

### M-2 — the contract section states that the FIRST chain's **non-empty** list wins and that a warning is logged. Both halves are false, and the project's own landed `112.1/REVIEW.md` N-1 measured them false before this sentence was written

`docs/envoy-rust/BEHAVIOR_CONTRACT.md:1141-1144`:

> `rustls::ServerConfig` property and `DownstreamTls::from_listener` builds one
> config per listener, so when several chains disagree the FIRST chain's
> **non-empty** list wins for the whole listener **and a warning is logged**.

**What the code does** (`crates/envoy-tls/src/lib.rs:168-183`; the `None`-arm anchor asserted to
occur exactly once):

```rust
match alpn_protocols {
    None => alpn_protocols = Some(&ctx.common_tls_context.alpn_protocols),
    Some(first) => {
        let this = &ctx.common_tls_context.alpn_protocols;
        if !this.is_empty() && this.as_slice() != first { tracing::warn!(…) }
    }
}
```

The `None` arm seeds `alpn_protocols` from the **first TLS chain unconditionally, empty list
included**. Both halves of the sentence fail, in opposite directions:

- **Chain A declares no ALPN, chain B declares `["h2"]`.** `first` is `[]`. The guard fires, chain
  B warns and is dropped, and the listener advertises **nothing at all**. The honoured list is the
  *empty* one — the precise opposite of "the FIRST chain's non-empty list wins".
- **Chain A declares `["h2"]`, chain B declares none.** `this.is_empty()` short-circuits, so **no
  warning fires** and chain B is silently given `["h2"]` it did not ask for. "and a warning is
  logged" is unqualified where the warning is one-directional — and this is the silent direction.

**This is not new information to the project.** `112.1/REVIEW.md` N-1 (`:466-489`, landed and
uneditable) measured both cases and wrote *"Chain A's **empty** list is what seeds `alpn_protocols`
at `:169` … the listener advertises nothing at all … honoring the first **non-empty** list would
remove it"* and *"no warning fires at all … it is the genuinely silent one"*. It banked them as
CF-112-10 at **Minor**, correctly, because at that point they were a warning-message and
code-shape observation with no strong claim attached to them.

**Why the same behaviour is Important here.** `BEHAVIOR_CONTRACT.md` states its own standing in the
file: it is the document from which fixture expectations are derived. A Minor code-shape
observation has become a **false statement in the contract of record**, in a section written after
the measurement that contradicts it, and the mis-statement is one word wide. A later phase writing
an ALPN × multi-chain fixture from this sentence would assert that chain B's list wins over an empty
chain A, get a RED, and misattribute it to the implementation — with the contract, not the code, as
the thing that lied. The section is otherwise unusually careful about hedging (see §6), which makes
this sentence stand out rather than blend in.

**Grade rationale.** Important, not Must-Fix: the sentence describes a *neighbouring* behaviour that
this phase does not test, no fixture in the tree has more than one TLS chain with a non-empty list,
and no measured cell is affected. But it is a false statement in the authority document, it was
false when written, and the evidence that it is false was already landed in the same phase family.
**Banked as CF-112-14.** The remedy is two sentences: *"the FIRST TLS filter chain's list — empty or
not — wins for the whole listener; a later chain's non-empty, differing list is dropped with a
warning, while a later chain's empty list is overridden silently (`CF-112-10`, `112.1/REVIEW.md`
N-1)."*

---

## §4 — Minor

### N-1 — `run_tls_tcp_probe_list_arm` never evaluates `equivalence`, so both new fixtures declare a `response_body: byte_exact` rule that does nothing — and the driver's docstring justifies this with reasoning that is affirmatively false

`tests/differential/src/lib.rs:5132-5140` (outputs discarded, no `assert_equivalence`), `:2103-2105`
(`out` never compared), `:2035-2038` (the false justification), and the two new fixtures'
`expectations.yaml` (`0091:29-31`, `0092:12-14`).

The arm drives both sides and throws both results away:

```rust
drive_tls_probes(upstream_addr, &payload, probes, roots.clone()).await.context("upstream envoy tls probes")?;
drive_tls_probes(subject_addr,  &payload, probes, roots       ).await.context("envoy-rust tls probes")?;
subject.shutdown(Duration::from_secs(5)).await.ok();
drop(upstream);
Ok(())
```

It is the **only** one of the five driver arms that does not call `assert_equivalence` (the others
are at `:4727`, `:4877`, `:4995`, `:5022` and `:5095` — the last inside `run_tls_tcp_arm`, so
`0004`'s cell 6 *does* get byte-exact equivalence while cells 1–5 do not). Inside the helper,
`payload` appears exactly three times — the parameter, the `write_all`, and `payload.len()` to size
the buffer — and `out` is never compared to anything.

The docstring at `:2035-2038` claims otherwise:

> byte-equality is enforced *inside* this helper (each probe writes `payload`, reads-exact
> `payload.len()` bytes, and the read would not have succeeded as a different byte sequence under
> `read_exact`-then-bail-on-trailing semantics)

`read_exact` is value-blind: it succeeds on any `payload.len()` bytes. A proxy echoing 18 corrupted
bytes passes all four `0091` probes. (A short or absent echo still fails, so total breakage is
caught; what is unenforced is byte *value* equality.)

**Grade rationale.** Minor, and the mechanism is **inherited, not introduced** — it dates to phase
03.2, and `0006-tls-sni:6-8` carries the identical inert block. The phase's actual deliverable is
unaffected: the ALPN assertions *are* evaluated, per probe and per side, and their conjunction is
exactly `SPEC.md` §3 E3's equivalence claim. What is new is that two fixtures now declare the inert
rule, and that the driver carrying the phase's flagship witness contains a justification a future
author will believe. **Banked as CF-112-15.** The remedy is either to compare `out == payload`
inside the loop and the two sides against each other, or to correct the docstring and drop the
`equivalence:` block from probe-list fixtures — but not to leave the false reasoning in place.

### N-2 — the harness's `client_alpn` is unvalidated: an empty element reaches a `debug_assert!` inside `rustls`, and an over-long element is silently truncated on the wire

`tests/differential/src/lib.rs:1973` and `:2062-2066`. Both sites are a bare
`Vec<String> → Vec<Vec<u8>>` map with no checks:

```rust
client_cfg.alpn_protocols = client_alpn.iter().map(|p| p.as_bytes().to_vec()).collect();
```

Two RFC 7301 constraints on `opaque ProtocolName<1..2^8-1>` are unenforced:

- **Empty element.** Every entry goes through `ProtocolName::from` →
  `PayloadU8::<NonEmpty>::new` → `debug_assert!(bytes.len() >= 1)`
  (`rustls-0.23.39/src/msgs/base.rs:169-172`, `NonEmpty::MIN = 1` at `:225-227`). `cargo test`
  is a debug build and `Cargo.toml` declares no `[profile.dev]` override, so
  `client_alpn: ["h2", ""]` **panics inside a dependency**, naming neither the fixture, the probe,
  nor the field.
- **Over-long element.** `PayloadU8::encode` does `(self.0.len() as u8).encode(bytes)`
  (`base.rs:181-186`) — a silently truncating cast, and `debug_assert!(len >= 1)` passes for 256.
  A 256-byte `client_alpn` entry emits a length byte of `0` followed by 256 bytes: a malformed
  ClientHello, with no error from the harness at all.

**Grade rationale.** Minor: no fixture does either, and reaching them requires an author to write a
malformed offer. It is recorded because it is the **client-side mirror of CF-112-9** — which
`112.1/REVIEW.md` M-2 banked for the *production upstream connect* path
(`crates/envoy-tls/src/lib.rs:369`), a different reachability — and because the contract section
this same phase wrote records a `>255` validator on the crates side, so the asymmetry is one a
reader would not expect. **Banked as CF-112-16.** The remedy is a two-line guard at each mapping
site naming the probe and the index.

### N-3 — two sites claim the pre-112 fixtures are unaffected, in a phase that edited one of them

`tests/differential/src/lib.rs:87-90` — *"so every pre-112 fixture (`0004-tls-downstream`
included) parses unchanged"* — and `docs/envoy-rust/BEHAVIOR_CONTRACT.md:1093-1094` — *"which is
why every pre-112 fixture is unaffected."*

Task 6 of this same phase (`55eb5ae`, `5 0`) edited exactly `0004-tls-downstream`, giving its client
an ALPN offer and a new assertion. Both sentences are **true of the proxy's code path** — `0004`'s
listener still configures no `alpn_protocols`, so `alpn_free_config` is `None` and the unchanged
`TlsAcceptor` branch is taken — and **false of the fixture set**. The `lib.rs` one is the sharper of
the two, because it names `0004` specifically as the example of a fixture that did not change.

This matters because the phase's own process draws the changed/unchanged line carefully and these
two sentences flatten it: `SPEC.md` §2.3 warns *"⚠ The `0004` edit changes the client's behaviour on
a green fixture … the PLAN must nonetheless treat `0004` as a **changed** fixture under §7.5(a), not
as a pre-existing one under (b)"*, and the state-4 gate commit records *"(a) PASS on 0091, 0092 AND
the CHANGED 0004, (b) PASS on the other 89."* A future reader citing either sentence to justify not
re-running a pre-112 TLS fixture after an ALPN change would be relying on a claim the phase itself
declined to make. **Banked as CF-112-17** with N-4…N-7.

### N-4 — `CF-112-7` is the one banked unmeasured cell the contract's list omits

`BEHAVIOR_CONTRACT.md:1123-1151` names CF-112-1, -2, -3, -4, -6, -8 and -9. `CF-112-7` — ALPN over
the io_uring H1 listener path — is absent, against a positive control of 7 `CF-112-` hits in the
same span. `112.2/SPEC.md` §8 lists it explicitly among NOT MEASURED, `ADR-0184` records it as
opened, and `SPEC.md` §4 states the deliverable as a section recording *"every cell left
unmeasured."*

The omitted one is the only entry that says a whole accept path may not reach the ALPN code at all
(`112.1/SPEC.md`: *"whether the io_uring path reaches that function is not established here"*), so a
reader taking the list as exhaustive concludes ALPN is settled on every listener path.
**Minor rather than Important** — see §5; it is an omission of one bullet, not a false statement,
and CF-112-7 remains banked and visible in `SPEC.md` and `DECISIONS.md`. One bullet fixes it.

### N-5 — the contract's preamble asserts one measurement methodology for values gathered under two

`BEHAVIOR_CONTRACT.md:1047-1051` says *"Every upstream-Envoy value below was MEASURED … on
loopback-mapped ports asserted free before each run, with a REACHABLE backend … Nothing here is
projected."* The six-cell table was measured that way. The **Element-validation** values at
`:1109-1121` were not: they came from a **networking-free `--mode validate` probe**
(`112.1/SPEC.md` §2.3; `112.1/REVIEW.md`: *"It is networking-free, so it needed no port and could
not collide with the foreign workstream's reservations"*), with no port and no backend.

The cost is not a wrong value — every value is right — but a future session reproducing the
CF-112-8 divergence would build a port-mapped container with a live backend, which is the *opposite*
of what `112.1` deliberately did to avoid port collisions on this host. Noted as Minor also because
the neighbouring `## Response trailers` section uses the identical preamble form followed by its own
carve-outs, so the pattern is this file's established convention rather than this section's
invention.

### N-6 — D6′ is stated unqualifiedly in the contract; the HelloRetryRequest residual hole is unrecorded

`BEHAVIOR_CONTRACT.md:1080-1094`, the *"the one piece of real engineering"* subsection, states the
accept path without qualification. `112.1/REVIEW.md` N-2 traced through pinned source that **D6′ is
defeated across a HelloRetryRequest** — the peek reads ClientHello1 while `rustls` decides on
ClientHello2, and `rustls` pins only SNI across a retry — and closed with *"it is recorded because
`SPEC.md` states D6′ unqualifiedly … and this is a residual hole in that statement."* The contract
now repeats the unqualified statement in a document of higher standing.

Minor because RFC 8446 §4.1.2 forbids a conforming client from changing extensions across a retry,
and **no fixture can express the case** — the handoff banks that explicitly. One appended clause
fixes it.

### N-7 — the `>255` rule's unit, its two sides and the CDS/EDS bypass are not stated plainly

`BEHAVIOR_CONTRACT.md:1109-1113` records the boundary only as a by-product of the CF-112-8
divergence paragraph. **The boundary itself is correct on every axis** — verified at
`crates/envoy-config/src/bootstrap.rs:5958`, `if proto.len() > 255` on a `Vec<String>`, so strictly
greater (255 accepted, 256 rejected) and measured in **bytes**, not chars; the contract's worked
examples are consistent in both directions and the error variant's field set
`{ side, index, len }` matches `crates/envoy-config/src/lib.rs:92-96` exactly. **No off-by-one and
no bytes/chars confusion.** What is missing is a plain statement of the rule, the fact that it runs
on both sides (`"listener"` at `bootstrap.rs:3793`, `"cluster"` at `:4188` — the reader must infer
this from the bare `side` field name), and CF-112-11's measured exception: a CDS-supplied
`type: EDS` cluster reaches no transport-socket validation at all, so *"envoy-rust rejects the
over-long element at config-load"* is unqualified where one supply path bypasses it.

### N-8 — the probe-list diagnostics carry no probe INDEX, and `0091`'s four probes share one SNI

`tests/differential/src/lib.rs:2089` (`expected_cn`), `:2094-2097` (ALPN), `:2101-2119` (write /
read_exact / trailing-byte) all name `probe.sni` and nothing ordinal, while all four of `0091`'s
probes read `sni: a.example.com`. The **ALPN** context is the one that also prints `client_alpn`,
and `0091`'s four offers happen to be pairwise distinct, so the load-bearing assertion *is*
attributable today — by luck of the fixture, not by construction. An `expected_cn` or read failure
names one of four identical strings, and the driver aborts at the first failing probe, so triage
needs a local rerun. `for (i, probe) in probes.iter().enumerate()` and an `probe #{i}` prefix on
every per-probe context in the loop fixes all of them at once.

### N-9 — `root_store.clone()` deep-clones the trust anchors once per probe where `rustls` accepts an `Arc`

`tests/differential/src/lib.rs:2060`. `with_root_certificates` takes
`impl Into<Arc<webpki::RootCertStore>>` (`rustls-0.23.39/src/client/builder.rs:51-54`), so hoisting
`let root_store = Arc::new(root_store);` above the loop turns a per-probe deep clone of the trust
anchors into an `Arc` bump. Correctness is unaffected; it is avoidable work the diff introduced.
(The `WebPkiServerVerifier::new_without_revocation` rebuild at `builder.rs:59` is inherent to
building a fresh `ClientConfig` and is not avoidable given the per-probe ALPN requirement.)

### N-10 — the per-probe `ClientConfig` rebuild also removes cross-probe TLS session resumption, which is load-bearing — but the comment says "every other probe-level discipline below is unchanged"

`tests/differential/src/lib.rs:2053-2057`. Moving the `ClientConfig` inside the loop moved its
`Resumption` store with it. `Resumption::default()` is `in_memory_sessions(256)`
(`rustls-0.23.39/src/client/client_conn.rs:508-519`) and `ClientSessionMemoryCache` is keyed by
`ServerName` (`client/handy.rs:73-74`), so the old hoisted config shared one session cache across
probes and the new one cannot.

This is **inert for `0006-tls-sni`** — its two probes use distinct server names
(`a.example.com`, `b.example.com`), hence distinct cache keys, so no cross-probe resumption was ever
possible there. It is **actively desirable for `0091`**, whose four probes all use
`sni: a.example.com`: under the old shape probes 2–4 would have offered a PSK, and a TLS 1.3 resumed
handshake sends no `Certificate` message — which would have put the four `expected_cn` assertions at
risk and introduced a proxy-dependent ticket-support confound into an ALPN differential.

So the change is right, and the handoff's question ("did `0006-tls-sni`'s multi-probe semantics
survive?") answers **yes, for a reason rather than by luck**. The finding is only that the comment
asserts *"Every other probe-level discipline below is unchanged"*, which under-describes a
side effect the new fixture depends on. A scope note worth recording alongside it: this driver can
therefore never witness a resumption divergence.

### N-11 — `drive_tls` is now field-for-field a one-probe `drive_tls_probes`

After this diff the two parameter sets are identical (`addr`, `payload`, `sni`, `root_store`,
`expected_cn`, `client_alpn`, `expected_alpn` vs `TlsTcpProbe`'s four fields plus the three shared
arguments), and the bodies — config build → connector → handshake → `expected_cn` → `expected_alpn`
→ write / read_exact / trailing-byte poll / shutdown — are the same modulo context strings. The
phase added roughly ten duplicated lines to each. `drive_tls` could become a one-element call into
`drive_tls_probes`. Recorded as the concrete reuse the phase *made available*, not as a defect: the
duplication predates the phase and both call sites are stable.

### N-12 — probe 3 is the D6′ witness and sits third in an abort-at-first-failure list, behind a whole upstream pass

`run_tls_tcp_probe_list_arm` runs **all** probes against upstream Envoy before starting the subject
side, and `drive_tls_probes` returns on the first per-probe failure. Probe 3 of `0091` is the only
cross-proxy witness of `112.1`'s D6′ accept path, so a failure in probe 1 or 2 — on either side —
masks it, and a red run names exactly one probe. In practice the upstream side is the reference and
is expected green, which bounds the exposure; the note is that the phase's most load-bearing cell is
the most maskable of the six, and that ordering it first would cost nothing.

### N-13 — the state-4 F6 row asserts "every anchor asserted to occur **exactly once** by `grep -cF`"; that is false for 2 of the 10 anchors it names

`docs/envoy-rust/phases/112.2-alpn-differential-witness/PROGRESS.md:552`. Eight of the ten listed
anchors do return 1. Two do not:

```
$ grep -cF 'TlsTcp {'          tests/differential/src/lib.rs   ->  5
$ grep -cF 'TlsTcpProbeList {' tests/differential/src/lib.rs   ->  8
$ grep -cF 'pub enum AlpnRule' tests/differential/src/lib.rs   ->  1     (control)
```

**Every line number the row produced is nonetheless correct** — `sed -n '91p'` and `sed -n '109p'`
land on the two enum-variant declarations — and the column-0-anchored forms `^    TlsTcp {$` and
`^    TlsTcpProbeList {$` each return **1**, so a correct method was available and produces the
recorded answers. The defect is the stated method, not the result.

It is recorded because the state-4 section's entire value proposition is that it *re-verifies* on
disk rather than rediscovering, and because it is the very trap the same table cell warns about two
sentences later for `fn check_alpn` and `#[serde(deny_unknown_fields)]`. `PROGRESS.md` is landed and
uneditable; the correction lives here.

### N-14 — `PLAN.md` states its own code subtotal as **596** in the sentence that adjudicates the §6.1 gate and **595** in the table that sentence summarises

`PLAN.md:86` — *"has **7 tasks** and a **measured 596** net code lines"* — against `PLAN.md:107` —
*"| **code subtotal** | | **595** | summed mechanically: 263+32+166+129+5 = 595 ✓ |"*. Summed
independently here: 263 + 32 + 166 + 129 + 5 = **595**. The table is right; the headline is off by
one, and the headline is the load-bearing figure. `PROGRESS.md` F1, `ADR-0189` and `ADR-0190` all
quote 595 and none notices the 596. No verdict changes — 595 and 652 are both far below the ~1500
ceiling — but the project's own "a stated subtotal is a claim, sum the table" discipline was applied
to the table and not to the prose above it.

### N-15 — `PROGRESS.md` Task 4's "every one matching the plan's per-file row exactly" holds for five of six

`PROGRESS.md:65-67` lists `envoy.yaml` 49, `envoy-rust.yaml` 42, `expectations.yaml` 31, `README.md`
43, `payload.bin` 1, `tls_alpn.rs` **25** and calls all six exact. `PLAN.md:953` says
*"Create: `tests/differential/tests/tls_alpn.rs` (32)"*, and `13d705f` landed **25**.

The code is right and the plan is right: `PLAN.md:953`'s "(32)" is the file's **post-Task-5** size
annotated on the task that creates only 25 of it — Task 4's own fence is 25 lines, Task 5 appends 7,
and the file reaches 32. The over-claim is the universal quantifier. A reader reconciling 25 against
953's 32 finds a contradiction with no note explaining it.

### N-16 — F6's heading attributes its citations to `SPEC.md` §2.4; two of the nine live in §2.5, and the one *unmoved* citation is one of those two

`PROGRESS.md:217` — *"every `SPEC.md` §2.4 citation into `lib.rs` is now stale except one"*.
Measured: §2.4 spans `SPEC.md:135-164` and §2.5 begins at `:165`. The `lib.rs` citations fall
`:144`, `:147` ×2, `:148`, `:150`, `:151`, `:152` (**seven, all in §2.4, all stale**) and `:168`,
`:169` (**two, both in §2.5** — `:38` unmoved and `:732`→`:748` moved).

So the correct statement is: **all seven §2.4 citations are stale**, and of §2.5's two, one moved and
one did not. The defect is **inherited** — `PLAN.md:205` says *"The nine `tests/differential/src/lib.rs`
citations in `SPEC.md` §2.4 are ALL VALID"*, also counting nine where §2.4 holds seven — so per the
project's own rule the original is the PLAN's, and F6 transcribed it faithfully. It matters because a
future session repairing citations by section will look in §2.4, find its seven all stale, and
conclude the "except one" is missing.

### N-17 — F6 censused `SPEC.md` only; `PLAN.md:48` carries the same `:732`, and it is now stale onto a plausible wrong target

`PLAN.md:47-48`: *"`TlsTcpProbe` carries `#[serde(deny_unknown_fields)]` (`:732`)"*. At HEAD,
`lib.rs:732` is `/// Probe `listener_address` (e.g. `127.0.0.1:8080`) in 100ms` — a doc comment —
while the attribute sits at `:748`. `PLAN.md:47`'s companion `:38` remains correct.

This is the standing rule "a phase invalidates citations it does not own — census the shared file,
don't sample" applied to a file the phase **does** own: `112.2` invalidated a citation inside its own
landed `PLAN.md` and F6's census did not reach it. The wrong target is plausible-looking rather than
obviously broken, which is the failure mode `PROGRESS.md` F6 itself warns about for `:732`'s
attribute-text form landing on `:30`.

### N-18 — `PLAN.md`'s Self-Review item 4 — "every Rust and YAML block above was taken from a scratch-worktree tree that … ran 176 lib tests green" — is falsified by the very fact F1 root-causes, and F1 does not say so

`PLAN.md:1707-1710`. F1 establishes that the prototype tree held **five** of the seven new tests
(176 = 171 + 5, against the tree's 178 = 171 + 7), so the two fences at `PLAN.md:592-615`
(`check_alpn_adjudicates_all_four_outcomes`, 23 lines) and `PLAN.md:835-867`
(`tls_probe_list_carries_distinct_per_probe_offers`, 32 lines) were **never in the tree that
compiled, passed `clippy -D warnings`, passed `fmt --check` and ran green**.

F1 records that "neither the LoC row nor the test count was re-measured", which is true and is the
arithmetic half. The other half is that the *method claim* went stale too: "every block was RUN" is
what made this plan unusually load-bearing (`ADR-0190` Context), and it is exactly as post-measurement
as the 595. The same drift reaches `PLAN.md:102`, whose row text reads "7 tests" beside a MEASURED
figure covering five. Both blocks did land and both are green today — this is a defect in the record's
account of itself, not in the code.

### N-19 — `SPEC.md` §9(a)'s assertion-deletion limb is unsatisfiable as written, and the "(a) PASS" verdict never adjudicates it

`SPEC.md:330-332`: *"**all three mutation-proved**: deleting the `alpn_protocols` line from a
fixture's `envoy-rust.yaml`, **or** the `expected_alpn` assertion from the driver, must turn the
fixture RED."* Removing an assertion cannot redden a test, and `PROGRESS.md:90` measured exactly
that — M2, "delete the probe-list assertion", predicted `fixture PASSES` and measured `passes`. The
PLAN's attempt to rescue the limb through a compile error is what F2 measured false in both legs;
M2′ (deleting **both** call sites) is what produces `error: function `check_alpn` is never used`.

**Gate (a) genuinely passes under the clause's own disjunctive reading** — the executed set (M1's
config deletion on `0091`, `0092`'s list inversion, `0004`'s cell-6 flip) discharges the "or", and
this review adjudicates it so in §8. The finding is that state 4 recorded "(a) PASS on all three
fixtures" without noting that one limb of the clause it was grading is a drafting slip. Minor, and
the correct answer really is "the clause is unsatisfiable", not "the phase fell short".

---

## §5 — Severity dissent, and subagent findings adjudicated on re-verification

**Downgraded — Important → Minor: "`CF-112-7` missing from the contract's unmeasured list" (N-4).**
The reviewer graded it Important on the strength of `SPEC.md` §4's *"every cell left unmeasured"*
and defended it as *"checkable in a single command and contradicted by the project's own landed
artifacts."* Both halves are true and I confirmed the omission on disk. It is nonetheless Minor:
the section makes **no false statement**, the omitted item stays banked and visible in `SPEC.md` §8
and in `ADR-0184`, and the harm requires a reader to treat a bulleted list as an exhaustive
inventory. Compare M-2, which I *kept* Important in the same document: the distinguishing property
is that M-2 asserts something untrue, and N-4 merely fails to assert something true. That line is
the one `112.1/REVIEW.md` drew between its M-1 and its N-6, and I have kept it.

**Upgraded — Minor → Important: "the contract mis-describes `from_listener`'s first-chain rule"
(M-2).** The reviewer graded it Important; a second reviewer did not raise it; `112.1/REVIEW.md`
N-1 graded the *underlying code behaviour* **Minor** and banked it as CF-112-10. I am keeping
Important, and the reason is that the object under review has changed rather than the behaviour:
N-1 graded a warning-message and code-shape observation with no claim attached, whereas 112.2 wrote
the claim, in `BEHAVIOR_CONTRACT.md`, after the measurement that refutes it. A false sentence in the
document that governs fixture expectations is a different defect from an awkward `match` arm.

**Downgraded — main-session Important → Minor: "the probe-list `equivalence` block is inert"
(N-1).** I derived this independently and initially graded it Important, on the grounds that two
NEW fixtures declare an assertion that does nothing. Two reviewers independently reached the same
finding and both graded it Minor, and on re-verification their reasoning is better than mine: the
mechanism dates to phase 03.2, `0006-tls-sni` carries the identical inert block, the phase's actual
deliverable — the ALPN witness — is fully enforced per probe and per side, and the residual
exposure is a length-correct-but-value-wrong echo. Downgraded, and recorded here because the
convergence of two independent reviewers on the *grade* was what moved me, not the finding itself.

**Downgraded — Important → Minor: "the state-4 F6 row's anchor-uniqueness method claim is false"
(N-13).** The reviewer proposed Important and offered the downgrade itself. It is Minor: `TlsTcp {`
and `TlsTcpProbeList {` really do return 5 and 8, so the sentence is false as written — but **every
line number it produced is correct**, the column-0-anchored forms are unique and yield exactly those
numbers, and no citation anywhere is wrong as a result. Compare `112.1/REVIEW.md` M-5, which was
Important because eighteen citations were themselves FALSE. A wrong claim about method with a right
result is not the same defect as a wrong result.

**The record audit found NO code defect at all**, and that is worth stating as a result rather than
as an absence: every arithmetic claim in `PROGRESS.md` that could be re-derived reproduced exactly —
652 net, the 57-line gap decomposed as 24 + 33, `331/11`, `178/171/+7`, `11549/11229`, `119 0`,
`92/91/89`, all fourteen F6 line numbers, the four md5s (including the *mutated* `0091` md5
`585a187f…`, reproduced by piping `sed '/alpn_protocols/d'` into `md5sum` without touching the tree),
the 16/14 `deny_unknown_fields` split, the 21-line `19cd44d8…` h2spec pin, and all five
stop-condition censuses. **N-13 … N-19 are, without exception, defects in what the record says about
itself — never in what it measured.**

**Subagent claims re-verified and CONFIRMED, listed because each was checkable and each held:**
the `serde_derive`/`serde` codegen chain behind M-1 (read at both ends: the `Style::Unit` arm that
drops `cattrs`, and the visitor that swallows every key); the `from_listener` `None`-arm behind M-2,
with its anchor asserted unique; `0005-tls-upstream` being `kind: tcp_echo` and therefore unable to
carry a client offer at all — the principled answer to the handoff's "should `0005` and `0006` also
carry one?"; `payload.bin` being 18 bytes and byte-identical (md5 `8a1f7b23acbf…`) across all five
TLS fixtures; zero `http_connection_manager` in either new fixture against a 161-file positive
control; the `>255` validator's `>`-not-`>=` and bytes-not-chars properties; and all 21 contract
symbols resolving.

**No subagent finding was rejected outright.** All fourteen findings below the two Importants are
either mine, a reviewer's confirmed on disk, or both — N-1 and N-3 were reached independently by
the main session and by a reviewer, which is corroboration only because the two derivations started
from different files (the driver arm vs. the fixture YAML; the `lib.rs` doc comment vs. the contract
sentence).

**Expected downgrades, delivered.** At the phase-111 review all seven proposed Importants were
downgraded; at `112.1`, three of five. Here four Importants were proposed across the reviewers and
**two survive**, with one Minor upgraded and one of my own Importants downgraded — a net that is in
line with the precedent rather than an exception to it.

---

## §6 — Deliberate decisions verified, not re-litigated

**`SPEC.md` §5 non-goal 1 — "any change to `crates/`" — holds, and was verified rather than
assumed.** The `crates/`, `Cargo.toml` and `Cargo.lock` diff between `00060ad` and `5e51add` is
empty. `112.1` landed complete; nothing was absorbed.

**CF-112-8 Consequence 2's hedge is intact and still reads as INFERRED.** The handoff asked
specifically whether the contract's wording had drifted. It has not
(`BEHAVIOR_CONTRACT.md:1129-1133`): *"**Whether a comma inside an element yields TWO offered
protocols on the wire is INFERRED, not measured** … If the segment is the unit of the length check
it is **probably** the unit of the wire encoding too"* — bolded label, conditional verb. The
adjacent Consequence 1 is correctly labelled `MEASURED`, because `112.1/REVIEW.md` M-1's
`--mode validate` table is a real measurement with a proved-non-vacuous control. **I am not
re-opening the banked rejection**: CF-112-8 Consequence 2 remains structurally unwitnessable by this
harness — `expected_alpn` is ONE rule evaluated against BOTH proxies (§3 E3), so a cell where the
two legitimately differ has no fixture form — and its absence is **not** graded as a coverage gap
of this phase.

**The `Selected` / `NoneSelected` grammar deliberately cannot express "any protocol", and the code
matches the intent.** Two variants, no wildcard arm, and `check_alpn` has no third branch.

**Every symbol the contract section names resolves.** The section cites no `file:line` by design
(CF-112-12), which makes a dangling *symbol* the failure mode to check. All 21 pass, against a
bogus-name control returning 0: `common_tls_context.alpn_protocols`
(`bootstrap.rs:1196`, `Vec<String>`), `DownstreamTlsContext` / `UpstreamTlsContext`,
`rustls::ServerConfig` / `rustls::ClientConfig`, the empty-list-means-no-extension rule
(`rustls handshake.rs:783-791`), `DownstreamTls::accept` (`envoy-tls/src/lib.rs:222`),
`tokio_rustls::LazyConfigAcceptor` (declared `tokio-rustls-0.26.4/src/server.rs:70`, used at
`envoy-tls/src/lib.rs:245`), `into_stream` (`server.rs:221`, used at `:273`), the ALPN-free twin,
the `our_protocols.is_empty()` branch (`rustls hs.rs:114`), the selection loop (`hs.rs:99-109`), the
pre-112 `TlsAcceptor` path (`:229`), `ConfigError::InvalidAlpnProtocol { side, index, len }`
(`envoy-config/src/lib.rs:92-96`, **field names and set exact**),
`ConfigError::Http2OverTlsNotSupported`, `DownstreamTls::from_listener` (`:145`),
`PayloadU8<NonEmpty>`'s `debug_assert!`, the cluster-only reachability of the empty element,
`Invalid ALPN protocol string`, the stat names, the image digest, `ADR-0183`/`ADR-0184`, the
CF references, and the three fixture directories. **No symbol dangles.** M-2 is a wrong
*description*, not a wrong symbol.

**The `0004` edit does not perturb anything else in that fixture.** Its only other assertion is
`equivalence.response_body: byte_exact`, and on the `TlsTcp` arm that one *is* enforced
(`:5095`). The edit changes only the ClientHello.

**The runner fails loudly on a typo'd fixture name.** `run_fixture` opens
`expectations.yaml` through `fs::read_to_string(...).with_context(...)`, so a missing directory is
an `Err` and the test's `.expect("fixture passes")` panics; there is no silent-skip path. CI picks
the new binary up from `cargo test --workspace` with no per-target registration — unlike a fuzz
target, which is why gate (d) needed the explicit check it got.

**Both READMEs are accurate.** `0091`'s four-row table matches its four probes including probe 4's
*"(no ALPN extension)"*; `0092`'s `rustls` selection-loop citation is exact; neither documents a
cell its YAML does not encode. `0091`'s *"this driver needs no final `assert_equivalence`"* is
scoped to the ALPN claim and is **true** as scoped — N-1 is about the `response_body` rule, not
about this sentence.

---

## §7 — Carry-forwards for the state-6 close-out to bank

**This review fixed nothing** (§6.3; `ADR-0165`). Six new carry-forwards open, and the entire
inherited set carries forward intact.

| new | finding | one-line remedy |
|---|---|---|
| **CF-112-13** | M-1 — `deny_unknown_fields` does not reach an internally-tagged **unit** variant, so `{ kind: none_selected, protocol: h2 }` silently inverts the assertion; repo-wide (`AlpnRule::NoneSelected`, `BodyRule::ByteExact`, `Driver::TcpEcho`, `Driver::TcpDirectResponse`) | `NoneSelected {}` + extend the existing pin to the unit arm |
| **CF-112-14** | M-2 — `BEHAVIOR_CONTRACT.md:1141-1144` states the first chain's **non-empty** list wins and that a warning is logged; both are false (`112.1/REVIEW.md` N-1, CF-112-10) | two sentences |
| **CF-112-15** | N-1 — the probe-list arm never evaluates `equivalence`; two new fixtures declare an inert `byte_exact`; the docstring's justification is false reasoning | compare the outputs, or correct the docstring and drop the block |
| **CF-112-16** | N-2 — harness `client_alpn` is unvalidated: empty element → `debug_assert!` panic, `>255` element → silent `as u8` truncation | a guard at each of the two mapping sites |
| **CF-112-17** | N-3…N-7 — contract wording and completeness: the "every pre-112 fixture is unaffected" sentences, the missing `CF-112-7` bullet, the over-broad methodology preamble, the unqualified D6′ statement, the unstated `>255` unit/sides/EDS bypass | five localised edits |
| **CF-112-18** | N-8…N-12 — harness diagnostics and shape: no probe index, per-probe `root_store` deep clone, the undocumented resumption side effect, `drive_tls`/`drive_tls_probes` duplication, probe-3 masking | independent, none urgent |
| **CF-112-19** | N-13…N-19 — record defects in landed, uneditable artifacts: the F6 anchor-uniqueness method claim, `PLAN.md`'s 596-vs-595, Task 4's "every one exactly", F6's §2.4-vs-§2.5 mislabel, the uncensused `PLAN.md:48` `:732`, the falsified "every block was RUN", and `SPEC.md` §9(a)'s unsatisfiable limb | corrections live in **this** document; nothing to edit |

**The family's first-day agenda: `CF-112-13`, landed together with `CF-112-14`.** CF-112-13 is the
one finding with a *silent inversion* failure mode, it is reachable from fixture data alone, its
blast radius is the whole fixture corpus's grammar rather than this phase's two fixtures, and the
phase shipped a green test asserting it cannot happen — which means it will not be rediscovered by
anyone reading the test suite. The fix is an enum-shape change plus one test case. CF-112-14 rides
in the same commit because it is a two-sentence edit to the document that governs the fixtures
CF-112-13 protects, and because leaving a false sentence in `BEHAVIOR_CONTRACT.md` while editing the
grammar it describes would be the worse of the two orderings.

**A second, cheaper candidate remains on the table from `112.1/REVIEW.md` §7**, unchanged by this
review: CF-112-8's comma split plus CF-112-9's empty-element skip are ~10 lines across two files and
should land **together**, with the wire measurement that would settle CF-112-8 Consequence 2 — a
measurement the `112.2` harness makes possible but, per §6, cannot itself express as a fixture.
Note that CF-112-16 is the harness-side sibling of CF-112-9 and would naturally join that commit.

**Inherited and still open, banked without change:** CF-112-1, -2, -3, -4, -6, -7, -8 (both
consequences), -9, -10, -11, -12; the `112.1` REVIEW's M-1…M-5 and N-1…N-12; phase 111's M-1…M-15,
N-1…N-13 and CF-111-1…CF-111-9 (CF-111-1, trailers bypassing the filter pipeline, is explicitly
**not** this family's to consume); the `110.2`, `110.1`, `109.2`, `109.1` and `108.2` REVIEW sets;
CF-110-1…9; CF-109-1/2/3; CF-108-1/2/3; CF-76-1; CF-75-2/3/4/5/6; CF-72-2/CF-75-1; M71-6;
CF-74-1/2/3/4/6; CF-73-1; and the HTTP-filters-family (1)–(4). **CF-112-5 stays CLOSED.**

---

## §8 — Assessment, and the §7.5 gate adjudicated

**`112.2` is a small, well-shaped phase that did exactly what its SPEC said and stayed inside every
non-goal it declared.** Six ALPN cells are now witnessed cross-proxy where before phase 112 the
divergence was boot-level; the harness gained an offer and an assertion without gaining a driver;
no crate changed; and the one edit to a landed fixture was placed on the single fixture whose code
path it cannot disturb. The execution record is unusually honest — `PROGRESS.md` F1 root-causes a
57-line overrun to two named test blocks and F2 records that the PLAN's own mutation prediction was
false in both legs, rather than quietly not running it. Both survive independent re-derivation here.

A separate audit of the record against the tree found **no code defect anywhere** and reproduced
every re-derivable arithmetic claim exactly. All seven of the remaining findings (N-13…N-19) are
defects in what the landed artifacts say **about themselves** — a method sentence, a headline
subtotal, a universal quantifier, a section label, an uncensused self-citation, a falsified
self-review item, and an unsatisfiable gate clause. None changes a measured number and none changes
a verdict. That is a good result for a record, and it is worth separating from the two Importants,
which are of a different kind.

The two Important findings share a shape worth naming: **each is a claim the phase made that its own
project had already measured false or left untested.** M-1 ships a green test whose comment
quantifies over a type while testing one arm of it; M-2 writes into the contract of record a
sentence that `112.1/REVIEW.md` N-1 had refuted before the sentence existed. Neither is a coding
error. Both are the failure mode this project's doctrine is most explicitly built against — *a
claim is not evidence* — occurring in the two artifacts a future session is most likely to trust
without re-deriving.

### The §7.5 gate, all six adjudicated

- **(a) — PASS.** State 4 ran the three fixtures (`0091`, `0092` and the CHANGED `0004`) green under
  full-parallel load and again alone after a settle gap, with `docker ps` polled *during* the run
  catching two distinct `envoyproxy/envoy:v1.33.0` containers — the audit a suspiciously fast
  differential green requires. M1 was re-run by that session independently of state 3, deleting the
  server list from `0091`'s `envoy-rust.yaml` only, with the target asserted to occur exactly once
  and an md5-adjudicated restore, and reddened exactly as predicted at probe 1 on the subject side.
  Not re-run here. **This review adds** that the fixtures' two config files diverge only in the four
  established harness hunks, that the `alpn_protocols` line is byte-identical across sides, and that
  every probe carries an `expected_alpn` — so the green is not vacuous on the property it exists to
  witness. N-1 bounds what else it proves.
- **(b) — PASS.** 89 other pre-existing fixtures, censused by RUNNER FILE NAME against the
  crate-relative prefix with a wrong-prefix control returning 0: 91 of 91 runner files executed in
  CI. The 92-fixtures/91-runners asymmetry is by design and `tls_alpn.rs` driving two is now part of
  it. `0004` correctly counted under (a), not here.
- **(c) — PASS in CI, vacuous locally by construction.** `h2spec_pass_rate_gate ... ok` with
  `h2spec not found` = 0 in CI; locally it prints the skip line and then `ok. 3 passed` in 0.00s.
  **Re-verified here:** `known-failures.txt` is **21** lines, md5
  `19cd44d86a8b15d825f76c6e7b265e65` — exactly `SPEC.md` §9(c) — and untouched by this phase. CI is
  authoritative (`ADR-0163`).
- **(d) — PASS, vacuous by construction.** Re-verified here:
  `git diff --stat 00060ad 5e51add -- fuzz '*/fuzz/*' .github` is EMPTY against 5 fuzz targets, so
  the "new fuzz target needs a `ci.yml` step" trap cannot fire. The CI fuzz job ran all five clean.
  The state-4 session recorded the `gh run view --job --log` 0-byte trap here and defeated it via
  the REST endpoint; that discipline is why this gate's evidence is trustworthy.
- **(e) — PASS locally and in CI.** Build 1 `Compiling`, clippy 0 findings over 1 `Checking` after
  an mtime-only `touch` forced the dirty set, `fmt --check` an 11-byte log, `deny check` four-ok.
  Two `--no-fail-fast` sweeps at `binaries=168` with identities `2269+5` and `2268+6` — both
  **2274**, CI's `passed` exactly — and every RED classified by ISOLATION with 30-second settle
  gaps, the stable core being the five recorded host members and `access_log_metadata_filter`
  passing alone.
- **(f) — CLOSED by this document. APPROVED.**

**All six gates are now adjudicated.** §2 is empty, so `112.2` advances to **§5 state 6**, whose
close-out flips ROADMAP rows `112.2` **and parent `112`** to `done` — the status cell only — and
closes the phase-112 family opener. This review touched no ROADMAP row.

### STOP CONDITION — re-derived from disk at this review. ALL THREE LEGS FALSE

No `stop` file was created; `ls stop` → `No such file or directory`.

- **Leg (i) FALSE.** **120** rows / **118** `done` / **1** `in-progress` / **1** `planned`, buckets
  summing to the row count. Status is field **4** on a `' | '` split driven from the `^\| [0-9]`
  prefix. The not-done set is exactly `112` (`in-progress`) and `112.2` (`planned`) — this
  sub-phase is still the last `planned` row, and a state-5 review does not move it. Control: the
  forbidden `NF == 6` filter reads **118**, coinciding with the `done` count by accident because it
  drops the two rows carrying unescaped in-cell pipes. Rows 38 and 39 were **not** "fixed".
- **Leg (ii) FALSE.** **14** crates, with `envoy-http3`, `envoy-grpc`, `envoy-wasm`,
  `envoy-protos` and `envoy-runtime` all absent by `test -d`; `quinn`, `wasmtime`, `tonic`,
  `opentelemetry` and `prost` = **0** across all **28** manifests from `git ls-files '*Cargo.toml'`
  — against a `tokio` positive control of **19** of those same 28, run with the identical
  invocation. (Denominator stated because it is method-dependent: `crates/*/Cargo.toml` plus the
  root gives a different one.)
- **Leg (iii) FALSE.** **11** `### ` family headings driven from a single `/^### /` rule, reading
  10/5/3/14/3/4/6/29/6/**0**/13 with **27** rows before the first heading, summing to 120.
  `### WASM host family` still carries **zero** rows. The Observability filing defect persists and
  was not repaired.

**All three legs must hold; zero do.** The mission is not complete.

### Next state

**§5 state 6 — the close-out — is a SEPARATE session** (§5.1; `ADR-0127`: a reviewer must not close
out what it reviewed). It is small and indivisible: flip ROADMAP rows `112.2` **and** parent `112`
(the status cell only), relocate the Notes subsection, close the phase-112 family opener, and bank
CF-112-13 … CF-112-18 alongside the inherited set. It writes no ADR and adds no Notes subsection.
**The CI identity must not move from `binaries=168 passed=2274 failed=0`.**
