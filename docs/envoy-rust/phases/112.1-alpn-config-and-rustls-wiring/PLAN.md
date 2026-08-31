# Phase 112.1 — ALPN config surface + `rustls` wiring on both sides Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every task is TDD per `superpowers:test-driven-development` — the failing test is written and RUN-TO-FAIL before the implementation.

**Goal:** Land `CommonTlsContext.alpn_protocols` and honor it on BOTH the downstream (`rustls::ServerConfig`) and upstream (`rustls::ClientConfig`) sides in the same commit, with an ALPN mismatch completing the handshake and selecting nothing — matching upstream Envoy v1.33.0, which the pinned `rustls 0.23.39` does not do by default.

**Architecture:** One `#[serde(default)] Vec<String>` field on the struct that is the type of both `DownstreamTlsContext.common_tls_context` and `UpstreamTlsContext.common_tls_context`, so both sides gain it in one landing. A config-load validator rejects only an element longer than 255 bytes. `DownstreamTls` carries the configured list wire-encoded plus an ALPN-free twin `ServerConfig`, and `accept()` branches: with no ALPN configured it keeps the pre-112 `tokio_rustls::TlsAcceptor` path byte-for-byte, and with ALPN configured it drives `tokio_rustls::LazyConfigAcceptor`, peeks the ClientHello, and hands `into_stream` the ALPN-free twin exactly when the client offered a non-empty set that does not intersect — so rustls takes its `our_protocols.is_empty()` branch, sends no `no_application_protocol` alert, and selects nothing. `UpstreamTls::from_context` assigns the same list to `ClientConfig::alpn_protocols`.

**Tech Stack:** Rust (pinned toolchain per `rust-toolchain.toml`), `rustls 0.23.39` and `tokio-rustls 0.26.4` (both already direct dependencies of `envoy-tls`; versions resolved from `Cargo.lock`, not from the registry directory listing), `serde`/`serde_yaml`, `tokio`, `rcgen` + `tempfile` (dev-only, already present). **ZERO new dependencies. ZERO manifest changes. ZERO `Cargo.lock` changes.**

**Spec:** `docs/envoy-rust/phases/112.1-alpn-config-and-rustls-wiring/SPEC.md` (499 lines, LANDED AND UNEDITABLE — read it alongside this plan). This plan argues from it and **corrects it in four places**, see §2. Its parent is `docs/envoy-rust/phases/112-tls-alpn-negotiation/SPEC.md` (548 lines, LANDED AND UNEDITABLE) — read the parent knowing that `ADR-0184` REPLACED two of its design decisions (D4→D4′, D6→D6′) and corrected three of its figures; **where the parent and `112.1/SPEC.md` disagree, `112.1/SPEC.md` wins.** Sibling: `docs/envoy-rust/phases/112.2-alpn-differential-witness/SPEC.md` (371 lines) — that is what is NOT yours.

**Scoping ADRs:** `ADR-0183` (the parent pick), `ADR-0184` (the split + three corrections), `ADR-0185` (THIS PLAN-write: the §6.1 adjudication, the four SPEC corrections, the two design decisions this plan owns, and the demonstrated D6′ prototype).

---

## Global Constraints

Every task's requirements implicitly include this section.

- **Upstream reference pin:** `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2` (`docs/envoy-rust/ENVOY_TARGET.md`). Never changed here.
- **`#![forbid(unsafe_code)]`** at every crate root (doctrine D-3.8). No `unsafe` in this sub-phase.
- **No new dependency, no new cargo FEATURE, no `Cargo.toml` edit, no `Cargo.lock` change.** §1 M-N1 proves none is needed.
- **NO new differential fixture, and NO edit to `tests/differential/**` or `tests/fixtures/**`.** That is sibling `112.2`'s entire scope; touching it re-merges the split (`112.1/SPEC.md` §5 non-goal 2). If a task appears to need it, STOP and raise it.
- **`DownstreamTls::accept`'s signature does not change** in either direction, so no consumer is touched (`112.1/SPEC.md` §5 non-goal 6). Its sole production consumer is `TlsAcceptingHandler::handle` in `crates/envoy-bin/src/tls_handler.rs` (anchor text `let post_handshake = tls`); leave that file alone.
- **Do NOT lift `ConfigError::Http2OverTlsNotSupported`** (`crates/envoy-config/src/bootstrap.rs`, anchor text `return Err(crate::ConfigError::Http2OverTlsNotSupported);`). **CF-112-1.**
- **Do NOT touch** `crates/envoy-http1/src/uring.rs` (the io_uring H1 path — **CF-112-7**), `crates/envoy-listener/`, or `crates/envoy-bin/src/main.rs`.
- **Never trim** `tests/conformance/h2spec/known-failures.txt` (21 lines, md5 `19cd44d86a8b15d825f76c6e7b265e65` — both re-verified at this PLAN-write).
- **Landed work is uneditable:** phases 00–74, 75.x, 76.x, the whole `108`/`109` families, the ENTIRE `110` family, the ENTIRE `111` phase, and **all three phase-112 SPECs** (parent, `112.1`, `112.2`). Preserve `ADR-0016` through `ADR-0185`.
- **Do NOT fix any banked finding** (§6.3; `ADR-0165`). Phase 111's M-1…M-15 / N-1…N-13, CF-111-1…CF-111-9, the `110.2` / `110.1` / `109.2` / `109.1` / `108.2` REVIEW sets, CF-110-1…9, CF-109-1/2/3, CF-108-1/2/3, CF-76-1, CF-75-2/3/4/5/6, CF-72-2/CF-75-1, M71-6, CF-74-1/2/3/4/6, CF-73-1, the HTTP-filters-family (1)-(4), and CF-112-1/2/3/4/6/7 all stay OPEN. **CF-111-1 is explicitly NOT this phase's to consume.**
- **Do NOT repair** `ROADMAP.md`'s mis-filed `Observability family:` rows or its two unescaped-pipe rows (38, 39).
- **Locate every code site by TEXT (`grep`), never by an inherited line number.** Every `file:line` in this plan was resolved at HEAD `9f2010a5fb2c0928b017bf21ac5f097ed85d25ea`; they drift, and this phase's own Task 1 shifts every line below `CommonTlsContext` in `bootstrap.rs`.
- **The CI identity at HEAD is `binaries=167 passed=2252 failed=0`.** This sub-phase's state-3 commit is a CODE commit and **must** move it. A code commit that does NOT move it is the alarm.

---

## §0. State at this PLAN-write

- HEAD `9f2010a5fb2c0928b017bf21ac5f097ed85d25ea`, branch `main`, `git status --porcelain` empty.
- The phase directory `docs/envoy-rust/phases/112.1-alpn-config-and-rustls-wiring/` held `SPEC.md` ONLY; this plan is the §5 state-2 output. Sibling `112.2/` likewise held `SPEC.md` only.
- Fixture census **90** (`ls -d tests/fixtures/*/ | wc -l`) — CONFIRMS `112.1/SPEC.md` §9(b).
- ROADMAP census **120 rows / 117 `done` / 1 `in-progress` / 2 `planned`**, buckets summing exactly to the row count (status is field 4 on a `' | '` split, driven from the `^\| [0-9]` prefix). Stop-condition leg (i) **FALSE**.
- Next free ADR: **`ADR-0185`** (`^## ADR-` headers: 181, all unique, highest `0184`; `grep -c 'ADR-0185' DECISIONS.md ROADMAP.md` = 0 + 0).

---

## §1. What this PLAN-write MEASURED — every figure re-derived, and a WORKING PROTOTYPE

`112.1/SPEC.md` §2 carries five measurements (M-1…M-5) taken at the split session. Each is a claim. This session re-tested every one that this plan depends on, and additionally built and ran a complete prototype of the sub-phase's core in a scratch git worktree at HEAD `9f2010a`. **The prototype is the single most important thing in this plan: it turns D6′ from a design argument into a demonstrated result, and it is where all four SPEC corrections in §2 come from.**

### M-R1 — the pinned versions, resolved from `Cargo.lock` FIRST

`rustls` **0.23.39**, `tokio-rustls` **0.26.4** — read from the `[[package]]` blocks in `/home/esa/git/envoy-rust/Cargo.lock`, not from a registry directory listing. **This host's registry cache holds exactly one directory for each** (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rustls-0.23.39` and `…/tokio-rustls-0.26.4`), but the glob `rustls-*` also matches `rustls-native-certs-0.8.3`, `rustls-pemfile-2.2.0`, `rustls-pki-types-1.14.0` and `rustls-webpki-0.103.13`, so `ls -d …/rustls-* | head -1` is correct here only by alphabetical luck. **Resolve from `Cargo.lock` and state the version.** CONFIRMS `112.1/SPEC.md` §2.1.

### M-R2 — M-5 re-derived at HEAD `9f2010a`, then CONFIRMED BY COMPILATION

`grep -rn 'CommonTlsContext {' --include='*.rs' crates/ tests/` returns **5** literals; one is the struct declaration (`crates/envoy-config/src/bootstrap.rs`, anchor `pub struct CommonTlsContext {`) and produces no `E0063`. **The blast is FOUR:**

| `file:line` at `9f2010a` | anchoring text |
|---|---|
| `crates/envoy-tls/src/tests.rs:135` | inside `pub fn ds_context_with(` |
| `crates/envoy-tls/src/tests.rs:240` | inside `async fn rejects_empty_tls_certificates()` |
| `crates/envoy-tls/src/tests.rs:454` | inside `pub fn us_context_with(` |
| `crates/envoy-tcp/src/lib.rs:1189` | inside `fn upstream_ctx_for(pki: &UpstreamPki, sni: &str)` |

Positive control: total `CommonTlsContext` mentions = **9** (`envoy-config/src/lib.rs` 1 re-export, `bootstrap.rs` 4, `envoy-tls/src/tests.rs` 3, `envoy-tcp/src/lib.rs` 1). **CONFIRMS `112.1/SPEC.md` §2.5**, and the prototype then proved it by compiling `cargo build --workspace --all-targets` clean with exactly those four one-line additions.

### M-R3 — **NEW: all four E0063 sites are in TEST-ONLY code**

Not in either SPEC nor in `ADR-0184`. `crates/envoy-tls/src/tests.rs` is reached only through `#[cfg(test)] mod tests;` in `crates/envoy-tls/src/lib.rs`, and `crates/envoy-tcp/src/lib.rs`'s site sits after that file's single `#[cfg(test)]` at `:390`. **Measured consequence: `cargo build --workspace` (without `--all-targets`) is GREEN with ZERO fixups; only `--all-targets` shows the blast.** This is why Task 1 must gate on `--all-targets` — a plain `cargo build` would be a false green.

### M-R4 — the `rustls` ALPN decision code, re-read in the pinned source

`rustls-0.23.39/src/server/hs.rs`, in `process_common` (anchor `let our_protocols = &config.alpn_protocols;`): selection iterates the **server's** list via `.find(|ours| their_protocols.iter().any(…))`, and

```rust
            } else if !our_protocols.is_empty() {
                return Err(cx.common.send_fatal_alert(
                    AlertDescription::NoApplicationProtocol,
                    Error::NoApplicationProtocol,
                ));
            }
```

**CONFIRMS `112.1/SPEC.md` §2.1**, with two refinements the SPEC does not state:

- **The trigger is `hello.protocols.is_some()`, not "a non-empty client set".** The enclosing `if let Some(their_protocols) = &hello.protocols` fires whenever the client sent the extension at all. If the client sends no ALPN extension the whole block is skipped **regardless of the server's list** — which is why the client-offers-nothing cell is parity for free and why `accept()` may safely hand the ALPN-carrying config to a client that offered nothing.
- **`Error::NoApplicationProtocol` is produced inside `Accepted::into_connection`, not inside `Acceptor::accept`.** `process_common` is reached from `hs::ExpectClientHello::with_certified_key`, which `Accepted::into_connection` calls; `Acceptor::accept()` only runs `hs::process_client_hello`, a parse-level step. **Therefore the alert surfaces from `start.into_stream(config).await`, never from `acceptor.await`** — which is exactly why peeking the ClientHello and choosing the config in between works at all.

`ServerConfig::alpn_protocols` (`rustls-0.23.39/src/server/server_conn.rs`, anchor `/// Protocol names we support, most preferred first.`) is `Vec<Vec<u8>>` and documents *"If empty we don't do ALPN at all"*, ratifying D3 and D5. `ClientConfig::alpn_protocols` (`rustls-0.23.39/src/client/client_conn.rs`, anchor `/// Which ALPN protocols we include in our client hello.`) is the same type and documents *"If empty, no ALPN extension is sent"*.

### M-R5 — **NEW: `rustls::ServerConfig` derives `Clone`, and the clone is cheap**

`rustls-0.23.39/src/server/server_conn.rs`, anchor `#[derive(Clone, Debug)]` immediately above `pub struct ServerConfig {`. Every non-trivial field is an `Arc` (`provider`, `session_storage`, `ticketer`, `cert_resolver`, `verifier`, `key_log`); only `alpn_protocols` and a few `bool`/`usize` are deep-copied. **`112.1/SPEC.md` §3 D6′ left open "whether the twin is built eagerly or lazily" and the handoff listed it as an open question. It is answered: clone the built config EAGERLY, BEFORE `alpn_protocols` is assigned, at construction time.** The twin is then byte-identical except for that one field by construction rather than by careful re-derivation, and it costs one `Arc`-field copy per listener, once.

### M-R6 — **NEW: no cargo feature and no manifest change are required**

`grep -n 'cfg(feature' tokio-rustls-0.26.4/src/server.rs` returns **exactly one** hit, at `:398`, and it is `#[cfg(feature = "early-data")]` on an unrelated `poll_fill_buf` arm. Positive control on the same file: `grep -c 'pub fn '` = **15**. Neither `LazyConfigAcceptor` nor `StartHandshake` carries any `cfg`, and both are re-exported ungated at `tokio-rustls-0.26.4/src/lib.rs` (anchor `pub use server::{Accept, FallibleAccept, LazyConfigAcceptor, StartHandshake, TlsAcceptor};`). The one real gate is on the rustls side — `rustls::server::Acceptor` is `#[cfg(feature = "std")]` — and `crates/envoy-tls/Cargo.toml` already enables it (`rustls = { version = "0.23", default-features = false, features = ["std", "tls12"] }`). **`crates/envoy-tls/Cargo.toml` requires no change.** Confirmed by the prototype compiling with the manifest untouched.

### M-R7 — **NEW: no new `TlsError` variant is required**

`112.1/SPEC.md` §7 prices "a `TlsError` variant" into the `envoy-tls/src/lib.rs` row and the handoff lists "The `TlsError` surface" as an open question this plan must decide. **Both await points on the new path yield `std::io::Error`:**

- `impl<IO> Future for LazyConfigAcceptor<IO> { type Output = Result<StartHandshake<IO>, io::Error>; }` (`tokio-rustls-0.26.4/src/server.rs`, anchor `type Output = Result<StartHandshake<IO>, io::Error>;`)
- `impl<IO: AsyncRead + AsyncWrite + Unpin> Future for Accept<IO> { type Output = io::Result<TlsStream<IO>>; }` (same file, anchor `type Output = io::Result<TlsStream<IO>>;`)

The `rustls::Error` is wrapped as the `io::Error`'s source (`io::Error::new(io::ErrorKind::InvalidData, error)`) before either future resolves. The existing `TlsError::Handshake { source: std::io::Error }` (`crates/envoy-tls/src/lib.rs`, anchor `#[error("TLS handshake: {source}")]`) already models exactly this shape. **DECISION: reuse `TlsError::Handshake`; add no variant.** The prototype confirms `accept_returns_handshake_error_on_garbage_input` stays green — and under D6′.1 that test takes the unchanged path anyway.

### M-R8 — **NEW: `tokio_rustls::TlsConnector::with_alpn` exists in the pinned version**

`tokio-rustls-0.26.4/src/client.rs`, anchor `pub fn with_alpn(&self, alpn_protocols: Vec<Vec<u8>>) -> TlsConnectorWithAlpn<'_> {`, with `TlsConnectorWithAlpn::connect` at anchor `pub fn connect<IO>(self, domain: ServerName<'static>, stream: IO) -> Connect<IO>`, re-exported at `tokio-rustls-0.26.4/src/lib.rs` (anchor `pub use client::{Connect, FallibleConnect, TlsConnector, TlsConnectorWithAlpn};`). It varies a single connection's client ALPN offer **without rebuilding `ClientConfig`**. Neither SPEC knows about it. It is what lets all five downstream cells share ONE handshake helper instead of five standalone scaffolds, and it is the direct cause of the §2 C-2 correction. **Sibling `112.2` will want this too.**

### M-R9 — the borrowck hazard on the accept path, and how the code avoids it

`ClientHello<'a>` borrows the `StartHandshake` (`rustls-0.23.39/src/server/server_conn.rs`, anchor `pub struct ClientHello<'a> {`, whose `alpn` field is `Option<&'a Vec<ProtocolName>>`), and `StartHandshake::client_hello(&self)` returns `ClientHello<'_>`. `into_stream(self, …)` takes `self` **by value**. `ClientHello::alpn()` returns `Option<impl Iterator<Item = &'a [u8]>>` — an unnameable iterator over **bytes, not `&str`**. **Holding the `ClientHello`, its iterator, or any `&[u8]` borrowed from it across the `into_stream` call is `E0505`.** The code in Task 5 reduces the peek to an owned `bool` inside a block expression so NLL ends the borrow before the move. This is the plan's single most likely compile failure and it is already discharged by the prototype.

### M-R10 — the calibration factors, re-derived at this session

`git diff --numstat <state-2-commit> <state-3-commit> -- . ':(exclude)docs/**'`, net = additions − deletions:

| slice | range | net | SPEC-stage estimate | factor |
|---|---|---|---|---|
| `110.2` | `0cd3f12` → `6af7649` | **817** | 615 | **1.33×** |
| `110.1` | `7747d69` → `29d25e5` | **1290** | 912 | **1.41×** |
| `111` | `be1aaf1` → `111b34a` | **1525** | 916 | **1.66×** |

Mean **1.47×**, worst **1.66×**. **CONFIRMS `112.1/SPEC.md` §7 and `ADR-0184` DECISION 1 exactly.** Phase 111 cleared the gate on a ≈916 estimate and landed at 1525 — 25 lines OVER the ~1500 threshold — which is why §3 below applies the worst observed factor rather than the mean.

### M-R11 — the in-tree pricing comparables

| comparable | measurement | method |
|---|---|---|
| `crates/envoy-tls/src/tests.rs` | **16 tests, MEDIAN 65 lines, mean 58.7** | CONFIRMS `112.1/SPEC.md` §2 exactly. Sizes: 10, 11, 14, 15, 30, 48, 56, 65, 65, 66, 68, 68, 76, 100, 120, 127 |
| `crates/envoy-config/src/bootstrap.rs` test fns | **623 fns, median 20, mean 25.4, Q3 31** | the YAML-literal kind (e.g. `rejects_unknown_field_in_common_tls_context`) sit near Q3 |
| `ConfigError` variant blocks incl. doc + `#[error]` | **49 variants, median 13, mean 18.4** | `112.1/SPEC.md` §7 priced this row 15; measured build cost **13** |
| top-level `validate_*` helpers in `bootstrap.rs` | **33 fns, median 40** — but the single-rule ones are 13–17 (`validate_set_metadata_config` 13, `validate_route_match_cardinality` 14, `validate_cdn_loop_config` 17) | `validate_alpn_protocols` is single-rule |
| tracked `parse_bootstrap` fuzz corpus seeds | `tls_downstream_single_cert.yaml` **40**, `tls_upstream_validation_context.yaml` **32**, `layered_runtime.yaml` **50** | 66 seeds tracked; `crates/envoy-config/fuzz/.gitignore` ignores `corpus/parse_bootstrap/*` and un-ignores each by name |
| `crates/envoy-tls/src/lib.rs` | **380 lines**, 3 config-build sites, `accept()` **11 lines** | CONFIRMS `112.1/SPEC.md` §2 |

### M-R12 — the fuzz corpus seed IS gitignored by default (verified with the PLAIN form)

`git check-ignore crates/envoy-config/fuzz/corpus/parse_bootstrap/zz_probe.yaml` → exit **0** (ignored). `crates/envoy-config/fuzz/.gitignore` opens with `corpus/parse_bootstrap/*` followed by one `!corpus/parse_bootstrap/<name>` line per tracked seed. **A new seed needs its own explicit `!` line and must be confirmed with `git ls-files`, not with `git status`.** CONFIRMS `112.1/SPEC.md` §9(d).

### M-R13 — **THE PROTOTYPE. D6′ is demonstrated, not projected.**

Built in a scratch `git worktree` at HEAD `9f2010a` (the session's own tree stayed clean throughout; `git status --porcelain` empty before and after). It implements **the whole of Tasks 1–6** — production code and tests — and was run to completion:

```
=== cargo build --workspace --all-targets ===
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 30.62s

=== cargo clippy --workspace --all-targets --all-features -- -D warnings ===
(0 error/warning lines)

=== cargo fmt --all -- --check ===
(0 lines)

=== cargo test -p envoy-config -p envoy-tls -p envoy-tcp ===
test result: ok. 716 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out   [envoy-config]
test result: ok.  16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out   [envoy-tcp]
test result: ok.  22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out   [envoy-tls]
```

**716 = the 708 pre-existing `envoy-config` tests plus 8 new ones. 22 = the 16 pre-existing `envoy-tls` tests plus 6 new ones. Not one pre-existing test regressed** — direct in-process evidence that `#[serde(default)]` keeps the config surface inert and that D6′.1's guard keeps the old accept path intact. That second property is the mechanism `112.1/SPEC.md` §4 relies on for §7.5 gate (b), and this is the strongest signal available without running the differential suite.

**And the mutation obligation `112.1/SPEC.md` §9 imposes was DISCHARGED IN ADVANCE — four mutations, each with the target asserted to occur exactly once, each with a forced rebuild confirmed by a `Compiling` line, each gated on the `test result` line's existence rather than on the exit code, and both controls green from the same tree:**

| mutation | target occurrences | result |
|---|---|---|
| *(control — unmutated)* | — | `envoy-tls ok. 6 passed`; `envoy-config ok. 8 passed` |
| **M1: delete the D6′ config swap** (`alpn_free.clone()` → `self.config.clone()`) | **1** | `FAILED. 5 passed; 1 failed` — **only** `alpn_mismatch_completes_handshake_with_no_protocol` |
| **M2: delete the `ServerConfig` ALPN threading** (`config.alpn_protocols = wire.clone();` → no-op) | **1** | `FAILED. 4 passed; 2 failed` — **only** `alpn_negotiates_h2_when_both_offer_it` + `alpn_selection_follows_server_preference` |
| **M3: delete the `ClientConfig` ALPN threading** | **1** | `FAILED. 5 passed; 1 failed` — **only** `upstream_offers_configured_alpn_to_the_server` |
| **M4: neuter the >255 validator** (`> 255` → `> usize::MAX`) | **1** | `FAILED. 6 passed; 2 failed` — **only** the two `rejects_alpn_element_longer_than_255_bytes_*` tests |

Every mutation reddens exactly the tests that assert the deleted behaviour and no others: none is mis-aimed, and none of the six ALPN tests is vacuous. **Task 8 must still re-run all four on the state-3 tree — a mutation proof is not transferable between trees — but it now knows the exact targets, that each occurs exactly once, and what RED looks like.**

### M-R14 — **the one thing the prototype got WRONG, recorded so you do not repeat it**

Two defects, both caught only by *running* the tools:

1. **`clippy::manual_contains`.** The natural spelling of the intersection test,
   `offered.iter().any(|theirs| *theirs == ours.as_slice())`, fails
   `-D warnings` with *"using `contains()` instead of `iter().any()` is more
   efficient … help: try: `offered.contains(&ours.as_slice())`"*. Task 5's code
   below carries the fixed spelling. **A plan's own code is a claim; this one
   was false until it was run.**
2. **An insertion orphaned a doc comment.** Placing `validate_alpn_protocols`
   immediately above `fn validate_data_source(` slid it between
   `validate_data_source`'s own doc comment and its `fn` line, silently
   re-attaching that prose to the new function. **No gate reads prose, so
   nothing catches this.** Task 2 specifies the placement explicitly.


---

## §2. Where this plan CORRECTS the landed `112.1/SPEC.md`

`112.1/SPEC.md` is landed and uneditable, so these are recorded here and in `ADR-0185` rather than applied to it. **None of them changes the sub-phase's scope, its deliverables, or the §6.1 verdict.** They are all in §7, the size-estimate table.

**First, the check the SPEC itself passes.** `ADR-0184` DECISION 2 records that the *parent* SPEC's §9 table sums to 795 while stating ≈573 — a 222-line error on which its whole gate verdict rested. **`112.1/SPEC.md` §7 was therefore re-summed MECHANICALLY here** (numbers extracted from the table rows by regex, not read off the subtotal): the six per-file rows are 145 + 15 + 115 + 320 + 3 + 3 = **601**, and the row labelled `code subtotal` reads **601**. **They MATCH.** A stated subtotal is the least-audited kind of claim in any inherited document; this one is sound.

### C-1 — the fuzz corpus seed row is priced at **3** and measures **44**

The SPEC's row reads *"no NEW target, so §7.4 needs no `ci.yml` edit; the seed needs an explicit `!`-un-ignore line, verified with `git ls-files`" — **3** LoC*. That counts the `.gitignore` line and **not the seed file itself**. A `parse_bootstrap` seed is a complete bootstrap — listener, filter chain, transport socket, filters, cluster, load assignment — and M-R11 measured the three analogous tracked seeds at **40 / 32 / 50** lines. The seed was actually written at this PLAN-write (`tls_downstream_single_cert.yaml` plus a renamed `node.id` and three `alpn_protocols` lines) and confirmed to parse: **43 lines**, plus 1 for the un-ignore. **Correction: +41.**

### C-2 — the `crates/envoy-tls/src/tests.rs` row is priced at **320** and measures **158**

The SPEC prices six new handshake tests at 320 against the file's measured median of 65 per test, and that anchor is right **for standalone tests** — M-R11 re-confirms the median at exactly 65. But roughly 50 of those 65 lines are *scaffold* (bind a `TcpListener`, spawn the server task, build a `RootCertStore` and a `ClientConfig`, connect, join), identical in every one. M-R8's `TlsConnector::with_alpn` — which neither SPEC knows about — lets that scaffold be parameterised **once**. Measured: two helpers + all **six** tests + the 3 literal fixups = **158**. **Correction: −162.** This is the largest single correction and it is the reason the whole estimate lands below the SPEC's despite C-1 and C-3.

### C-3 — the `crates/envoy-config/src/bootstrap.rs` row is priced at **145** and measures **219**

The correction runs the other way here, and it is the one a state-3 session would have felt. The SPEC's 145 covers "field + doc; the D4′ >255 validator; invert the unknown-field test; parse tests for present / absent / empty-list / empty-element / duplicate / 255 / 256". Measured, the field is 9 and the validator plus both call sites is 23 — but the **eight tests need two YAML bootstrap builders and an accessor**, which together are 85 lines, and the cluster-side builder must additionally carry a plaintext listener (§1, and Task 3's warning) or every cluster-side assertion is vacuous. Row total **219**. **Correction: +74.**

### C-4 — `crates/envoy-config/src/lib.rs` **15** → **13**; `crates/envoy-tcp/src/lib.rs` **3** → **1**; `crates/envoy-tls/src/lib.rs` **115** → **116**

All three measured. The `ConfigError` row matches that enum's own median variant-block size of 13 exactly. The `envoy-tcp` fixup is literally one line. **And the SPEC called the hardest row — the dual-config plus `LazyConfigAcceptor` rewrite — to within ONE line**, which is worth recording precisely because C-1, C-2 and C-3 are all large: the SPEC's judgement was not uniformly unreliable, it was unreliable exactly where it priced *test* and *fixture data* volume rather than production code. **Correction: −4.**

### C-5 — three of §7's stated unknowns are ANSWERED, and none of them costs what the SPEC reserved

The SPEC's `envoy-tls/src/lib.rs` row reads *"the D6′ dual-config + `LazyConfigAcceptor` rewrite **+ a `TlsError` variant** (~85)"*, and the scope handoff lists three open questions. M-R5, M-R6 and M-R7 answer them: the twin is an **eager cheap `Clone`** taken before assignment (`ServerConfig` derives `Clone`; every heavy field is an `Arc`), **no cargo feature and no manifest edit are needed**, and **no `TlsError` variant is needed** because both new await points already yield `std::io::Error`. The row still measures 116 because the accept-path rewrite and `from_listener`'s chain walk absorbed the difference.

**Net effect on the estimate: 601 → 551.** Same order of magnitude, same §6.1 verdict — the five corrections very nearly cancel (+41 +74 −162 −4 = −51). They matter not because they move the gate but because they move in **both directions at once**: a state-3 session that budgeted 3 lines for the fuzz seed and 320 for the `envoy-tls` tests would have been badly wrong on each, and the two errors would have hidden each other in the total.

---

## §3. The §6.1 split gate — adjudicated on THIS session's own bottom-up estimate

`112.1/SPEC.md` §7 projects 601 raw / 884 central / 998 worst and explicitly hands the adjudication forward: *"the gate is nonetheless the `112.1` state-2 session's to adjudicate on its **own** re-derived estimate — that session must not inherit this table, for exactly the reason `ADR-0184` fired."* This section does not inherit it.

**507 of the 550 lines below are not estimated at all.** They exist in the §1 M-R13 prototype, they compile, `cargo clippy --workspace --all-targets --all-features -- -D warnings` finds nothing in them, `cargo fmt --all -- --check` prints no diff, and the tests they carry pass. The figures are `git diff --numstat` on that tree, net = additions − deletions. Only the fuzz corpus seed is projected, and it is anchored on three measured comparables.

| # | file | work | net LoC | basis |
|---|---|---|---|---|
| 1 | `crates/envoy-config/src/bootstrap.rs` | the `alpn_protocols` field; `validate_alpn_protocols` + both call sites; two YAML builders + an accessor; 8 parse/validate tests; inverting `rejects_unknown_field_in_common_tls_context` | **219** | **MEASURED** (`228 9`) |
| 2 | `crates/envoy-config/src/lib.rs` | `ConfigError::InvalidAlpnProtocol` + doc | **13** | **MEASURED**; equals this enum's median variant-block size (49 variants, median 13) |
| 3 | `crates/envoy-tls/src/lib.rs` | `finish_server_config`; both `ServerConfig` sites; `from_listener`'s first-chain-wins + warn; the `ClientConfig` site; the D6′ `accept()` rewrite | **116** | **MEASURED** (`122 6`) |
| 4 | `crates/envoy-tls/src/tests.rs` | 3 literal fixups; `ds_context_with_alpn` + `alpn_handshake`; all **six** ALPN tests | **158** | **MEASURED** |
| 5 | `crates/envoy-tcp/src/lib.rs` | 1 literal fixup | **1** | **MEASURED** |
| 6 | `crates/envoy-config/fuzz/` | corpus seed (43) + one `!`-un-ignore line | 44 | the analogous tracked seeds are 40 / 32 / 50; the seed was built and confirmed to PARSE at this PLAN-write |
| | **code subtotal** | | **551** | **507 measured, 44 projected** |

No docs row: `112.1` ships no `BEHAVIOR_CONTRACT.md` section (that is `112.2`'s deliverable 6). `PROGRESS.md` is excluded from every calibration range by `':(exclude)docs/**'`.

**Calibration** (§1 M-R10), applied to the whole 551: **733 (1.33×) / 810 (1.47×) / 915 (1.66×)**.

**Task count: 8.**

> ### §6.1 VERDICT for `112.1`: **THE GATE DOES NOT FIRE. `112.1` IS NOT SPLIT FURTHER. NO `112.1.1`/`112.1.2`; `ADR-0185` records a NOT-FIRE, not a split.**
>
> - **Task leg:** 8 against a ~25 ceiling. Not approached.
> - **LoC leg:** 551 raw against a ~1500 ceiling. **The factor required to fire is 2.72×.** The worst factor this project has ever recorded is **1.66×** (phase 111); the mean of the three measured slices is 1.47×. At the worst observed factor this lands at **915**, with 585 lines of headroom.
>
> **The phase-111 warning is understood and does not apply here.** Phase 111 cleared this gate on a **≈916** raw SPEC-stage estimate and landed at **1525**, 25 lines over the threshold — that overrun is exactly why `ADR-0184` fired the parent's gate at a raw 1250, and it is the standing argument against complacency. Two things separate `112.1` from it. First, the raw estimate is **551**, 60% of phase 111's. Second, and decisively, **phase 111's 916 was entirely projection, whereas 92% of this figure is a line count of code that already exists** — the overrun mechanism (a plan discovering at implementation time that a design needs more code than it looked like) has already been paid here, in this session, against a real compiler. The remaining exposure is 44 lines of YAML.
>
> **The residual risk is named rather than waved away.** Three things could still move the number: (i) review-driven churn at state 5, which the calibration factors already include; (ii) `PROGRESS.md`, which is docs and excluded; (iii) a state-3 session that rejects `alpn_handshake` and writes six standalone tests instead, at the file's measured median of 65 — that would add ~230 and reach 781 raw / 1297 at 1.66×, **still clear**. There is no path from here to 1500 that this session can see.
>
> The §6.1 **mid-execution** trigger — any single task's sub-steps blowing past ~10 items on contact with reality — remains live and is the release valve if that judgement is wrong.

---

## §4. Design decisions this plan owns

The scope handoff named six open questions that are the PLAN's, not the SPEC's. All six are decided here.

**D-PLAN-1 — `DownstreamTls` carries three things, and the twin is built EAGERLY.**

```rust
pub struct DownstreamTls {
    config: Arc<ServerConfig>,
    alpn_free_config: Option<Arc<ServerConfig>>,
    alpn: Vec<Vec<u8>>,
}
```

`alpn_free_config` is `Some` **exactly when** `alpn` is non-empty, and that biconditional is the D6′.1 guard: `accept()` reads `self.alpn_free_config.as_ref()` and takes the pre-112 path on `None`. Encoding the guard as an `Option` rather than as `if !self.alpn.is_empty()` makes it impossible to reach the new path without a twin to hand `into_stream`. Eager construction is chosen over lazy because M-R5 measured `ServerConfig: Clone` to be an `Arc`-field copy — a per-listener one-off, versus a per-connection `OnceLock` dance that would buy nothing.

**D-PLAN-2 — `from_listener` honors the FIRST TLS filter chain's list and WARNS on disagreement.** This is the question the handoff flagged: `from_listener` walks every chain but builds ONE `ServerConfig`, so per-chain ALPN is inexpressible (a declared non-goal, **CF-112-4**). Three options were weighed:

- *(a) First-chain-wins, silently.* **Rejected** — it is a parses-then-silently-ignores state for the second chain, which `ADR-0049`'s all-fatal posture and `ADR-0176`'s *"no landed state ever parses-then-silently-ignores"* forbid.
- *(b) Boot-fatal when two TLS chains declare different non-empty lists.* **Rejected** — upstream Envoy's per-chain-vs-per-listener semantics are **unmeasured** (CF-112-4), so rejecting there risks a reject-direction divergence. That is precisely the error `ADR-0184` DECISION 4 corrected in D4, and repeating it here would be worse, not better.
- ***(c) First-chain-wins plus a `tracing::warn!` naming the honored and the ignored list.*** **CHOSEN.** It changes neither the accept set nor the reject set, so it cannot manufacture a divergence, and it is not silent. `envoy-tls` already depends on `tracing`.

The warn fires only when a later chain's list is **non-empty and different** — a chain that omits the field is not disagreeing. The residual (envoy-rust honors one list where upstream may honor per-chain) stays banked under CF-112-4 and is **not** widened into a new carry-forward.

**D-PLAN-3 — the D6′.1 guard gets its own test, and it is the one that protects §7.5 gate (b).** `alpn_empty_server_list_does_not_advertise` asserts that with no server list a client offering `h2,http/1.1` negotiates nothing. That is the shape of every TLS config in the tree today, `0004-tls-downstream` / `0005-tls-upstream` / `0006-tls-sni` included. It is the in-process proxy for the differential gate that cannot run here, and it passed on the prototype alongside all 16 pre-existing tests.

**D-PLAN-4 — `TlsError` gains nothing.** Per M-R7. `accept()`'s signature is unchanged in both directions and `accept_returns_handshake_error_on_garbage_input` stays green untouched.

**D-PLAN-5 — the >255 validator lives in `bootstrap.rs`'s `validate()`, not in the `envoy-tls` constructors.** Reasons: it is a *config* rule, so it must fire at config-load for both sides uniformly (a listener-side and a cluster-side call); `envoy-tls`'s constructors are not reached at all for a cluster whose `UpstreamTls` is built later; and every peer rule (`EmptyTlsCertificates`, `MissingValidationContext`, `EmptyUpstreamSni`) already lives there. The `side` discriminant is `"listener"`/`"cluster"`, matching `ConfigError::EmptyTlsCertificates`'s existing convention.

**D-PLAN-6 — `rejects_unknown_field_in_common_tls_context` is INVERTED IN PLACE, not deleted.** Its own comment concedes the point (*"alpn_protocols is a phase-04 surface; phase 03 fixtures do not include it"*). Deleting it would lose the only coverage that this exact YAML shape parses; inverting it converts a reject-assertion into an accept-assertion on the identical document. Renamed to `accepts_alpn_protocols_in_common_tls_context`. **It will FAIL the moment Task 1's field lands** — measured on the prototype: `test result: FAILED. 708 passed; 1 failed`, the one failure being exactly this test. That failure IS Task 1's RED.

---

## §5. File structure

| file | responsibility | tasks |
|---|---|---|
| `crates/envoy-config/src/bootstrap.rs` | the `alpn_protocols` field on `CommonTlsContext`; `validate_alpn_protocols` + its two call sites; all config-layer tests | 1, 2 |
| `crates/envoy-config/src/lib.rs` | the `ConfigError::InvalidAlpnProtocol` variant | 2 |
| `crates/envoy-tls/src/lib.rs` | `finish_server_config`; ALPN on both `ServerConfig` sites and the `ClientConfig` site; `from_listener`'s chain walk; the D6′ `accept()` rewrite | 4, 5, 6 |
| `crates/envoy-tls/src/tests.rs` | 3 `E0063` fixups; `ds_context_with_alpn`; `alpn_handshake`; six ALPN tests | 1, 4, 5, 6 |
| `crates/envoy-tcp/src/lib.rs` | 1 `E0063` fixup | 1 |
| `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_downstream_alpn.yaml` | new fuzz corpus seed | 7 |
| `crates/envoy-config/fuzz/.gitignore` | the `!`-un-ignore line for that seed | 7 |
| `docs/envoy-rust/phases/112.1-alpn-config-and-rustls-wiring/PROGRESS.md` | the running log, appended on each task completion | all |

**Untouched by design:** every `Cargo.toml`, `Cargo.lock`, `ci.yml`, all of `tests/`, `crates/envoy-bin/`, `crates/envoy-listener/`, `crates/envoy-http1/`, `crates/envoy-http2/`.

---

## §6. Carry-forwards — banked, not consumed

Inherited and still OPEN: **CF-112-1** (`Http2OverTlsNotSupported` not lifted), **CF-112-2** (the upstream ALPN offer is unit-tested but not differentially witnessed), **CF-112-3** (ALPN × SNI filter-chain selection unmeasured), **CF-112-4** (per-chain vs per-listener ALPN unmeasured upstream — D-PLAN-2 honors the first chain and warns), **CF-112-6** (upstream accepts an empty/duplicate element and negotiates nothing at all under an empty one; envoy-rust's runtime behaviour there stays unspecified and untested), **CF-112-7** (ALPN over the io_uring H1 path stays unmeasured). **CF-112-5 was CLOSED by `ADR-0184`.** This plan opens **no new carry-forward** and clears none (§6.3; `ADR-0165`).

---

## §7. NOT MEASURED — stated explicitly per D-3.4

- **Any `ssl.*` or ALPN-specific stat.** `112.1/SPEC.md` §8 says no `ssl.alpn*` stat was searched for, and this session did not search either. **Do not assert any `ssl.*` stat without measuring it first.**
- **envoy-rust's runtime behaviour under an empty `alpn_protocols` element** (CF-112-6). D4′ accepts it, matching upstream; what rustls then puts on the wire is untested here.
- **Whether `LazyConfigAcceptor` changes observable timing.** Not measured. It is not on the path for any config in the tree today (D6′.1).
- **Every cell of upstream Envoy's behaviour** — all of it is inherited from `112.1/SPEC.md` §2's measurements at the split session. This session re-read the pinned `rustls`/`tokio-rustls` source and built a prototype; it ran **no** Docker probe against the upstream image.
- **The differential surface.** By construction: `112.1` ships no fixture. Sibling `112.2` witnesses every cell, and its §8 names cell 3 (the mismatch) as the one that depends entirely on this sub-phase's D6′.

---

## §8. Definition of done — the §7.5 gate, instantiated for `112.1`

- **(a)** No new/changed differential fixture, so **vacuous by construction**; discharged by sibling `112.2`.
- **(b)** All **90** pre-existing differential fixtures still green — **the load-bearing gate here**, and specifically `0004-tls-downstream`, `0005-tls-upstream` and `0006-tls-sni`, which exercise the rewritten accept path. `#[serde(default)]` on the new field plus D6′.1's confinement of the rewrite are what make this a design property; the prototype's `21 passed` with zero pre-existing regressions is the in-process leading indicator. ⚠ Differential fixtures **flake under full-parallel `cargo test`** on this host and pass in isolation; **only isolation classifies a RED**, never the failure text. Use `--no-fail-fast`, redirect to a FILE (never `tail` — it truncates the `failures:` block), extract failures from the `---- <name> stdout ----` markers, and leave a settle gap between Docker-spawning isolation runs. CI is authoritative.
- **(c)** h2spec at `PASS_RATE_GATE = 0.95` with `known-failures.txt` **untrimmed** (21 lines, md5 `19cd44d86a8b15d825f76c6e7b265e65`). ⚠ The local run **self-skips silently** — a local green needs `--nocapture`, and a `0.00s` conformance runtime is the tell. CI genuinely runs it (`ADR-0163`, SETTLED — do not re-raise).
- **(d)** No NEW fuzz target, so **no `ci.yml` edit**. The new corpus seed needs its explicit `!`-un-ignore line — verify with `git ls-files`, not `git status`.
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean locally **and** in CI, quoted into `PROGRESS.md` at state 4.
- **(f)** `REVIEW.md` approved at state 5.

**Mutation proof (Task 9).** Re-run M-R13's two mutations on the state-3 tree. A mutation proof is not transferable between trees.

**The CI identity is `binaries=167 passed=2252 failed=0` and this sub-phase's code commit MUST move it.** Expect `+2252 → 2252 + 6 config-layer tests + 6 envoy-tls tests ≈ 2264`, and `binaries` unchanged at 167 (no new test binary is added).


---

## §9. Tasks

Eight tasks. Each ends with an independently testable deliverable and a green
`cargo build --workspace --all-targets` + `cargo clippy --workspace
--all-targets --all-features -- -D warnings` + `cargo fmt --all -- --check`.
Append to `PROGRESS.md` on each task completion.

**All code blocks below were COMPILED AND RUN at this PLAN-write** in a scratch
worktree at HEAD `9f2010a` (§1 M-R13). They are copied verbatim from a tree
that passed `cargo fmt --all -- --check` with zero diff and
`cargo clippy --workspace --all-targets --all-features -- -D warnings` with
zero findings. **Paste them as-is; do not "tidy" them** — §1 M-R14 records the
two spellings that a natural tidy-up would reintroduce and that the gate then
rejects.

---

### Task 1: The `alpn_protocols` config field, the four `E0063` fixups, and the inverted unknown-field test

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` — the `CommonTlsContext` declaration (anchor `pub struct CommonTlsContext {`) and the test `rejects_unknown_field_in_common_tls_context`
- Modify: `crates/envoy-tls/src/tests.rs` — 3 struct literals
- Modify: `crates/envoy-tcp/src/lib.rs` — 1 struct literal

**Interfaces:**
- Produces: `envoy_config::CommonTlsContext { …, pub alpn_protocols: Vec<String> }` — every later task reads this field. Its `#[serde(default)]` is what keeps all 90 fixtures and 708 pre-existing `envoy-config` tests parsing unchanged.

⚠ **Gate this task on `cargo build --workspace --all-targets`, NOT on `cargo build --workspace`.** §1 M-R3 measured that all four `E0063` sites are in test-only code, so a plain `cargo build` is GREEN with zero fixups — a false green.

- [ ] **Step 1: Write the failing test** — this task's RED is produced by *inverting an existing test*, per D-PLAN-6. In `crates/envoy-config/src/bootstrap.rs`, find `fn rejects_unknown_field_in_common_tls_context()` (locate by TEXT). Rename it and replace its comment and its final assertion block, leaving the YAML literal in the middle **byte-identical**:

```rust
    #[test]
    fn accepts_alpn_protocols_in_common_tls_context() {
        // 112.1 D1 INVERTS this test. It was written in phase 03 to pin
        // `deny_unknown_fields` REJECTING `alpn_protocols`, and its own comment
        // conceded the point ("alpn_protocols is a phase-04 surface"). The
        // field now exists, so the same document must PARSE, and the parsed
        // value must be the configured list.
```

and, replacing the trailing `let err = crate::parse_bootstrap(yaml).expect_err(…); … );`:

```rust
        let bs = crate::parse_bootstrap(yaml).expect("alpn_protocols must now PARSE");
        let listener = &bs.static_resources.listeners[0];
        let ts = listener.filter_chains[0]
            .transport_socket
            .as_ref()
            .expect("transport_socket");
        let crate::TransportSocketTypedConfig::Downstream(ctx) = &ts.typed_config else {
            panic!("expected DownstreamTlsContext");
        };
        assert_eq!(ctx.common_tls_context.alpn_protocols, vec!["h2".to_string()]);
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p envoy-config --lib accepts_alpn_protocols_in_common`
Expected: **compile error** "no field `alpn_protocols` on type `&CommonTlsContext`". (A compile error is an acceptable RED *for a test that references a field that does not exist yet*; it is NOT acceptable as a mutation RED — see Task 8.)

- [ ] **Step 3: Add the field**

In `crates/envoy-config/src/bootstrap.rs`, inside `pub struct CommonTlsContext {`, after the `validation_context` field:

```rust
    /// 112.1 D1: ALPN protocol identifiers, most preferred first. Honored on
    /// BOTH sides because this struct is the type of `DownstreamTlsContext`
    /// (`rustls::ServerConfig`) and `UpstreamTlsContext` (`rustls::ClientConfig`).
    /// Absent or empty means "do not advertise ALPN" (D3).
    #[serde(default)]
    pub alpn_protocols: Vec<String>,
```

- [ ] **Step 4: Run it to verify it passes**

Run: `cargo test -p envoy-config --lib accepts_alpn_protocols_in_common`
Expected: `test result: ok. 1 passed`

- [ ] **Step 5: Fix the four `E0063` construction sites**

Run `cargo build --workspace --all-targets` and expect exactly four "E0063 missing field `alpn_protocols`" errors. Add **one line**, `alpn_protocols: vec![],`, to each `envoy_config::CommonTlsContext { … }` literal:

| file | enclosing item |
|---|---|
| `crates/envoy-tls/src/tests.rs` | `pub fn ds_context_with(` |
| `crates/envoy-tls/src/tests.rs` | `async fn rejects_empty_tls_certificates()` |
| `crates/envoy-tls/src/tests.rs` | `pub fn us_context_with(` |
| `crates/envoy-tcp/src/lib.rs` | `fn upstream_ctx_for(pki: &UpstreamPki, sni: &str)` |

- [ ] **Step 6: Verify the whole workspace**

Run: `cargo build --workspace --all-targets && cargo test -p envoy-config -p envoy-tls -p envoy-tcp`
Expected: build `Finished`; `envoy-config` `709 passed; 0 failed`, `envoy-tls` `16 passed`, `envoy-tcp` green. **If any pre-existing test fails, STOP** — `#[serde(default)]` is supposed to make this inert.

- [ ] **Step 7: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-tls/src/tests.rs crates/envoy-tcp/src/lib.rs
git commit -m "phase 112.1 task 1: CommonTlsContext.alpn_protocols + four E0063 fixups"
```

---

### Task 2: `ConfigError::InvalidAlpnProtocol` and the >255-byte validator (D4′)

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` — the `ConfigError` enum (anchor `TooManyListeners(usize),`)
- Modify: `crates/envoy-config/src/bootstrap.rs` — a new helper plus two call sites

**Interfaces:**
- Consumes: `CommonTlsContext::alpn_protocols` (Task 1)
- Produces: `crate::ConfigError::InvalidAlpnProtocol { side: &'static str, index: usize, len: usize }`

⚠ **D4′ rejects `len > 255` and NOTHING ELSE.** Upstream Envoy v1.33.0 was measured to ACCEPT `[""]`, `["h2",""]`, `["h2","h2"]` and `[]`, and to accept 254- and 255-byte elements. Rejecting any wider set manufactures a reject-direction divergence — the exact error `ADR-0184` DECISION 4 corrected in the parent's D4.

⚠ **`crate::parse_bootstrap` ALREADY calls `bootstrap::validate`** (anchor `bootstrap::validate(&mut bootstrap)?;` in `crates/envoy-config/src/lib.rs`). Reject tests therefore use `parse_bootstrap(...).expect_err(...)` directly; do not add a second `validate` call.

- [ ] **Step 1: Write the failing tests**

Add to `crates/envoy-config/src/bootstrap.rs`'s `mod tests`, immediately above `fn accepts_alpn_protocols_in_common_tls_context`. These reference two YAML builders that Task 3 also uses; introduce the builders here.

```rust
    /// 112.1: build a minimal single-listener bootstrap whose
    /// `DownstreamTlsContext.common_tls_context` carries `alpn_line` verbatim.
    /// Pass `""` to omit the field entirely.
    fn ds_bootstrap_with_alpn(alpn_line: &str) -> String {
        format!(
            r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                {alpn_line}
                tls_certificates:
                  - certificate_chain:
                      filename: /tmp/leaf.pem
                    private_key:
                      filename: /tmp/leaf.key
          filters: []
  clusters: []
"#
        )
    }

    /// 112.1: the same, for a cluster-side `UpstreamTlsContext`. NOTE the
    /// plaintext listener: a bootstrap with neither a listener nor an admin
    /// endpoint is rejected `ConfigError::NoRuntime` before the cluster walk
    /// is ever reached, which would make every assertion below vacuous.
    fn us_bootstrap_with_alpn(alpn_line: &str) -> String {
        format!(
            r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      transport_socket:
        name: envoy.transport_sockets.tls
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext
          sni: backend.example.com
          common_tls_context:
            {alpn_line}
            validation_context:
              trusted_ca:
                filename: /tmp/ca.pem
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#
        )
    }

    /// 112.1: the downstream `alpn_protocols` of an already-parsed bootstrap.
    fn ds_alpn_of(bs: &crate::Bootstrap) -> &[String] {
        let ts = bs.static_resources.listeners[0].filter_chains[0]
            .transport_socket
            .as_ref()
            .expect("transport_socket");
        let crate::TransportSocketTypedConfig::Downstream(ctx) = &ts.typed_config else {
            panic!("expected DownstreamTlsContext");
        };
        &ctx.common_tls_context.alpn_protocols
    }
```

and the two boundary tests:

```rust
    #[test]
    fn accepts_alpn_element_of_exactly_255_bytes() {
        // D4': upstream accepts 254 and 255; only >255 is rejected.
        let e = "a".repeat(255);
        let yaml = ds_bootstrap_with_alpn(&format!("alpn_protocols: [\"{e}\"]"));
        crate::parse_bootstrap(&yaml).expect("255 bytes must be ACCEPTED");
    }

    #[test]
    fn rejects_alpn_element_longer_than_255_bytes_on_listener() {
        let e = "a".repeat(256);
        let yaml = ds_bootstrap_with_alpn(&format!("alpn_protocols: [\"{e}\"]"));
        let err = crate::parse_bootstrap(&yaml).expect_err("256 bytes must be REJECTED");
        assert!(
            matches!(
                err,
                crate::ConfigError::InvalidAlpnProtocol {
                    side: "listener",
                    index: 0,
                    len: 256
                }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_alpn_element_longer_than_255_bytes_on_cluster() {
        let e = "a".repeat(256);
        let yaml = us_bootstrap_with_alpn(&format!("alpn_protocols: [\"h2\", \"{e}\"]"));
        let err = crate::parse_bootstrap(&yaml).expect_err("256 bytes must be REJECTED");
        assert!(
            matches!(
                err,
                crate::ConfigError::InvalidAlpnProtocol {
                    side: "cluster",
                    index: 1,
                    len: 256
                }
            ),
            "got {err:?}"
        );
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p envoy-config --lib alpn_element`
Expected: **compile error** "no variant or associated item named `InvalidAlpnProtocol` found for enum `ConfigError`".

- [ ] **Step 3: Add the `ConfigError` variant**

In `crates/envoy-config/src/lib.rs`, in `pub enum ConfigError`, immediately after `TooManyListeners(usize),`:

```rust
    /// 112.1 D4': an `alpn_protocols` element longer than 255 bytes. Upstream
    /// Envoy v1.33.0 was MEASURED to ACCEPT a zero-length element, a duplicate
    /// element and an empty list, and to REJECT only this case with
    /// `Invalid ALPN protocol string`. The two reject sets therefore coincide;
    /// rejecting any wider set would manufacture a reject-direction divergence.
    #[error(
        "invalid ALPN protocol string on the {side} side: element {index} is {len} bytes; the maximum is 255"
    )]
    InvalidAlpnProtocol {
        side: &'static str,
        index: usize,
        len: usize,
    },
```

- [ ] **Step 4: Add the validator**

⚠ **PLACEMENT MATTERS AND NO GATE CHECKS IT.** §1 M-R14 records that inserting this immediately above `fn validate_data_source(` slides it *between that function's doc comment and its `fn` line*, silently re-attaching `validate_data_source`'s prose to the new function. **Insert it after the complete `validate_data_source` item (its closing `}` at column 4), or above `validate_data_source`'s FIRST doc-comment line — never between them.** After inserting, run `sed -n '/fn validate_data_source(/,-8p'` (or just read the surrounding 12 lines) and confirm `validate_data_source` still owns its own doc comment.

```rust
/// 112.1 D4': reject an `alpn_protocols` element longer than 255 bytes and
/// nothing else. RFC 7301 encodes each identifier with a single-octet length
/// prefix, so 255 is the wire maximum; upstream Envoy v1.33.0 rejects exactly
/// this case with `Invalid ALPN protocol string` and ACCEPTS the empty
/// element, the duplicate element and the empty list. `side` is "listener" or
/// "cluster" and only reaches the error message.
fn validate_alpn_protocols(
    ctx: &CommonTlsContext,
    side: &'static str,
) -> Result<(), crate::ConfigError> {
    for (index, proto) in ctx.alpn_protocols.iter().enumerate() {
        if proto.len() > 255 {
            return Err(crate::ConfigError::InvalidAlpnProtocol {
                side,
                index,
                len: proto.len(),
            });
        }
    }
    Ok(())
}
```

- [ ] **Step 5: Wire both call sites**

In `pub(crate) fn validate(bootstrap: &mut Bootstrap)`, in the listener walk's `TransportSocketTypedConfig::Downstream(ctx) => {` arm, as the **first** statement (locate by the text `if ctx.common_tls_context.tls_certificates.is_empty() {` that currently opens the arm):

```rust
                        validate_alpn_protocols(&ctx.common_tls_context, "listener")?;
```

In `pub(crate) fn validate_cluster(cluster: &Cluster)`, in the `TransportSocketTypedConfig::Upstream(ctx) => {` arm, as the **first** statement (locate by `if !ctx.common_tls_context.tls_certificates.is_empty() {`):

```rust
                validate_alpn_protocols(&ctx.common_tls_context, "cluster")?;
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p envoy-config --lib alpn`
Expected: `test result: ok. 4 passed` (the inverted test, the two boundary tests, the 255-byte test).

- [ ] **Step 7: Verify nothing regressed, then commit**

Run: `cargo test -p envoy-config && cargo clippy -p envoy-config --all-targets --all-features -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs
git commit -m "phase 112.1 task 2: D4' >255-byte alpn_protocols validator on both sides"
```

---

### Task 3: The remaining config-layer parse tests

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` — `mod tests` only. **No production code changes in this task.**

**Interfaces:**
- Consumes: `ds_bootstrap_with_alpn`, `us_bootstrap_with_alpn`, `ds_alpn_of` (Task 2)

These pin the four D4′/D3 acceptance cells measured on upstream Envoy that Task 2's reject tests do not cover. They are characterization pins: they pass the moment they are written, so **the RED is produced by mutation** — Step 2 below.

- [ ] **Step 1: Write the tests**

```rust
    #[test]
    fn alpn_protocols_defaults_to_empty_when_absent() {
        let bs = crate::parse_bootstrap(&ds_bootstrap_with_alpn("")).expect("parse");
        assert!(ds_alpn_of(&bs).is_empty(), "absent must default to empty");
    }

    #[test]
    fn accepts_empty_alpn_protocols_list() {
        // D4': upstream Envoy ACCEPTS `[]` (measured, exit 0).
        let bs =
            crate::parse_bootstrap(&ds_bootstrap_with_alpn("alpn_protocols: []")).expect("parse");
        assert!(ds_alpn_of(&bs).is_empty());
    }

    #[test]
    fn accepts_empty_and_duplicate_alpn_elements() {
        // D4': upstream Envoy ACCEPTS `[""]`, `["h2",""]` and `["h2","h2"]`
        // (all measured, exit 0). Rejecting them would manufacture a
        // reject-direction divergence. CF-112-6 banks the runtime quirk.
        let bs = crate::parse_bootstrap(&ds_bootstrap_with_alpn(
            r#"alpn_protocols: ["h2", "", "h2"]"#,
        ))
        .expect("parse");
        assert_eq!(
            ds_alpn_of(&bs),
            ["h2".to_string(), String::new(), "h2".to_string()]
        );
    }

    #[test]
    fn accepts_alpn_protocols_on_upstream_tls_context() {
        // D2b: the field is on the SHARED CommonTlsContext, so the cluster
        // side gains it in the same landing.
        let bs = crate::parse_bootstrap(&us_bootstrap_with_alpn(
            r#"alpn_protocols: ["h2", "http/1.1"]"#,
        ))
        .expect("parse");
        let ts = bs.static_resources.clusters[0]
            .transport_socket
            .as_ref()
            .expect("transport_socket");
        let crate::TransportSocketTypedConfig::Upstream(ctx) = &ts.typed_config else {
            panic!("expected UpstreamTlsContext");
        };
        assert_eq!(
            ctx.common_tls_context.alpn_protocols,
            ["h2".to_string(), "http/1.1".to_string()]
        );
    }
```

⚠ **`us_bootstrap_with_alpn` MUST carry the plaintext listener.** Measured at this PLAN-write: a bootstrap with `listeners: []` and only a cluster is rejected `ConfigError::NoRuntime` — *"bootstrap configures neither an admin endpoint nor a listener"* — **before the cluster walk is reached**, so `accepts_alpn_protocols_on_upstream_tls_context` and `rejects_alpn_element_longer_than_255_bytes_on_cluster` both fail with `got NoRuntime` and every cluster-side assertion is vacuous. This was hit and fixed here; do not re-introduce it.

- [ ] **Step 2: Run them, then prove they are not vacuous**

Run: `cargo test -p envoy-config --lib alpn`
Expected: `test result: ok. 8 passed; 0 failed`

Then, in a **scratch worktree** (never the real tree), delete `#[serde(default)]` from the `alpn_protocols` field and re-run. `alpn_protocols_defaults_to_empty_when_absent` must go RED ("missing field `alpn_protocols`"). Restore.

- [ ] **Step 3: Commit**

```bash
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 112.1 task 3: config-layer parse tests for the D3/D4' acceptance cells"
```


---

### Task 4: Thread the configured list into BOTH `rustls::ServerConfig` sites (D2a, D3, D5)

**Files:**
- Modify: `crates/envoy-tls/src/lib.rs` — `struct DownstreamTls`, `from_context`, `from_listener`
- Modify: `crates/envoy-tls/src/tests.rs` — two helpers + three tests

**Interfaces:**
- Consumes: `envoy_config::CommonTlsContext::alpn_protocols` (Task 1)
- Produces: `fn finish_server_config(config: ServerConfig, alpn_protocols: &[String]) -> (Arc<ServerConfig>, Vec<Vec<u8>>)` — **Task 5 widens this return to a 3-tuple**; `DownstreamTls { config, alpn }` — **Task 5 adds `alpn_free_config`**; and the test helpers `ds_context_with_alpn` / `alpn_handshake`, which Tasks 5 and 6 reuse.

⚠ **This task deliberately lands the 2-tuple shape and Task 5 widens it.** Landing Task 5's `alpn_free_config` field here would leave it unread, and `-D warnings` rejects a dead field. The churn is two lines and it keeps every task green on its own.

⚠ **`accept()` is NOT touched in this task.** It still uses `tokio_rustls::TlsAcceptor` with `self.config`, which is correct for all three tests below. The mismatch cell stays RED until Task 5 — that is the point of the split.

- [ ] **Step 1: Write the failing tests**

Append to `crates/envoy-tls/src/tests.rs`. The two helpers first — `alpn_handshake` is the scaffold all five downstream cells share, and `TlsConnector::with_alpn` (pinned `tokio-rustls 0.26.4`, §1 M-R8) is what lets one helper vary the client offer:

```rust
/// Build a `DownstreamTlsContext` carrying `alpn_protocols`.
fn ds_context_with_alpn(pki: &pki::Pki, alpn: &[&str]) -> envoy_config::DownstreamTlsContext {
    let mut ctx = pki::ds_context_with(&pki.leaf_cert_pem, &pki.leaf_key_pem);
    ctx.common_tls_context.alpn_protocols = alpn.iter().map(|s| s.to_string()).collect();
    ctx
}

/// Drive ONE real loopback handshake: a `DownstreamTls` built from `server_alpn`
/// against a `tokio_rustls` client offering `client_alpn` (empty = offer none).
/// Returns `(server_selected, client_selected)` as owned bytes. Panics on a
/// handshake failure, which is itself the D6' assertion for the mismatch cell.
async fn alpn_handshake(
    pki: &pki::Pki,
    server_alpn: &[&str],
    client_alpn: &[&str],
) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    use rustls::pki_types::ServerName;
    let ctx = ds_context_with_alpn(pki, server_alpn);
    let downstream = DownstreamTls::from_context(&ctx).expect("from_context");

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        downstream.accept(stream).await
    });

    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(pki.ca_der_for_root_store.clone())
        .expect("add ca");
    let client_cfg = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));
    let server_name = ServerName::try_from("a.example.com").expect("server name");
    let tcp = tokio::net::TcpStream::connect(addr).await.expect("connect");

    let client_tls = if client_alpn.is_empty() {
        connector.connect(server_name, tcp).await
    } else {
        connector
            .with_alpn(client_alpn.iter().map(|s| s.as_bytes().to_vec()).collect())
            .connect(server_name, tcp)
            .await
    }
    .expect("client handshake must SUCCEED");

    let server_tls = server_task
        .await
        .expect("server task joins")
        .expect("server handshake must SUCCEED");

    (
        server_tls.get_ref().1.alpn_protocol().map(|p| p.to_vec()),
        client_tls.get_ref().1.alpn_protocol().map(|p| p.to_vec()),
    )
}
```

then the three cells this task turns green:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn alpn_negotiates_h2_when_both_offer_it() {
    install_provider_once();
    let pki = pki::build();
    let (s, c) = alpn_handshake(&pki, &["h2", "http/1.1"], &["h2", "http/1.1"]).await;
    assert_eq!(s.as_deref(), Some(&b"h2"[..]), "server side");
    assert_eq!(c.as_deref(), Some(&b"h2"[..]), "client side");
}

#[tokio::test(flavor = "multi_thread")]
async fn alpn_selection_follows_server_preference() {
    install_provider_once();
    let pki = pki::build();
    let (s, c) = alpn_handshake(&pki, &["http/1.1", "h2"], &["h2", "http/1.1"]).await;
    assert_eq!(s.as_deref(), Some(&b"http/1.1"[..]), "server side");
    assert_eq!(c.as_deref(), Some(&b"http/1.1"[..]), "client side");
}

#[tokio::test(flavor = "multi_thread")]
async fn alpn_empty_server_list_does_not_advertise() {
    install_provider_once();
    let pki = pki::build();
    let (s, c) = alpn_handshake(&pki, &[], &["h2", "http/1.1"]).await;
    assert_eq!(s, None);
    assert_eq!(c, None);
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p envoy-tls --lib alpn_`
Expected: `alpn_negotiates_h2_when_both_offer_it` and `alpn_selection_follows_server_preference` FAIL with "assertion `left == right` failed … left: None, right: Some([104, 50])" — the server never advertised. `alpn_empty_server_list_does_not_advertise` PASSES already (it is a characterization pin; Task 8's mutation M2 is its RED).

- [ ] **Step 3: Add the field and the helper**

Replace `pub struct DownstreamTls { config: Arc<ServerConfig>, }` in `crates/envoy-tls/src/lib.rs` with:

```rust
pub struct DownstreamTls {
    config: Arc<ServerConfig>,
    /// The configured ALPN list, wire-encoded. Task 5 adds the D6' twin
    /// alongside it; until then this field is read only by the tests.
    alpn: Vec<Vec<u8>>,
}

/// 112.1 D2a/D3: finish a built `ServerConfig` by attaching the configured ALPN
/// list. An empty list leaves `alpn_protocols` empty, which is rustls' own
/// documented "don't do ALPN at all" (D3).
fn finish_server_config(
    mut config: ServerConfig,
    alpn_protocols: &[String],
) -> (Arc<ServerConfig>, Vec<Vec<u8>>) {
    let wire: Vec<Vec<u8>> = alpn_protocols
        .iter()
        .map(|p| p.as_bytes().to_vec())
        .collect();
    config.alpn_protocols = wire.clone();
    (Arc::new(config), wire)
}
```

- [ ] **Step 4: Wire both `ServerConfig` construction sites**

In `from_context`, replace the `Ok(Self { config: Arc::new(config) })` that follows `.with_cert_resolver(resolver);`:

```rust
        let (config, alpn) = finish_server_config(config, &cfg.common_tls_context.alpn_protocols);
        Ok(Self { config, alpn })
```

In `from_listener`, D-PLAN-2. Add before the `for chain in &listener.filter_chains {` loop:

```rust
        // 112.1 D2a': ALPN is a `rustls::ServerConfig` property and this
        // constructor builds ONE config for the whole listener, so per-chain
        // ALPN is inexpressible (CF-112-4, a declared non-goal). The FIRST
        // filter chain carrying a `DownstreamTlsContext` supplies the list;
        // a later chain declaring a DIFFERENT non-empty list is warned about
        // rather than silently dropped or rejected — rejecting would
        // manufacture a reject-direction divergence against upstream Envoy,
        // whose per-chain semantics are unmeasured.
        let mut alpn_protocols: Option<&[String]> = None;
```

inside the loop, immediately before `let certs = &ctx.common_tls_context.tls_certificates;`:

```rust
            match alpn_protocols {
                None => alpn_protocols = Some(&ctx.common_tls_context.alpn_protocols),
                Some(first) => {
                    let this = &ctx.common_tls_context.alpn_protocols;
                    if !this.is_empty() && this.as_slice() != first {
                        tracing::warn!(
                            listener = %listener.name,
                            honored = ?first,
                            ignored = ?this,
                            "per-filter-chain alpn_protocols is not supported; \
                             honoring the first TLS filter chain's list for the \
                             whole listener (CF-112-4)"
                        );
                    }
                }
            }
```

and replace that constructor's `Ok(Self { config: Arc::new(config) })`:

```rust
        let (config, alpn) = finish_server_config(config, alpn_protocols.unwrap_or(&[]));
        Ok(Self { config, alpn })
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p envoy-tls`
Expected: `test result: ok. 19 passed; 0 failed` (16 pre-existing + 3 new). **If any of the 16 pre-existing tests fails, STOP** — none of them configures ALPN, so `finish_server_config` must be inert for them.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-tls/src/lib.rs crates/envoy-tls/src/tests.rs
git commit -m "phase 112.1 task 4: alpn_protocols into both rustls::ServerConfig sites"
```

---

### Task 5: The D6′ accept-path rewrite — a mismatch must NOT send `no_application_protocol`

**Files:**
- Modify: `crates/envoy-tls/src/lib.rs` — `struct DownstreamTls`, `finish_server_config`, both constructors' `Ok(Self { … })`, and `accept`
- Modify: `crates/envoy-tls/src/tests.rs` — two tests

**Interfaces:**
- Consumes: `finish_server_config` (Task 4), `alpn_handshake` (Task 4)
- Produces: `DownstreamTls { config, alpn_free_config: Option<Arc<ServerConfig>>, alpn }`. **`accept()`'s signature is UNCHANGED in both directions** — no consumer is touched (non-goal 6).

**This is the sub-phase's one piece of real engineering.** The pinned `rustls 0.23.39` sends a fatal `no_application_protocol` alert where upstream Envoy completes the handshake with nothing selected (§1 M-R4). Four facts make the fix work, all re-verified at this PLAN-write:

1. `Error::NoApplicationProtocol` is produced inside `Accepted::into_connection`, **not** inside `Acceptor::accept` — so peeking the ClientHello between the two is possible at all (M-R4).
2. `rustls::ServerConfig` derives `Clone` and every heavy field is an `Arc`, so the ALPN-free twin is a cheap eager clone taken **before** `alpn_protocols` is assigned (M-R5).
3. Both new await points yield `std::io::Error`, which `TlsError::Handshake { source }` already models — **no new variant** (M-R7).
4. `ClientHello<'a>` borrows the `StartHandshake` that `into_stream(self, …)` consumes, so the peek must reduce to an owned value inside a block — otherwise `E0505` (M-R9).

⚠ **Do not "simplify" the intersection test to `offered.iter().any(|theirs| *theirs == ours.as_slice())`.** That spelling fails `-D warnings` with `clippy::manual_contains` (§1 M-R14). The code below carries the accepted spelling.

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test(flavor = "multi_thread")]
async fn alpn_mismatch_completes_handshake_with_no_protocol() {
    install_provider_once();
    let pki = pki::build();
    let (s, c) = alpn_handshake(&pki, &["h2", "http/1.1"], &["h3"]).await;
    assert_eq!(s, None, "server must select nothing");
    assert_eq!(c, None, "client must select nothing");
}

#[tokio::test(flavor = "multi_thread")]
async fn alpn_client_offers_nothing_negotiates_none() {
    install_provider_once();
    let pki = pki::build();
    let (s, c) = alpn_handshake(&pki, &["h2", "http/1.1"], &[]).await;
    assert_eq!(s, None);
    assert_eq!(c, None);
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p envoy-tls --lib alpn_mismatch alpn_client_offers`
Expected: `alpn_mismatch_completes_handshake_with_no_protocol` **FAILS** — `client handshake must SUCCEED: Custom { kind: InvalidData, error: NoApplicationProtocol }`. That failure is the divergence this task exists to remove. `alpn_client_offers_nothing_negotiates_none` passes already (M-R4: rustls skips the whole block when the client sends no extension).

- [ ] **Step 3: Widen the struct and `finish_server_config`**

Replace the Task-4 versions with:

```rust
pub struct DownstreamTls {
    config: Arc<ServerConfig>,
    /// 112.1 D6': an ALPN-free twin of `config`, identical except that
    /// `alpn_protocols` is left empty. `None` means no ALPN is configured, and
    /// `accept()` then takes the unchanged pre-112 `TlsAcceptor` path (D6'.1).
    alpn_free_config: Option<Arc<ServerConfig>>,
    /// The configured ALPN list, wire-encoded, for `accept()`'s intersection
    /// test. Empty exactly when `alpn_free_config` is `None`.
    alpn: Vec<Vec<u8>>,
}

/// 112.1 D2a/D3/D6': finish a built `ServerConfig` by attaching the configured
/// ALPN list, and — only when that list is non-empty — produce the ALPN-free
/// twin D6' hands to `into_stream` on a mismatch. The twin is cloned BEFORE
/// `alpn_protocols` is set, so it is byte-identical except for that one field;
/// `rustls::ServerConfig` derives `Clone` and every non-trivial field is an
/// `Arc`, so the clone is cheap.
fn finish_server_config(
    mut config: ServerConfig,
    alpn_protocols: &[String],
) -> (Arc<ServerConfig>, Option<Arc<ServerConfig>>, Vec<Vec<u8>>) {
    let wire: Vec<Vec<u8>> = alpn_protocols
        .iter()
        .map(|p| p.as_bytes().to_vec())
        .collect();
    if wire.is_empty() {
        return (Arc::new(config), None, wire);
    }
    let alpn_free = Arc::new(config.clone());
    config.alpn_protocols = wire.clone();
    (Arc::new(config), Some(alpn_free), wire)
}
```

and in **both** constructors replace `let (config, alpn) = finish_server_config(…); Ok(Self { config, alpn })` with the 3-tuple form:

```rust
        let (config, alpn_free_config, alpn) =
            finish_server_config(config, &cfg.common_tls_context.alpn_protocols);
        Ok(Self {
            config,
            alpn_free_config,
            alpn,
        })
```

(in `from_listener` the second argument is `alpn_protocols.unwrap_or(&[])` instead).

- [ ] **Step 4: Rewrite `accept()`**

```rust
    pub async fn accept(
        &self,
        downstream: TcpStream,
    ) -> Result<tokio_rustls::server::TlsStream<TcpStream>, TlsError> {
        // 112.1 D6'.1: no ALPN configured -> the unchanged pre-112 path. Every
        // config in the tree today, fixtures 0004/0005/0006 included, lands here.
        let Some(alpn_free) = self.alpn_free_config.as_ref() else {
            let acceptor = tokio_rustls::TlsAcceptor::from(self.config.clone());
            return acceptor
                .accept(downstream)
                .await
                .map_err(|source| TlsError::Handshake { source });
        };

        // 112.1 D6': ALPN IS configured. rustls decides ALPN inside
        // `process_common` from the `ServerConfig` already in force, and sends a
        // FATAL `no_application_protocol` alert when the client offered a
        // non-empty set that does not intersect a non-empty server list.
        // Upstream Envoy instead completes the handshake with nothing selected.
        // So peek the ClientHello first and hand `into_stream` the ALPN-free
        // config on a mismatch: rustls then takes the `our_protocols.is_empty()`
        // branch, sends no alert, and selects nothing.
        let start =
            tokio_rustls::LazyConfigAcceptor::new(rustls::server::Acceptor::default(), downstream)
                .await
                .map_err(|source| TlsError::Handshake { source })?;

        // `ClientHello<'a>` borrows `start`, and `into_stream` consumes it, so
        // the borrow must be dead first: reduce to an owned `bool` in this block.
        let advertise = {
            let hello = start.client_hello();
            match hello.alpn() {
                // Client sent no ALPN extension. rustls skips the selection
                // block entirely, so the ALPN-carrying config selects nothing
                // and sends no alert — parity with Envoy for free.
                None => true,
                Some(offered) => {
                    let offered: Vec<&[u8]> = offered.collect();
                    self.alpn
                        .iter()
                        .any(|ours| offered.contains(&ours.as_slice()))
                }
            }
        };

        let config = if advertise {
            self.config.clone()
        } else {
            alpn_free.clone()
        };
        start
            .into_stream(config)
            .await
            .map_err(|source| TlsError::Handshake { source })
    }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p envoy-tls`
Expected: `test result: ok. 21 passed; 0 failed` (16 pre-existing + 5 ALPN). ⚠ Confirm specifically that **`accept_returns_handshake_error_on_garbage_input` is still green** — it takes the unchanged path under D6′.1, and `112.1/SPEC.md` §8 names it as the pin on that behaviour.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-tls/src/lib.rs crates/envoy-tls/src/tests.rs
git commit -m "phase 112.1 task 5: D6' LazyConfigAcceptor accept path — a mismatch completes with no protocol and no alert"
```

---

### Task 6: Offer the list on the UPSTREAM side (D2b, D7)

**Files:**
- Modify: `crates/envoy-tls/src/lib.rs` — `UpstreamTls::from_context`
- Modify: `crates/envoy-tls/src/tests.rs` — one test

**Interfaces:**
- Consumes: `envoy_config::CommonTlsContext::alpn_protocols` (Task 1)
- Produces: nothing new — `UpstreamTls`'s shape and `connect()`'s signature are unchanged.

**This half may NOT be deferred to `112.2`.** `CommonTlsContext` is the type of both contexts, so Task 1 already put the field on `UpstreamTlsContext`; honoring it only downstream would land a parses-then-silently-ignores state, which `ADR-0049` and `ADR-0176` forbid and which `ADR-0184` DECISION 7 names as the binding constraint on where the split seam falls.

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test(flavor = "multi_thread")]
async fn upstream_offers_configured_alpn_to_the_server() {
    install_provider_once();
    let pki = upstream_pki::build();

    // A rustls server that DOES do ALPN, listing ONLY `h2`. It can select `h2`
    // only if `UpstreamTls` actually put `h2` on the wire, so the server's
    // post-handshake `alpn_protocol()` is a direct witness of the client offer.
    let resolver: Arc<dyn rustls::server::ResolvesServerCert> =
        Arc::new(StaticResolver(Arc::new(pki.server_certified_key)));
    let mut server_cfg = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(resolver);
    server_cfg.alpn_protocols = vec![b"h2".to_vec()];
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_cfg));

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        acceptor.accept(stream).await.expect("server handshake")
    });

    // The client lists `http/1.1` FIRST, but the server lists only `h2`, so
    // `h2` must win — which proves BOTH names went out, not just the first.
    let mut ctx = upstream_pki::us_context_with(&pki.ca_pem, "envoy-rust.test");
    ctx.common_tls_context.alpn_protocols = vec!["http/1.1".to_string(), "h2".to_string()];
    let upstream = UpstreamTls::from_context(&ctx).expect("upstream from_context");

    let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let client_tls = upstream.connect(stream).await.expect("upstream connect");
    let server_tls = server_task.await.expect("task joins");

    assert_eq!(
        server_tls.get_ref().1.alpn_protocol(),
        Some(&b"h2"[..]),
        "server must have seen h2 in the client offer"
    );
    assert_eq!(
        client_tls.get_ref().1.alpn_protocol(),
        Some(&b"h2"[..]),
        "client must agree on h2"
    );
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p envoy-tls --lib upstream_offers`
Expected: FAIL — `upstream connect: Custom { kind: InvalidData, error: NoApplicationProtocol }`. The test server lists only `h2` and the client currently offers nothing, so **rustls' server side** sends the alert. That is the same rustls behaviour Task 5 works around downstream, here acting as the assertion.

- [ ] **Step 3: Assign the list to `ClientConfig`**

In `UpstreamTls::from_context`, change `let config = ClientConfig::builder()` to `let mut config = …` and insert after the builder chain, before `let server_name = parse_dns_server_name(&cfg.sni)?;`:

```rust
        let mut config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        // 112.1 D2b/D7: offer the configured list verbatim, in the configured
        // order. Empty means no ALPN extension is sent (D3).
        config.alpn_protocols = cfg
            .common_tls_context
            .alpn_protocols
            .iter()
            .map(|p| p.as_bytes().to_vec())
            .collect();
```

- [ ] **Step 4: Run it to verify it passes**

Run: `cargo test -p envoy-tls`
Expected: `test result: ok. 22 passed; 0 failed`.

- [ ] **Step 5: Full sweep, then commit**

Run: `cargo build --workspace --all-targets && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo fmt --all -- --check && cargo test -p envoy-config -p envoy-tls -p envoy-tcp`

```bash
git add crates/envoy-tls/src/lib.rs crates/envoy-tls/src/tests.rs
git commit -m "phase 112.1 task 6: UpstreamTls offers the configured alpn_protocols on ClientConfig"
```

---

### Task 7: The `parse_bootstrap` fuzz corpus seed

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_downstream_alpn.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` — one `!`-un-ignore line

**Interfaces:** none — data only.

**No NEW fuzz target, so §7.5(d) needs NO `ci.yml` edit.** The existing `crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs` already covers the new config field; it needs a seed that exercises it.

⚠ **A new seed is GITIGNORED BY DEFAULT.** `crates/envoy-config/fuzz/.gitignore` opens with `corpus/parse_bootstrap/*` and un-ignores each of the 66 tracked seeds by name. Verified at this PLAN-write with the **plain** `git check-ignore` form (exit 0 = ignored) — the `-v` form reports negation rules and does not answer the question. **Confirm the seed is tracked with `git ls-files`, never with `git status`.**

- [ ] **Step 1: Create the seed**

It is `tls_downstream_single_cert.yaml` (the closest existing seed, 40 lines) with a renamed `node.id` and three added lines. 43 lines total.

```yaml
node:
  id: fuzz-seed-tls-alpn
  cluster: fuzz
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                alpn_protocols:
                  - h2
                  - http/1.1
                tls_certificates:
                  - certificate_chain:
                      filename: /tmp/cert.pem
                    private_key:
                      filename: /tmp/key.pem
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
```

- [ ] **Step 2: Un-ignore it**

Append to `crates/envoy-config/fuzz/.gitignore` (the file's existing shape is one `!corpus/parse_bootstrap/<name>` line per seed; order does not matter, but keep it after the `corpus/parse_bootstrap/*` line):

```
!corpus/parse_bootstrap/tls_downstream_alpn.yaml
```

- [ ] **Step 3: Verify it is actually tracked**

```bash
git add crates/envoy-config/fuzz/.gitignore crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_downstream_alpn.yaml
git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_downstream_alpn.yaml
```

Expected: the path is printed. **An empty result means the `!` line did not take — do not proceed.** Expected seed census afterwards: `git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/ | wc -l` = **67** (66 before).

- [ ] **Step 4: Verify the seed parses**

The seed must be a *valid* bootstrap, or it seeds the fuzzer with a document that dies in the YAML tokenizer and never reaches the field. Confirmed at this PLAN-write via a throwaway test that ran `crate::parse_bootstrap` over the file and asserted `alpn_protocols == ["h2", "http/1.1"]` — it passed. Re-confirm cheaply:

```bash
cargo run -p envoy-bin -- --mode validate -c crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_downstream_alpn.yaml
```

(or, if that mode is unavailable for this shape, re-add the throwaway test, run it, and delete it before committing).

- [ ] **Step 5: Run the fuzz target briefly**

```bash
cd crates/envoy-config/fuzz && cargo fuzz run parse_bootstrap corpus/parse_bootstrap -- -max_total_time=60
```

⚠ **`cargo fuzz` runs from the CRATE directory, not the repo root, and it takes a DIRECTORY.** Expected: no crash, no new `artifacts/` entry.

- [ ] **Step 6: Commit**

```bash
git commit -m "phase 112.1 task 7: parse_bootstrap fuzz corpus seed for alpn_protocols"
```

---

### Task 8: Mutation proof — the obligation a fixture-free sub-phase carries

**Files:** none committed. This task runs entirely in a **scratch `git worktree`** and leaves the real tree untouched.

`112.1` ships no differential fixture, so §7.5(a) is vacuous by construction and the entire non-vacuity obligation falls on the unit tests (`112.1/SPEC.md` §9). All four mutations below were RUN at this PLAN-write on the prototype (§1 M-R13) and each went RED on exactly the right tests — **but a mutation proof is not transferable between trees, so re-run every one here.**

⚠ **Method, all five rules, every one of which has produced a false result on this project before:**
1. **Assert the target occurs EXACTLY ONCE before mutating.** A `sed` that hits both the implementation and a test fakes a GREEN and reads as "vacuous tests".
2. **Force a rebuild** and confirm it happened — `grep 'Compiling envoy-tls'` (or `envoy-config`). A stale test binary is a FALSE PASS. `touch` the crate root; note that `touch` **creates** files, so name an existing path.
3. **Gate on the `test result` line's EXISTENCE, not on the exit code.** A compile error is NOT a mutation RED.
4. **Run an UNMUTATED control from the same tree**, and confirm it is GREEN.
5. **Use a scratch worktree** — mutation checks collide with anything else running `cargo`, and a worktree branches from the session's START commit, so `git reset --hard main` it first.

- [ ] **Step 1: Set up the worktree and take the control**

```bash
git worktree add --detach /tmp/wt-112-1-mutate HEAD
cd /tmp/wt-112-1-mutate
cargo test -p envoy-tls --lib alpn 2>&1 | grep -E 'Compiling envoy-tls|test result'
cargo test -p envoy-config --lib alpn 2>&1 | grep -E 'Compiling envoy-config|test result'
```

Expected control: `envoy-tls` `ok. 6 passed; 0 failed`; `envoy-config` `ok. 8 passed; 0 failed`.

- [ ] **Step 2: M1 — delete the D6′ config swap**

```bash
grep -c '^            alpn_free.clone()$' crates/envoy-tls/src/lib.rs   # MUST print 1
sed -i 's|^            alpn_free\.clone()$|            self.config.clone()|' crates/envoy-tls/src/lib.rs
touch crates/envoy-tls/src/lib.rs
cargo test -p envoy-tls --lib alpn 2>&1 | grep -E 'Compiling envoy-tls|^test |test result'
git checkout -- crates/envoy-tls/src/lib.rs
```

Expected: `FAILED. 5 passed; 1 failed` — **only** `alpn_mismatch_completes_handshake_with_no_protocol`. Any other test reddening means the mutation is mis-aimed; a GREEN means the mismatch test is vacuous.

- [ ] **Step 3: M2 — delete the `ServerConfig` ALPN threading**

```bash
grep -c '^    config.alpn_protocols = wire.clone();$' crates/envoy-tls/src/lib.rs   # MUST print 1
sed -i 's|^    config\.alpn_protocols = wire\.clone();$|    let _ = \&wire;|' crates/envoy-tls/src/lib.rs
touch crates/envoy-tls/src/lib.rs
cargo test -p envoy-tls --lib alpn 2>&1 | grep -E 'Compiling envoy-tls|^test |test result'
git checkout -- crates/envoy-tls/src/lib.rs
```

Expected: `FAILED. 4 passed; 2 failed` — **only** `alpn_negotiates_h2_when_both_offer_it` and `alpn_selection_follows_server_preference`.

- [ ] **Step 4: M3 — delete the `ClientConfig` ALPN threading**

```bash
grep -c '^        config.alpn_protocols = cfg$' crates/envoy-tls/src/lib.rs   # MUST print 1
```

Replace the six-line assignment (`config.alpn_protocols = cfg … .collect();`) with `let _ = &cfg.common_tls_context.alpn_protocols;`, `touch`, re-run.

Expected: `FAILED. 5 passed; 1 failed` — **only** `upstream_offers_configured_alpn_to_the_server`. This is the mutation that proves the upstream half is real rather than parses-then-silently-ignores.

- [ ] **Step 5: M4 — neuter the >255 validator**

```bash
grep -c '^        if proto.len() > 255 {$' crates/envoy-config/src/bootstrap.rs   # MUST print 1
sed -i 's|^        if proto\.len() > 255 {$|        if proto.len() > usize::MAX {|' crates/envoy-config/src/bootstrap.rs
touch crates/envoy-config/src/lib.rs
cargo test -p envoy-config --lib alpn 2>&1 | grep -E 'Compiling envoy-config|^test |test result'
git checkout -- crates/envoy-config/src/bootstrap.rs
```

Expected: `FAILED. 6 passed; 2 failed` — **only** `rejects_alpn_element_longer_than_255_bytes_on_listener` and `…_on_cluster`.

- [ ] **Step 6: Re-take the control, then tear down**

Re-run Step 1's two commands and confirm both are GREEN again, then:

```bash
cd - && git worktree remove /tmp/wt-112-1-mutate
```

⚠ **Remove only YOUR OWN worktree.** `.claude/worktrees/agent-*` belong to a parallel workstream.

- [ ] **Step 7: Record the evidence**

Quote all four mutation runs plus both controls verbatim into `PROGRESS.md`. Nothing is committed to the tree by this task; the evidence IS the deliverable.

---

## §10. Self-review of this plan

Run against `112.1/SPEC.md` with fresh eyes, per `superpowers:writing-plans`.

**Spec coverage — every §1 deliverable maps to a task:**

| `112.1/SPEC.md` §1 deliverable | task |
|---|---|
| 1. `CommonTlsContext.alpn_protocols: Vec<String>` with `#[serde(default)]` (D1) | 1 |
| 2. Honored on `rustls::ServerConfig` (D2a) | 4 |
| 3. Honored on `rustls::ClientConfig` (D2b) | 6 |
| 4. Absent or empty = do not advertise (D3) | 4 (`alpn_empty_server_list_does_not_advertise`), 3 (`alpn_protocols_defaults_to_empty_when_absent`, `accepts_empty_alpn_protocols_list`) |
| 5. Reject ONLY `len > 255` (D4′) | 2 |
| 6. A mismatch completes the handshake, no alert (D6′) | 5 |
| 7. Unit tests for all of the above + a fuzz corpus seed | 3, 4, 5, 6, 7, 8 |
| D5 (server preference) | 4 (`alpn_selection_follows_server_preference`) |
| D6′.1 (the guard) | 5, and D-PLAN-3 |
| D7 (upstream is a straight assignment) | 6 |

No gap. Every §5 non-goal is a Global Constraint.

**Placeholder scan:** no `TBD`, no `TODO`, no "add appropriate error handling", no "similar to Task N", no "write tests for the above". Every code step carries a real, compiled block. Every referenced type, function and field is either pre-existing in the tree (cited by anchoring text) or defined in an earlier task's **Interfaces**.

**Type consistency:** `finish_server_config` returns a **2-tuple in Task 4** and a **3-tuple in Task 5**, and both tasks say so explicitly with the reason (a dead `alpn_free_config` field fails `-D warnings`). `alpn_handshake` and `ds_context_with_alpn` are introduced in Task 4 and reused unchanged in Task 5; `ds_bootstrap_with_alpn` / `us_bootstrap_with_alpn` / `ds_alpn_of` are introduced in Task 2 and reused unchanged in Task 3. `ConfigError::InvalidAlpnProtocol`'s three fields (`side: &'static str`, `index: usize`, `len: usize`) are spelled identically in the variant, the validator and both reject tests. `DownstreamTls`'s field names (`config`, `alpn_free_config`, `alpn`) are identical across Tasks 4, 5 and 8's mutation targets.

**Citation audit (this plan's own `file:line`s):** this plan cites code by **anchoring text** almost everywhere, precisely because Task 1 inserts six lines into `bootstrap.rs` above `CommonTlsContext` and shifts every line below it. The four numeric citations that remain are all in §1 M-R2's `E0063` table and are explicitly stamped "at `9f2010a`"; they are superseded by their own anchoring-text column the moment Task 1 lands. **No citation in this plan points into a line this plan itself moves without saying so.**

---

## §11. Execution handoff

**Plan complete.** Two execution options for the §5 **state-3** session (a SEPARATE session — §5.1 and `ADR-0127`: the context that wrote an artifact must not grade it):

1. **Subagent-Driven (recommended)** — `superpowers:subagent-driven-development`, a fresh subagent per task with review between tasks. Tasks 1→2→3 and 4→5→6 are strictly sequential within their chains, but the two chains touch disjoint crates after Task 1 lands, and Task 7 is independent of everything after Task 1. **Any tree-mutating subagent gets its OWN worktree, reset to current `main`** — worktrees branch from the session's START commit. Every subagent gets full zero-context instructions (D-3.4) and does TDD (D-3.1). **The main session is the SOLE writer of `PROGRESS.md` and the state ledger.**
2. **Inline Execution** — `superpowers:executing-plans`, batched with checkpoints.

**Do NOT run the §7.5 gate in state 3** — that is state 4, and it is where CI, the differential suite, `cargo deny` and the fmt check first really execute.
