# Phase 01 Progress

## Task 1 — ADRs 0008 / 0009 / 0010 (2026-04-24)

- Commit: 497bde5
- Change: appended ADR-0008 (envoy-config crate extraction), ADR-0009 (cargo-fuzz + libfuzzer-sys as dev tooling), ADR-0010 (nightly toolchain for fuzz-only invocation) to DECISIONS.md.
- Verification: `grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md` → 10 (ADR-0001 through ADR-0010).
- Deviation: PLAN Task 1 Step 6's sed-then-amend idiom captures the pre-amend SHA (orphaned after amend). Switched to the follow-up-commit convention that PLAN Task 2 Step 6 explicitly permits, and will apply it for every remaining Phase-01 task. SHA above is now the on-branch Task-1 main commit (497bde5); the SHA-patch follow-up commit lands separately.

## Task 2 — scaffold envoy-config crate (2026-04-24)

- Commit: 16581b8
- Change: created crates/envoy-config/{Cargo.toml,src/lib.rs,src/bootstrap.rs}; added envoy-config to root workspace members.
- Verification: cargo build --workspace --all-targets → 0; cargo clippy --workspace --all-targets --all-features -- -D warnings → 0; cargo fmt --all -- --check → 0; cargo test --workspace → test result: ok. 0 passed; 0 failed (envoy_config: 0 passed, 0 failed).

## Task 3 — envoy-config bootstrap type tree (2026-04-24)

- Commit: 639075e
- Change: populated crates/envoy-config/src/bootstrap.rs with the 10-struct Bootstrap type tree (SPEC §D1) + 2 serde shape tests (parses_phase00_minimal_into_bootstrap, parses_admin_only_bootstrap). No parse_bootstrap/ConfigError yet (Task 4).
- Verification: cargo test -p envoy-config → 2 passed, 0 failed; cargo clippy -p envoy-config --all-targets --all-features -- -D warnings → 0; cargo fmt --all -- --check → 0.
- TDD evidence: Step-2 red run failed with: `error[E0425]: cannot find type 'Bootstrap' in this scope` at crates/envoy-config/src/bootstrap.rs:22:16; Step-4 post-implement → 1 passed; Step-6 full gate → 2 passed, 0 failed.

## Task 4 — parse_bootstrap + ConfigError + validate + N2 closure (2026-04-24)

- Commit: 569ec07
- Change: lib.rs rewritten with pub re-exports (Address, Admin, Bootstrap, Cluster, FilterChain, Listener, NetworkFilter, Node, SocketAddress, StaticResources), ECHO_FILTER const, ConfigError enum (4 variants: Yaml, NoRuntime, TooManyListeners, UnsupportedFilter), and parse_bootstrap entrypoint; bootstrap.rs gained pub(crate) fn validate (implementing SPEC §D1's three relaxations) + 19 new tests (14 SPEC §D1 + 5 N2 closure for deny_unknown_fields regression on StaticResources, Address, SocketAddress, FilterChain, NetworkFilter).
- Verification: cargo test -p envoy-config → 21 passed, 0 failed; cargo clippy -p envoy-config --all-targets --all-features -- -D warnings → 0; cargo fmt --all -- --check → 0; cargo test --workspace → all green.
- TDD evidence: Step-1 red run failed with `error[E0425]: cannot find function 'parse_bootstrap' in the crate root` at crates/envoy-config/src/bootstrap.rs:147:24; Step-3 post-implement (single test) → 1 passed; Step-5 full-suite (all 21 tests) → 21 passed, 0 failed.
- N2 closure: the five rejects_unknown_{static_resources,address,socket_address,filter_chain,network_filter}_field tests close phase-00 N2 (STATE.md lines 87–90).
- Deviation from PLAN Task 4 Step 4 assert_unknown_field helper: PLAN lines 773–780 specify probes `err.to_string()` and `format!("{err:#}")`, but under PLAN's own ConfigError::Yaml variant (which carries `#[error("parsing bootstrap YAML")]`), both Display and alternate-Display yield only "parsing bootstrap YAML" — the wrapped serde_yaml "unknown field" text is reachable only via Debug (`{err:?}` / `{err:#?}`) or `err.source()`. The implemented helper uses three probes (`{err:?}`, `{err}`, `{err:#?}`) and surfaces "unknown field" in all 11 deny_unknown_fields tests. PLAN defect, not code defect; delivers the PLAN's stated intent. Not ADR-worthy (single helper; no public-API impact). Subsequent tasks that call `assert_unknown_field` rely on the 3-probe version.

## Task 5 — envoy-bin consumes envoy-config (2026-04-24)

- Commit: 9324506 (main); 19ef581 (support — thiserror bump + deny.toml wildcard handling); 0eae0b8 (re-review fix — deny.toml tightened to allow-wildcard-paths idiom + this PROGRESS entry).
- Change: deleted `crates/envoy-bin/src/config.rs` (phase-00 inline parser); rewrote `crates/envoy-bin/Cargo.toml` `[dependencies]` (dropped `serde` + `serde_yaml`; added `envoy-config = { path = "../envoy-config" }`; preserved `anyhow`, `tokio` feature list, `tracing`, `tracing-subscriber`); swapped `crates/envoy-bin/src/main.rs` two lines (`mod config;` removed, `config::parse_bootstrap` → `envoy_config::parse_bootstrap`).
- Verification: `cargo build --workspace --all-targets` → 0; `cargo clippy --workspace --all-targets --all-features -- -D warnings` → 0; `cargo fmt --all -- --check` → 0; `cargo test --workspace` → envoy-bin 8 passed (6 argv + 2 echo); envoy-config 21 passed; differential 12 passed + 1 ignored (Docker-gated); `cargo deny check` → advisories ok, bans ok, licenses ok, sources ok.
- Deviation 1 — thiserror 1 → 2: the workspace already carries `thiserror 2.x` transitively. PLAN Task 2 Step 1 pinned envoy-config at `thiserror = "1"`, which introduced a `multiple-versions` cargo-deny conflict once Task 5 wired envoy-config into the main dep graph. Resolved by bumping envoy-config's direct thiserror pin to `"2"` to collapse the dep graph to a single version. API-compatible for our usage (`#[derive(thiserror::Error)]`, `#[error("...")]`, `#[from]`); all 21 envoy-config tests pass post-bump. PLAN-defect deviation; not ADR-worthy.
- Deviation 2 — deny.toml `allow-wildcard-paths` exemption: the `envoy-config = { path = "../envoy-config" }` dep is a path dependency without a version spec; cargo-deny's `wildcards = "deny"` (from ADR-0005) treats such deps as wildcard violations. Initially, commit `19ef581` relaxed the global setting to `wildcards = "warn"` — spec review flagged this as weakening the supply-chain policy ADR-0005 established. Corrected in the re-review fix commit: restored `wildcards = "deny"` and added `allow-wildcard-paths = true` to `[bans]`, cargo-deny's targeted idiom for admitting path deps without broadening the wildcard rule. `cargo deny check` still passes. Not ADR-worthy — the fix preserves ADR-0005's policy surface; it only narrows an exemption cargo-deny provides out-of-the-box for this exact case.

## Task 6 — scaffold envoy-config-fuzz subcrate (2026-04-24)

- Commit: 3d20bae
- Change: created crates/envoy-config/fuzz/{Cargo.toml,.gitignore,fuzz_targets/parse_bootstrap.rs,corpus/parse_bootstrap/minimal.yaml}; added crates/envoy-config/fuzz to root workspace exclude list.
- Verification: `cargo build --workspace --all-targets` → 0 (fuzz subcrate excluded from main workspace); `cargo test --workspace` → 41 passed + 1 ignored (unchanged since Task 5); `cargo deny check` → advisories ok, bans ok, licenses ok, sources ok.
- Seed-file note: seed is the admin-only shape (not the phase-00 typed_config shape) because NetworkFilter's deny_unknown_fields (Task 3/4) rejects upstream Envoy's typed_config block. PLAN-minor deviation per PLAN lines 1083–1084; not ADR-worthy.
- Fuzz smoke-run deferred to Task 7 CI fuzz job (PLAN Step 7 marked optional; developer toolchain has no nightly locally).

## Task 7 — CI parallel fuzz job (2026-04-24)

- Commit: 2a969a8
- Change: rewrote .github/workflows/ci.yml to define two parallel jobs: 'build' (renamed from the pre-existing 'build-test-lint' job; unchanged cargo fmt/clippy/build/test/deny sequence) and 'fuzz' (new; nightly toolchain + rust-src component + cargo-fuzz install --locked + cargo fuzz run parse_bootstrap -- -max_total_time=30 under working-directory crates/envoy-config). Concurrency group 'ci-${{ github.ref }}' with cancel-in-progress: true; permissions: contents: read; timeout-minutes: 30 for build, 15 for fuzz.
- Verification: `python3 -c 'import yaml,sys; yaml.safe_load(open(".github/workflows/ci.yml"))'` → ok; `cargo fmt --all -- --check` → 0; `cargo deny check` → advisories ok, bans ok, licenses ok, sources ok; `cargo test --workspace` → 42 passed + 1 ignored (unchanged).
- CI push-and-watch deferred to Task 19 (state-4 phase-done gate), per PLAN Step 4.

## Task 8 — ADR-0011 defer response-header equivalence (2026-04-24)

- Commit: 31c7017
- Change: appended ADR-0011 (phase 01 asserts status + body equivalence only; header-allow-list population deferred to phase 04 when the HCM response-header pipeline lands). Locks in `server: envoy-rust` divergence from upstream's `server: envoy` as tolerated until phase 04. BEHAVIOR_CONTRACT.md remains untouched (its "Header allow-list" subsection stays empty per the "populated starting phase 04" convention).
- Verification: `grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md` → 11; `grep -n '^## ADR-00' docs/envoy-rust/DECISIONS.md | tail -3` shows ADR-0009/0010/0011 in ascending line order.

## Task 9 — admin render_response + IMF-fixdate helper (2026-04-24)

- Commit: 1d04796
- Change: added crates/envoy-bin/src/admin.rs with render_response/render_response_at (hand-rolled HTTP/1.1 response framing; server: envoy-rust per ADR-0011), rfc7231_imf_fixdate (RFC 7231 IMF-fixdate formatter), and civil_from_days (Howard Hinnant's public-domain Gregorian civil date algo). 5 tests: 3 date + 2 response. Registered `mod admin;` with scoped `#[allow(dead_code)]` in main.rs (pattern carried from phase-00 Task 6; removed when Task 11 consumes admin). Added httparse = "1" to envoy-bin's [dependencies] (unused until Task 10).
- Verification: `cargo test -p envoy-bin admin::tests` → 5 passed; `cargo test -p envoy-bin` → 13 passed (6 argv + 2 echo + 5 admin); `cargo test --workspace` → 47 passed + 1 ignored; `cargo clippy -p envoy-bin --all-targets --all-features -- -D warnings` → 0; `cargo fmt --all -- --check` → 0; `cargo deny check` → advisories ok, bans ok, licenses ok, sources ok.
- PLAN-text minor: PLAN line 1519 says "11 tests passing (6 argv + 5 admin)" — undercounts by the 2 echo tests landed in phase-00 Task 7. Actual: 13. Code is correct; plan prose has a minor count error not worth a formal deviation or ADR.

## Task 10 — admin::serve accept loop + handler + drain (2026-04-24)

- Commit: 95d0584
- Change: extended crates/envoy-bin/src/admin.rs with pub(crate) async fn serve (accept loop over a TcpListener via tokio::select!, spawn each connection on a JoinSet, graceful drain on shutdown signal with 5s timeout matching phase-00's echo server) + async fn handle_one (httparse tokenize read-loop over a Vec buffer, dispatch on (method, path) → (GET, /ready) → 200 LIVE or _ → 404 Not Found, render_response via Task 9 helpers, write to stream, return OK()). Shutdown mechanism: generic Future parameter (called shutdown; tests pass oneshot channel via async move block per PLAN lines 1574-1577). Buffer cap: 8 KiB MAX_REQUEST_HEAD; 431 Request Header Fields Too Large on overflow. 5 new #[tokio::test] tests: serves_ready_live, a404s_unknown_path, a404s_non_get_ready, rejects_oversized_request_headers, drain_exits_within_budget. No new Cargo.toml deps (httparse + tokio JoinSet already in place from Tasks 5+9). All imports correct per PLAN (Future, Duration, anyhow::Result, tokio::io::{AsyncReadExt, AsyncWriteExt}, tokio::net::{TcpListener, TcpStream}, tokio::task::JoinSet, tokio::time::timeout).
- Verification: `cargo test -p envoy-bin admin::tests` → 10 passed (5 Task-9 + 5 Task-10); `cargo test -p envoy-bin` → 18 passed (6 argv + 2 echo + 10 admin); `cargo test --workspace` → 52 passed + 1 ignored; `cargo clippy -p envoy-bin --all-targets --all-features -- -D warnings` → 0; `cargo fmt --all -- --check` → 0; `cargo deny check` → advisories ok, bans ok, licenses ok, sources ok.
- TDD evidence: Step-2 red run failed with `error[E0425]: cannot find function \`serve\` in this scope` at crates/envoy-bin/src/admin.rs:125:13; Step-4 post-implement (single test) → 1 passed; Step-6 full-suite (all 18 tests) → 18 passed. rejects_oversized_request_headers initially had a race (connection-reset-by-peer on stream.read_to_end after shutdown) due to interaction with the drive helper's shutdown pattern; fixed by handling read errors gracefully in the test's custom loop instead of relying on the standard drive helper.
- Admin is still #[allow(dead_code)] in main.rs; Task 11 wires serve into run().
