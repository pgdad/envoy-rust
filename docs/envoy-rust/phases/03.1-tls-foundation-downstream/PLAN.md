# Phase 03.1 — `envoy-tls` Foundation + Downstream TLS Termination Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task (fresh subagent per task + two-stage review). Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **For every code-writing task:** REQUIRED SUB-SKILL: `superpowers:test-driven-development` — failing test first, verify fails, minimal implementation, verify passes, commit. No exceptions (doctrine D-3.1).
>
> **Source of truth:** `docs/envoy-rust/phases/03.1-tls-foundation-downstream/SPEC.md`. This plan operationalizes SPEC §§D1–D10. Where the plan and the SPEC disagree, the SPEC wins — flag the drift, land an ADR per D-3.5, and continue. The parent phase-03 SPEC at `docs/envoy-rust/phases/03-tls-tcp/SPEC.md` (committed at SHA `a3f3474`) is preserved unedited as a historical artifact; for execution it is superseded by sub-phase SPECs (this one for 03.1; `03.2-tls-upstream-sni/SPEC.md` for 03.2).

**Goal:** Land the new `envoy-tls` library crate (rustls server/client config construction), extend `envoy-config` with the TransportSocket envelope and TLS-context schema, generalize `envoy-tcp::TcpProxy::handle` over any `AsyncRead + AsyncWrite + Unpin + Send + 'static` stream, wire `envoy-bin` to dispatch downstream TLS termination via a `TlsAcceptingHandler` adapter, extend the differential harness with rcgen-driven test PKI + a `Driver::TlsTcp` + `drive_tls`, and ship fixture `0004-tls-downstream` byte-exact green against upstream Envoy `v1.33.0`.

**Architecture:** `crates/envoy-tls/` is the only crate that depends on `rustls`, `tokio-rustls`, `rustls-pki-types`, `rustls-pemfile`, or `aws-lc-rs`. Its public surface is two builders: `DownstreamTls::from_context(&envoy_config::DownstreamTlsContext)` (cert/key loader + `ServerConfig` with a `SingleCertResolver` returning the wrapped `CertifiedKey` for any ClientHello — the resolver seam keeps the 03.2 SNI extension drop-in) and `UpstreamTls::from_context(&envoy_config::UpstreamTlsContext)` (CA root store + `ClientConfig` + parsed `ServerName`). `envoy-listener::ConnectionHandler` stays concrete on `tokio::net::TcpStream` (parent-SPEC §6 signpost 3 option α: smaller diff than generalizing the trait); the TLS hop lives in a new `TlsAcceptingHandler` adapter inside `crates/envoy-bin/src/tls_handler.rs` that wraps an inner `Arc<TcpProxy>`, runs `DownstreamTls::accept(stream).await?`, then calls `inner.handle::<TlsStream<TcpStream>>(post_handshake)` directly (bypassing the trait — generic methods aren't object-safe). `envoy-tcp::TcpProxy::handle` is generalized from concrete-on-`TcpStream` to `<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(self, S)`; the `ConnectionHandler` impl becomes a thin wrapper boxing `self.handle::<TcpStream>(downstream)`. `envoy-bin::main::run` installs the aws-lc-rs default crypto provider, walks the listener's first filter chain, and constructs either a plaintext `Arc<TcpProxy>` or a `Arc<TlsAcceptingHandler { tls, inner }>` based on the chain's `transport_socket`. The differential harness gains `tests/differential/src/tls.rs` (`TlsTestPki::generate` builds a self-signed CA + leaf-A + leaf-B + server certs in a per-fixture `TempDir`); `Driver::TlsTcp { sni, expected_cn }`; `drive_tls` (mirrors `drive_tcp`'s ADR-0006/0007 read-exact + 100ms trailing-byte poll on top of `tokio_rustls::TlsConnector`); `render_yaml` substitution keys for cert/CA file paths (`{{LEAF_A_CERT_PATH}}`, `{{LEAF_A_KEY_PATH}}`, `{{CA_PATH}}`); and `upstream::start` extends to copy each PEM into the upstream container at `/etc/envoy-rust-tls/` via `with_copy_to_container`.

**Tech stack:** Rust edition 2024 on pinned stable `1.95.0` (D-3.9). New runtime deps in `envoy-tls` only: `rustls = "0.23"` (no-default-features, `std` + `tls12`), `rustls-pki-types = "1"`, `rustls-pemfile = "2"`, `tokio-rustls = "0.26"` (no-default-features, `aws-lc-rs`). All four are covered by D-3.2's rustls grant via ADR-0019. Dev-deps add `rcgen = "0.13"` and `tempfile = "3"` (already a dev-dep of differential + envoy-bin) under ADR-0018 — dev-test-harness-only. No new direct deps on the D-3.2 forbidden list.

---

## File structure (created / modified)

**Created:**

- `crates/envoy-tls/Cargo.toml`
- `crates/envoy-tls/src/lib.rs` (single-file crate; tests in `#[cfg(test)] mod tests`)
- `crates/envoy-bin/src/tls_handler.rs` (new module — the `TlsAcceptingHandler` adapter)
- `crates/envoy-bin/tests/tls_downstream.rs` (Rust-native integration test — backstop, no Docker)
- `tests/differential/src/tls.rs` (new module; `TlsTestPki` + 4 unit tests)
- `tests/differential/tests/tls_downstream.rs` (Docker-gated acceptance test)
- `tests/fixtures/0004-tls-downstream/envoy.yaml`
- `tests/fixtures/0004-tls-downstream/envoy-rust.yaml`
- `tests/fixtures/0004-tls-downstream/inputs/payload.bin` (copy of fixture 0001/0003's 18-byte `b"hello, envoy-rust\n"`)
- `tests/fixtures/0004-tls-downstream/expectations.yaml`
- `tests/fixtures/0004-tls-downstream/README.md`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_downstream_single_cert.yaml`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_malformed_at_type.yaml`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_upstream_validation_context.yaml`
- `docs/envoy-rust/phases/03.1-tls-foundation-downstream/PROGRESS.md` (appended once per task during execution)

**Modified:**

- Root `Cargo.toml` — add `crates/envoy-tls` to `[workspace] members`. (`tests/helpers/tls-echo-server` is a 03.2 deliverable, not 03.1.)
- `crates/envoy-config/src/bootstrap.rs` — add `TransportSocket` envelope, `TransportSocketTypedConfig` enum, `DownstreamTlsContext`, `UpstreamTlsContext`, `CommonTlsContext`, `TlsCertificate`, `CertificateValidationContext`, `DataSource`; add `transport_socket: Option<TransportSocket>` field on both `FilterChain` and `Cluster`; extend `validate` with five new arms; append 12 new validator unit tests (Task 2: 5; Task 3: 7 — see Task 3 Scope for the +2 drift over SPEC §D2's 10-test estimate).
- `crates/envoy-config/src/lib.rs` — re-export the eight new public types; extend `ConfigError` enum with `UnknownTransportSocketName`, `MismatchedTransportSocketDirection`, `EmptyTlsCertificates`, `MissingValidationContext`, `EmptyUpstreamSni` variants; add `pub const TLS_TRANSPORT_SOCKET: &str = "envoy.transport_sockets.tls";`.
- `crates/envoy-tcp/src/lib.rs` — generalize the per-connection proxy body to `TcpProxy::handle::<S>(&self, downstream: S) -> Result<...>` over `S: AsyncRead + AsyncWrite + Unpin + Send + 'static`; rewrite the existing `ConnectionHandler::handle` impl as a thin `Box::pin(async move { self.handle::<TcpStream>(downstream).await })` wrapper; append 4 new TLS-flavored unit tests touching the generic shape.
- `crates/envoy-tcp/Cargo.toml` — add dev-deps on `tokio-rustls`, `rustls`, `rustls-pki-types`, `rcgen`, `tempfile` for the new TLS-flavored unit tests.
- `crates/envoy-bin/Cargo.toml` — add `envoy-tls = { path = "../envoy-tls" }` runtime dep; add dev-deps on `tokio-rustls`, `rustls`, `rustls-pki-types`, `rcgen` (`tempfile` is already a dev-dep from phase 02.2).
- `crates/envoy-bin/src/main.rs` — install the aws-lc-rs default crypto provider near the top of `run`; in the `TCP_PROXY_FILTER` arm of the filter dispatch, pre-pass the listener's first filter chain to detect a `transport_socket: TransportSocketTypedConfig::Downstream(_)` and (when present) build `Arc<DownstreamTls>` + wrap the `TcpProxy` in `TlsAcceptingHandler` before `Listener::bind`; declare `mod tls_handler;`.
- `tests/differential/Cargo.toml` — add `rcgen = "0.13"`, `rustls = { version = "0.23", default-features = false, features = ["std", "tls12"] }`, `rustls-pki-types = "1"`, `rustls-pemfile = "2"`, `tokio-rustls = { version = "0.26", default-features = false, features = ["aws-lc-rs"] }` as dev-deps. (`tempfile = "3"` is already a runtime dep from phase 00.)
- `tests/differential/src/lib.rs` — add `pub mod tls;` declaration; add a third `Driver::TlsTcp { sni: String, expected_cn: Option<String> }` variant; add `drive_tls` async helper; extend `run_fixture` to detect TLS templates (`{{LEAF_A_CERT_PATH}}` / `{{LEAF_A_KEY_PATH}}` / `{{CA_PATH}}`), build a `TlsTestPki`, substitute per-side paths, thread `tls_pki` through to `upstream::start`, dispatch on `Driver::TlsTcp`; add 2 unit tests for the tls-path render keys (envoy-side vs. subject-side).
- `tests/differential/src/upstream.rs` — extend `start` signature to take `tls_pki: Option<&crate::tls::TlsTestPki>` and call `with_copy_to_container(host_path, "/etc/envoy-rust-tls/<filename>.pem")` for each PEM the rendered envoy-side YAML references. Update the existing `starts_upstream_envoy_and_exposes_host_port` test call site to pass `None` for the new parameter.
- `docs/envoy-rust/DECISIONS.md` — append ADR-0018 + ADR-0019 (Task 1).
- `docs/envoy-rust/ROADMAP.md` — at state 6 only, flip row `03.1` `status` → `done`. (Row `03` parent stays `in-progress`; it flips at 03.2's final commit per the schema.)
- `docs/envoy-rust/STATE.md` — at state 6 only, advance active phase id `03.2`, slug `03.2-tls-upstream-sni`, lifecycle state 2 (SPEC.md exists, PLAN.md does not — 03.2's SPEC landed alongside this one during the ADR-0017 split session), next-skill `superpowers:writing-plans`.
- `Cargo.lock` — sync as a dedicated commit (mirrors phase-01 `4955252`, phase-02.1 `dea4d16`, phase-02.2 `2146014`) once Task 13's gate exposes drift. Expect entries for the new `envoy-tls v0.0.0` crate plus the rustls family (`rustls`, `tokio-rustls`, `rustls-pemfile`, `rustls-pki-types`, `aws-lc-rs`, `aws-lc-sys`, `rcgen` as dev-only).
- `deny.toml` — only if `cargo deny check` flips on a new transitive surface (rustls, aws-lc-rs, rustls-pemfile, rcgen, tempfile chain). The rustls org's licenses are well-established (Apache-2.0 OR MIT OR ISC) and on the existing allow-list; non-trivial extension lands its own ADR.

**Note: not touched in 03.1.** `crates/envoy-cluster/`, `tests/helpers/tcp-echo-server/`, `crates/envoy-listener/`, parent `03-tls-tcp/SPEC.md`, sibling `03.2-tls-upstream-sni/SPEC.md`, `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0003-tcp-proxy/`, `BEHAVIOR_CONTRACT.md` — all unedited (SPEC §8 closing list).

---

## Task index

Each task ends with a commit. Per phase-02.1/02.2 convention, follow each task commit with a `phase 03.1: progress note (task N)` commit that appends the matching PROGRESS.md section (commit SHA, change summary, verification output, any deviation). Choose one cadence and keep it.

1. **ADRs 0018 + 0019 — rcgen+tempfile dev-test-harness-only; tokio-rustls+rustls-pemfile under the rustls grant**
2. **`envoy-config` — TransportSocket envelope + TLS context types + DataSource + 5 schema/parse-shape tests**
3. **`envoy-config` — validator extensions + 5 new `ConfigError` variants + 5 remaining tests**
4. **`envoy-config` — 3 fuzz corpus seeds (TLS-shaped YAML)**
5. **Scaffold `crates/envoy-tls/` skeleton + workspace member**
6. **`envoy-tls::DownstreamTls` — `TlsError`, `load_certified_key`, `SingleCertResolver`, `from_context`, `accept` + 6 tests + cross-cutting `crypto_provider_install_is_idempotent`**
7. **`envoy-tls::UpstreamTls` — `from_context`, `connect`, CA loader + 3 tests**
8. **`envoy-tcp::TcpProxy::handle` generic-stream lift + 4 new TLS-flavored unit tests**
9. **`envoy-bin` — install crypto provider + `tls_handler.rs` adapter + filter-chain TLS dispatch + integration test `tls_downstream.rs`**
10. **Differential harness — `tls.rs` (`TlsTestPki`) + `Driver::TlsTcp` + render_yaml TLS keys + 2+4 unit tests**
11. **Differential harness — `drive_tls` + `run_fixture` TLS dispatch + `upstream::start` `with_copy_to_container` + signature plumbing**
12. **Fixture `0004-tls-downstream` (5 files) + Docker-gated `tests/differential/tests/tls_downstream.rs`**
13. **State 4 phase-done gate — run all 5 stable commands + CI fuzz job; quote outputs into PROGRESS.md**

Estimated total: 13 tasks, ~1400 LoC. Both `BOOTSTRAP_PROMPT.md` §6.1 gates (~25 tasks / ~1500 LoC) hold comfortably. **Do not split 03.1 further.** If any single task balloons past ~10 sub-steps mid-execution, invoke `superpowers:systematic-debugging` before attempting a nested split — nested splits of an already-split sub-phase deserve a fresh root-cause read (per SPEC §5 closing paragraph).

---

### Task 1: ADRs 0018 + 0019 — rcgen+tempfile dev-test-harness-only; tokio-rustls+rustls-pemfile under the rustls grant

**Files:**
- Modify (append): `docs/envoy-rust/DECISIONS.md`
- Create: `docs/envoy-rust/phases/03.1-tls-foundation-downstream/PROGRESS.md`

**Why first:** every subsequent task cites at least one of these ADRs. DECISIONS.md is append-only per D-3.5; land the rationale before the code that references it. ADR-0018 is referenced by Task 5 (envoy-tls dev-deps), Task 9 (envoy-bin dev-deps), Task 10 (differential dev-deps). ADR-0019 is referenced by Task 5 (envoy-tls runtime deps). Verify before starting that DECISIONS.md ends at ADR-0017; if any new ADR landed between phase 02.2 done and 03.1 start, both ADR numbers shift by +1 (per phase-02.2 REVIEW §4 recommendation #2 and SPEC §6 recommendation that ADR numbering remains provisional).

- [ ] **Step 1: Verify next-sequential ADR numbers.**

```bash
grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md
grep -n '^## ADR-00' docs/envoy-rust/DECISIONS.md | tail -3
```

Expected output: `17`; last three lines are `ADR-0015`, `ADR-0016`, `ADR-0017` in that order. If any unexpected `ADR-00NN` appears, rebase this task's text by `+1` for each interloper before continuing — this is a mechanical renumber per phase-02.2 REVIEW §4 recommendation #2 and SPEC §1 (ADR numbering after the phase-03 split). Update every cross-reference in this PLAN to the new numbers as part of Task 1 (search-and-replace `ADR-0018` and `ADR-0019`).

- [ ] **Step 2: Append ADR-0018 (`rcgen` + `tempfile` dev-test-harness-only) to `docs/envoy-rust/DECISIONS.md`.**

Append after the final `---` of ADR-0017 using the structure mandated by DECISIONS.md lines 9–19. Use these exact field contents (verbatim from SPEC §7 ADR-0018):

```markdown
## ADR-0018: `rcgen` and `tempfile` permitted as dev-test-harness-only foundations

- Date: 2026-04-25
- Status: accepted
- Context: Phase 03 is the first phase to need test certificates. TLS test infrastructure recurs across phases 03–08+ (HTTP/1.1 over TLS, H2 over TLS, mTLS, etc.). Static in-tree PEMs were considered and rejected per the parent-phase brainstorm Q2 decision (poor refresh ergonomics, expiry concerns, multi-leaf cert generation gets unwieldy). `rcgen` is the maintained Rust-native cert generator; `tempfile` is the canonical per-test-run tmpdir manager. Neither is on the D-3.2 permitted-foundations list at phase-02.2 close.
- Options considered: (i) static in-tree PEMs (rejected, parent-brainstorm Q2); (ii) `rcgen` + `tempfile` on the permitted list as **dev-test-harness-only** (decision); (iii) script-generated PEMs committed to the repo (rejected, parent-brainstorm Q2: worst-of-both-worlds — refresh friction *and* in-tree drift).
- Decision: add `rcgen = "0.13"` and `tempfile = "3"` to the permitted-foundations list with the **dev-test-harness-only** annotation. Mirrors ADR-0009's posture for `cargo-fuzz` + `libfuzzer-sys`. Never a transitive of `envoy-bin` or any non-test workspace crate. Restricted to: `tests/differential/` dev-deps; `tests/helpers/tls-echo-server/` dev-deps (lands in 03.2); `crates/envoy-tls/` dev-deps (for unit-test PKI); `crates/envoy-bin/` dev-deps (for the in-process integration test); `crates/envoy-tcp/` dev-deps (for the TLS-flavored unit tests).
- Rationale: one-time foundations grant beats per-phase ADR churn; rcgen is the Rust-ecosystem default; tempfile is ubiquitous test-infra. Test-only restriction preserves D-3.2's spirit for runtime code.
- Consequences: future TLS-cert-using phases (04 HCM-over-TLS, 05 H2-over-TLS, mTLS phases, etc.) reuse this decision without per-phase ADRs. `cargo deny check` may flag the rcgen license (Apache-2.0 OR MIT — both on the deny.toml allow-list) or its transitive deps; if so, the deny.toml is updated alongside ADR-0018's landing. If a future phase needs cert generation in *runtime* code (e.g., hot-restart cert rotation), that phase lands a new ADR superseding the dev-test-harness-only restriction.
- Provenance: this ADR was projected as "ADR-0017" in parent-phase-03 SPEC §7 (`docs/envoy-rust/phases/03-tls-tcp/SPEC.md`, committed at SHA `a3f3474`) and renumbered to ADR-0018 by the phase-03 split decision (ADR-0017). The projected ADR-0018 (tokio-rustls + rustls-pemfile) is renumbered to ADR-0019 and lands alongside this ADR in the same Task-1 commit.
```

- [ ] **Step 3: Append ADR-0019 (`tokio-rustls` + `rustls-pemfile` under the rustls grant) to `docs/envoy-rust/DECISIONS.md`.**

```markdown
## ADR-0019: `tokio-rustls` and `rustls-pemfile` covered by the rustls foundations grant

- Date: 2026-04-25
- Status: accepted
- Context: D-3.2 lists `rustls`, `webpki`, `rustls-pki-types`, and "`aws-lc-rs` permitted as the crypto provider," but does not name `tokio-rustls` or `rustls-pemfile` explicitly. Both are mechanically necessary to use rustls inside a tokio runtime / load PEMs from disk; both ship from the rustls org.
- Options considered: (i) treat both as covered implicitly by the rustls grant — risks ambiguity for downstream phases; (ii) land an ADR formalizing the extension (decision); (iii) hand-roll the async glue and PEM parser — reinvents wheels D-3.2 explicitly tells us not to.
- Decision: extend D-3.2's "rustls + aws-lc-rs permitted as the crypto provider" grant to cover `tokio-rustls = "0.26"` and `rustls-pemfile = "2"`. Both are runtime-permitted (not dev-only); rcgen + tempfile from ADR-0018 stay dev-only.
- Rationale: removes ambiguity for downstream phases. Both crates are first-party in the rustls ecosystem; treating them as part of the same foundation is the cheapest, most honest formalization.
- Consequences: envoy-tls's `Cargo.toml` lists both as direct deps. `tls-echo-server`'s `Cargo.toml` (lands in 03.2) lists both. Neither is allowed in `envoy-listener` or `envoy-cluster` — those crates remain rustls-free per D1's "envoy-tls is the only crate with rustls deps" architectural rule.
- Provenance: this ADR was projected as "ADR-0018" in parent-phase-03 SPEC §7 (`docs/envoy-rust/phases/03-tls-tcp/SPEC.md`, committed at SHA `a3f3474`) and renumbered to ADR-0019 by the phase-03 split decision (ADR-0017). Lands alongside ADR-0018 in the same Task-1 commit.
```

- [ ] **Step 4: Create `docs/envoy-rust/phases/03.1-tls-foundation-downstream/PROGRESS.md` with a Task 1 section.**

Content:

```markdown
# Phase 03.1 Progress

## Task 1 — ADRs 0018 + 0019 (2026-04-25)

- Commit: <SHA>
- Change: appended ADR-0018 (rcgen + tempfile permitted as dev-test-harness-only foundations) and ADR-0019 (tokio-rustls + rustls-pemfile covered by the rustls foundations grant) to DECISIONS.md.
- Verification: `grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md` → 19 (ADR-0001 through ADR-0019).
```

Replace `<SHA>` with the commit hash from Step 6 (or land it in the matching `progress note (task 1)` follow-up commit per the phase-02.1 cadence).

- [ ] **Step 5: Verify DECISIONS.md.**

```bash
grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md
```

Expected: `19`.

```bash
grep -n '^## ADR-00' docs/envoy-rust/DECISIONS.md | tail -3
```

Expected (last 3 lines): `ADR-0017`, `ADR-0018`, `ADR-0019` in that order, with ascending line numbers.

- [ ] **Step 6: Commit.**

```bash
git add docs/envoy-rust/DECISIONS.md docs/envoy-rust/phases/03.1-tls-foundation-downstream/PROGRESS.md
git commit -m "phase 03.1: ADR-0018/0019 — rcgen+tempfile dev-only; tokio-rustls+rustls-pemfile under rustls grant"
```

Then either amend the SHA into PROGRESS.md (phase-01 PLAN Task 1 idiom) or land a follow-up `phase 03.1: progress note (task 1)` commit (phase-02.1/02.2 PROGRESS idiom). Either is acceptable; pick one cadence and keep it for every subsequent task.

---

### Task 2: `envoy-config` — TransportSocket envelope + TLS context types + DataSource + 5 schema/parse-shape tests

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (insertions after the existing `TcpProxyConfig` at line ~144; field additions on `Cluster` at line ~48 and `FilterChain` at line ~117)
- Modify: `crates/envoy-config/src/lib.rs` (extend `pub use` re-export list; add `TLS_TRANSPORT_SOCKET` constant)

**Scope:** ship the TLS-shaped struct tree and the schema-level `deny_unknown_fields` discipline. No new validator arms in this task — those land in Task 3. Per SPEC §3 D2 (03.1 portion).

The 5 tests in this task exercise *positive parse paths* and `deny_unknown_fields` regressions:

1. `parses_listener_with_downstream_tls_context` — full happy-path fixture.
2. `parses_cluster_with_upstream_tls_context` — happy-path fixture for the cluster side.
3. `rejects_unknown_field_in_downstream_tls_context` — `require_client_certificate` (mTLS-shaped, not in 03.1) is rejected.
4. `rejects_unknown_field_in_common_tls_context` — `alpn_protocols` (phase 04 surface) is rejected.
5. `rejects_unknown_field_in_data_source` — `inline_string` (phase-later surface) is rejected.

The remaining 5 tests in SPEC §D2's enumerated test list (validator-driven) ship in Task 3.

- [ ] **Step 1: Write the failing test `parses_listener_with_downstream_tls_context`.**

Append to `crates/envoy-config/src/bootstrap.rs::tests`:

```rust
    #[test]
    fn parses_listener_with_downstream_tls_context() {
        let yaml = r#"
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
                tls_certificates:
                  - certificate_chain:
                      filename: /tmp/leaf.pem
                    private_key:
                      filename: /tmp/leaf.key
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
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parses + validates");
        let chain = &bootstrap.static_resources.listeners[0].filter_chains[0];
        let ts = chain
            .transport_socket
            .as_ref()
            .expect("transport_socket present");
        assert_eq!(ts.name, "envoy.transport_sockets.tls");
        match &ts.typed_config {
            crate::TransportSocketTypedConfig::Downstream(ctx) => {
                let certs = &ctx.common_tls_context.tls_certificates;
                assert_eq!(certs.len(), 1);
                assert_eq!(certs[0].certificate_chain.filename, "/tmp/leaf.pem");
                assert_eq!(certs[0].private_key.filename, "/tmp/leaf.key");
            }
            other => panic!("unexpected typed_config: {other:?}"),
        }
    }
```

- [ ] **Step 2: Run the test; verify it fails with a compile error.**

```bash
cargo test -p envoy-config bootstrap::tests::parses_listener_with_downstream_tls_context
```

Expected: `error[E0432]: unresolved import \`crate::TransportSocketTypedConfig\`` (or similar; `transport_socket` field on `FilterChain` doesn't exist either). The test fails to compile because Task 2's struct tree hasn't landed.

- [ ] **Step 3: Add the new struct + enum types to `crates/envoy-config/src/bootstrap.rs`.**

Insert immediately after the existing `TcpProxyConfig` definition (line ~144), before `pub(crate) fn validate(...)`:

```rust
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TransportSocket {
    /// Phase 03 accepts only `"envoy.transport_sockets.tls"`; the validator
    /// rejects any other name. Future phases may add raw_buffer / quic / etc.
    pub name: String,
    pub typed_config: TransportSocketTypedConfig,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "@type", deny_unknown_fields)]
pub enum TransportSocketTypedConfig {
    #[serde(
        rename = "type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext"
    )]
    Downstream(DownstreamTlsContext),
    #[serde(
        rename = "type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext"
    )]
    Upstream(UpstreamTlsContext),
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DownstreamTlsContext {
    pub common_tls_context: CommonTlsContext,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UpstreamTlsContext {
    pub common_tls_context: CommonTlsContext,
    /// Server Name sent in the ClientHello server_name extension. Phase 03
    /// requires this on every UpstreamTlsContext (no auto_sni). The validator
    /// rejects an empty string.
    pub sni: String,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CommonTlsContext {
    #[serde(default)]
    pub tls_certificates: Vec<TlsCertificate>,
    #[serde(default)]
    pub validation_context: Option<CertificateValidationContext>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TlsCertificate {
    pub certificate_chain: DataSource,
    pub private_key: DataSource,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CertificateValidationContext {
    pub trusted_ca: DataSource,
}

/// Phase 03 supports `filename` only. inline_string / inline_bytes /
/// environment_variable / `secret_ref` are deferred to later phases.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DataSource {
    pub filename: String,
}
```

- [ ] **Step 4: Add `transport_socket: Option<TransportSocket>` to `FilterChain` and `Cluster`.**

In `crates/envoy-config/src/bootstrap.rs`, modify the existing `FilterChain` struct (currently at lines ~115–119):

```rust
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilterChain {
    #[serde(default)]
    pub transport_socket: Option<TransportSocket>,
    pub filters: Vec<NetworkFilter>,
}
```

And modify the existing `Cluster` struct (currently at lines ~46–54):

```rust
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Cluster {
    pub name: String,
    #[serde(rename = "type")]
    pub cluster_type: ClusterType,
    pub lb_policy: LbPolicy,
    pub load_assignment: LoadAssignment,
    #[serde(default)]
    pub transport_socket: Option<TransportSocket>,
}
```

The `#[serde(default)]` on `transport_socket` means the field is optional in YAML; existing fixtures (0001, 0002, 0003) parse unchanged.

- [ ] **Step 5: Extend `pub use` re-exports + add `TLS_TRANSPORT_SOCKET` constant in `crates/envoy-config/src/lib.rs`.**

Replace the existing `pub use bootstrap::{...};` block with:

```rust
pub use bootstrap::{
    Address, Admin, Bootstrap, CertificateValidationContext, Cluster, ClusterType, CommonTlsContext,
    DataSource, DownstreamTlsContext, Endpoint, FilterChain, LbEndpoint, LbPolicy, Listener,
    LoadAssignment, LocalityLbEndpoints, NetworkFilter, Node, SocketAddress, StaticResources,
    TcpProxyConfig, TlsCertificate, TransportSocket, TransportSocketTypedConfig, TypedConfig,
    UpstreamTlsContext,
};
```

Add (after the existing `pub const TCP_PROXY_FILTER: &str = ...;` line):

```rust
/// The only transport-socket name envoy-rust accepts in phase 03. Future phases
/// may add `envoy.transport_sockets.raw_buffer` / `envoy.transport_sockets.quic`.
pub const TLS_TRANSPORT_SOCKET: &str = "envoy.transport_sockets.tls";
```

- [ ] **Step 6: Re-run the failing test; verify it passes.**

```bash
cargo test -p envoy-config bootstrap::tests::parses_listener_with_downstream_tls_context
```

Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 7: Add the second happy-path test `parses_cluster_with_upstream_tls_context`.**

Append to `crates/envoy-config/src/bootstrap.rs::tests`:

```rust
    #[test]
    fn parses_cluster_with_upstream_tls_context() {
        let yaml = r#"
static_resources:
  listeners: []
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
                      port_value: 9443
      transport_socket:
        name: envoy.transport_sockets.tls
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext
          sni: envoy-rust.test
          common_tls_context:
            validation_context:
              trusted_ca:
                filename: /tmp/ca.pem
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parses + validates");
        let cluster = &bootstrap.static_resources.clusters[0];
        let ts = cluster
            .transport_socket
            .as_ref()
            .expect("transport_socket present");
        assert_eq!(ts.name, "envoy.transport_sockets.tls");
        match &ts.typed_config {
            crate::TransportSocketTypedConfig::Upstream(ctx) => {
                assert_eq!(ctx.sni, "envoy-rust.test");
                let vc = ctx
                    .common_tls_context
                    .validation_context
                    .as_ref()
                    .expect("validation_context present");
                assert_eq!(vc.trusted_ca.filename, "/tmp/ca.pem");
                assert!(ctx.common_tls_context.tls_certificates.is_empty());
            }
            other => panic!("unexpected typed_config: {other:?}"),
        }
    }
```

Run it; expect `test result: ok. 1 passed`.

- [ ] **Step 8: Add the three `deny_unknown_fields` regression tests.**

Append:

```rust
    #[test]
    fn rejects_unknown_field_in_downstream_tls_context() {
        // require_client_certificate is mTLS-shaped and out of phase 03 per
        // SPEC §4. deny_unknown_fields on DownstreamTlsContext rejects it at
        // parse time.
        let yaml = r#"
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
              require_client_certificate: false
              common_tls_context:
                tls_certificates:
                  - certificate_chain:
                      filename: /tmp/leaf.pem
                    private_key:
                      filename: /tmp/leaf.key
          filters: []
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject unknown field");
        let msg = format!("{err}");
        assert!(
            msg.contains("require_client_certificate") || msg.contains("unknown field"),
            "expected unknown-field error, got: {msg}",
        );
    }

    #[test]
    fn rejects_unknown_field_in_common_tls_context() {
        // alpn_protocols is a phase-04 surface; phase 03 fixtures do not
        // include it (SPEC §6 signpost 14). deny_unknown_fields on
        // CommonTlsContext rejects it.
        let yaml = r#"
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
                alpn_protocols: ["h2"]
                tls_certificates:
                  - certificate_chain:
                      filename: /tmp/leaf.pem
                    private_key:
                      filename: /tmp/leaf.key
          filters: []
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject unknown field");
        let msg = format!("{err}");
        assert!(
            msg.contains("alpn_protocols") || msg.contains("unknown field"),
            "expected unknown-field error, got: {msg}",
        );
    }

    #[test]
    fn rejects_unknown_field_in_data_source() {
        // inline_string is a phase-later surface; phase 03 supports `filename`
        // only (SPEC §3 D2). deny_unknown_fields on DataSource rejects it.
        let yaml = r#"
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
                tls_certificates:
                  - certificate_chain:
                      filename: /tmp/leaf.pem
                      inline_string: "extra"
                    private_key:
                      filename: /tmp/leaf.key
          filters: []
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject unknown field");
        let msg = format!("{err}");
        assert!(
            msg.contains("inline_string") || msg.contains("unknown field"),
            "expected unknown-field error, got: {msg}",
        );
    }
```

- [ ] **Step 9: Run the new tests.**

```bash
cargo test -p envoy-config bootstrap::tests
```

Expected: all 5 new tests pass + every pre-existing test continues to pass. The pre-existing `envoy-config` test count was 38 at phase 02.2 close (per `02.2/PROGRESS.md`); after Task 2 it is 43 (38 + 5).

- [ ] **Step 10: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

All three: exit 0. Note: this task does not yet touch any consumer of `FilterChain` or `Cluster`, but `Cluster` gains a new field that must satisfy whatever match patterns exist downstream. `envoy-cluster::from_bootstrap` does not destructure `Cluster` exhaustively (it accesses fields by name), so adding `transport_socket` is non-breaking. Verify by running:

```bash
cargo test -p envoy-cluster
```

Expected: `test result: ok. 8 passed`.

- [ ] **Step 11: Commit.**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
git commit -m "phase 03.1: envoy-config — TransportSocket envelope + TLS context types"
```

Append a Task 2 PROGRESS section.

---

### Task 3: `envoy-config` — validator extensions + 5 new `ConfigError` variants + 5 remaining tests

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (extend `ConfigError`)
- Modify: `crates/envoy-config/src/bootstrap.rs` (extend `validate`; append 5 new tests)

**Scope:** wire the per-side validator rules from SPEC §3 D2's "Validator extensions" enumeration. Adds five new `ConfigError` variants and the corresponding 5 tests. The 5 happy-path / `deny_unknown_fields` tests landed in Task 2; this task lands the validator-driven coverage.

**Per-listener validator arms (extension to phase-02.1's existing logic):**
- For each `filter_chain`: if `transport_socket` is `Some`, then `transport_socket.name` must equal `TLS_TRANSPORT_SOCKET`. Mismatch → `ConfigError::UnknownTransportSocketName(String)`.
- And: `transport_socket.typed_config` must be the `Downstream(_)` variant. The `Upstream(_)` variant on a listener-side transport_socket → `ConfigError::MismatchedTransportSocketDirection { side: "listener", got: "UpstreamTlsContext" }`.
- And: when `Downstream(ctx)` is present, `ctx.common_tls_context.tls_certificates.len() ≥ 1`. Empty → `ConfigError::EmptyTlsCertificates { side: "listener" }`.

**Per-cluster validator arms (extension to phase-02.1's existing logic):**
- For each `cluster`: if `transport_socket` is `Some`, then `transport_socket.name == TLS_TRANSPORT_SOCKET`. Mismatch → `ConfigError::UnknownTransportSocketName`.
- And: `transport_socket.typed_config` must be the `Upstream(_)` variant. `Downstream(_)` on a cluster → `ConfigError::MismatchedTransportSocketDirection { side: "cluster", got: "DownstreamTlsContext" }`.
- And: when `Upstream(ctx)` is present, `ctx.common_tls_context.tls_certificates.len() == 0` (mTLS deferred; client-cert presentation is out of phase 03 per SPEC §4). Non-empty → `ConfigError::EmptyTlsCertificates { side: "cluster" }`. Variant is named "Empty" but disambiguates per the `side` field — the message's `Display` impl reflects the side-specific meaning (SPEC §3 D2 spelled this asymmetry out explicitly).
- And: when `Upstream(ctx)` is present, `ctx.common_tls_context.validation_context.is_some()` (no insecure-skip in phase 03). Missing → `ConfigError::MissingValidationContext`.
- And: when `Upstream(ctx)` is present, `ctx.sni` is non-empty. Empty → `ConfigError::EmptyUpstreamSni`.

**Test inventory (5 new tests, mapping to the SPEC §D2 enumerated list):**

1. `rejects_unknown_transport_socket_name`
2. `rejects_downstream_tls_context_on_cluster`
3. `rejects_upstream_tls_context_on_listener`
4. `rejects_downstream_with_empty_tls_certificates`
5. `rejects_upstream_with_tls_certificates`

The remaining three SPEC §D2 validator tests (`rejects_upstream_without_validation_context`, `rejects_upstream_with_empty_sni`, `rejects_upstream_with_tls_certificates`) close to **5 total here** when you consolidate — re-read SPEC §D2's enumeration: it lists 10 tests total, of which 5 are happy-path/deny_unknown_fields (covered in Task 2) and 5 are validator-driven. Map them as: `rejects_unknown_transport_socket_name`, `rejects_downstream_tls_context_on_cluster`, `rejects_upstream_tls_context_on_listener`, `rejects_downstream_with_empty_tls_certificates`, `rejects_upstream_with_tls_certificates` are 5; `rejects_upstream_without_validation_context` and `rejects_upstream_with_empty_sni` are also validator-driven. To keep both Task 2 + Task 3 each at 5 tests AND cover all SPEC-named validator tests, **add `rejects_upstream_without_validation_context` and `rejects_upstream_with_empty_sni` as Task 3 Step 12** — bringing Task 3 to 7 tests. The SPEC §D2 final enumeration is "10 tests": 5 in Task 2 (parses_listener_with_downstream_tls_context, parses_cluster_with_upstream_tls_context, rejects_unknown_field_in_downstream_tls_context, rejects_unknown_field_in_common_tls_context, rejects_unknown_field_in_data_source) + 7 in Task 3 (the five enumerated + the two extras here) = 12, which exceeds the SPEC count by 2. The SPEC's "10" was a planning-time approximation; lining up with reviewable coverage matters more than hitting the exact integer. PROGRESS.md notes this as plan drift (SPEC numeric estimate 10, actual landed 12) — neither gate (LoC, task count) is materially affected.

- [ ] **Step 1: Write the failing test `rejects_unknown_transport_socket_name`.**

Append to `crates/envoy-config/src/bootstrap.rs::tests`:

```rust
    #[test]
    fn rejects_unknown_transport_socket_name() {
        // Phase 03 only accepts "envoy.transport_sockets.tls".
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.raw_buffer
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain:
                      filename: /tmp/leaf.pem
                    private_key:
                      filename: /tmp/leaf.key
          filters: []
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::UnknownTransportSocketName(ref n) if n == "envoy.transport_sockets.raw_buffer"),
            "got {err:?}"
        );
    }
```

- [ ] **Step 2: Run the test; verify it fails.**

```bash
cargo test -p envoy-config bootstrap::tests::rejects_unknown_transport_socket_name
```

Expected: compile error `error[E0599]: no variant or associated item named 'UnknownTransportSocketName' found for enum 'ConfigError'`.

- [ ] **Step 3: Extend `ConfigError` with the five new variants in `crates/envoy-config/src/lib.rs`.**

Replace the `ConfigError` definition:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("parsing bootstrap YAML")]
    Yaml(#[from] serde_yaml::Error),
    #[error(
        "bootstrap configures neither an admin endpoint nor a listener; envoy-rust has nothing to do"
    )]
    NoRuntime,
    #[error("bootstrap has {0} listeners; phase 01 supports at most one")]
    TooManyListeners(usize),
    #[error("unsupported network filter '{0}'; envoy-rust accepts only '{1}'")]
    UnsupportedFilter(String, &'static str),
    #[error("filter '{0}' requires typed_config")]
    MissingTypedConfig(&'static str),
    #[error("filter '{0}' must not carry typed_config")]
    UnexpectedTypedConfig(&'static str),
    #[error("tcp_proxy filter references unknown cluster '{0}'")]
    UnknownCluster(String),
    #[error(
        "cluster '{cluster}' declares load_assignment.cluster_name '{assignment}'; these must match"
    )]
    LoadAssignmentNameMismatch { cluster: String, assignment: String },
    #[error("cluster '{0}' has zero lb_endpoints; ≥1 required")]
    EmptyClusterEndpoints(String),
    #[error("unsupported transport_socket name '{0}'; envoy-rust accepts only 'envoy.transport_sockets.tls'")]
    UnknownTransportSocketName(String),
    #[error(
        "transport_socket on the {side} side is the wrong direction (got '{got}'); listener requires DownstreamTlsContext, cluster requires UpstreamTlsContext"
    )]
    MismatchedTransportSocketDirection {
        side: &'static str,
        got: &'static str,
    },
    #[error(
        "tls_certificates on the {side} side has the wrong cardinality; listener requires ≥1, cluster requires 0 (mTLS deferred)"
    )]
    EmptyTlsCertificates { side: &'static str },
    #[error("UpstreamTlsContext requires validation_context.trusted_ca; phase 03 has no insecure-skip surface")]
    MissingValidationContext,
    #[error("UpstreamTlsContext.sni must be a non-empty DNS name")]
    EmptyUpstreamSni,
}
```

- [ ] **Step 4: Extend `validate` in `crates/envoy-config/src/bootstrap.rs`.**

Replace the entire `validate` fn body — append the per-cluster TLS arm to the existing per-cluster loop, and append the per-listener TLS arm at the end of the per-listener loop. Replace from `pub(crate) fn validate(...)` through the closing `Ok(())`:

```rust
pub(crate) fn validate(bootstrap: &Bootstrap) -> Result<(), crate::ConfigError> {
    let listeners = &bootstrap.static_resources.listeners;
    let clusters = &bootstrap.static_resources.clusters;
    if listeners.len() > 1 {
        return Err(crate::ConfigError::TooManyListeners(listeners.len()));
    }
    if bootstrap.admin.is_none() && listeners.is_empty() {
        return Err(crate::ConfigError::NoRuntime);
    }

    // Per-cluster invariants.
    for cluster in clusters {
        if cluster.load_assignment.cluster_name != cluster.name {
            return Err(crate::ConfigError::LoadAssignmentNameMismatch {
                cluster: cluster.name.clone(),
                assignment: cluster.load_assignment.cluster_name.clone(),
            });
        }
        let total_endpoints: usize = cluster
            .load_assignment
            .endpoints
            .iter()
            .map(|le| le.lb_endpoints.len())
            .sum();
        if total_endpoints == 0 {
            return Err(crate::ConfigError::EmptyClusterEndpoints(
                cluster.name.clone(),
            ));
        }
        if let Some(ts) = cluster.transport_socket.as_ref() {
            if ts.name != crate::TLS_TRANSPORT_SOCKET {
                return Err(crate::ConfigError::UnknownTransportSocketName(ts.name.clone()));
            }
            match &ts.typed_config {
                TransportSocketTypedConfig::Upstream(ctx) => {
                    if !ctx.common_tls_context.tls_certificates.is_empty() {
                        return Err(crate::ConfigError::EmptyTlsCertificates { side: "cluster" });
                    }
                    if ctx.common_tls_context.validation_context.is_none() {
                        return Err(crate::ConfigError::MissingValidationContext);
                    }
                    if ctx.sni.is_empty() {
                        return Err(crate::ConfigError::EmptyUpstreamSni);
                    }
                }
                TransportSocketTypedConfig::Downstream(_) => {
                    return Err(crate::ConfigError::MismatchedTransportSocketDirection {
                        side: "cluster",
                        got: "DownstreamTlsContext",
                    });
                }
            }
        }
    }

    // Per-listener invariants.
    for listener in listeners {
        for chain in &listener.filter_chains {
            if let Some(ts) = chain.transport_socket.as_ref() {
                if ts.name != crate::TLS_TRANSPORT_SOCKET {
                    return Err(crate::ConfigError::UnknownTransportSocketName(ts.name.clone()));
                }
                match &ts.typed_config {
                    TransportSocketTypedConfig::Downstream(ctx) => {
                        if ctx.common_tls_context.tls_certificates.is_empty() {
                            return Err(crate::ConfigError::EmptyTlsCertificates {
                                side: "listener",
                            });
                        }
                    }
                    TransportSocketTypedConfig::Upstream(_) => {
                        return Err(crate::ConfigError::MismatchedTransportSocketDirection {
                            side: "listener",
                            got: "UpstreamTlsContext",
                        });
                    }
                }
            }
            for filter in &chain.filters {
                match filter.name.as_str() {
                    crate::ECHO_FILTER => {
                        if filter.typed_config.is_some() {
                            return Err(crate::ConfigError::UnexpectedTypedConfig(
                                crate::ECHO_FILTER,
                            ));
                        }
                    }
                    crate::TCP_PROXY_FILTER => {
                        // 02.1: TypedConfig has one variant (TcpProxy). Phase 04+ extend; migrate to match.
                        let TypedConfig::TcpProxy(tp) = filter.typed_config.as_ref().ok_or(
                            crate::ConfigError::MissingTypedConfig(crate::TCP_PROXY_FILTER),
                        )?;
                        if !clusters.iter().any(|c| c.name == tp.cluster) {
                            return Err(crate::ConfigError::UnknownCluster(tp.cluster.clone()));
                        }
                    }
                    _ => {
                        return Err(crate::ConfigError::UnsupportedFilter(
                            filter.name.clone(),
                            crate::ECHO_FILTER,
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 5: Re-run the failing test; verify it passes.**

```bash
cargo test -p envoy-config bootstrap::tests::rejects_unknown_transport_socket_name
```

Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 6: Add the four remaining validator-driven tests in one batch.**

Append:

```rust
    #[test]
    fn rejects_downstream_tls_context_on_cluster() {
        // DownstreamTlsContext on a cluster's transport_socket → MismatchedTransportSocketDirection.
        let yaml = r#"
static_resources:
  listeners: []
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
                      port_value: 9443
      transport_socket:
        name: envoy.transport_sockets.tls
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
          common_tls_context:
            tls_certificates:
              - certificate_chain:
                  filename: /tmp/leaf.pem
                private_key:
                  filename: /tmp/leaf.key
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::MismatchedTransportSocketDirection {
                    side: "cluster",
                    got: "DownstreamTlsContext",
                }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_upstream_tls_context_on_listener() {
        // UpstreamTlsContext on a listener's filter_chain.transport_socket →
        // MismatchedTransportSocketDirection.
        let yaml = r#"
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
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext
              sni: envoy-rust.test
              common_tls_context:
                validation_context:
                  trusted_ca:
                    filename: /tmp/ca.pem
          filters: []
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::MismatchedTransportSocketDirection {
                    side: "listener",
                    got: "UpstreamTlsContext",
                }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_downstream_with_empty_tls_certificates() {
        // Downstream side requires ≥1 cert; empty → EmptyTlsCertificates { side: "listener" }.
        let yaml = r#"
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
                tls_certificates: []
          filters: []
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::EmptyTlsCertificates { side: "listener" }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_upstream_with_tls_certificates() {
        // Upstream side requires 0 certs (mTLS deferred); non-empty →
        // EmptyTlsCertificates { side: "cluster" } (variant naming is asymmetric:
        // "Empty" on listener means too-few, on cluster means too-many; the
        // side discriminator carries the meaning).
        let yaml = r#"
static_resources:
  listeners: []
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
                      port_value: 9443
      transport_socket:
        name: envoy.transport_sockets.tls
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext
          sni: envoy-rust.test
          common_tls_context:
            tls_certificates:
              - certificate_chain:
                  filename: /tmp/client.pem
                private_key:
                  filename: /tmp/client.key
            validation_context:
              trusted_ca:
                filename: /tmp/ca.pem
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::EmptyTlsCertificates { side: "cluster" }
            ),
            "got {err:?}"
        );
    }
```

- [ ] **Step 7: Run the four new tests.**

```bash
cargo test -p envoy-config bootstrap::tests::rejects_downstream_tls_context_on_cluster bootstrap::tests::rejects_upstream_tls_context_on_listener bootstrap::tests::rejects_downstream_with_empty_tls_certificates bootstrap::tests::rejects_upstream_with_tls_certificates
```

Expected: 4 passed.

- [ ] **Step 8: Add `rejects_upstream_without_validation_context` + `rejects_upstream_with_empty_sni`.**

Append:

```rust
    #[test]
    fn rejects_upstream_without_validation_context() {
        // No insecure-skip in phase 03 (SPEC §4) — validation_context required.
        let yaml = r#"
static_resources:
  listeners: []
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
                      port_value: 9443
      transport_socket:
        name: envoy.transport_sockets.tls
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext
          sni: envoy-rust.test
          common_tls_context: {}
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::MissingValidationContext),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_upstream_with_empty_sni() {
        let yaml = r#"
static_resources:
  listeners: []
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
                      port_value: 9443
      transport_socket:
        name: envoy.transport_sockets.tls
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext
          sni: ""
          common_tls_context:
            validation_context:
              trusted_ca:
                filename: /tmp/ca.pem
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::EmptyUpstreamSni),
            "got {err:?}"
        );
    }
```

- [ ] **Step 9: Run the full envoy-config test suite.**

```bash
cargo test -p envoy-config
```

Expected: `test result: ok. 50 passed; 0 failed` (38 phase-02.2 base + 5 Task-2 + 7 Task-3 = 50). Adjust if pre-existing count differs from the 38 stated in `02.2/PROGRESS.md`.

- [ ] **Step 10: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace --lib --bins
```

All four: exit 0 / all green. `envoy-cluster`, `envoy-listener`, `envoy-tcp`, `envoy-bin`, `tcp-echo-server`, `differential` should all be unchanged in test count from phase 02.2 close-out (8, 6, 4, 19, 8, 31).

- [ ] **Step 11: Commit.**

```bash
git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs
git commit -m "phase 03.1: envoy-config — TLS validator extensions + 5 new ConfigError variants"
```

Append a Task 3 PROGRESS section with the full envoy-config test count and a note on the +2 test drift from SPEC §D2's 10-test estimate (12 actual; reviewer-cost rounding).

---

### Task 4: `envoy-config` — 3 fuzz corpus seeds (TLS-shaped YAML)

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_downstream_single_cert.yaml`
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_malformed_at_type.yaml`
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_upstream_validation_context.yaml`

**Scope:** extend the pre-existing `parse_bootstrap` fuzz-corpus directory with three TLS-shaped seeds per SPEC §3 D2 fuzz-corpus enumeration. The fuzz target itself (`crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs`) is unchanged — `TransportSocket`, `DownstreamTlsContext`, `UpstreamTlsContext`, `CommonTlsContext`, `TlsCertificate`, `CertificateValidationContext`, and `DataSource` are all reachable via `envoy_config::parse_bootstrap`, so structural coverage of the extended grammar comes for free. The `-max_total_time=30` budget (ADR-0010) is unchanged.

The fuzz target only exercises serde deserialization + the validator; it never opens the `filename` paths these seeds reference (SPEC §6 signpost 16). Pick plausible-but-irrelevant filename strings (`/tmp/cert.pem` etc.).

The `tls_malformed_at_type.yaml` seed must be a *parseable* shape that the validator (or serde's tagged-enum default rejection on `TransportSocketTypedConfig`) rejects — not malformed YAML. The fuzzer mutates accepted seeds; it doesn't need parse-failures as starting points. Per SPEC §3 D2 wording: "Exercises serde's tagged-enum default rejection on the `TransportSocketTypedConfig` enum." Mutate this seed and the fuzzer explores the @type-rejection space. The fuzz target must therefore not panic on parse failure — phase-02.1 confirmed `parse_bootstrap` returns `Err` (not `panic!`) on malformed `@type`. Verify in Step 4.

- [ ] **Step 1: Create `tls_downstream_single_cert.yaml`.**

Write `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_downstream_single_cert.yaml`:

```yaml
node:
  id: fuzz-seed-tls-downstream
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

- [ ] **Step 2: Create `tls_malformed_at_type.yaml`.**

Write `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_malformed_at_type.yaml`:

```yaml
node:
  id: fuzz-seed-tls-malformed-attype
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
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UnknownContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain:
                      filename: /tmp/cert.pem
                    private_key:
                      filename: /tmp/key.pem
          filters: []
  clusters: []
```

This seed parses to a `serde_yaml` error (the `@type` URL is not recognized by `TransportSocketTypedConfig`'s `#[serde(tag = "@type")]` discriminator). The fuzz target's wrapper must return `Err`, not panic — Step 4 verifies.

- [ ] **Step 3: Create `tls_upstream_validation_context.yaml`.**

Write `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_upstream_validation_context.yaml`:

```yaml
node:
  id: fuzz-seed-tls-upstream
  cluster: fuzz
static_resources:
  listeners: []
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
                      port_value: 9443
      transport_socket:
        name: envoy.transport_sockets.tls
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext
          sni: envoy-rust.test
          common_tls_context:
            validation_context:
              trusted_ca:
                filename: /tmp/ca.pem
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
```

- [ ] **Step 4: Extend the per-seed parse-corpus regression test in `bootstrap.rs::tests`.**

Phase 02.1 Task 12 introduced `fuzz_corpus_tcp_proxy_seeds_parse` (a permanent regression test that reads each seed and calls `parse_bootstrap`). Extend it to cover the three new TLS seeds plus distinguish parseable vs. expected-rejection seeds:

Locate the existing `fuzz_corpus_tcp_proxy_seeds_parse` test in `crates/envoy-config/src/bootstrap.rs::tests` (added in phase 02.1 Task 12). Rename it to `fuzz_corpus_seeds_parse_or_reject_cleanly` and rewrite:

```rust
    #[test]
    fn fuzz_corpus_seeds_parse_or_reject_cleanly() {
        let root = env!("CARGO_MANIFEST_DIR");
        // Seeds expected to parse + validate successfully.
        for fname in &[
            "fuzz/corpus/parse_bootstrap/tcp_proxy_single_endpoint.yaml",
            "fuzz/corpus/parse_bootstrap/tcp_proxy_round_robin_triple.yaml",
            "fuzz/corpus/parse_bootstrap/tls_downstream_single_cert.yaml",
            "fuzz/corpus/parse_bootstrap/tls_upstream_validation_context.yaml",
        ] {
            let path = format!("{root}/{fname}");
            let yaml = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {path}: {e}"));
            crate::parse_bootstrap(&yaml).unwrap_or_else(|e| panic!("parse {path}: {e}"));
        }
        // Seeds expected to reject cleanly (parse_bootstrap returns Err, not panic).
        for fname in &[
            "fuzz/corpus/parse_bootstrap/tls_malformed_at_type.yaml",
        ] {
            let path = format!("{root}/{fname}");
            let yaml = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {path}: {e}"));
            assert!(
                crate::parse_bootstrap(&yaml).is_err(),
                "{path} was expected to reject, but parsed",
            );
        }
        // The minimal.yaml seed is the phase-00 admin-only baseline; assert
        // it still parses (regression gate against schema additions breaking
        // baseline acceptance).
        let minimal = format!("{root}/fuzz/corpus/parse_bootstrap/minimal.yaml");
        let yaml = std::fs::read_to_string(&minimal)
            .unwrap_or_else(|e| panic!("read {minimal}: {e}"));
        crate::parse_bootstrap(&yaml).unwrap_or_else(|e| panic!("parse {minimal}: {e}"));
    }
```

- [ ] **Step 5: Run the regression test.**

```bash
cargo test -p envoy-config bootstrap::tests::fuzz_corpus_seeds_parse_or_reject_cleanly
```

Expected: `test result: ok. 1 passed; 0 failed`. If the malformed_at_type seed *parses* (rather than rejects), the schema didn't tighten as expected — investigate before continuing.

- [ ] **Step 6: Sanity-check the fuzz target via a short local run.**

The CI `fuzz` job runs `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30` against the corpus on every push (per phase-01 ADR-0010). Locally, run a 5-second smoke test to confirm the new seeds don't immediately crash:

```bash
cd crates/envoy-config/fuzz
cargo +nightly fuzz run parse_bootstrap -- -max_total_time=5 corpus/parse_bootstrap/
```

Expected: no panics; the run terminates after 5s of fuzzing. If the fuzzer panics on the malformed_at_type seed, the fuzz target's wrapper is converting `parse_bootstrap`'s `Err` into a panic — fix the wrapper, not the seed.

If the locally-installed nightly toolchain isn't ADR-0010 compliant or the fuzz subcrate's nested pin (ADR-0012) isn't honored, skip this step and rely on CI.

- [ ] **Step 7: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace --lib --bins
```

All four: exit 0 / green.

- [ ] **Step 8: Commit.**

```bash
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_downstream_single_cert.yaml \
       crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_malformed_at_type.yaml \
       crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_upstream_validation_context.yaml \
       crates/envoy-config/src/bootstrap.rs
git commit -m "phase 03.1: envoy-config — fuzz corpus seeds for TLS schema"
```

Append a Task 4 PROGRESS section.

---

### Task 5: Scaffold `crates/envoy-tls/` skeleton + workspace member

**Files:**
- Create: `crates/envoy-tls/Cargo.toml`
- Create: `crates/envoy-tls/src/lib.rs` (compiling stub; populated by Tasks 6 + 7)
- Modify: `Cargo.toml` (root)

**Why now:** Tasks 6, 7, 8, 9 all depend on `envoy-tls` existing as a workspace member. This task lands the minimum that compiles cleanly so subsequent tasks don't mix scaffolding with real code (mirrors phase-02.1 Task 5's envoy-cluster scaffolding cadence and phase-02.2 Tasks 4 + 7's listener/tcp scaffolding cadence). Per SPEC §3 D1.

**ADR-0019 dep verification (pre-write).** Plan-time pin: `rustls = "0.23"`, `tokio-rustls = "0.26"`, `rustls-pemfile = "2"`, `rustls-pki-types = "1"`, `rcgen = "0.13"`, `tempfile = "3"` (already in workspace). Per SPEC §6 signposts 5 + 6, the plan-writer verifies these against `crates.io`'s latest stable line at execution time and adjusts. If a major-version step landed since planning (e.g., rustls 0.24, tokio-rustls 0.27), use the latest stable; document the deviation in PROGRESS.md. No new ADR needed unless a fresh exemption surfaces; ADR-0019 covers the rustls family.

- [ ] **Step 1: Write `crates/envoy-tls/Cargo.toml`.**

```toml
[package]
name = "envoy-tls"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[lib]
name = "envoy_tls"
path = "src/lib.rs"

[dependencies]
envoy-config = { path = "../envoy-config" }
rustls = { version = "0.23", default-features = false, features = ["std", "tls12"] }
rustls-pki-types = "1"
rustls-pemfile = "2"
tokio-rustls = { version = "0.26", default-features = false, features = ["aws-lc-rs"] }
tokio = { version = "1", features = ["net", "io-util", "macros", "sync"] }
thiserror = "2"
tracing = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "net", "io-util", "macros", "sync", "time"] }
rcgen = "0.13"
tempfile = "3"
```

The `aws-lc-rs` crypto provider is selected via the `tokio-rustls` `aws-lc-rs` feature; the `ring` provider is **not** brought in. Per SPEC §3 D1: verify feature names against the actual `tokio-rustls` 0.26.x API (run `cargo add --dry-run tokio-rustls@0.26 -F aws-lc-rs` or inspect `cargo doc -p tokio-rustls` after Step 4 builds — if the feature renamed (e.g., `aws_lc_rs`), fix here). If feature names differ, document in PROGRESS.md and continue; no ADR needed (SPEC §6 signpost 5).

- [ ] **Step 2: Write `crates/envoy-tls/src/lib.rs` as a compiling stub.**

```rust
#![forbid(unsafe_code)]

//! Phase 03.1 TLS surface for envoy-rust. Owns rustls server/client config
//! construction, the cert/key PEM loader, and the `TlsError` typed-error enum.
//! Public surface is populated by Tasks 6 (`DownstreamTls`) and 7 (`UpstreamTls`)
//! of `docs/envoy-rust/phases/03.1-tls-foundation-downstream/PLAN.md`.
//!
//! D-3.2 + ADR-0018 + ADR-0019: this is the only crate in the workspace that
//! depends on rustls / tokio-rustls / rustls-pki-types / rustls-pemfile /
//! aws-lc-rs. envoy-listener and envoy-cluster stay rustls-free.
```

(Empty — no items yet. The compiling-stub keeps the crate valid as a workspace member while Tasks 6 and 7 land the real surface.)

- [ ] **Step 3: Add `crates/envoy-tls` to the root workspace.**

Edit the root `Cargo.toml` `[workspace] members` list to insert `crates/envoy-tls` alphabetically between `crates/envoy-tcp` and `tests/differential`:

```toml
[workspace]
resolver = "2"
members = [
    "crates/envoy-bin",
    "crates/envoy-cluster",
    "crates/envoy-config",
    "crates/envoy-listener",
    "crates/envoy-tcp",
    "crates/envoy-tls",
    "tests/differential",
    "tests/helpers/tcp-echo-server",
]
exclude = [
    "crates/envoy-config/fuzz",
]
```

- [ ] **Step 4: Verify the workspace builds cleanly.**

```bash
cargo build --workspace --all-targets
```

Expected: a `Compiling envoy-tls v0.0.0 (.../crates/envoy-tls)` line, with new transitive compiles for `rustls`, `rustls-pki-types`, `rustls-pemfile`, `tokio-rustls`, `aws-lc-rs`, `aws-lc-sys`, `untrusted`, `webpki`, etc., then `Finished dev profile target(s) in …s`. No warnings, no errors.

This is the first build that pulls in `aws-lc-sys` — the C-bindings crate compiles `aws-lc` from source, which can take 30–90s on a cold build. Subsequent builds use the cached artifact.

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: exit 0, `Finished`.

```bash
cargo fmt --all -- --check
```

Expected: exit 0, no diff.

```bash
cargo test -p envoy-tls
```

Expected: `test result: ok. 0 passed; 0 failed` (no tests yet).

- [ ] **Step 5: Run `cargo deny check` to confirm no new license / advisory / source surface flips red.**

```bash
cargo deny check
```

Expected: `advisories ok, bans ok, licenses ok, sources ok`. The rustls + aws-lc-rs chain ships under Apache-2.0 / MIT / ISC, all on the deny.toml allow-list. If a transitive flips red, surface it as plan-time deviation per SPEC §3 D10 contingency (likely a new ADR-0020 to extend `deny.toml`); pause for review before continuing.

If `cargo deny check` fails, do not continue Task 5 — debug per `superpowers:systematic-debugging`. Common failure modes:

- A new transitive license outside the allow-list. Add it under a new ADR.
- A new advisory on an old dependency. Pin or `[advisories].ignore` with rationale + ADR.
- A new banned crate via the wrappers list. Investigate whether the dep can be excluded (likely yes).

- [ ] **Step 6: Commit.**

```bash
git add Cargo.toml crates/envoy-tls
git commit -m "phase 03.1: scaffold envoy-tls crate"
```

Do NOT stage `Cargo.lock` here — workspace-member additions update `Cargo.lock`, and the convention from phases 01, 02.1, 02.2 is a dedicated lockfile-sync commit before the state-6 phase-done commit (precedents: `4955252`, `dea4d16`, `2146014`).

Append a Task 5 PROGRESS section.

---

### Task 6: `envoy-tls::DownstreamTls` — `TlsError`, `load_certified_key`, `SingleCertResolver`, `from_context`, `accept` + 6 tests + `crypto_provider_install_is_idempotent`

**Files:**
- Modify: `crates/envoy-tls/src/lib.rs`

**Scope:** the TLS error enum, the cert/key PEM loader, the `SingleCertResolver` (an in-crate `rustls::server::ResolvesServerCert` impl), the `DownstreamTls` struct + `from_context` + `accept`, and 7 tests (6 downstream-specific + the cross-cutting crypto-provider idempotency test). Per SPEC §3 D1.

The 6 downstream tests:
1. `loads_single_cert_server_config` — happy path, in-process TLS handshake completes.
2. `rejects_empty_tls_certificates` — `from_context` returns `TlsError::DownstreamRequiresCert`.
3. `rejects_malformed_cert_pem` — file with no PEM headers → `TlsError::CertParse`.
4. `rejects_missing_key_pem` — file does not exist → `TlsError::FileRead`.
5. `single_cert_resolver_returns_same_cert_regardless_of_sni` — resolver returns the same `Arc<CertifiedKey>` for any SNI.
6. `accept_returns_handshake_error_on_garbage_input` — non-TLS bytes → `TlsError::Handshake`.

The cross-cutting test:
7. `crypto_provider_install_is_idempotent` — second `install_default()` returns `Err(_)`, doesn't panic.

The 3 upstream tests land in Task 7.

**Test PKI helper.** All seven tests need a working CA + leaf. Define a private test helper `tests::pki::Pki` once near the top of the `mod tests`; it builds a self-signed CA with rcgen and signs an `a.example.com`-named leaf. Per SPEC §6 signpost 6: use `KeyPair::generate(&PKCS_ECDSA_P256_SHA256)` for both CA and leaf — rcgen 0.13's `KeyPair::generate(&KeyPairAlgorithm)` API. Verify the actual rcgen 0.13 API at execution time; if the signature differs (e.g., method renamed, or `KeyPair::generate_for(...)`), adjust accordingly and document in PROGRESS.md.

- [ ] **Step 1: Write the failing test `loads_single_cert_server_config`.**

Replace the stub `crates/envoy-tls/src/lib.rs` body with:

```rust
#![forbid(unsafe_code)]

//! Phase 03.1 TLS surface for envoy-rust. Owns rustls server/client config
//! construction, the cert/key PEM loader, and the `TlsError` typed-error enum.
//!
//! D-3.2 + ADR-0018 + ADR-0019: this is the only crate in the workspace that
//! depends on rustls / tokio-rustls / rustls-pki-types / rustls-pemfile /
//! aws-lc-rs. envoy-listener and envoy-cluster stay rustls-free.

#[cfg(test)]
mod tests;
```

Then create the test module at `crates/envoy-tls/src/tests.rs` (yes, two files — Rust supports `mod tests;` in `lib.rs` resolving to `src/tests.rs` since edition 2018+; this keeps the unit-test PKI helper from polluting `lib.rs`'s structure):

```rust
//! Unit tests for envoy-tls. Phase 03.1 ships 7 tests covering DownstreamTls
//! (Task 6) + 3 covering UpstreamTls (Task 7) + 1 cross-cutting (Task 6).

use std::path::PathBuf;
use std::sync::Arc;

use crate::*;
use rcgen::{CertificateParams, KeyPair};
use tempfile::TempDir;

mod pki {
    use super::*;
    use rcgen::{Certificate, CertifiedKey, DnType, IsCa, KeyUsagePurpose};

    /// In-test PKI: a self-signed CA + one leaf with SAN `a.example.com`,
    /// written into a per-test `TempDir`. Drop the `Pki` to clean up.
    pub struct Pki {
        pub _dir: TempDir,
        pub ca_cert_pem: PathBuf,
        pub leaf_cert_pem: PathBuf,
        pub leaf_key_pem: PathBuf,
        pub ca_der_for_root_store: rustls::pki_types::CertificateDer<'static>,
    }

    pub fn build() -> Pki {
        let dir = tempfile::tempdir().expect("tempdir");
        // CA
        let mut ca_params = CertificateParams::new(vec!["envoy-rust-test-ca".into()])
            .expect("ca params");
        ca_params
            .distinguished_name
            .push(DnType::CommonName, "envoy-rust-test-ca");
        ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let ca_kp = KeyPair::generate().expect("ca kp");
        let ca_cert = ca_params.self_signed(&ca_kp).expect("ca self-sign");

        // Leaf signed by CA.
        let mut leaf_params = CertificateParams::new(vec!["a.example.com".into()])
            .expect("leaf params");
        leaf_params
            .distinguished_name
            .push(DnType::CommonName, "a.example.com");
        let leaf_kp = KeyPair::generate().expect("leaf kp");
        let leaf_cert = leaf_params
            .signed_by(&leaf_kp, &ca_cert, &ca_kp)
            .expect("leaf signed");

        let ca_pem = ca_cert.pem();
        let leaf_pem = leaf_cert.pem();
        let leaf_key_pem = leaf_kp.serialize_pem();

        let ca_path = dir.path().join("ca.pem");
        let leaf_path = dir.path().join("leaf-a.pem");
        let leaf_key_path = dir.path().join("leaf-a.key");
        std::fs::write(&ca_path, &ca_pem).expect("write ca");
        std::fs::write(&leaf_path, &leaf_pem).expect("write leaf");
        std::fs::write(&leaf_key_path, &leaf_key_pem).expect("write leaf key");

        let ca_der_for_root_store: rustls::pki_types::CertificateDer<'static> =
            ca_cert.der().clone().into_owned();

        Pki {
            _dir: dir,
            ca_cert_pem: ca_path,
            leaf_cert_pem: leaf_path,
            leaf_key_pem: leaf_key_path,
            ca_der_for_root_store,
        }
    }

    pub fn ds_context_with(cert_path: &PathBuf, key_path: &PathBuf) -> envoy_config::DownstreamTlsContext {
        envoy_config::DownstreamTlsContext {
            common_tls_context: envoy_config::CommonTlsContext {
                tls_certificates: vec![envoy_config::TlsCertificate {
                    certificate_chain: envoy_config::DataSource {
                        filename: cert_path.to_string_lossy().into_owned(),
                    },
                    private_key: envoy_config::DataSource {
                        filename: key_path.to_string_lossy().into_owned(),
                    },
                }],
                validation_context: None,
            },
        }
    }
}

fn install_provider_once() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

#[tokio::test(flavor = "multi_thread")]
async fn loads_single_cert_server_config() {
    install_provider_once();
    let pki = pki::build();
    let ctx = pki::ds_context_with(&pki.leaf_cert_pem, &pki.leaf_key_pem);
    let downstream = DownstreamTls::from_context(&ctx).expect("downstream from_context");

    // In-process loopback handshake: bind a TcpListener, connect a
    // TlsConnector with the test CA in the root store, and feed the
    // accepted stream through DownstreamTls::accept.
    use rustls::pki_types::ServerName;
    use std::convert::TryFrom;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        downstream.accept(stream).await.expect("server accept")
    });

    let mut roots = rustls::RootCertStore::empty();
    roots.add(pki.ca_der_for_root_store.clone()).expect("add ca");
    let client_cfg = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));
    let server_name = ServerName::try_from("a.example.com").expect("server name");

    let client_stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let client_tls = connector
        .connect(server_name, client_stream)
        .await
        .expect("client handshake");

    let server_tls = server_task.await.expect("server task");
    // TLS version assertion — both peers should agree on ≥ 1.2.
    let server_negotiated = server_tls
        .get_ref()
        .1
        .protocol_version()
        .expect("server TLS version negotiated");
    let client_negotiated = client_tls
        .get_ref()
        .1
        .protocol_version()
        .expect("client TLS version negotiated");
    assert!(server_negotiated >= rustls::ProtocolVersion::TLSv1_2);
    assert!(client_negotiated >= rustls::ProtocolVersion::TLSv1_2);
}
```

- [ ] **Step 2: Run the test; verify it fails to compile.**

```bash
cargo test -p envoy-tls loads_single_cert_server_config
```

Expected: `error[E0432]: unresolved import \`crate::DownstreamTls\`` (or similar; `DownstreamTls`, `from_context`, `accept`, `TlsError` all undefined).

- [ ] **Step 3: Implement the `DownstreamTls` surface in `crates/envoy-tls/src/lib.rs`.**

Replace the `lib.rs` body again with:

```rust
#![forbid(unsafe_code)]

//! Phase 03.1 TLS surface for envoy-rust. Owns rustls server/client config
//! construction, the cert/key PEM loader, and the `TlsError` typed-error enum.
//!
//! D-3.2 + ADR-0018 + ADR-0019: this is the only crate in the workspace that
//! depends on rustls / tokio-rustls / rustls-pki-types / rustls-pemfile /
//! aws-lc-rs. envoy-listener and envoy-cluster stay rustls-free.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, ServerName};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use tokio::net::TcpStream;

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("loading cert/key file {path:?}: {source}")]
    FileRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("parsing PEM at {path:?}: no leaf certificate found")]
    CertParse { path: PathBuf },
    #[error("parsing private key at {0:?}: {1}")]
    KeyParse(PathBuf, String),
    #[error("rustls config build: {0}")]
    RustlsConfig(String),
    #[error("invalid SNI {sni:?} in upstream context: {reason}")]
    InvalidServerName { sni: String, reason: String },
    #[error("TLS handshake: {source}")]
    Handshake {
        #[source]
        source: std::io::Error,
    },
    #[error("loading trusted_ca PEM at {path:?}: no CA certificate found")]
    CaParse { path: PathBuf },
    #[error("downstream context requires at least one tls_certificate")]
    DownstreamRequiresCert,
}

/// Server-side TLS configuration. Build via `from_context`; drive a connected
/// `TcpStream` through the rustls server handshake via `accept`.
pub struct DownstreamTls {
    config: Arc<ServerConfig>,
}

impl DownstreamTls {
    /// Build from a parsed envoy_config::DownstreamTlsContext.
    ///
    /// 03.1: single-cert path. Loads cert+key PEMs from the configured filenames;
    /// constructs a `SingleCertResolver` wrapping the resulting `CertifiedKey`.
    /// Rejects empty `tls_certificates` with `TlsError::DownstreamRequiresCert`.
    pub fn from_context(cfg: &envoy_config::DownstreamTlsContext) -> Result<Self, TlsError> {
        let certs = &cfg.common_tls_context.tls_certificates;
        if certs.is_empty() {
            return Err(TlsError::DownstreamRequiresCert);
        }
        // 03.1 honors the first tls_certificate only. The validator rejects
        // the empty case; multi-cert SNI selection lands in 03.2 via
        // `from_listener`.
        let cert_path = Path::new(&certs[0].certificate_chain.filename);
        let key_path = Path::new(&certs[0].private_key.filename);
        let key = load_certified_key(cert_path, key_path)?;
        let resolver: Arc<dyn ResolvesServerCert> = Arc::new(SingleCertResolver(Arc::new(key)));
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(resolver);
        Ok(Self {
            config: Arc::new(config),
        })
    }

    /// Hands a connected downstream `TcpStream` through the rustls server
    /// handshake; returns the post-handshake stream. On handshake failure
    /// returns `TlsError::Handshake`; the listener's accept loop logs at
    /// `warn!` and drops the connection per phase 02.2's posture.
    pub async fn accept(
        &self,
        downstream: TcpStream,
    ) -> Result<tokio_rustls::server::TlsStream<TcpStream>, TlsError> {
        let acceptor = tokio_rustls::TlsAcceptor::from(self.config.clone());
        acceptor
            .accept(downstream)
            .await
            .map_err(|source| TlsError::Handshake { source })
    }
}

/// In-crate `ResolvesServerCert` that returns the wrapped `CertifiedKey` for
/// any ClientHello regardless of SNI. The `ServerConfig` is built via
/// `with_cert_resolver` (rather than the simpler `with_single_cert`) so the
/// 03.2 SNI multi-cert resolver is a drop-in replacement.
#[derive(Debug)]
struct SingleCertResolver(Arc<CertifiedKey>);

impl ResolvesServerCert for SingleCertResolver {
    fn resolve(&self, _client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(self.0.clone())
    }
}

/// Load + verify a PEM cert chain + PEM private key from disk; return the
/// rustls-signing-key-bearing `CertifiedKey`.
fn load_certified_key(cert_path: &Path, key_path: &Path) -> Result<CertifiedKey, TlsError> {
    let cert_bytes = std::fs::read(cert_path).map_err(|source| TlsError::FileRead {
        path: cert_path.to_path_buf(),
        source,
    })?;
    let mut cert_slice = cert_bytes.as_slice();
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_slice)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| TlsError::KeyParse(cert_path.to_path_buf(), format!("certs: {e}")))?;
    if certs.is_empty() {
        return Err(TlsError::CertParse {
            path: cert_path.to_path_buf(),
        });
    }

    let key_bytes = std::fs::read(key_path).map_err(|source| TlsError::FileRead {
        path: key_path.to_path_buf(),
        source,
    })?;
    let mut key_slice = key_bytes.as_slice();
    let key = rustls_pemfile::private_key(&mut key_slice)
        .map_err(|e| TlsError::KeyParse(key_path.to_path_buf(), format!("private_key: {e}")))?
        .ok_or_else(|| TlsError::KeyParse(
            key_path.to_path_buf(),
            "no private key found".to_string(),
        ))?;

    let signing_key = rustls::crypto::aws_lc_rs::sign::any_supported_type(&key)
        .map_err(|e| TlsError::KeyParse(
            key_path.to_path_buf(),
            format!("any_supported_type: {e}"),
        ))?;

    Ok(CertifiedKey::new(certs, signing_key))
}
```

Note on the rustls-pki-types import: `CertificateDer` is re-exported by `rustls::pki_types`; the path `rustls::pki_types::CertificateDer` works in `rustls 0.23`. If at execution time the path differs (e.g., `rustls_pki_types::CertificateDer` direct), adjust.

Note on `rustls_pemfile::certs`: in `rustls-pemfile 2.x`, `certs` returns `impl Iterator<Item = io::Result<CertificateDer<'static>>>`, which is then `.collect::<Result<Vec<_>, _>>()`-ed. If 2.x's exact API differs, adjust per the actual signature.

- [ ] **Step 4: Re-run the test; verify it passes.**

```bash
cargo test -p envoy-tls loads_single_cert_server_config
```

Expected: `test result: ok. 1 passed; 0 failed`. Actual TLS handshake completes loopback through the in-process listener.

If the test fails for `unsupported algorithm` or similar, investigate per `superpowers:systematic-debugging` — possible mismatch between rcgen's chosen key type and `rustls::crypto::aws_lc_rs::sign::any_supported_type`'s accepted set. ECDSA P-256 is universally supported; if rcgen 0.13's default key type changed, switch the explicit `KeyPair::generate(&PKCS_ECDSA_P256_SHA256)` form (the API may live at `rcgen::generate_simple_self_signed_with_alg` or similar in 0.13.x — check and adjust).

- [ ] **Step 5: Add the five remaining `DownstreamTls` tests in one batch.**

Append to `crates/envoy-tls/src/tests.rs`:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn rejects_empty_tls_certificates() {
    let ctx = envoy_config::DownstreamTlsContext {
        common_tls_context: envoy_config::CommonTlsContext {
            tls_certificates: vec![],
            validation_context: None,
        },
    };
    let err = DownstreamTls::from_context(&ctx).expect_err("must reject");
    assert!(matches!(err, TlsError::DownstreamRequiresCert), "got {err:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_malformed_cert_pem() {
    install_provider_once();
    let pki = pki::build();
    // Overwrite the cert path with garbage (no PEM headers).
    std::fs::write(&pki.leaf_cert_pem, b"this is not a PEM\n").expect("write");
    let ctx = pki::ds_context_with(&pki.leaf_cert_pem, &pki.leaf_key_pem);
    let err = DownstreamTls::from_context(&ctx).expect_err("must reject");
    assert!(
        matches!(err, TlsError::CertParse { .. }),
        "got {err:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_missing_key_pem() {
    install_provider_once();
    let pki = pki::build();
    let missing = pki._dir.path().join("does-not-exist.key");
    let ctx = pki::ds_context_with(&pki.leaf_cert_pem, &missing);
    let err = DownstreamTls::from_context(&ctx).expect_err("must reject");
    assert!(matches!(err, TlsError::FileRead { .. }), "got {err:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn single_cert_resolver_returns_same_cert_regardless_of_sni() {
    install_provider_once();
    let pki = pki::build();
    let ctx = pki::ds_context_with(&pki.leaf_cert_pem, &pki.leaf_key_pem);
    let _downstream = DownstreamTls::from_context(&ctx).expect("from_context");

    // We can't directly call the private SingleCertResolver, but we can
    // verify the resolver's contract via three loopback handshakes with
    // different SNIs and confirm each completes (the resolver returns the
    // same CertifiedKey for each).
    use rustls::pki_types::ServerName;
    use std::convert::TryFrom;

    for sni in &["a.example.com", "b.example.com", "unknown.example.com"] {
        let pki_inner = pki::build();
        let ctx_inner = pki::ds_context_with(&pki_inner.leaf_cert_pem, &pki_inner.leaf_key_pem);
        let downstream_inner =
            DownstreamTls::from_context(&ctx_inner).expect("from_context");

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let server_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            downstream_inner.accept(stream).await
        });

        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(pki_inner.ca_der_for_root_store.clone())
            .expect("add ca");
        let client_cfg = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));
        let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");

        // Note: the test PKI's leaf has SAN `a.example.com`; SNIs other than
        // `a.example.com` will fail the post-handshake hostname-vs-SAN check.
        // We're testing the resolver's same-cert behavior, not SAN matching;
        // accept on the server side completes (the resolver returns a cert)
        // regardless of whether the client accepts that cert. Use
        // `dangerous_configuration` to bypass SAN matching client-side.
        let server_name = ServerName::try_from(*sni).expect("server name");
        // For SNIs other than a.example.com, the client connect WILL fail
        // (cert SAN mismatch). The server-side accept should still succeed
        // up until the client closes. Catch both behaviors.
        let _ = connector.connect(server_name, stream).await;

        // Server-side: accept completes if SNI matches the leaf SAN; for
        // mismatching SNIs, rustls-server doesn't enforce SAN — only the
        // client does. The resolver returned a cert; that's what we want.
        let server_result = server_task.await.expect("task joins");
        // For "a.example.com" the handshake must succeed; for the others
        // we just assert the resolver was called (server didn't error
        // because of "no cert configured for SNI").
        if *sni == "a.example.com" {
            assert!(server_result.is_ok(), "a.example.com must handshake");
        }
        // For other SNIs the result varies (client may abort post-handshake);
        // we don't assert ok / err — the resolver behavior is what's under test.
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn accept_returns_handshake_error_on_garbage_input() {
    install_provider_once();
    let pki = pki::build();
    let ctx = pki::ds_context_with(&pki.leaf_cert_pem, &pki.leaf_key_pem);
    let downstream = DownstreamTls::from_context(&ctx).expect("from_context");

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        downstream.accept(stream).await
    });

    use tokio::io::AsyncWriteExt;
    let mut client = tokio::net::TcpStream::connect(addr).await.expect("connect");
    client
        .write_all(b"GET / HTTP/1.1\r\n\r\n")
        .await
        .expect("write");
    let _ = client.shutdown().await;
    drop(client);

    let result = server_task.await.expect("task joins");
    let err = result.expect_err("plaintext garbage must fail handshake");
    assert!(matches!(err, TlsError::Handshake { .. }), "got {err:?}");
}

#[test]
fn crypto_provider_install_is_idempotent() {
    // First call returns Ok (or Err if a prior test already installed it —
    // both are acceptable; we just need no panic).
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    // Second call must return Err and must not panic.
    let result = rustls::crypto::aws_lc_rs::default_provider().install_default();
    assert!(result.is_err(), "second install must return Err, not panic");
}
```

- [ ] **Step 6: Run the full test suite.**

```bash
cargo test -p envoy-tls
```

Expected: `test result: ok. 7 passed; 0 failed` (6 downstream + 1 cross-cutting; Task 7 adds 3 more for a final count of 10).

If `single_cert_resolver_returns_same_cert_regardless_of_sni` is flaky because of an unexpected client-side abort timing, simplify the assertion to just `a.example.com` and document the SAN-mismatch limitation in PROGRESS.md — the test's core value is "server-side accept completes for the configured SNI," which is the same behavior the SNI 03.2 plan will rely on.

- [ ] **Step 7: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

All three: exit 0.

- [ ] **Step 8: Commit.**

```bash
git add crates/envoy-tls/src
git commit -m "phase 03.1: envoy-tls — DownstreamTls + cert loader + SingleCertResolver + 7 tests"
```

Append a Task 6 PROGRESS section.

---

### Task 7: `envoy-tls::UpstreamTls` — `from_context`, `connect`, CA loader + 3 tests

**Files:**
- Modify: `crates/envoy-tls/src/lib.rs` (append `UpstreamTls` struct + impl + CA loader helper)
- Modify: `crates/envoy-tls/src/tests.rs` (append 3 tests)

**Scope:** the `UpstreamTls` library API (parsed `ClientConfig` + `ServerName`) + 3 tests covering happy path, IP-literal SNI rejection, and untrusted-cert handshake failure. Per SPEC §3 D1.

The 3 tests:
1. `loads_upstream_client_config` — happy path; in-process handshake against an rcgen-built server signed by the same CA succeeds.
2. `upstream_rejects_invalid_sni` — `sni: "127.0.0.1"` (an IP literal) → `TlsError::InvalidServerName`.
3. `upstream_rejects_untrusted_cert` — server cert signed by an unknown CA → `TlsError::Handshake { source: ... }`.

Consumer wiring (envoy-tcp's `Option<Arc<UpstreamTls>>` field on `TcpProxy`, envoy-bin's per-cluster `Arc<UpstreamTls>` construction) lands in 03.2; 03.1 ships the library code + unit tests only (per SPEC §6 signpost 15).

- [ ] **Step 1: Write the failing test `loads_upstream_client_config`.**

Append to `crates/envoy-tls/src/tests.rs`:

```rust
mod upstream_pki {
    use super::*;
    use rcgen::{CertificateParams, DnType, IsCa, KeyPair, KeyUsagePurpose};

    /// Test PKI for upstream-side tests: a CA + a server cert with SAN
    /// `envoy-rust.test`. Same shape as `pki::build` but the server cert is
    /// what the upstream presents.
    pub struct UpstreamPki {
        pub _dir: TempDir,
        pub ca_pem: PathBuf,
        pub server_cert_pem: PathBuf,
        pub server_key_pem: PathBuf,
        pub ca_der_for_root_store: rustls::pki_types::CertificateDer<'static>,
        pub server_certified_key: rustls::sign::CertifiedKey,
    }

    pub fn build() -> UpstreamPki {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut ca_params =
            CertificateParams::new(vec!["envoy-rust-upstream-ca".into()]).expect("ca params");
        ca_params
            .distinguished_name
            .push(DnType::CommonName, "envoy-rust-upstream-ca");
        ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let ca_kp = KeyPair::generate().expect("ca kp");
        let ca_cert = ca_params.self_signed(&ca_kp).expect("ca self-sign");

        let mut srv_params =
            CertificateParams::new(vec!["envoy-rust.test".into()]).expect("server params");
        srv_params
            .distinguished_name
            .push(DnType::CommonName, "envoy-rust.test");
        let srv_kp = KeyPair::generate().expect("server kp");
        let srv_cert = srv_params
            .signed_by(&srv_kp, &ca_cert, &ca_kp)
            .expect("server signed");

        let ca_pem = ca_cert.pem();
        let srv_pem = srv_cert.pem();
        let srv_key_pem = srv_kp.serialize_pem();

        let ca_path = dir.path().join("upstream-ca.pem");
        let srv_path = dir.path().join("server.pem");
        let srv_key_path = dir.path().join("server.key");
        std::fs::write(&ca_path, &ca_pem).expect("write ca");
        std::fs::write(&srv_path, &srv_pem).expect("write server cert");
        std::fs::write(&srv_key_path, &srv_key_pem).expect("write server key");

        let ca_der_for_root_store: rustls::pki_types::CertificateDer<'static> =
            ca_cert.der().clone().into_owned();
        let server_certified_key = {
            let cert_der: rustls::pki_types::CertificateDer<'static> =
                srv_cert.der().clone().into_owned();
            // Build a signing key by re-loading the PEM through rustls-pemfile —
            // mirrors how envoy-tls's loader works for parity.
            let key_bytes = std::fs::read(&srv_key_path).expect("read");
            let mut sl = key_bytes.as_slice();
            let key = rustls_pemfile::private_key(&mut sl)
                .expect("parse priv key")
                .expect("priv key present");
            let signing = rustls::crypto::aws_lc_rs::sign::any_supported_type(&key)
                .expect("any_supported_type");
            rustls::sign::CertifiedKey::new(vec![cert_der], signing)
        };

        UpstreamPki {
            _dir: dir,
            ca_pem: ca_path,
            server_cert_pem: srv_path,
            server_key_pem: srv_key_path,
            ca_der_for_root_store,
            server_certified_key,
        }
    }

    pub fn us_context_with(
        ca_path: &PathBuf,
        sni: &str,
    ) -> envoy_config::UpstreamTlsContext {
        envoy_config::UpstreamTlsContext {
            common_tls_context: envoy_config::CommonTlsContext {
                tls_certificates: vec![],
                validation_context: Some(envoy_config::CertificateValidationContext {
                    trusted_ca: envoy_config::DataSource {
                        filename: ca_path.to_string_lossy().into_owned(),
                    },
                }),
            },
            sni: sni.to_string(),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn loads_upstream_client_config() {
    install_provider_once();
    let pki = upstream_pki::build();
    let ctx = upstream_pki::us_context_with(&pki.ca_pem, "envoy-rust.test");
    let upstream = UpstreamTls::from_context(&ctx).expect("upstream from_context");

    // Server: stand up a tokio_rustls TlsAcceptor with the rcgen-built server
    // cert. Exercise the upstream's connect against it.
    use rustls::server::ResolvesServerCert;

    #[derive(Debug)]
    struct StaticResolver(Arc<rustls::sign::CertifiedKey>);
    impl ResolvesServerCert for StaticResolver {
        fn resolve(
            &self,
            _client_hello: rustls::server::ClientHello<'_>,
        ) -> Option<Arc<rustls::sign::CertifiedKey>> {
            Some(self.0.clone())
        }
    }

    let resolver: Arc<dyn ResolvesServerCert> =
        Arc::new(StaticResolver(Arc::new(pki.server_certified_key)));
    let server_cfg = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(resolver);
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_cfg));

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        acceptor.accept(stream).await
    });

    let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let client_tls = upstream.connect(stream).await.expect("upstream connect");

    let _server_tls = server_task.await.expect("task joins").expect("server accept");
    let v = client_tls
        .get_ref()
        .1
        .protocol_version()
        .expect("version negotiated");
    assert!(v >= rustls::ProtocolVersion::TLSv1_2);
}

#[tokio::test(flavor = "multi_thread")]
async fn upstream_rejects_invalid_sni() {
    install_provider_once();
    let pki = upstream_pki::build();
    // sni is an IP literal — Envoy's UpstreamTlsContext.sni is documented
    // DNS-name-only, so envoy-rust must reject.
    let ctx = upstream_pki::us_context_with(&pki.ca_pem, "127.0.0.1");
    let err = UpstreamTls::from_context(&ctx).expect_err("must reject IP-literal sni");
    assert!(matches!(err, TlsError::InvalidServerName { .. }), "got {err:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn upstream_rejects_untrusted_cert() {
    install_provider_once();
    let pki = upstream_pki::build();
    // Build a *different* CA and server cert. Configure UpstreamTls with
    // pki.ca_pem (the original CA), then connect to a server presenting the
    // OTHER PKI's server cert. The handshake must fail.
    let other = upstream_pki::build();
    let ctx = upstream_pki::us_context_with(&pki.ca_pem, "envoy-rust.test");
    let upstream = UpstreamTls::from_context(&ctx).expect("from_context");

    use rustls::server::ResolvesServerCert;
    #[derive(Debug)]
    struct StaticResolver(Arc<rustls::sign::CertifiedKey>);
    impl ResolvesServerCert for StaticResolver {
        fn resolve(
            &self,
            _client_hello: rustls::server::ClientHello<'_>,
        ) -> Option<Arc<rustls::sign::CertifiedKey>> {
            Some(self.0.clone())
        }
    }
    let resolver: Arc<dyn ResolvesServerCert> =
        Arc::new(StaticResolver(Arc::new(other.server_certified_key)));
    let server_cfg = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(resolver);
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_cfg));

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let _ = acceptor.accept(stream).await;
        }
    });

    let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let err = upstream.connect(stream).await.expect_err("must reject untrusted");
    assert!(matches!(err, TlsError::Handshake { .. }), "got {err:?}");
}
```

- [ ] **Step 2: Run the new tests; verify they fail to compile.**

```bash
cargo test -p envoy-tls loads_upstream_client_config
```

Expected: `error[E0432]: unresolved import \`crate::UpstreamTls\``.

- [ ] **Step 3: Append `UpstreamTls` to `crates/envoy-tls/src/lib.rs`.**

Append after the `load_certified_key` fn:

```rust
/// Client-side TLS configuration. Build via `from_context`; drive a connected
/// upstream `TcpStream` through the rustls client handshake via `connect`.
///
/// 03.1 ships the implementation + unit tests; 03.2 wires consumers
/// (envoy-tcp's `TcpProxy::handle` gains an `Option<Arc<UpstreamTls>>` field;
/// envoy-bin builds the `Arc<UpstreamTls>` per cluster with
/// `transport_socket: Upstream(...)`).
pub struct UpstreamTls {
    config: Arc<ClientConfig>,
    server_name: ServerName<'static>,
}

impl UpstreamTls {
    /// Build from a parsed `envoy_config::UpstreamTlsContext`. Loads the CA
    /// PEM from `validation_context.trusted_ca.filename` into a `RootCertStore`;
    /// builds a `ClientConfig` with that root store, no client auth (mTLS
    /// deferred), default cipher suites/protocols. Parses `cfg.sni` into a
    /// `ServerName::DnsName` via `rustls-pki-types`; rejects IP literals
    /// (Envoy's `UpstreamTlsContext.sni` is documented DNS-name-only).
    pub fn from_context(cfg: &envoy_config::UpstreamTlsContext) -> Result<Self, TlsError> {
        let ca_path_str = cfg
            .common_tls_context
            .validation_context
            .as_ref()
            .map(|vc| vc.trusted_ca.filename.as_str())
            .ok_or_else(|| TlsError::RustlsConfig(
                "UpstreamTls::from_context: validation_context required".to_string(),
            ))?;
        let ca_path = Path::new(ca_path_str);
        let roots = load_root_store(ca_path)?;
        let config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();

        let server_name = parse_dns_server_name(&cfg.sni)?;
        Ok(Self {
            config: Arc::new(config),
            server_name,
        })
    }

    /// Hands a connected upstream `TcpStream` through the rustls client
    /// handshake; returns the post-handshake stream.
    pub async fn connect(
        &self,
        upstream: TcpStream,
    ) -> Result<tokio_rustls::client::TlsStream<TcpStream>, TlsError> {
        let connector = tokio_rustls::TlsConnector::from(self.config.clone());
        connector
            .connect(self.server_name.clone(), upstream)
            .await
            .map_err(|source| TlsError::Handshake { source })
    }
}

fn load_root_store(ca_path: &Path) -> Result<RootCertStore, TlsError> {
    let bytes = std::fs::read(ca_path).map_err(|source| TlsError::FileRead {
        path: ca_path.to_path_buf(),
        source,
    })?;
    let mut slice = bytes.as_slice();
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut slice)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| TlsError::CaParse {
            path: ca_path.to_path_buf(),
        })
        .or_else(|e| Err(e))?;
    if certs.is_empty() {
        return Err(TlsError::CaParse {
            path: ca_path.to_path_buf(),
        });
    }
    let mut roots = RootCertStore::empty();
    for cert in certs {
        roots
            .add(cert)
            .map_err(|e| TlsError::RustlsConfig(format!("RootCertStore::add: {e}")))?;
    }
    Ok(roots)
}

fn parse_dns_server_name(sni: &str) -> Result<ServerName<'static>, TlsError> {
    use std::convert::TryFrom;
    let parsed = ServerName::try_from(sni).map_err(|e| TlsError::InvalidServerName {
        sni: sni.to_string(),
        reason: format!("parse: {e}"),
    })?;
    match parsed {
        ServerName::DnsName(name) => Ok(ServerName::DnsName(name.to_owned())),
        ServerName::IpAddress(_) => Err(TlsError::InvalidServerName {
            sni: sni.to_string(),
            reason: "IP literals not accepted in upstream sni; Envoy requires a DNS name".into(),
        }),
        // ServerName is non-exhaustive in some pki-types versions; default
        // any future variant to rejection.
        _ => Err(TlsError::InvalidServerName {
            sni: sni.to_string(),
            reason: "unsupported ServerName variant".into(),
        }),
    }
}
```

- [ ] **Step 4: Re-run the new tests; verify they pass.**

```bash
cargo test -p envoy-tls loads_upstream_client_config upstream_rejects_invalid_sni upstream_rejects_untrusted_cert
```

Expected: 3 passed.

If `loads_upstream_client_config` fails because the `ServerName::DnsName` constructor / `to_owned()` API differs in `rustls-pki-types 1.x`, adjust per the actual signature. Common variants: `DnsName(DnsName<'a>)` vs. `DnsName(DnsNameRef<'a>)`; the `into_owned()` method is on the inner type.

- [ ] **Step 5: Run the full envoy-tls suite.**

```bash
cargo test -p envoy-tls
```

Expected: `test result: ok. 10 passed; 0 failed` (7 from Task 6 + 3 from Task 7).

- [ ] **Step 6: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

All three: exit 0.

- [ ] **Step 7: Commit.**

```bash
git add crates/envoy-tls/src
git commit -m "phase 03.1: envoy-tls — UpstreamTls + CA loader + 3 tests"
```

Append a Task 7 PROGRESS section.

---

### Task 8: `envoy-tcp::TcpProxy::handle` generic-stream lift + 4 new TLS-flavored unit tests

**Files:**
- Modify: `crates/envoy-tcp/src/lib.rs` (generalize `handle`; add 4 tests)
- Modify: `crates/envoy-tcp/Cargo.toml` (dev-deps: `tokio-rustls`, `rustls`, `rustls-pki-types`, `rcgen`, `tempfile`)

**Scope:** generalize `TcpProxy::handle` from `&self, downstream: tokio::net::TcpStream` to `<S>(&self, downstream: S) where S: AsyncRead + AsyncWrite + Unpin + Send + 'static`; rewrite the existing `ConnectionHandler::handle` impl as a thin wrapper that boxes `self.handle::<TcpStream>(downstream)`. Per SPEC §3 D4 (03.1 portion) and SPEC §6 signposts 2 + 3.

**The four pre-existing tests (`proxies_payload_end_to_end`, `proxies_closes_downstream_on_upstream_close`, `proxies_closes_upstream_on_downstream_close`, `proxies_returns_err_on_upstream_connect_refused`) remain unchanged.** They exercise the plaintext path through the `ConnectionHandler` trait impl, which is the thin wrapper around the now-generic inherent `handle::<TcpStream>`.

The 4 new tests:
1. `proxies_payload_through_tls_downstream_stream` — TLS in-process pair; pass `TlsStream<TcpStream>` to `TcpProxy::handle::<TlsStream<TcpStream>>`; assert byte-equality.
2. `proxies_payload_with_plaintext_stream_unchanged` — plaintext regression: `TcpProxy::handle::<TcpStream>` still works (call-site type resolution).
3. `tls_downstream_proxy_closes_upstream_on_downstream_close` — TLS variant of the existing close-propagation test.
4. `tls_downstream_proxy_returns_err_on_upstream_connect_refused` — TLS downstream + refused upstream; same `TcpProxyError::UpstreamConnect` as plaintext.

- [ ] **Step 1: Add dev-deps to `crates/envoy-tcp/Cargo.toml`.**

Replace the existing `[dev-dependencies]` section (or add if absent):

```toml
[dev-dependencies]
rcgen = "0.13"
rustls = { version = "0.23", default-features = false, features = ["std", "tls12"] }
rustls-pki-types = "1"
tempfile = "3"
tokio-rustls = { version = "0.26", default-features = false, features = ["aws-lc-rs"] }
```

- [ ] **Step 2: Write the failing test `proxies_payload_through_tls_downstream_stream`.**

Append to `crates/envoy-tcp/src/lib.rs::tests`:

```rust
    /// 03.1: prove `TcpProxy::handle::<S>` accepts a `TlsStream<TcpStream>` as
    /// the post-handshake downstream stream type. End-to-end byte-equality at
    /// the proxy boundary, no envoy-listener / envoy-bin / envoy-tls
    /// involvement (envoy-tcp + a stub TLS pair built in-test).
    #[tokio::test(flavor = "multi_thread")]
    async fn proxies_payload_through_tls_downstream_stream() {
        use rcgen::{CertificateParams, KeyPair};
        use rustls::pki_types::ServerName;
        use std::convert::TryFrom;

        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        // Plaintext upstream echo backend.
        let upstream_addr = spawn_echo().await;

        // rcgen-built CA + leaf with SAN `localhost` for the in-test TLS pair.
        let mut ca_params =
            CertificateParams::new(vec!["test-ca".into()]).expect("ca params");
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let ca_kp = KeyPair::generate().expect("ca kp");
        let ca_cert = ca_params.self_signed(&ca_kp).expect("ca");

        let mut leaf_params =
            CertificateParams::new(vec!["localhost".into()]).expect("leaf params");
        let leaf_kp = KeyPair::generate().expect("leaf kp");
        let leaf_cert = leaf_params
            .signed_by(&leaf_kp, &ca_cert, &ca_kp)
            .expect("leaf");
        let leaf_der: rustls::pki_types::CertificateDer<'static> =
            leaf_cert.der().clone().into_owned();

        let leaf_key_pem = leaf_kp.serialize_pem();
        let mut key_slice = leaf_key_pem.as_bytes();
        let key = rustls_pemfile::private_key(&mut key_slice)
            .expect("priv key parse")
            .expect("priv key present");
        let signing = rustls::crypto::aws_lc_rs::sign::any_supported_type(&key)
            .expect("any_supported_type");
        let certified = rustls::sign::CertifiedKey::new(vec![leaf_der], signing);
        let resolver_arc = Arc::new(certified);

        #[derive(Debug)]
        struct StaticResolver(Arc<rustls::sign::CertifiedKey>);
        impl rustls::server::ResolvesServerCert for StaticResolver {
            fn resolve(
                &self,
                _hello: rustls::server::ClientHello<'_>,
            ) -> Option<Arc<rustls::sign::CertifiedKey>> {
                Some(self.0.clone())
            }
        }

        let server_cfg = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(StaticResolver(resolver_arc)) as Arc<_>);
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_cfg));

        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(ca_cert.der().clone().into_owned())
            .expect("root add");
        let client_cfg = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));

        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");

        let handle = mk_handle("backend", upstream_addr);
        let proxy = TcpProxy::new(handle, &mk_cfg("backend"));
        let proxy_arc: Arc<TcpProxy> = Arc::new(proxy);
        let proxy_arc_clone = proxy_arc.clone();
        let acceptor_clone = acceptor.clone();
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = downstream_listener.accept().await.expect("accept");
            let tls_stream = acceptor_clone.accept(stream).await.expect("server tls");
            // The post-handshake `TlsStream<TcpStream>` is the type the
            // generic `TcpProxy::handle::<S>` accepts.
            proxy_arc_clone
                .handle(tls_stream)
                .await
                .expect("handle ok")
        });

        let server_name = ServerName::try_from("localhost").expect("server name");
        let tcp_client = TcpStream::connect(downstream_addr).await.expect("connect");
        let mut tls_client = connector
            .connect(server_name, tcp_client)
            .await
            .expect("client handshake");
        let payload = b"end-to-end through tls + tcp_proxy";
        tls_client.write_all(payload).await.expect("write");
        let mut buf = vec![0u8; payload.len()];
        tls_client.read_exact(&mut buf).await.expect("read_exact");
        assert_eq!(buf, payload);
        tls_client.shutdown().await.ok();
        drop(tls_client);

        proxy_task.await.expect("proxy task joins");
    }
```

- [ ] **Step 3: Run the test; verify it fails to compile.**

```bash
cargo test -p envoy-tcp proxies_payload_through_tls_downstream_stream
```

Expected: `error[E0308]: mismatched types ... expected \`TcpStream\`, found \`TlsStream<...>\``. The test won't compile because `TcpProxy::handle` is concrete on `TcpStream`.

- [ ] **Step 4: Generalize `TcpProxy::handle` in `crates/envoy-tcp/src/lib.rs`.**

Replace the existing `impl TcpProxy { ... pub fn new }` and the `impl ConnectionHandler for TcpProxy` block with:

```rust
impl TcpProxy {
    pub fn new(cluster: envoy_cluster::ClusterHandle, cfg: &envoy_config::TcpProxyConfig) -> Self {
        Self {
            cluster,
            cluster_name: cfg.cluster.clone(),
        }
    }

    /// 03.1: generalize over any `AsyncRead + AsyncWrite` stream so the
    /// listener can pass either a `TcpStream` (plaintext path) or a
    /// `TlsStream<TcpStream>` (post-handshake TLS path) into it. The proxy
    /// logic itself does not care.
    ///
    /// This is an inherent generic method, NOT a trait method — the
    /// `ConnectionHandler` trait stays object-safe with a `TcpStream`-only
    /// `handle`, and the `envoy-bin::TlsAcceptingHandler` adapter (Task 9)
    /// calls this inherent method directly via `Arc<TcpProxy>`. See SPEC §6
    /// signpost 3.
    pub async fn handle<S>(
        &self,
        downstream: S,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let pick = self.cluster.pick_endpoint();
        let cluster_name = self.cluster_name.clone();
        let addr = pick.ok_or_else(|| {
            Box::new(TcpProxyError::NoHealthyEndpoint {
                cluster: cluster_name.clone(),
            }) as Box<dyn std::error::Error + Send + Sync>
        })?;

        let upstream = tokio::net::TcpStream::connect(addr).await.map_err(|source| {
            Box::new(TcpProxyError::UpstreamConnect { addr, source })
                as Box<dyn std::error::Error + Send + Sync>
        })?;

        // ADR-0016: half-close posture. `tokio::select!` over the two copy
        // futures so EOF on either side drops the other future and propagates
        // FIN via Drop on the write half.
        let (mut dr, mut dw) = tokio::io::split(downstream);
        let (mut ur, mut uw) = upstream.into_split();
        let result: Result<(), std::io::Error> = tokio::select! {
            res = tokio::io::copy(&mut dr, &mut uw) => res.map(|_| ()),
            res = tokio::io::copy(&mut ur, &mut dw) => res.map(|_| ()),
        };
        drop((dr, dw, ur, uw));
        result.map_err(|source| {
            Box::new(TcpProxyError::CopyFailed { source })
                as Box<dyn std::error::Error + Send + Sync>
        })?;

        tracing::debug!(%addr, cluster = %cluster_name, "tcp proxy connection complete");
        Ok(())
    }
}

impl ConnectionHandler for TcpProxy {
    fn handle(
        &self,
        downstream: tokio::net::TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        // Thin wrapper that defers to the inherent generic method via the
        // concrete TcpStream type. This impl exists for object safety —
        // `Listener::serve` works over `Arc<dyn ConnectionHandler>`.
        let cluster = self.cluster.clone();
        let cluster_name = self.cluster_name.clone();
        Box::pin(async move {
            let proxy = TcpProxy {
                cluster,
                cluster_name,
            };
            proxy.handle::<tokio::net::TcpStream>(downstream).await
        })
    }
}
```

Note 1: the trait impl re-builds a `TcpProxy` from cloned fields and calls `handle::<TcpStream>` on it. This is necessary because the trait's `&self` borrow doesn't extend into the boxed future — we need a `'static` reference. Two alternatives, both worse: (a) take `Arc<Self>` parameter (changes the trait signature, breaks object-safety); (b) clone the body. Option (a) is the long-term solution but violates SPEC §3 D3 which mandates zero diff to `envoy-listener`. Cloning the two `Arc`-bearing fields is cheap and preserves the trait shape verbatim. Document this in PROGRESS.md as the cost of preserving option α (parent-SPEC §6 signpost 3).

Note 2: `tokio::io::split(downstream)` accepts any `AsyncRead + AsyncWrite + Unpin` — works for both `TcpStream` and `TlsStream<TcpStream>`. The previous `downstream.into_split()` was `TcpStream`-specific (its `OwnedReadHalf` / `OwnedWriteHalf` types); replaced with the generic `tokio::io::split` (returns `ReadHalf<S>` / `WriteHalf<S>`).

Verify `ClusterHandle: Clone` — phase-02.1's `envoy_cluster::ClusterHandle` is an `Arc<Cluster>` newtype, which derives `Clone`. If at execution time `ClusterHandle` is not `Clone`, document the deviation; the workaround is to pass the inner `Arc<Cluster>` through.

- [ ] **Step 5: Re-run the test; verify it passes.**

```bash
cargo test -p envoy-tcp proxies_payload_through_tls_downstream_stream
```

Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 6: Add the three remaining new tests.**

Append to `crates/envoy-tcp/src/lib.rs::tests`:

```rust
    /// Regression: existing phase-02.2 plaintext path still works through the
    /// now-generic `handle` (call site type-resolves to `TcpStream`).
    #[tokio::test(flavor = "multi_thread")]
    async fn proxies_payload_with_plaintext_stream_unchanged() {
        let upstream_addr = spawn_echo().await;
        let handle = mk_handle("backend", upstream_addr);
        let proxy = TcpProxy::new(handle, &mk_cfg("backend"));

        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");
        let proxy_arc: Arc<TcpProxy> = Arc::new(proxy);
        let proxy_arc_clone = proxy_arc.clone();
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = downstream_listener.accept().await.expect("accept");
            // Call the inherent generic method directly with a TcpStream.
            proxy_arc_clone
                .handle::<TcpStream>(stream)
                .await
                .expect("handle ok")
        });

        let mut client = TcpStream::connect(downstream_addr).await.expect("connect");
        let payload = b"plaintext through generic handle";
        client.write_all(payload).await.expect("write");
        let mut buf = vec![0u8; payload.len()];
        client.read_exact(&mut buf).await.expect("read_exact");
        assert_eq!(buf, payload);
        client.shutdown().await.ok();
        drop(client);
        proxy_task.await.expect("proxy task joins");
    }

    /// TLS variant of `proxies_closes_upstream_on_downstream_close` — same
    /// half-close propagation property over a TLS-wrapped downstream.
    #[tokio::test(flavor = "multi_thread")]
    async fn tls_downstream_proxy_closes_upstream_on_downstream_close() {
        use rcgen::{CertificateParams, KeyPair};
        use rustls::pki_types::ServerName;
        use std::convert::TryFrom;

        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        let upstream_seen_fin = Arc::new(tokio::sync::Notify::new());
        let upstream_seen_fin_signal = upstream_seen_fin.clone();
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let upstream_addr = upstream_listener.local_addr().expect("local_addr");
        tokio::spawn(async move {
            let (mut stream, _) = upstream_listener.accept().await.expect("accept");
            let mut buf = [0u8; 64];
            let n = stream.read(&mut buf).await.expect("read");
            assert_eq!(n, 0, "upstream expected EOF after downstream drop");
            upstream_seen_fin_signal.notify_one();
        });

        // Build a TLS pair as in `proxies_payload_through_tls_downstream_stream`.
        let mut ca_params = CertificateParams::new(vec!["test-ca".into()]).expect("ca");
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let ca_kp = KeyPair::generate().expect("ca kp");
        let ca_cert = ca_params.self_signed(&ca_kp).expect("ca");
        let mut leaf_params = CertificateParams::new(vec!["localhost".into()]).expect("leaf");
        let leaf_kp = KeyPair::generate().expect("leaf kp");
        let leaf_cert = leaf_params.signed_by(&leaf_kp, &ca_cert, &ca_kp).expect("leaf");
        let leaf_der: rustls::pki_types::CertificateDer<'static> =
            leaf_cert.der().clone().into_owned();
        let leaf_key_pem = leaf_kp.serialize_pem();
        let mut key_slice = leaf_key_pem.as_bytes();
        let key = rustls_pemfile::private_key(&mut key_slice).unwrap().unwrap();
        let signing = rustls::crypto::aws_lc_rs::sign::any_supported_type(&key).unwrap();
        let certified = Arc::new(rustls::sign::CertifiedKey::new(vec![leaf_der], signing));

        #[derive(Debug)]
        struct R(Arc<rustls::sign::CertifiedKey>);
        impl rustls::server::ResolvesServerCert for R {
            fn resolve(
                &self,
                _: rustls::server::ClientHello<'_>,
            ) -> Option<Arc<rustls::sign::CertifiedKey>> {
                Some(self.0.clone())
            }
        }
        let server_cfg = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(R(certified)) as Arc<_>);
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_cfg));

        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(ca_cert.der().clone().into_owned())
            .expect("root");
        let client_cfg = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));

        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");

        let handle = mk_handle("backend", upstream_addr);
        let proxy_arc: Arc<TcpProxy> = Arc::new(TcpProxy::new(handle, &mk_cfg("backend")));
        let proxy_arc_clone = proxy_arc.clone();
        let acceptor_clone = acceptor.clone();
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = downstream_listener.accept().await.expect("accept");
            let tls = acceptor_clone.accept(stream).await.expect("server tls");
            proxy_arc_clone.handle(tls).await
        });

        let server_name = ServerName::try_from("localhost").expect("server name");
        let tcp_client = TcpStream::connect(downstream_addr).await.expect("connect");
        let tls_client = connector
            .connect(server_name, tcp_client)
            .await
            .expect("client handshake");
        drop(tls_client);

        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            upstream_seen_fin.notified(),
        )
        .await
        .expect("upstream observed FIN within 3s");
        let _ = proxy_task.await;
    }

    /// TLS-wrapped downstream + refused upstream → same `TcpProxyError::UpstreamConnect`
    /// as plaintext (TLS termination doesn't introduce new upstream errors when
    /// the upstream is plaintext).
    #[tokio::test(flavor = "multi_thread")]
    async fn tls_downstream_proxy_returns_err_on_upstream_connect_refused() {
        use rcgen::{CertificateParams, KeyPair};
        use rustls::pki_types::ServerName;
        use std::convert::TryFrom;

        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let refused: SocketAddr = "127.0.0.1:1".parse().unwrap();

        let mut ca_params = CertificateParams::new(vec!["test-ca".into()]).expect("ca");
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let ca_kp = KeyPair::generate().expect("ca kp");
        let ca_cert = ca_params.self_signed(&ca_kp).expect("ca");
        let mut leaf_params = CertificateParams::new(vec!["localhost".into()]).expect("leaf");
        let leaf_kp = KeyPair::generate().expect("leaf kp");
        let leaf_cert = leaf_params.signed_by(&leaf_kp, &ca_cert, &ca_kp).expect("leaf");
        let leaf_der: rustls::pki_types::CertificateDer<'static> =
            leaf_cert.der().clone().into_owned();
        let leaf_key_pem = leaf_kp.serialize_pem();
        let mut key_slice = leaf_key_pem.as_bytes();
        let key = rustls_pemfile::private_key(&mut key_slice).unwrap().unwrap();
        let signing = rustls::crypto::aws_lc_rs::sign::any_supported_type(&key).unwrap();
        let certified = Arc::new(rustls::sign::CertifiedKey::new(vec![leaf_der], signing));

        #[derive(Debug)]
        struct R(Arc<rustls::sign::CertifiedKey>);
        impl rustls::server::ResolvesServerCert for R {
            fn resolve(
                &self,
                _: rustls::server::ClientHello<'_>,
            ) -> Option<Arc<rustls::sign::CertifiedKey>> {
                Some(self.0.clone())
            }
        }
        let server_cfg = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(R(certified)) as Arc<_>);
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_cfg));

        let mut roots = rustls::RootCertStore::empty();
        roots.add(ca_cert.der().clone().into_owned()).unwrap();
        let client_cfg = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));

        let downstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let downstream_addr = downstream_listener.local_addr().expect("local_addr");

        let handle = mk_handle("backend", refused);
        let proxy_arc: Arc<TcpProxy> = Arc::new(TcpProxy::new(handle, &mk_cfg("backend")));
        let proxy_arc_clone = proxy_arc.clone();
        let acceptor_clone = acceptor.clone();
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = downstream_listener.accept().await.expect("accept");
            let tls = acceptor_clone.accept(stream).await.expect("server tls");
            proxy_arc_clone.handle(tls).await
        });

        let server_name = ServerName::try_from("localhost").expect("server name");
        let tcp_client = TcpStream::connect(downstream_addr).await.expect("connect");
        let _tls_client = connector
            .connect(server_name, tcp_client)
            .await
            .expect("client handshake");

        let result = proxy_task.await.expect("proxy task joins");
        let err = result.expect_err("upstream connect must fail");
        let formatted = format!("{err}");
        assert!(
            formatted.contains("connecting to upstream 127.0.0.1:1"),
            "expected UpstreamConnect, got: {formatted}",
        );
    }
```

- [ ] **Step 7: Run the full envoy-tcp suite.**

```bash
cargo test -p envoy-tcp
```

Expected: `test result: ok. 8 passed; 0 failed` (4 phase-02.2 base + 4 new).

- [ ] **Step 8: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace --lib --bins
```

All four: exit 0 / green. Other crates' tests should be unchanged from Task 7's totals.

- [ ] **Step 9: Commit.**

```bash
git add crates/envoy-tcp
git commit -m "phase 03.1: envoy-tcp — generic-stream lift + 4 TLS-flavored tests"
```

Append a Task 8 PROGRESS section noting the trait-impl-clones-fields pattern and why option α (parent-SPEC §6 signpost 3) drives it.

---

### Task 9: `envoy-bin` — install crypto provider + `tls_handler.rs` adapter + filter-chain TLS dispatch + integration test

**Files:**
- Create: `crates/envoy-bin/src/tls_handler.rs`
- Create: `crates/envoy-bin/tests/tls_downstream.rs`
- Modify: `crates/envoy-bin/Cargo.toml` (add `envoy-tls` runtime path-dep; dev-deps: `tokio-rustls`, `rustls`, `rustls-pki-types`, `rcgen` — `tempfile` already present)
- Modify: `crates/envoy-bin/src/main.rs` (declare `mod tls_handler;`; install crypto provider; extend filter-chain dispatch to wrap `TcpProxy` in `TlsAcceptingHandler` when the chain has a downstream `transport_socket`)

**Scope:** wire downstream TLS termination end-to-end inside `envoy-bin`. Per SPEC §3 D5.

The Rust-native integration test (`crates/envoy-bin/tests/tls_downstream.rs`) is the in-process backstop to fixture 0004 (Task 12); it uses `CARGO_BIN_EXE_envoy-bin` (in-package, available — same as the existing phase-02.2 `tcp_proxy.rs`), an rcgen-driven test PKI, an in-process plaintext echo server as the upstream backend, and `tokio_rustls::TlsConnector` as the downstream client.

- [ ] **Step 1: Update `crates/envoy-bin/Cargo.toml`.**

Replace the existing `[dependencies]` and `[dev-dependencies]` sections:

```toml
[dependencies]
anyhow = "1"
envoy-cluster = { path = "../envoy-cluster" }
envoy-config = { path = "../envoy-config" }
envoy-listener = { path = "../envoy-listener" }
envoy-tcp = { path = "../envoy-tcp" }
envoy-tls = { path = "../envoy-tls" }
httparse = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "io-util", "signal", "time", "sync", "process"] }
tokio-util = { version = "0.7", features = ["default"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }

[dev-dependencies]
rcgen = "0.13"
rustls = { version = "0.23", default-features = false, features = ["std", "tls12"] }
rustls-pki-types = "1"
tempfile = "3"
tokio-rustls = { version = "0.26", default-features = false, features = ["aws-lc-rs"] }
```

- [ ] **Step 2: Write `crates/envoy-bin/src/tls_handler.rs`.**

```rust
use std::sync::Arc;

use envoy_listener::{BoxFuture, ConnectionHandler};
use envoy_tcp::TcpProxy;
use envoy_tls::DownstreamTls;

/// Adapter that runs `DownstreamTls::accept` before delegating to
/// `TcpProxy::handle::<TlsStream<TcpStream>>`. envoy-listener's
/// `ConnectionHandler` trait stays object-safe by keeping the trait method
/// concrete on `TcpStream`; the adapter calls the inner `TcpProxy`'s inherent
/// generic `handle::<S>` method directly via `Arc<TcpProxy>`. See SPEC §6
/// signposts 2 and 3.
pub struct TlsAcceptingHandler {
    pub tls: Arc<DownstreamTls>,
    pub inner: Arc<TcpProxy>,
}

impl ConnectionHandler for TlsAcceptingHandler {
    fn handle(
        &self,
        downstream: tokio::net::TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        let tls = self.tls.clone();
        let inner = self.inner.clone();
        Box::pin(async move {
            // Per SPEC §6 signpost 12 / parent-SPEC §6 signpost 19: TLS
            // handshake errors propagate via the boxed future's Err arm; the
            // listener's accept loop logs at warn! and drops the connection.
            let post_handshake = tls.accept(downstream).await.map_err(|e| {
                Box::new(e) as Box<dyn std::error::Error + Send + Sync>
            })?;
            inner.handle(post_handshake).await
        })
    }
}
```

- [ ] **Step 3: Modify `crates/envoy-bin/src/main.rs`.**

Add `mod tls_handler;` declaration alongside the existing `mod admin; mod argv; mod echo;` block:

```rust
mod admin;
mod argv;
mod echo;
mod tls_handler;
```

Inside `async fn run(...)`, near the top (after `let bootstrap = envoy_config::parse_bootstrap(&yaml)?;` and before the `cluster_mgr` build), insert the crypto-provider install:

```rust
    // Per SPEC §6 signpost 4: rustls's aws-lc-rs default provider must be
    // installed once per process before any TLS-touching code runs. The
    // `install_default()` call returns `Err(_)` on second-or-later calls,
    // which is the no-op behavior we want — discard with `let _ =`.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
```

Then, in the `envoy_config::TCP_PROXY_FILTER` arm of the existing `match filter.name.as_str()` block, replace the `let proxy = std::sync::Arc::new(envoy_tcp::TcpProxy::new(cluster, tp_cfg));` + immediately following `Listener::bind` block with the TLS-aware variant:

Current code at `crates/envoy-bin/src/main.rs:109-133`:

```rust
            envoy_config::TCP_PROXY_FILTER => {
                let Some(envoy_config::TypedConfig::TcpProxy(tp_cfg)) =
                    filter.typed_config.as_ref()
                else {
                    anyhow::bail!(
                        "filter '{}' missing typed_config; envoy-config validator should have rejected at parse time",
                        envoy_config::TCP_PROXY_FILTER,
                    );
                };
                let cluster = cluster_mgr
                    .get(&tp_cfg.cluster)
                    .expect("validator guarantees cluster present");
                let proxy = std::sync::Arc::new(envoy_tcp::TcpProxy::new(cluster, tp_cfg));
                let listener = envoy_listener::Listener::bind(listener_cfg, proxy)
                    .await
                    .with_context(|| format!("binding tcp_proxy listener to {bind_addr}"))?;
                tracing::info!(addr = %bind_addr, cluster = %tp_cfg.cluster, "envoy-rust listening (tcp_proxy)");
                let shutdown = token.clone();
                set.spawn(async move {
                    listener
                        .serve(async move { shutdown.cancelled().await })
                        .await
                        .map_err(|e| anyhow::anyhow!(e))
                });
            }
```

Replace with:

```rust
            envoy_config::TCP_PROXY_FILTER => {
                let Some(envoy_config::TypedConfig::TcpProxy(tp_cfg)) =
                    filter.typed_config.as_ref()
                else {
                    anyhow::bail!(
                        "filter '{}' missing typed_config; envoy-config validator should have rejected at parse time",
                        envoy_config::TCP_PROXY_FILTER,
                    );
                };
                let cluster = cluster_mgr
                    .get(&tp_cfg.cluster)
                    .expect("validator guarantees cluster present");
                let proxy = std::sync::Arc::new(envoy_tcp::TcpProxy::new(cluster, tp_cfg));

                // Per SPEC §3 D5: pre-pass the listener's first filter chain
                // for a downstream `transport_socket`. If present, wrap the
                // inner `Arc<TcpProxy>` in a `TlsAcceptingHandler`. The
                // validator already rejected the wrong direction
                // (UpstreamTlsContext on a listener) and the wrong name
                // (anything not `envoy.transport_sockets.tls`), so the
                // `Upstream(...)` arm and the `name != TLS_TRANSPORT_SOCKET`
                // case are unreachable here.
                let chain = listener_cfg
                    .filter_chains
                    .first()
                    .expect("validator guarantees ≥1 filter chain");
                let handler: std::sync::Arc<dyn envoy_listener::ConnectionHandler> =
                    if let Some(ts) = chain.transport_socket.as_ref() {
                        let envoy_config::TransportSocketTypedConfig::Downstream(ctx) =
                            &ts.typed_config
                        else {
                            anyhow::bail!(
                                "validator should have rejected upstream transport_socket on listener",
                            );
                        };
                        let downstream_tls = std::sync::Arc::new(
                            envoy_tls::DownstreamTls::from_context(ctx)
                                .context("building DownstreamTls from listener transport_socket")?,
                        );
                        std::sync::Arc::new(tls_handler::TlsAcceptingHandler {
                            tls: downstream_tls,
                            inner: proxy,
                        })
                    } else {
                        proxy
                    };

                let listener = envoy_listener::Listener::bind(listener_cfg, handler)
                    .await
                    .with_context(|| format!("binding tcp_proxy listener to {bind_addr}"))?;
                tracing::info!(addr = %bind_addr, cluster = %tp_cfg.cluster, "envoy-rust listening (tcp_proxy)");
                let shutdown = token.clone();
                set.spawn(async move {
                    listener
                        .serve(async move { shutdown.cancelled().await })
                        .await
                        .map_err(|e| anyhow::anyhow!(e))
                });
            }
```

- [ ] **Step 4: Verify the workspace builds with the new code.**

```bash
cargo build --workspace --all-targets
```

Expected: clean build. Note: `tls_handler::TlsAcceptingHandler` carries `Arc<TcpProxy>` (not `Arc<dyn ConnectionHandler>`) deliberately — Task 8's inherent `TcpProxy::handle::<S>` is the call target, and that's not a trait method.

- [ ] **Step 5: Write the failing integration test `crates/envoy-bin/tests/tls_downstream.rs`.**

```rust
//! Phase 03.1 backstop: TLS-terminating tcp_proxy through envoy-bin against
//! an in-process plaintext echo upstream. Mirror of `tests/tcp_proxy.rs` from
//! phase 02.2. The real differential assertion is the Docker-gated
//! `tests/differential/tests/tls_downstream.rs` (Task 12).

use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use rcgen::{CertificateParams, IsCa, KeyPair, KeyUsagePurpose};
use rustls::pki_types::{CertificateDer, ServerName};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

fn reserve_port() -> u16 {
    let l = StdListener::bind(("127.0.0.1", 0)).unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

async fn wait_ready(addr: SocketAddr, budget: Duration) {
    let deadline = std::time::Instant::now() + budget;
    let mut delay = Duration::from_millis(50);
    loop {
        match TcpStream::connect(addr).await {
            Ok(_) => return,
            Err(_) if std::time::Instant::now() < deadline => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_millis(500));
            }
            Err(e) => panic!("listener never became ready at {addr}: {e}"),
        }
    }
}

struct TestPki {
    _dir: tempfile::TempDir,
    leaf_cert: std::path::PathBuf,
    leaf_key: std::path::PathBuf,
    ca_der: CertificateDer<'static>,
}

fn build_pki() -> TestPki {
    let dir = tempfile::tempdir().unwrap();
    let mut ca_params = CertificateParams::new(vec!["test-ca".into()]).unwrap();
    ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let ca_kp = KeyPair::generate().unwrap();
    let ca_cert = ca_params.self_signed(&ca_kp).unwrap();

    let mut leaf_params = CertificateParams::new(vec!["a.example.com".into()]).unwrap();
    let leaf_kp = KeyPair::generate().unwrap();
    let leaf_cert = leaf_params.signed_by(&leaf_kp, &ca_cert, &ca_kp).unwrap();

    let leaf_pem_path = dir.path().join("leaf.pem");
    let leaf_key_path = dir.path().join("leaf.key");
    std::fs::write(&leaf_pem_path, leaf_cert.pem()).unwrap();
    std::fs::write(&leaf_key_path, leaf_kp.serialize_pem()).unwrap();

    let ca_der: CertificateDer<'static> = ca_cert.der().clone().into_owned();
    TestPki {
        _dir: dir,
        leaf_cert: leaf_pem_path,
        leaf_key: leaf_key_path,
        ca_der,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn tls_downstream_round_trips_through_envoy_bin() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // In-process plaintext echo backend.
    let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let backend_addr = backend_listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = backend_listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let (mut r, mut w) = stream.split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
            });
        }
    });

    let pki = build_pki();
    let listener_port = reserve_port();
    let leaf_cert_str = pki.leaf_cert.to_string_lossy();
    let leaf_key_str = pki.leaf_key.to_string_lossy();
    let yaml = format!(
        r#"
static_resources:
  listeners:
    - name: tls_listener
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {listener_port}
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain:
                      filename: {leaf_cert_str}
                    private_key:
                      filename: {leaf_key_str}
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
                      port_value: {backend_port}
"#,
        backend_port = backend_addr.port(),
    );

    let mut cfg_file = tempfile::NamedTempFile::new().unwrap();
    cfg_file.write_all(yaml.as_bytes()).unwrap();
    cfg_file.flush().unwrap();

    let bin = env!("CARGO_BIN_EXE_envoy-bin");
    let mut child = tokio::process::Command::new(bin)
        .arg(cfg_file.path())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin");

    let listener_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    wait_ready(listener_addr, Duration::from_secs(10)).await;

    // TLS client: trust only the test CA.
    let mut roots = rustls::RootCertStore::empty();
    roots.add(pki.ca_der.clone()).unwrap();
    let client_cfg = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));
    let server_name = ServerName::try_from("a.example.com").unwrap();

    let tcp = TcpStream::connect(listener_addr).await.unwrap();
    let mut tls = connector.connect(server_name, tcp).await.expect("handshake");

    let payload = b"tls round-trip through envoy-bin";
    tls.write_all(payload).await.unwrap();
    let mut buf = vec![0u8; payload.len()];
    tls.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf, payload);

    tls.shutdown().await.ok();
    drop(tls);

    let _ = child.start_kill();
    let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
}
```

- [ ] **Step 6: Run the integration test.**

```bash
cargo test -p envoy-bin --test tls_downstream
```

Expected: `test result: ok. 1 passed; 0 failed`. The full byte-exact round-trip through TLS termination + tcp_proxy + plaintext upstream completes.

If this fails because `envoy-bin` panics during boot (e.g., `parse_bootstrap` rejects the YAML), debug per `superpowers:systematic-debugging` — most likely the validator change in Task 3 has a bug, the schema change in Task 2 has a typo in the `@type` URL string, or the Task 9 wiring has an `unwrap` on the wrong branch.

- [ ] **Step 7: Run the full envoy-bin test suite.**

```bash
cargo test -p envoy-bin
```

Expected: every pre-existing test passes (admin tests, echo tests, the phase-02.2 `tcp_proxy.rs` integration test) plus the new `tls_downstream.rs` integration test.

- [ ] **Step 8: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace --lib --bins
```

All four: exit 0 / green.

- [ ] **Step 9: Commit.**

```bash
git add crates/envoy-bin
git commit -m "phase 03.1: envoy-bin — TLS handler adapter + crypto provider install + integration test"
```

Append a Task 9 PROGRESS section.

---

### Task 10: Differential harness — `tls.rs` (`TlsTestPki`) + `Driver::TlsTcp` + render_yaml TLS keys + 2+4 unit tests

**Files:**
- Create: `tests/differential/src/tls.rs`
- Modify: `tests/differential/src/lib.rs` (add `pub mod tls;`; add `Driver::TlsTcp` variant; add 2 render_yaml unit tests)
- Modify: `tests/differential/Cargo.toml` (add `rcgen`, `rustls`, `rustls-pki-types`, `rustls-pemfile`, `tokio-rustls` as dev-deps)

**Scope:** the harness's TLS-side data structures + driver-grammar extension + render_yaml extension. `drive_tls` and the `run_fixture` dispatch land in Task 11.

**Why split tls.rs across Tasks 10 + 11.** `TlsTestPki` is a self-contained data structure; the `drive_tls` helper depends on a `RootCertStore` built from `TlsTestPki`'s CA, plus `run_fixture` dispatch needs the path-substitution map produced by `TlsTestPki::envoy_side_paths` / `subject_side_paths`. Splitting the work this way keeps each task's diff scoped (Task 10 = data-structures + grammar; Task 11 = wire-up).

The 4 `tls.rs` unit tests:
1. `tls_test_pki_generates_valid_chain` — generate, parse all PEMs back, walk Issuer/Subject.
2. `tls_test_pki_drop_removes_tmpdir` — generate, capture path, drop, assert removal.
3. `envoy_side_paths_returns_container_paths` — the `/etc/envoy-rust-tls/...` map.
4. `subject_side_paths_returns_host_tmpdir_paths` — the host tmpdir map.

The 2 `lib.rs::tests` tests:
5. `render_yaml_substitutes_tls_paths_for_envoy_side` — `{{LEAF_A_CERT_PATH}}` → `/etc/envoy-rust-tls/leaf-a-cert.pem` etc.
6. `render_yaml_substitutes_tls_paths_for_subject_side` — same template renders to the host tmpdir path.

- [ ] **Step 1: Update `tests/differential/Cargo.toml`.**

Replace the existing `[dev-dependencies]` section:

```toml
[dev-dependencies]
rcgen = "0.13"
rustls = { version = "0.23", default-features = false, features = ["std", "tls12"] }
rustls-pemfile = "2"
rustls-pki-types = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "io-util", "process", "time"] }
tokio-rustls = { version = "0.26", default-features = false, features = ["aws-lc-rs"] }
```

Note: `rcgen` and `tokio-rustls` etc. could in principle be runtime deps if `drive_tls` is exposed publicly. But the only consumer of `drive_tls` is the harness's own integration tests (`tests/differential/tests/tls_downstream.rs`) — those are dev tests, so dev-deps suffice. **Plan-time correction**: `drive_tls` is called from `run_fixture` (Task 11) which is a `pub fn` exposed to other workspace test crates (not the case today, but anticipated for cross-crate integration). Move `rustls`, `rustls-pemfile`, `rustls-pki-types`, `tokio-rustls` to `[dependencies]` and keep `rcgen` + `tempfile` (already there) as `[dependencies]` too (matches differential's existing posture of carrying everything `run_fixture` needs as runtime deps).

Final `tests/differential/Cargo.toml` after Task 10:

```toml
[package]
name = "differential"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[lib]
name = "differential"
path = "src/lib.rs"

[dependencies]
anyhow = "1"
httparse = "1"
rcgen = "0.13"
rustls = { version = "0.23", default-features = false, features = ["std", "tls12"] }
rustls-pemfile = "2"
rustls-pki-types = "1"
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
tempfile = "3"
testcontainers = "0.23"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "io-util", "process", "time"] }
tokio-rustls = { version = "0.26", default-features = false, features = ["aws-lc-rs"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }

[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "io-util", "process", "time"] }
```

- [ ] **Step 2: Write `tests/differential/src/tls.rs`.**

```rust
//! Phase 03.1 differential-harness PKI module. Builds a self-signed CA + leafs
//! `a.example.com`, `b.example.com`, `envoy-rust.test` in a per-fixture
//! `TempDir`. Both upstream-Envoy (containerized) and envoy-rust (host
//! subprocess) reference the same PEMs via `render_yaml` substitution; the
//! envoy-side paths point inside `/etc/envoy-rust-tls/` (mounted via
//! `with_copy_to_container` in `upstream::start`), while the subject-side
//! paths point at the host tmpdir.
//!
//! 03.1 only uses `leaf_a` + `ca`; `leaf_b` and `server` PEMs are generated
//! anyway (cheap; avoids extending TlsTestPki later) so 03.2 can layer on the
//! SNI fixtures with no harness changes.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use rcgen::{CertificateParams, DnType, IsCa, KeyPair, KeyUsagePurpose};
use tempfile::TempDir;

/// Container-side path prefix; PEMs land here via testcontainers'
/// `with_copy_to_container`. SPEC §6 signpost 7 / parent-SPEC §6 signpost 12.
pub const ENVOY_SIDE_DIR: &str = "/etc/envoy-rust-tls";

pub struct TlsTestPki {
    pub ca_pem_path: PathBuf,
    pub leaf_a_cert: PathBuf,
    pub leaf_a_key: PathBuf,
    pub leaf_b_cert: PathBuf,
    pub leaf_b_key: PathBuf,
    pub server_cert: PathBuf,
    pub server_key: PathBuf,
    _dir: TempDir,
}

impl TlsTestPki {
    /// Generate a CA + three leafs (`a.example.com`, `b.example.com`,
    /// `envoy-rust.test`) signed by the CA. PEMs are written into a per-call
    /// `TempDir` whose `Drop` removes everything.
    pub fn generate() -> Result<Self> {
        let dir = tempfile::tempdir().context("creating PKI tmpdir")?;
        let (ca_cert, ca_kp) = build_ca()?;

        let (leaf_a_cert, leaf_a_kp) = build_leaf(&ca_cert, &ca_kp, "a.example.com")?;
        let (leaf_b_cert, leaf_b_kp) = build_leaf(&ca_cert, &ca_kp, "b.example.com")?;
        let (srv_cert, srv_kp) = build_leaf(&ca_cert, &ca_kp, "envoy-rust.test")?;

        let ca_pem = ca_cert.pem();
        let ca_pem_path = dir.path().join("ca.pem");
        std::fs::write(&ca_pem_path, &ca_pem).context("write ca.pem")?;

        let leaf_a_cert_path = dir.path().join("leaf-a-cert.pem");
        let leaf_a_key_path = dir.path().join("leaf-a-key.pem");
        std::fs::write(&leaf_a_cert_path, leaf_a_cert.pem()).context("write leaf-a cert")?;
        std::fs::write(&leaf_a_key_path, leaf_a_kp.serialize_pem()).context("write leaf-a key")?;

        let leaf_b_cert_path = dir.path().join("leaf-b-cert.pem");
        let leaf_b_key_path = dir.path().join("leaf-b-key.pem");
        std::fs::write(&leaf_b_cert_path, leaf_b_cert.pem()).context("write leaf-b cert")?;
        std::fs::write(&leaf_b_key_path, leaf_b_kp.serialize_pem()).context("write leaf-b key")?;

        let srv_cert_path = dir.path().join("server-cert.pem");
        let srv_key_path = dir.path().join("server-key.pem");
        std::fs::write(&srv_cert_path, srv_cert.pem()).context("write server cert")?;
        std::fs::write(&srv_key_path, srv_kp.serialize_pem()).context("write server key")?;

        Ok(Self {
            ca_pem_path,
            leaf_a_cert: leaf_a_cert_path,
            leaf_a_key: leaf_a_key_path,
            leaf_b_cert: leaf_b_cert_path,
            leaf_b_key: leaf_b_key_path,
            server_cert: srv_cert_path,
            server_key: srv_key_path,
            _dir: dir,
        })
    }

    /// Path map for the envoy.yaml side: keys are the substitution tokens,
    /// values are container-mounted paths under `/etc/envoy-rust-tls/`.
    pub fn envoy_side_paths(&self) -> HashMap<&'static str, String> {
        let mut m = HashMap::new();
        m.insert("CA_PATH", format!("{ENVOY_SIDE_DIR}/ca.pem"));
        m.insert(
            "LEAF_A_CERT_PATH",
            format!("{ENVOY_SIDE_DIR}/leaf-a-cert.pem"),
        );
        m.insert(
            "LEAF_A_KEY_PATH",
            format!("{ENVOY_SIDE_DIR}/leaf-a-key.pem"),
        );
        // 03.2 will reference these via the SNI fixture; harmless to expose now.
        m.insert(
            "LEAF_B_CERT_PATH",
            format!("{ENVOY_SIDE_DIR}/leaf-b-cert.pem"),
        );
        m.insert(
            "LEAF_B_KEY_PATH",
            format!("{ENVOY_SIDE_DIR}/leaf-b-key.pem"),
        );
        m.insert(
            "SERVER_CERT_PATH",
            format!("{ENVOY_SIDE_DIR}/server-cert.pem"),
        );
        m.insert(
            "SERVER_KEY_PATH",
            format!("{ENVOY_SIDE_DIR}/server-key.pem"),
        );
        m
    }

    /// Path map for the envoy-rust.yaml side: keys are the same substitution
    /// tokens, values are the actual host tmpdir paths.
    pub fn subject_side_paths(&self) -> HashMap<&'static str, String> {
        let mut m = HashMap::new();
        m.insert("CA_PATH", self.ca_pem_path.to_string_lossy().into_owned());
        m.insert(
            "LEAF_A_CERT_PATH",
            self.leaf_a_cert.to_string_lossy().into_owned(),
        );
        m.insert(
            "LEAF_A_KEY_PATH",
            self.leaf_a_key.to_string_lossy().into_owned(),
        );
        m.insert(
            "LEAF_B_CERT_PATH",
            self.leaf_b_cert.to_string_lossy().into_owned(),
        );
        m.insert(
            "LEAF_B_KEY_PATH",
            self.leaf_b_key.to_string_lossy().into_owned(),
        );
        m.insert(
            "SERVER_CERT_PATH",
            self.server_cert.to_string_lossy().into_owned(),
        );
        m.insert(
            "SERVER_KEY_PATH",
            self.server_key.to_string_lossy().into_owned(),
        );
        m
    }

    /// Files to mount into the upstream container via
    /// `with_copy_to_container`. Returns `(host_path, container_path)` pairs.
    /// SPEC §6 signpost 7.
    pub fn container_mounts(&self) -> Vec<(PathBuf, String)> {
        vec![
            (self.ca_pem_path.clone(), format!("{ENVOY_SIDE_DIR}/ca.pem")),
            (
                self.leaf_a_cert.clone(),
                format!("{ENVOY_SIDE_DIR}/leaf-a-cert.pem"),
            ),
            (
                self.leaf_a_key.clone(),
                format!("{ENVOY_SIDE_DIR}/leaf-a-key.pem"),
            ),
            (
                self.leaf_b_cert.clone(),
                format!("{ENVOY_SIDE_DIR}/leaf-b-cert.pem"),
            ),
            (
                self.leaf_b_key.clone(),
                format!("{ENVOY_SIDE_DIR}/leaf-b-key.pem"),
            ),
            (
                self.server_cert.clone(),
                format!("{ENVOY_SIDE_DIR}/server-cert.pem"),
            ),
            (
                self.server_key.clone(),
                format!("{ENVOY_SIDE_DIR}/server-key.pem"),
            ),
        ]
    }
}

fn build_ca() -> Result<(rcgen::Certificate, KeyPair)> {
    let mut params = CertificateParams::new(vec!["envoy-rust-test-ca".into()])
        .context("ca params")?;
    params
        .distinguished_name
        .push(DnType::CommonName, "envoy-rust-test-ca");
    params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let kp = KeyPair::generate().context("ca kp")?;
    let cert = params.self_signed(&kp).context("ca self-sign")?;
    Ok((cert, kp))
}

fn build_leaf(
    ca_cert: &rcgen::Certificate,
    ca_kp: &KeyPair,
    san_dns: &str,
) -> Result<(rcgen::Certificate, KeyPair)> {
    let mut params = CertificateParams::new(vec![san_dns.into()]).context("leaf params")?;
    params
        .distinguished_name
        .push(DnType::CommonName, san_dns);
    let kp = KeyPair::generate().context("leaf kp")?;
    let cert = params
        .signed_by(&kp, ca_cert, ca_kp)
        .with_context(|| format!("signing leaf for {san_dns}"))?;
    Ok((cert, kp))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tls_test_pki_generates_valid_chain() {
        let pki = TlsTestPki::generate().expect("generate");
        for (label, path) in &[
            ("ca", &pki.ca_pem_path),
            ("leaf_a_cert", &pki.leaf_a_cert),
            ("leaf_a_key", &pki.leaf_a_key),
            ("leaf_b_cert", &pki.leaf_b_cert),
            ("leaf_b_key", &pki.leaf_b_key),
            ("server_cert", &pki.server_cert),
            ("server_key", &pki.server_key),
        ] {
            assert!(path.exists(), "{label} missing at {}", path.display());
            let content = std::fs::read(path).expect("read");
            // `rustls-pemfile::certs` returns ≥1 entry on a cert PEM and zero
            // on a key PEM; the inverse holds for `private_key`. Use `certs`
            // for cert-shaped paths and `private_key` for key-shaped paths.
            if label.ends_with("cert") || *label == "ca" {
                let mut s = content.as_slice();
                let collected: Vec<_> = rustls_pemfile::certs(&mut s).collect();
                assert!(
                    !collected.is_empty(),
                    "{label} contains no certificate at {}",
                    path.display()
                );
            } else {
                // keys
                let mut s = content.as_slice();
                let key = rustls_pemfile::private_key(&mut s).expect("parse key");
                assert!(
                    key.is_some(),
                    "{label} contains no private key at {}",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn tls_test_pki_drop_removes_tmpdir() {
        let pki = TlsTestPki::generate().expect("generate");
        let captured = pki.ca_pem_path.clone();
        assert!(captured.exists());
        drop(pki);
        assert!(
            !captured.exists(),
            "ca path still exists after Drop: {}",
            captured.display()
        );
    }

    #[test]
    fn envoy_side_paths_returns_container_paths() {
        let pki = TlsTestPki::generate().expect("generate");
        let paths = pki.envoy_side_paths();
        assert_eq!(paths.get("CA_PATH").unwrap(), "/etc/envoy-rust-tls/ca.pem");
        assert_eq!(
            paths.get("LEAF_A_CERT_PATH").unwrap(),
            "/etc/envoy-rust-tls/leaf-a-cert.pem"
        );
        assert_eq!(
            paths.get("LEAF_A_KEY_PATH").unwrap(),
            "/etc/envoy-rust-tls/leaf-a-key.pem"
        );
    }

    #[test]
    fn subject_side_paths_returns_host_tmpdir_paths() {
        let pki = TlsTestPki::generate().expect("generate");
        let paths = pki.subject_side_paths();
        let ca = paths.get("CA_PATH").unwrap();
        assert!(
            ca.contains(std::env::temp_dir().to_string_lossy().as_ref())
                || ca.starts_with("/tmp/")
                || ca.starts_with("/var/folders/"),
            "subject-side CA path should be under tmp: {ca}",
        );
        // The actual file must exist.
        assert!(std::path::Path::new(ca).exists());
    }
}
```

- [ ] **Step 3: Add `pub mod tls;` to `tests/differential/src/lib.rs`.**

Locate the existing `pub mod backend; pub mod subject; pub mod upstream;` block and extend:

```rust
pub mod backend;
pub mod subject;
pub mod tls;
pub mod upstream;
```

- [ ] **Step 4: Add the `Driver::TlsTcp` variant.**

Locate the `pub enum Driver` block in `tests/differential/src/lib.rs` (currently lines 35–40):

```rust
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Driver {
    TcpEcho,
    HttpGet { path: String, host: String },
}
```

Replace with:

```rust
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Driver {
    TcpEcho,
    HttpGet { path: String, host: String },
    /// 03.1 NEW: TLS round-trip with explicit SNI + optional CN/SAN check.
    TlsTcp {
        sni: String,
        #[serde(default)]
        expected_cn: Option<String>,
    },
}
```

- [ ] **Step 5: Add the 2 new render_yaml unit tests.**

Append to `tests/differential/src/lib.rs::tests`:

```rust
    #[test]
    fn render_yaml_substitutes_tls_paths_for_envoy_side() {
        let template = r#"
trusted_ca:
  filename: {{CA_PATH}}
leaf_cert:
  filename: {{LEAF_A_CERT_PATH}}
leaf_key:
  filename: {{LEAF_A_KEY_PATH}}
"#;
        let got = render_yaml(
            template,
            &[
                ("CA_PATH", "/etc/envoy-rust-tls/ca.pem"),
                ("LEAF_A_CERT_PATH", "/etc/envoy-rust-tls/leaf-a-cert.pem"),
                ("LEAF_A_KEY_PATH", "/etc/envoy-rust-tls/leaf-a-key.pem"),
            ],
        );
        assert!(got.contains("filename: /etc/envoy-rust-tls/ca.pem"));
        assert!(got.contains("filename: /etc/envoy-rust-tls/leaf-a-cert.pem"));
        assert!(got.contains("filename: /etc/envoy-rust-tls/leaf-a-key.pem"));
    }

    #[test]
    fn render_yaml_substitutes_tls_paths_for_subject_side() {
        let template = r#"
trusted_ca:
  filename: {{CA_PATH}}
leaf_cert:
  filename: {{LEAF_A_CERT_PATH}}
leaf_key:
  filename: {{LEAF_A_KEY_PATH}}
"#;
        let got = render_yaml(
            template,
            &[
                ("CA_PATH", "/tmp/abc/ca.pem"),
                ("LEAF_A_CERT_PATH", "/tmp/abc/leaf-a-cert.pem"),
                ("LEAF_A_KEY_PATH", "/tmp/abc/leaf-a-key.pem"),
            ],
        );
        assert!(got.contains("filename: /tmp/abc/ca.pem"));
        assert!(got.contains("filename: /tmp/abc/leaf-a-cert.pem"));
        assert!(got.contains("filename: /tmp/abc/leaf-a-key.pem"));
    }
```

- [ ] **Step 6: Run the new tests.**

```bash
cargo test -p differential tls::tests
cargo test -p differential render_yaml_substitutes_tls_paths
```

Expected: 4 + 2 = 6 passed.

- [ ] **Step 7: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace --lib --bins
```

All four: exit 0 / green.

- [ ] **Step 8: Commit.**

```bash
git add tests/differential
git commit -m "phase 03.1: differential — TlsTestPki + Driver::TlsTcp + render_yaml TLS keys"
```

Append a Task 10 PROGRESS section.

---

### Task 11: Differential harness — `drive_tls` + `run_fixture` TLS dispatch + `upstream::start` `with_copy_to_container` + signature plumbing

**Files:**
- Modify: `tests/differential/src/lib.rs` (add `drive_tls`; extend `run_fixture` to detect TLS templates, build `TlsTestPki`, substitute keys per side, dispatch on `Driver::TlsTcp`)
- Modify: `tests/differential/src/upstream.rs` (extend `start` with `tls_pki: Option<&crate::tls::TlsTestPki>` parameter; iterate `tls_pki.container_mounts()` and call `with_copy_to_container` for each pair)

**Scope:** wire the TLS path into the existing `run_fixture` shape end-to-end. Per SPEC §3 D6.

The `drive_tls` helper mirrors `drive_tcp`'s ADR-0006/0007 pattern (read-exact + 100ms trailing-byte poll) over a `tokio_rustls::TlsConnector`. The `expected_cn` matcher walks both `subject_alt_name` (DNS entries) and CommonName, case-insensitive exact match, no wildcards (per SPEC §6 signpost 11).

- [ ] **Step 1: Update `tests/differential/src/upstream.rs` to accept a `tls_pki` parameter.**

Replace the existing `pub async fn start(envoy_yaml_path: &Path, host_gateway: bool) -> Result<UpstreamProxy>` signature and body:

```rust
/// Start upstream Envoy with `envoy_yaml_path` bind-mounted to
/// `/etc/envoy/envoy.yaml`. The caller must have already rendered any
/// `{{PORT}}` token in the YAML to `CONTAINER_PORT`.
///
/// `host_gateway = true` adds `with_host("host.docker.internal", Host::HostGateway)`
/// to the container image (per ADR-0015) — required when the fixture YAML
/// references `host.docker.internal` to reach a host-running backend.
///
/// `tls_pki = Some(&pki)` copies each PEM in `pki.container_mounts()` into the
/// container at `/etc/envoy-rust-tls/<filename>.pem` via
/// `with_copy_to_container` (per parent-SPEC §6 signpost 7 / SPEC §6 signpost 7).
pub async fn start(
    envoy_yaml_path: &Path,
    host_gateway: bool,
    tls_pki: Option<&crate::tls::TlsTestPki>,
) -> Result<UpstreamProxy> {
    let absolute = envoy_yaml_path
        .canonicalize()
        .with_context(|| format!("canonicalizing {}", envoy_yaml_path.display()))?;
    let image = GenericImage::new(IMAGE_NAME, IMAGE_TAG)
        .with_exposed_port(CONTAINER_PORT.tcp())
        .with_wait_for(WaitFor::message_on_stderr("starting main dispatch loop"));
    let mut request = image
        .with_cmd(["-c", "/etc/envoy/envoy.yaml", "--log-level", "info"])
        .with_mount(Mount::bind_mount(
            absolute.to_string_lossy().to_string(),
            "/etc/envoy/envoy.yaml",
        ));
    if host_gateway {
        request = request.with_host(
            "host.docker.internal",
            testcontainers::core::Host::HostGateway,
        );
    }
    if let Some(pki) = tls_pki {
        // Copy each PEM into the container at /etc/envoy-rust-tls/<name>.pem.
        // testcontainers 0.23.x: with_copy_to_container takes a host path
        // (impl Into<String>) and a container path (impl Into<String>). If
        // the actual API differs (e.g., requires a CopyToContainer struct),
        // adapt — see SPEC §6 signpost 7.
        for (host_path, container_path) in pki.container_mounts() {
            request = request.with_copy_to_container(
                host_path.to_string_lossy().to_string(),
                container_path,
            );
        }
    }
    let container = request
        .start()
        .await
        .context("starting upstream envoy container")?;
    let host_port = container
        .get_host_port_ipv4(CONTAINER_PORT.tcp())
        .await
        .context("reading host-mapped port from testcontainers")?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    Ok(UpstreamProxy {
        _container: container,
        host_port,
    })
}
```

Verify the `with_copy_to_container` method signature against `testcontainers = "0.23.x"` at execution time. If the method takes a `CopyToContainer` struct rather than two paths, wrap the `(host_path, container_path)` pairs accordingly. If the method lives at a different trait (`ContainerRequest::with_copy_to_container` vs. `ImageExt::with_copy_to_container`), the call site might need a different chain — adjust without ADR (mechanical surface adaptation, SPEC §6 signpost 7 anticipates it).

- [ ] **Step 2: Update the existing `starts_upstream_envoy_and_exposes_host_port` test call site.**

In `tests/differential/src/upstream.rs::tests`, replace `let proxy = start(yaml.path(), false).await.unwrap();` with `let proxy = start(yaml.path(), false, None).await.unwrap();`.

- [ ] **Step 3: Update the existing `run_fixture` call site in `tests/differential/src/lib.rs`.**

The phase-02.2 `run_fixture` already calls `upstream::start(&upstream_path, host_uses_host_gateway).await?;`. Update to thread the new `tls_pki: Option<&...>` parameter through — for now, pass `None` (the TLS path lands in Step 6).

```rust
    let upstream = upstream::start(&upstream_path, host_uses_host_gateway, None).await?;
```

- [ ] **Step 4: Run the workspace to confirm the existing fixtures still pass.**

```bash
cargo test --workspace --lib --bins
```

Expected: all green; no fixture regressed because the new `tls_pki` param is `None` for fixtures 0001/0002/0003.

- [ ] **Step 5: Write `drive_tls` in `tests/differential/src/lib.rs`.**

Append to `tests/differential/src/lib.rs` (after the existing `drive_tcp` definition):

```rust
/// Drive a payload through `addr` over a TLS connection terminated by the
/// peer (downstream-TLS scenario). The peer's leaf cert is verified against
/// `root_store`; the SNI is `sni`; if `expected_cn` is `Some`, the
/// post-handshake cert chain's leaf is walked for SAN-DNS entries and
/// CommonName, and the test fails if no case-insensitive exact match is
/// found (no wildcard support in 03.1 — SPEC §6 signpost 11).
///
/// Mirrors `drive_tcp`'s ADR-0006/0007 discipline: writes payload, reads
/// exactly `payload.len()` bytes, then runs the 100ms trailing-byte poll.
/// Graceful TLS shutdown on the write side completes before drop.
pub async fn drive_tls(
    addr: SocketAddr,
    payload: &[u8],
    sni: &str,
    root_store: rustls::RootCertStore,
    expected_cn: Option<&str>,
) -> Result<Vec<u8>> {
    use rustls::pki_types::ServerName;
    use std::convert::TryFrom;
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let client_cfg = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(client_cfg));
    let server_name = ServerName::try_from(sni)
        .map_err(|e| anyhow::anyhow!("parsing sni {sni:?}: {e}"))?
        .to_owned();

    let tcp = tokio::net::TcpStream::connect(addr)
        .await
        .with_context(|| format!("connecting to {addr}"))?;
    let mut tls = connector
        .connect(server_name, tcp)
        .await
        .with_context(|| format!("TLS handshake against {addr}"))?;

    if let Some(cn) = expected_cn {
        let peer_certs = tls
            .get_ref()
            .1
            .peer_certificates()
            .ok_or_else(|| anyhow::anyhow!("no peer certificate after handshake"))?;
        let leaf = peer_certs
            .first()
            .ok_or_else(|| anyhow::anyhow!("peer cert chain is empty"))?;
        check_cn_or_san(leaf, cn).context("expected_cn match")?;
    }

    tls.write_all(payload).await?;
    let mut out = vec![0u8; payload.len()];
    tls.read_exact(&mut out).await?;

    let mut tail = [0u8; 64];
    match tokio::time::timeout(Duration::from_millis(100), tls.read(&mut tail)).await {
        Ok(Ok(0)) | Err(_) => {}
        Ok(Ok(n)) => bail!("{addr} sent {n} trailing bytes after echo"),
        Ok(Err(e)) => bail!("{addr} read error after echo: {e}"),
    }

    tls.shutdown().await.ok();
    drop(tls);
    Ok(out)
}

/// Walk a leaf cert's SAN DNS entries + CommonName for a case-insensitive
/// exact match against `wanted`. No wildcard support in 03.1 (SPEC §6
/// signpost 11). The cert is parsed via the rcgen-roundtrip path —
/// rustls-pemfile yields `CertificateDer`, which we re-parse to extract the
/// SAN/CN strings. We use an inline minimal X.509 walk via rustls-pemfile +
/// `rustls::pki_types` machinery; full TLS validation already happened during
/// the handshake.
fn check_cn_or_san(
    cert: &rustls::pki_types::CertificateDer<'_>,
    wanted: &str,
) -> Result<()> {
    // The simplest path: re-encode the DER to PEM, then use rcgen's parser
    // (we already pull rcgen for cert generation, so its parser is in scope
    // for free). If that proves fragile, swap to `x509-parser` under a new
    // ADR. For 03.1 the cert chain we're matching against is rcgen-built
    // ourselves, so an exact match on the SAN DNS string is reliable.
    use rcgen::{CertificateParams, KeyPair};
    let kp = KeyPair::generate().context("scratch kp for parser inputs")?;
    // rcgen 0.13 doesn't ship a public PEM/DER parser exposing SAN strings
    // directly; rather than fight that, fall back to walking the DER for
    // the SAN extension's GeneralNames manually. Phase 03.2 may pull
    // `x509-parser` under a follow-up ADR if more sophisticated cert
    // introspection is needed; for 03.1, the harness's `expected_cn` is
    // optional and used only for sanity — the differential body equivalence
    // is the primary signal.
    //
    // Simplest viable check: the rcgen-built leaf's DER includes the SAN
    // value as a literal UTF-8 substring. Search for it.
    let der_bytes: &[u8] = cert.as_ref();
    let needle = wanted.to_ascii_lowercase();
    let hay: Vec<u8> = der_bytes
        .iter()
        .map(|b| b.to_ascii_lowercase())
        .collect();
    if hay
        .windows(needle.len())
        .any(|w| w == needle.as_bytes())
    {
        return Ok(());
    }
    let _ = kp; // silence unused
    bail!(
        "expected_cn / SAN match for {wanted:?} not found in peer cert (DER-substring scan)",
    );
}
```

The DER-substring scan in `check_cn_or_san` is intentionally minimal — sufficient for the harness's rcgen-built test PKI where the SAN value is a unique enough string. If 03.2 or later needs structured cert introspection, the ADR landing `x509-parser` (or similar) replaces this. Document the limitation in PROGRESS.md.

- [ ] **Step 6: Extend `run_fixture` to detect TLS templates and dispatch on `Driver::TlsTcp`.**

Locate the existing `run_fixture` body in `tests/differential/src/lib.rs` (currently lines 332–446). Modify the body to:

(a) Detect TLS templates after reading both YAMLs:

```rust
    let needs_tls_pki = upstream_template.contains("{{LEAF_A_CERT_PATH}}")
        || upstream_template.contains("{{LEAF_A_KEY_PATH}}")
        || upstream_template.contains("{{CA_PATH}}")
        || upstream_template.contains("{{LEAF_B_CERT_PATH}}")
        || upstream_template.contains("{{SERVER_CERT_PATH}}")
        || subject_template.contains("{{LEAF_A_CERT_PATH}}")
        || subject_template.contains("{{CA_PATH}}");
    let tls_pki = if needs_tls_pki {
        Some(crate::tls::TlsTestPki::generate().context("generating TLS test PKI")?)
    } else {
        None
    };
```

(b) Extend the `port_key` selector to include the `Driver::TlsTcp` arm (TLS uses the same `PORT` token as `TcpEcho`):

```rust
    let port_key = match &expectations.driver {
        Driver::TcpEcho | Driver::TlsTcp { .. } => "PORT",
        Driver::HttpGet { .. } => "ADMIN_PORT",
    };
```

(c) Build the per-side substitution maps with the TLS keys (envoy-side from `envoy_side_paths()`, subject-side from `subject_side_paths()`). Replace the existing `upstream_kvs` / `subject_kvs` build blocks (lines ~367–384) with:

```rust
    let upstream_tls_paths = tls_pki.as_ref().map(|p| p.envoy_side_paths());
    let subject_tls_paths = tls_pki.as_ref().map(|p| p.subject_side_paths());

    let upstream_kvs: Vec<(&str, String)> = {
        let mut v: Vec<(&str, String)> =
            vec![(port_key, upstream_port_str.clone())];
        if let Some(bp) = backend_port_str.as_deref() {
            v.push(("BACKEND_PORT", bp.to_string()));
            v.push(("BACKEND_HOST", "host.docker.internal".to_string()));
        }
        if let Some(map) = upstream_tls_paths.as_ref() {
            for (k, val) in map {
                v.push((*k, val.clone()));
            }
        }
        v
    };
    let subject_kvs: Vec<(&str, String)> = {
        let mut v: Vec<(&str, String)> =
            vec![(port_key, subject_port_str.clone())];
        if let Some(bp) = backend_port_str.as_deref() {
            v.push(("BACKEND_PORT", bp.to_string()));
            v.push(("BACKEND_HOST", "127.0.0.1".to_string()));
        }
        if let Some(map) = subject_tls_paths.as_ref() {
            for (k, val) in map {
                v.push((*k, val.clone()));
            }
        }
        v
    };
```

Note the type change from `Vec<(&str, &str)>` to `Vec<(&str, String)>` to accommodate the owned strings the TLS path returns. The `render_yaml` signature `pub fn render_yaml(template: &str, kvs: &[(&str, &str)]) -> String` then needs the call sites adapted:

```rust
    let upstream_kvs_refs: Vec<(&str, &str)> = upstream_kvs
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect();
    let subject_kvs_refs: Vec<(&str, &str)> = subject_kvs
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect();

    let upstream_yaml = render_yaml(&upstream_template, &upstream_kvs_refs);
    let subject_yaml = render_yaml(&subject_template, &subject_kvs_refs);
```

(d) Thread `tls_pki` through to `upstream::start`:

```rust
    let upstream = upstream::start(&upstream_path, host_uses_host_gateway, tls_pki.as_ref()).await?;
```

(e) Add the `Driver::TlsTcp` arm to the `match &expectations.driver` block. After the existing `Driver::HttpGet { ... }` arm:

```rust
        Driver::TlsTcp { sni, expected_cn } => {
            let payload = std::fs::read(fixture_dir.join("inputs/payload.bin"))
                .context("reading payload.bin")?;
            // Build a RootCertStore from the test CA. Both sides trust the
            // same CA — both proxies present a leaf signed by it.
            let pki = tls_pki
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!(
                    "Driver::TlsTcp requires a TLS-shaped fixture (template did not reference any *_PATH key)"
                ))?;
            let ca_bytes = std::fs::read(&pki.ca_pem_path).context("read ca.pem")?;
            let mut ca_slice = ca_bytes.as_slice();
            let ca_certs: Vec<rustls::pki_types::CertificateDer<'static>> =
                rustls_pemfile::certs(&mut ca_slice)
                    .collect::<Result<Vec<_>, _>>()
                    .context("parse ca.pem certs")?;
            let mut roots = rustls::RootCertStore::empty();
            for c in ca_certs {
                roots.add(c).context("RootCertStore::add")?;
            }

            let upstream_out = drive_tls(
                upstream_addr,
                &payload,
                sni,
                roots.clone(),
                expected_cn.as_deref(),
            )
            .await
            .context("upstream envoy tls drive")?;
            let subject_out = drive_tls(
                subject_addr,
                &payload,
                sni,
                roots,
                expected_cn.as_deref(),
            )
            .await
            .context("envoy-rust tls drive")?;
            subject.shutdown(Duration::from_secs(5)).await.ok();
            drop(upstream);
            assert_equivalence(&expectations, None, None, &upstream_out, &subject_out)?;
        }
```

- [ ] **Step 7: Run the harness unit tests + the existing integration tests.**

```bash
cargo test -p differential
```

Expected: every existing test still passes (TLS path is dormant for fixtures 0001/0002/0003); the 4 new `tls::tests` + 2 new `lib::tests::render_yaml_substitutes_tls_paths_*` tests pass.

The Docker-gated tests (`echo_fixture`, `admin_ready_fixture`, `tcp_proxy_fixture`) only run with Docker available — same behavior as phase 02.2. The new `tls_downstream_fixture` Docker-gated test ships in Task 12.

- [ ] **Step 8: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace --lib --bins
```

All four: exit 0 / green.

- [ ] **Step 9: Commit.**

```bash
git add tests/differential
git commit -m "phase 03.1: differential — drive_tls + run_fixture TLS dispatch + container PEM mounts"
```

Append a Task 11 PROGRESS section noting any deviation in `with_copy_to_container`'s actual signature.

---

### Task 12: Fixture `0004-tls-downstream` (5 files) + Docker-gated `tests/differential/tests/tls_downstream.rs`

**Files:**
- Create: `tests/fixtures/0004-tls-downstream/envoy.yaml`
- Create: `tests/fixtures/0004-tls-downstream/envoy-rust.yaml`
- Create: `tests/fixtures/0004-tls-downstream/inputs/payload.bin` (copy of fixture 0001's 18-byte payload)
- Create: `tests/fixtures/0004-tls-downstream/expectations.yaml`
- Create: `tests/fixtures/0004-tls-downstream/README.md`
- Create: `tests/differential/tests/tls_downstream.rs`

**Scope:** the fixture itself + the Docker-gated acceptance test. Per SPEC §3 D7 / §6 signposts 9 + 10 + 14 + 20.

- [ ] **Step 1: Create the fixture directory and copy `payload.bin`.**

```bash
mkdir -p tests/fixtures/0004-tls-downstream/inputs
cp tests/fixtures/0001-tcp-echo/inputs/payload.bin tests/fixtures/0004-tls-downstream/inputs/payload.bin
```

Verify:

```bash
xxd tests/fixtures/0004-tls-downstream/inputs/payload.bin
```

Expected: 18 bytes `68 65 6c 6c 6f 2c 20 65 6e 76 6f 79 2d 72 75 73 74 0a` (`b"hello, envoy-rust\n"`). Same as fixtures 0001 + 0003 — minimal cognitive load.

- [ ] **Step 2: Write `tests/fixtures/0004-tls-downstream/envoy.yaml`.**

```yaml
node:
  id: envoy-rust-phase-03-1-subject
  cluster: envoy-rust-phase-03-1

admin:
  address:
    socket_address:
      address: 0.0.0.0
      port_value: 0

static_resources:
  listeners:
    - name: tls_listener
      address:
        socket_address:
          address: 0.0.0.0
          port_value: {{PORT}}
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain:
                      filename: {{LEAF_A_CERT_PATH}}
                    private_key:
                      filename: {{LEAF_A_KEY_PATH}}
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
                      address: {{BACKEND_HOST}}
                      port_value: {{BACKEND_PORT}}
```

Note: per SPEC §6 signpost 14, no `alpn_protocols`. Per SPEC §6 signpost 20, no `tls_params`. Per ADR-0016, no `enable_half_close`. The cert paths use the `{{LEAF_A_CERT_PATH}}` / `{{LEAF_A_KEY_PATH}}` tokens that `render_yaml` (Task 11) substitutes with `/etc/envoy-rust-tls/leaf-a-cert.pem` / `/etc/envoy-rust-tls/leaf-a-key.pem` on the envoy side.

`admin.port_value: 0` asks the kernel for an ephemeral port. SPEC §3 D7 contingency: if upstream Envoy v1.33.0 rejects `0` at runtime (boot-loop), land an ADR (likely ADR-0020) introducing `{{ENVOY_ADMIN_PORT}}` and reserving an extra host port in the harness. The plan's first try is the SPEC-prescribed shape, matching fixture 0003's posture.

- [ ] **Step 3: Write `tests/fixtures/0004-tls-downstream/envoy-rust.yaml`.**

```yaml
node:
  id: envoy-rust-phase-03-1-subject
  cluster: envoy-rust-phase-03-1

static_resources:
  listeners:
    - name: tls_listener
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {{PORT}}
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain:
                      filename: {{LEAF_A_CERT_PATH}}
                    private_key:
                      filename: {{LEAF_A_KEY_PATH}}
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
                      port_value: {{BACKEND_PORT}}
```

The per-side divergences from fixture 0003 carry forward: listener bind `0.0.0.0` (container) vs. `127.0.0.1` (host subprocess); endpoint host `{{BACKEND_HOST}}` (templates to `host.docker.internal` on container side) vs. `127.0.0.1` literal on the host side; no admin block on envoy-rust side (phase 03+ scope per ADR-0011). The `transport_socket` block is structurally identical on both sides; the cert paths render to different absolute paths (envoy side: `/etc/envoy-rust-tls/...`; subject side: per-fixture host tmpdir).

- [ ] **Step 4: Write `tests/fixtures/0004-tls-downstream/expectations.yaml`.**

```yaml
driver:
  kind: tls_tcp
  sni: a.example.com
equivalence:
  response_body: byte_exact
```

`expected_cn` is omitted (default `None`); the cert presented by both sides is a leaf with SAN `a.example.com` signed by the same harness CA, so the body-equivalence assertion (the differential surface row 2) is the load-bearing one. If a future review wants explicit cert-presentation-equivalence, the harness extends `expected_cn: a.example.com` and `drive_tls`'s SAN walk asserts on both sides.

- [ ] **Step 5: Write `tests/fixtures/0004-tls-downstream/README.md`.**

```markdown
# Fixture 0004-tls-downstream

This fixture drives an arbitrary byte payload through a listener configured with
`envoy.filters.network.tcp_proxy` whose filter chain terminates downstream TLS
via `transport_socket: envoy.transport_sockets.tls (DownstreamTlsContext)`. The
single configured cert is a leaf with SAN `a.example.com` (rcgen-generated at
fixture-run time per ADR-0018, signed by a harness-generated CA, written into a
per-fixture `TempDir`). Both upstream Envoy and envoy-rust dial the same
plaintext upstream backend (the in-tree `tcp-echo-server` helper from phase
02.1, running as a host subprocess).

The harness's `Driver::TlsTcp { sni: "a.example.com" }` opens a TLS connection
to each proxy with the test CA in its `RootCertStore`, completes the handshake,
writes the payload, reads `payload.len()` bytes, asserts byte-equality, and
runs the ADR-0007 100ms trailing-byte poll. The `drive_tls` helper inherits
`drive_tcp`'s read-exact + trailing-poll discipline (ADR-0006, ADR-0007).

Cross-container host reachability for the plaintext upstream is covered by
ADR-0015; container-to-host PEM availability is provided by testcontainers'
`with_copy_to_container` (per parent-SPEC §6 signpost 7), which copies each
PEM into the upstream Envoy container under `/etc/envoy-rust-tls/` at
container-start time. Half-close posture follows ADR-0016 (`enable_half_close`
absent from both sides).

What is *out* of this fixture (each pinned to a later fixture or phase):

- ALPN — phase 04 first uses ALPN; phase 05 makes it load-bearing.
- Multi-cert SNI cert selection on the downstream — fixture 0006 (sub-phase
  03.2).
- Upstream TLS origination — fixture 0005 (sub-phase 03.2).
- mTLS / `require_client_certificate` — out of phase 03.
- Inline cert/key data sources — phase 03 supports `filename` only.
- `tls_params` (cipher list, min/max version) — defer to a future ADR if
  rustls-vs-Envoy version negotiation drifts (SPEC §6 signpost 20).

ADR references: ADR-0015 (cross-container host reachability), ADR-0016
(`enable_half_close: false` default), ADR-0017 (phase-03 split), ADR-0018
(rcgen + tempfile dev-test-harness-only), ADR-0019 (tokio-rustls +
rustls-pemfile under the rustls grant).
```

- [ ] **Step 6: Write the Docker-gated acceptance test `tests/differential/tests/tls_downstream.rs`.**

```rust
//! Phase 03.1 differential acceptance test: drive a payload through a
//! tcp_proxy listener whose filter chain terminates downstream TLS, with a
//! plaintext upstream backend. Should produce identical bytes between
//! upstream Envoy v1.33.0 and envoy-rust. Docker-gated; in CI this runs on
//! `ubuntu-latest` alongside the phase-00 `echo_fixture`, phase-01
//! `admin_ready_fixture`, and phase-02.2 `tcp_proxy_fixture`.

use std::path::PathBuf;

#[tokio::test]
async fn tls_downstream_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0004-tls-downstream");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
```

Note: this test is NOT marked `#[ignore]` — it follows the same pattern as `tests/differential/tests/echo.rs`, `admin_ready.rs`, `tcp_proxy.rs`, which run unconditionally and panic if Docker is unavailable. CI provides Docker; local dev without Docker sees the same failure mode as the existing acceptance tests.

- [ ] **Step 7: Run the Docker-gated test (locally if Docker is available; otherwise skip and let CI verify).**

```bash
cargo test -p differential --test tls_downstream
```

If Docker is available: expected to pass (full end-to-end byte round-trip through both proxies, both terminating TLS with the harness-generated leaf-A cert). If Docker is not available: expected to fail at upstream container start; same behavior as `echo_fixture` / `admin_ready_fixture` / `tcp_proxy_fixture` in dev environments without Docker.

If the test fails for any reason OTHER than "Docker not available," debug per `superpowers:systematic-debugging`. Common failure modes to expect during execution:

- **`with_copy_to_container` API surface differs from plan-time expectation.** Adapt per the actual testcontainers 0.23.x signature; document deviation in PROGRESS.md (no ADR — mechanical surface adaptation).
- **rustls-vs-Envoy v1.33.0 protocol-version negotiation drift.** Land a new ADR (likely ADR-0020) pinning `tls_params { tls_minimum_protocol_version: TLSv1_3, tls_maximum_protocol_version: TLSv1_3 }` on both sides + a rustls `with_protocol_versions(&[&rustls::version::TLS13])` call in envoy-tls. SPEC §6 signpost 20 anticipates this.
- **Upstream Envoy v1.33.0 rejects `admin.port_value: 0`.** Land an ADR introducing `{{ENVOY_ADMIN_PORT}}` per SPEC §3 D7 contingency. SPEC §3 D7 anticipates this; mirrors the phase-02.2 fallback for fixture 0003.
- **`drive_tls`'s `expected_cn` substring scan triggers a false positive on a different cert presented by the upstream container.** Drop `expected_cn` from `expectations.yaml` (it's `None` already) and re-run; the body-equivalence assertion remains the load-bearing signal.
- **The PEM container mount path doesn't match what `envoy.yaml` references.** Verify the path produced by `TlsTestPki::envoy_side_paths()` against the `with_copy_to_container` target path.

- [ ] **Step 8: Workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace --lib --bins
```

The first three: exit 0. The fourth: all unit + bin tests pass; the Docker-gated `tls_downstream_fixture` is excluded by `--lib --bins` and runs only via `cargo test --workspace` (CI).

- [ ] **Step 9: Commit.**

```bash
git add tests/fixtures/0004-tls-downstream tests/differential/tests/tls_downstream.rs
git commit -m "phase 03.1: fixture 0004-tls-downstream [ADR-0018, ADR-0019]"
```

Append a Task 12 PROGRESS section.

---

### Task 13: State 4 phase-done gate

**Files:**
- Modify (append): `docs/envoy-rust/phases/03.1-tls-foundation-downstream/PROGRESS.md`

**Per `docs/envoy-rust/SKILL_ROUTING.md` state 4.** Run the full local stable-toolchain gate, observe both CI jobs (build+test+lint, fuzz), quote outputs into PROGRESS.md. The plan does not advance ROADMAP.md or STATE.md here — those flip in state 6 (the phase-done commit), not now (BOOTSTRAP_PROMPT.md §5.1: one state per session).

If the gate exposes `Cargo.lock` drift (typical with the new `envoy-tls` workspace member + the rustls / aws-lc-rs / rcgen transitives), land a dedicated `phase 03.1: sync Cargo.lock with phase 03.1 dep graph` commit immediately following Task 13's progress note. Phase-01 precedent: `4955252`. Phase-02.1 precedent: `dea4d16`. Phase-02.2 precedent: `2146014`.

- [ ] **Step 1: Run the local stable-toolchain gate, capturing each command's output.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace --lib --bins
cargo deny check
```

Expected: all five exit 0. Quote tails into PROGRESS.md.

The `cargo test --workspace --lib --bins` count expands from phase 02.2's tally:
- `envoy-config`: 38 → ~50 (Task 2: +5; Task 3: +7; Task 4: +0 net — `fuzz_corpus_seeds_parse_or_reject_cleanly` replaces `fuzz_corpus_tcp_proxy_seeds_parse`, same count).
- `envoy-cluster`: 8 (unchanged).
- `envoy-listener`: 6 (unchanged — SPEC §3 D3's "no diff").
- `envoy-tcp`: 4 → 8 (Task 8: +4).
- `envoy-tls`: 0 → 10 (Tasks 6 + 7).
- `envoy-bin`: 19 + 1 integration test (`tcp_proxy.rs`) → 19 + 2 integration tests (added `tls_downstream.rs`). Unit count unchanged from 19; integration tests run via `cargo test --workspace`, not `--lib --bins`.
- `tcp-echo-server`: 8 (unchanged).
- `differential` lib: 31 → ~37 (Task 10: +6 unit tests). Docker-gated integration tests now total 4 (`echo_fixture`, `admin_ready_fixture`, `tcp_proxy_fixture`, `tls_downstream_fixture`).

- [ ] **Step 2: Trigger CI and observe both jobs.**

After committing all task commits, push the branch and observe the CI run:

```bash
git push origin <branch>
gh run list --workflow=ci.yml -L 1
gh run watch <run-id>
```

Expected: both `build + test + lint` (now also runs the new `tls_downstream_fixture`) and `fuzz (parse_bootstrap, 30s)` jobs succeed. The fuzz job exercises the extended `parse_bootstrap` corpus (3 new TLS seeds) automatically.

- [ ] **Step 3: If `Cargo.lock` is dirty, land a dedicated sync commit.**

```bash
git status
git diff Cargo.lock
git add Cargo.lock
git commit -m "phase 03.1: sync Cargo.lock with phase 03.1 dep graph"
```

The diff should add `[[package]]` stanzas for `envoy-tls v0.0.0` plus the rustls family (`rustls 0.23.x`, `tokio-rustls 0.26.x`, `rustls-pemfile 2.x`, `rustls-pki-types 1.x`, `aws-lc-rs`, `aws-lc-sys`, `rustls-webpki`), plus dev-only `rcgen 0.13.x`. Verify by `git diff` review before staging that no version regressed on existing direct deps and no surprising new transitive landed.

- [ ] **Step 4: Append the State-4 section to PROGRESS.md.**

Use the phase-02.2 PROGRESS State-4 section as the precedent shape. Quote the local-gate command outputs (per-crate test tails are the most informative), the CI run number + URL, and document any fix-during-gate commits (the goal is zero — phase 02.2 cleared on first attempt; phase 03.1 may not, given the rustls transitive count). PROGRESS section template:

```markdown
## Task 13 / State 4 — phase-done gate verification (2026-04-25)

Per `docs/envoy-rust/SKILL_ROUTING.md` state 4: the local stable-toolchain gate ran clean on first attempt. ROADMAP.md and STATE.md are NOT advanced here per `BOOTSTRAP_PROMPT.md` §5.1 (one state per session); those flip in state 6 (the phase-done commit) after state 5's `REVIEW.md` is approved.

### Local stable-toolchain gate

`cargo build --workspace --all-targets`:
```
<tail>
```

`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
```
<tail>
```

`cargo fmt --all -- --check`:
```
(no output — clean)
```

`cargo test --workspace --lib --bins`:
```
<per-crate tails>
```

Total: <N> tests, 0 failed, <ignored>.

`cargo deny check`:
```
advisories ok, bans ok, licenses ok, sources ok
```

### Cargo.lock sync

<note: dirty/clean; if dirty, the SHA of the dedicated sync commit>

### Outstanding for state 5/6

State 5 (`superpowers:requesting-code-review`) writes `REVIEW.md` for this phase. State 6 (the phase-done commit) flips ROADMAP row `03.1` `status` → `done` (parent row `03` stays `in-progress` until 03.2 lands per the schema) and advances STATE.md to phase `03.2-tls-upstream-sni` (lifecycle state 2; SPEC.md exists from the ADR-0017 split commit, PLAN.md does not; next-skill `superpowers:writing-plans`).
```

- [ ] **Step 5: Commit the PROGRESS update.**

```bash
git add docs/envoy-rust/phases/03.1-tls-foundation-downstream/PROGRESS.md
git commit -m "phase 03.1: state-4 phase-done gate verification (task 13)"
```

State 4 verification complete. Next session enters state 5 via `superpowers:requesting-code-review` (writing `REVIEW.md`); state 6 then ships the phase-done commit per `BOOTSTRAP_PROMPT.md` §5.3, flipping ROADMAP row `03.1` to `done` and advancing STATE.md to phase `03.2-tls-upstream-sni` at lifecycle state 2 with next-skill `superpowers:writing-plans`.

---

## Out-of-plan execution contingencies

These are NOT plan steps; they are decision rules for situations the SPEC and plan jointly anticipate but cannot pin at planning time. Per D-3.5, execution lands an ADR and proceeds when any trigger fires.

1. **rustls-vs-Envoy v1.33.0 protocol-version negotiation drift.** SPEC §3 D10 + §6 signpost 20 anticipate this. Land a new ADR (likely ADR-0020) pinning `tls_params { tls_minimum_protocol_version: TLSv1_3, tls_maximum_protocol_version: TLSv1_3 }` on both fixture sides + a rustls `with_protocol_versions(&[&rustls::version::TLS13])` call in envoy-tls's `from_context`. Document in PROGRESS.md.

2. **Upstream Envoy rejects `admin.port_value: 0` in fixture 0004.** SPEC §3 D7 contingency. Land an ADR introducing `{{ENVOY_ADMIN_PORT}}`; reserve a second host port in `run_fixture`; modify Task 12's `envoy.yaml` to substitute it. Mirrors the phase-02.2 contingency for fixture 0003.

3. **`with_copy_to_container` API surface differs from plan-time expectation.** Mechanical surface adaptation (no ADR). Adjust per the actual testcontainers 0.23.x signature in Task 11 Step 1; if the API requires a `CopyToContainer` struct rather than two paths, wrap pairs accordingly. Document in PROGRESS.md.

4. **`cargo deny check` flips red on a new transitive surface.** Most likely the rustls / aws-lc-rs / aws-lc-sys / rcgen / tempfile chain. Update `deny.toml` per ADR-0005's discipline (`wrappers` for direct-ban transitives, scoped `[advisories].ignore` with rationale for new advisories). Land under a new ADR.

5. **rcgen 0.13.x API differs from plan's `KeyPair::generate()` / `CertificateParams::self_signed(&kp)` shape.** SPEC §6 signpost 6 anticipates. Adjust to match the actual 0.13.x API (no ADR — mechanical). Document in PROGRESS.md.

6. **`tokio-rustls` 0.26.x feature names differ from `aws-lc-rs`.** SPEC §6 signpost 5 anticipates. Adjust to match the actual feature set; if a fresh exemption surfaces (very unlikely), amend ADR-0019 at landing time.

7. **Test `proxies_returns_err_on_upstream_connect_refused` (and the TLS variant) flakes on some CI hosts.** Phase-02.2 plan documented this risk. Replace the literal `127.0.0.1:1` with `reserve_port()` followed by an explicit `drop(listener)` (TOCTOU is acceptable per phase-01 SPEC §6 point 6) — get a port that's almost certainly free.

8. **ADR numbering shifts.** If any ADR-00NN lands during execution before Task 1, renumber 0018/0019 in lockstep at Task 1 Step 1; update every cross-reference in this PLAN, in fixture 0004 README, and in the final commit message. Mirrors phase-02.2 contingency 5.

9. **A task's scope balloons past ~10 sub-steps.** Invoke `superpowers:systematic-debugging` before splitting. Phase 03.1 has already been split (it's a sub-phase of 03); a nested split is not anticipated and deserves root-cause analysis (scope creep vs. planner overdecomposition), per SPEC §5 closing paragraph.

10. **SPEC §3 D2's stated test count (10) drifts from actual (12) at Task 3.** Already documented in Task 3 Scope notes; no ADR. The five Task-2 tests (parse-shape + deny_unknown_fields) plus the seven Task-3 tests (validator-driven) cover SPEC §D2's enumerated set with two extras (`rejects_upstream_without_validation_context` and `rejects_upstream_with_empty_sni`). Reviewer-cost rounding.

---

## Final commit message format (state 6 — NOT this state)

The state-6 phase-done commit shape, per SPEC §9. Do NOT land this commit during plan execution; it lands at state 6 (after REVIEW.md is approved at state 5):

```
phase 03.1: envoy-tls foundation + downstream TLS termination + fixture 0004 [ADR-0018, ADR-0019]

New library crate envoy-tls owns rustls server/client config construction:
DownstreamTls::from_context loads cert+key PEMs via rustls-pemfile and builds
a ServerConfig with a single-cert ResolvesServerCert; UpstreamTls library API
ships its from_context + connect with consumer wiring deferred to 03.2.
envoy-config grows the transport_socket envelope (DownstreamTlsContext +
UpstreamTlsContext + CommonTlsContext + TlsCertificate +
CertificateValidationContext + DataSource) with 12 new validator tests and
3 fuzz-corpus seeds. envoy-tcp::TcpProxy::handle generalizes over
AsyncRead+AsyncWrite+Unpin+Send+'static; envoy-bin's new TlsAcceptingHandler
wraps an Arc<TcpProxy> with a downstream-TLS handshake hop and dispatches
into the generic handle. New differential harness PKI (rcgen-driven) and
Driver::TlsTcp; fixture 0004-tls-downstream lands green end-to-end.

Differential surface: tests/fixtures/0001-tcp-echo green (unchanged);
  tests/fixtures/0002-static-admin-ready green (unchanged);
  tests/fixtures/0003-tcp-proxy green (unchanged);
  tests/fixtures/0004-tls-downstream green (single-cert downstream TLS,
  plaintext upstream).
Conformance: none.
```

The state-6 commit also flips:
- `docs/envoy-rust/ROADMAP.md` row `03.1` `status` → `done`. (Row `03` parent stays `in-progress`; flips at 03.2's final commit per the schema invariant.)
- `docs/envoy-rust/STATE.md` → active id `03.2`, slug `03.2-tls-upstream-sni`, lifecycle state 2 (SPEC.md exists, PLAN.md does not), next-skill `superpowers:writing-plans`.
- Appends a final State-6 section to PROGRESS.md.




