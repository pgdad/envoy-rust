# Phase 42 — `42-accesslog-response-code-details` — PROGRESS

> **Lifecycle state 3 (implementation output).** Routed via `superpowers:subagent-driven-development`
> (fresh implementer subagent per task-group, controller-reviewed between; one commit per PLAN task).
> Implements the `%RESPONSE_CODE_DETAILS%` access-log command operator per the APPROVED `PLAN.md` —
> an exact mirror of the phase-41 `%ROUTE_NAME%` pattern. **Commit range `247d1e6`..`a41206a`** (on `f7d96ce`).

## Summary

The 6 TDD tasks all landed (one commit each), each via failing-test → verify-fail → minimal-impl →
verify-pass → commit. The `%RESPONSE_CODE_DETAILS%` operator renders Envoy's response-code-details string:
a `direct_response` route → `direct_response`; a proxy-success (upstream-routed) response → `via_upstream`;
every other path (error synths, filter synths) → `None` (the `-` sentinel / json `null` — §2.2 deferred,
exercised by no fixture). It is an `Option<String>`-backed operator IDENTICAL in shape to `%ROUTE_NAME%`/
`%UPSTREAM_HOST%`. **NO new crate/dependency/fuzz-target/`ConfigError` variant; ONE new `AccessLogRecord`
field; ONE new `Op` variant; the `BuildOutcome::Synth` 2-tuple change is internal.** `#![forbid(unsafe_code)]`
holds in all four touched crates.

## Per-task evidence

| Task | Commit | What landed | Test evidence |
|---|---|---|---|
| **T1** record field | `247d1e6` | `pub response_code_details: Option<String>` on `AccessLogRecord` (after `route_name`); every workspace `AccessLogRecord { … }` literal gets `response_code_details: None` (compiler-found). | `record_response_code_details_defaults_and_carries_value` RED→GREEN; `cargo build --workspace --all-targets` green. |
| **T2** parse + text render | `b588dfe` | `Op::ResponseCodeDetails` variant; `"RESPONSE_CODE_DETAILS"` no-arg keyword (rejects `(...)` AND `:N` — the §6.2 strict-no-arg grammar); `render_op` arm `…unwrap_or(empty_or_dash)`. (The `encode_single_op` arm also landed here because `encode_single_op` has no wildcard — needed to keep the commit compiling.) | `response_code_details_parses_as_no_arg_op` / `…_rejects_paren_argument` / `…_text_renders_value_or_dash` RED→GREEN. |
| **T3** json typed render | `6858cba` | The 3 json single-op tests (the `encode_single_op` `quote_opt` arm already landed in T2). | `response_code_details_single_op_present_emits_quoted_string` / `…_absent_emits_null` / `…_mixed…` RED→GREEN. |
| **T4** `BuildOutcome::Synth` + H1 | `e034db7` | `BuildOutcome::Synth(Response)` → `Synth(Response, Option<&'static str>)`; the 5 H1 construction sites tagged (`synth_direct_response`→`Some("direct_response")`, the 4 error synths `synth_501`/`synth_400`/`synth_404`×2→`None`); H1 writer-arm reads the detail into a new `response_code_details_for_log`; proxy-success arm → `Some("via_upstream")`; record build reads the local; H2 kept green via a minimal `Synth(r, _)`. | `hcm_h1_sets_response_code_details_from_response_path` RED (`d=-`)→GREEN (`d=direct_response`); each commit compiles. |
| **T5** H2 plumbing | `b0c87a0` | H2 `Synth(r, details) => { response_code_details_for_log_h2 = details.map(str::to_owned); r }`; proxy-success arm → `Some("via_upstream")`; the `response_code_details_for_log_h2` parameter threaded into `finalize_h2_stream` (mirror `route_name_for_log_h2`); record build reads it. | `hcm_h2_sets_response_code_details_from_response_path` RED→GREEN. |
| **T6** fixture + seed + contract | `a41206a` | Fixture `0050-accesslog-response-code-details` (clone of `0049`; a `direct_response` route + a `json_format` with `%RESPONSE_CODE_DETAILS%`) + the differential test `access_log_response_code_details.rs` + a `parse_bootstrap` fuzz seed (`response_code_details.yaml` + `!`-un-ignore line, `git ls-files`-confirmed tracked) + the BEHAVIOR_CONTRACT row. | **The differential fixture `0050` ran GREEN against live `envoyproxy/envoy:v1.33.0`** — the byte-exact line `{"method":"GET","proto":"HTTP/1.1","rcd":"d=direct_response","single_rcd":"direct_response"}` matched on both sides (live-captured; `envoy-bin` rebuilt before the run). |

## Local verification (state-3 close; the AUTHORITATIVE §7.5 gate runs at state-4)

- `cargo build --workspace` — green.
- `cargo test -p envoy-accesslog` — **91 passed, 0 failed**.
- `cargo test -p envoy-http1` — **136 passed, 0 failed**.
- `cargo test -p envoy-http2` — **75 passed, 1 ignored, + 1 host-flake** — the ONE failing test
  `client::tests::send_request_maps_h2_handshake_failure_to_typed_error` is the **documented pre-existing
  host-flake** (the h2 handshake unexpectedly succeeds on this host's networking; CI-authoritative; NOT a
  regression — phase 42 does not touch the H2 client handshake path).
- `cargo test -p differential --test access_log_response_code_details` — **1 passed** (Docker-gated; byte-exact
  cross-proxy match).
- `#![forbid(unsafe_code)]` intact in `envoy-accesslog`/`envoy-http1`/`envoy-http2`/`envoy-config`; NO
  `Cargo.toml`/`Cargo.lock` change.

## Deferred to the state-4 §7.5 verification gate (per project discipline)

`cargo fmt --check`, `cargo clippy`, `cargo deny check`, the full differential suite `0001`-`0050`
simultaneously, h2spec, and the fuzz run are the state-4 gate (a)-(e) — NOT run at state-3. The single local
`0050` differential pass + the green per-crate tests are state-3 evidence only; the AUTHORITATIVE evidence is
the Linux CI run quoted at state-4.

## §7.5 Verification gate (state-4) — GREEN on AUTHORITATIVE Linux CI

**AUTHORITATIVE run: `28301067467` @ `344dbd6` — `completed/success`**
(https://github.com/pgdad/envoy-rust/actions/runs/28301067467). Both CI jobs green:
- **`build + test + lint`** → success — covers gates **(a)** fixture `0050-accesslog-response-code-details`
  green (the byte-exact `direct_response` line) + **(b)** all `0001`-`0049` green SIMULTANEOUSLY
  (default-absent byte-preservation — the new `response_code_details` field defaults `None` + the operator is
  new) + **(c)** h2spec ≥95% (NO HTTP/2 codec change) + **(e)** `cargo build`/`clippy`/`fmt --check`/`test`/
  `deny` clean.
- **`fuzz (parse_bootstrap + jwt_parse + cdn_loop_parse + accesslog_format_parse, 30s each)`** → success —
  covers gate **(d)** the fuzz targets are clean WITH the new `response_code_details.yaml` `parse_bootstrap`
  seed; NO new fuzz target.

**State-4 gate-fix commit:** `344dbd6` (`style: cargo fmt the %RESPONSE_CODE_DETAILS% no-arg keyword list`) —
the state-3 implementer added `"RESPONSE_CODE_DETAILS"` to the no-arg keyword match arm, widening it past the
line limit; `cargo fmt` reformats the list one keyword per line (pure formatting, NO behavior change). This is
the documented "State-4 = CI's first real execution" red-at-fmt — caught + fixed at THIS gate per project
discipline (cargo-fmt-check first runs at state-4). The superseded implementation-HEAD run (`5decdbe`) was
auto-cancelled by the `344dbd6` push.

**§7.5 (a)-(e) = GREEN.** (f) `REVIEW.md` is the state-5 code-review (the SESSION AFTER). The
`client::tests::…h2_handshake…` host-flake that false-REDs locally is GREEN on this Linux CI run (it is
CI-authoritative — not a regression; phase 42 does not touch the H2 client handshake path).

## Carry-forwards (NONE blocks)
M39-1/M39-2 + M38-1/M38-2 (adjacent — the encoder surface was touched but they were NOT folded this phase;
they stay live) + CF-39-1 + M40-1 + M37-*/M36-*/M34-*/M33-* + older. Phase 42 does not touch `rbac.rs`.

_State-3 implementation COMPLETE; state-4 §7.5 (a)-(e) gate GREEN on AUTHORITATIVE CI `28301067467` @ `344dbd6`.
The next session is the state-5 code-review (`superpowers:requesting-code-review`)._
