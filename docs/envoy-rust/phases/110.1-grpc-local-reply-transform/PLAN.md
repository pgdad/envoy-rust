# Sub-phase 110.1 — gRPC-aware local replies over HTTP/1.1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every task is TDD per `superpowers:test-driven-development` — the failing test comes first, no exceptions (doctrine D-3.1).

**Goal:** Make envoy-rust rewrite every **locally generated** HTTP/1.1 response into upstream Envoy's gRPC shape (status `200`, `content-type: application/grpc`, empty body, `content-length: 0`, `grpc-status`, conditional `grpc-message`) whenever the request that provoked it carried a gRPC `content-type` — proven **entirely in-process**, with no differential fixture.

**Architecture:** Three pure total functions (`is_grpc_request`, `http_to_grpc_status`, `grpc_message_encode`) plus one transform (`apply_grpc_local_reply`) live in a new private module `crates/envoy-http1/src/grpc.rs`. The transform is installed at the **two HTTP/1.1 wire funnels** — `serve_connection` in `hcm.rs` (tokio) and `write_owned` in `uring.rs` (io_uring worker) — gated on a "this response was locally generated" bit. It is **never** installed in `synth_with`, in any `synth_*` wrapper, or in `build_response`/`build_response_in`, because those are shared with HTTP/2 (see **Global Constraint 1**).

**Tech Stack:** Rust 2024 edition, `bytes::Bytes`, `tokio`. **No new dependency**, no `Cargo.toml` change, no `Cargo.lock` change.

**Spec:** `docs/envoy-rust/phases/110.1-grpc-local-reply-transform/SPEC.md` (read it alongside this plan). Parent background: `docs/envoy-rust/phases/110-grpc-aware-local-replies/SPEC.md` (LANDED, UNEDITABLE).

---

## Global Constraints

Every task's requirements implicitly include this section.

1. **The transform MUST NOT be installed in `synth_with`, in any `synth_*` wrapper, or in `build_response` / `build_response_in`.** RE-MEASURED at this PLAN-write on disk: `crates/envoy-http2/src/hcm.rs:18` imports `build_response` from `envoy_http1`, and `crates/envoy-http2/src/hcm.rs:513-518` calls it:
   ```rust
   let request_path = match decode_decision {
       envoy_filter::Decision::Continue => {
           H2RequestPath::Match(build_response(
               &config.inner,
               &mut envoy_req,
               /* close = */ false,
           ))
       }
   ```
   `crates/envoy-http1/src/hcm.rs:2126-2128` documents the sharing outright: *"ONE arm serves BOTH codecs — H2 has no route-action dispatch of its own and calls this function."* A transform placed at or below `build_response` would rewrite H2's **route-decision** replies (direct_response / 400 / 404 / redirect) while leaving H2's own `synth_h2_*` **upstream-failure** family untouched — a partially-covered family on the H2 wire, exactly the ADR-0049 silent-divergence class. **HTTP/2 is out of scope for this sub-phase (CF-110-1) and must be left byte-for-byte unchanged.** Task 8 is the positive witness of that.

2. **NO new config surface.** No new field, no new validator, no new `ConfigError` variant, no `deny_unknown_fields` arm. Consequently **no new fuzz target** and no new `parse_bootstrap` corpus seed (§7.4's trigger does not fire). If a task believes a config surface is unavoidable, STOP — the scope is wrong.

3. **NO differential fixture.** Fixture `0089` belongs to sibling `110.2`. Do not create `tests/fixtures/0089*`. The fixture-directory census must still read **88** at this sub-phase's close.

4. **NO trailer API of any kind.** Every header in this surface rides on a bodiless (`content-length: 0`) reply, so no trailer section exists. If a task finds itself needing to read, forward or emit a trailer, the scope is wrong.

5. **Proxied (upstream-originated) responses must NOT be transformed** — that is CF-110-2. Only locally generated replies are in scope. The `outgoing_local` bit in Task 5 is what enforces this, and Task 6 Step 7 is its negative witness.

6. **Do NOT fix any banked finding.** The `109.2` REVIEW's M-1…M-8 + N-1…N-11, the `109.1` M-5 + N-1…N-6 set, the `108.2` M-2 + N-1…N-6 set, CF-110-1/2/3, CF-109-1/2/3, CF-108-1/2/3, CF-76-1, CF-75-2/3/4/5/6, CF-72-2/CF-75-1, M71-6, CF-74-1/2/3/4/6, CF-73-1 and the HTTP-filters-family (1)-(4) are all carried unchanged (§6.3; ADR-0165).

7. **`#![forbid(unsafe_code)]` is already at `crates/envoy-http1/src/lib.rs:1` and must stay** (D-3.8). Nothing in this plan needs `unsafe`.

8. **The `uring` module is feature-gated**: `crates/envoy-http1/src/lib.rs:24-25` reads
   ```rust
   #[cfg(all(feature = "uring", target_os = "linux"))]
   pub mod uring; // EXPERIMENTAL io_uring data-plane worker (perf prototype).
   ```
   with the feature declared at `crates/envoy-http1/Cargo.toml:27-28`. A plain `cargo test --workspace` does **not** compile it. **`cargo clippy --workspace --all-targets --all-features -- -D warnings` DOES**, and is the gate that catches a broken uring seam. Task 7 must run that command explicitly.

9. **Exact measured upstream contract** (re-probed at this PLAN-write against `ENVOY_TARGET.md`'s pinned `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2` verified by `docker image inspect` BEFORE any probe; all probe containers torn down after, `docker ps` = 0 running):
   - **Mapping:** `400→13`, `401→16`, `403→7`, `404→12`, `429→14`, `502→14`, `503→14`, `504→14`; **every other status → 2**, including all 2xx/3xx and `405`, `408`, `409`, `412`, `413`, `499`, `500`, `501`.
   - **Detection:** fires iff the request `content-type` value is **exactly** `application/grpc` or **begins with** `application/grpc+`. Byte-exact and CASE-SENSITIVE on the VALUE; header-NAME lookup stays case-insensitive.
   - **Encoding:** a byte passes through unchanged iff it is in `0x20..=0x7D` **and** is not `%` (0x25); every other byte becomes `%` + TWO UPPERCASE hex digits, UTF-8 encoded PER BYTE.
   - **Header order:** pass-through headers first (original relative order), then `content-type`, `grpc-status`, `[grpc-message]`, then `date`, `server`, `connection`, then `content-length: 0`.

---

## What was re-measured at this PLAN-write (W-1…W-7 all re-confirmed FRESH)

Everything below is a MEASUREMENT taken this session, not an inherited claim. Later tasks cite these.

### W-1 — the mapping matrix: RECONFIRMED, all 20 cells

Raw-socket HTTP/1.1 client, one connection per probe, `connection: close`, no header-dict (a dict client destroys response header ORDER and CASE). One `direct_response` route per status at its own distinct path with body `B<status>`, each probed WITH and WITHOUT `content-type: application/grpc`.

| cfg status | gRPC → | `grpc-status` | `grpc-message` | control |
|---|---|---|---|---|
| 200 | 200 | 2 | `B200` | 200, body `B200` |
| 201 | 200 | 2 | `B201` | 201 |
| 204 | 200 | 2 | `B204` | 204 |
| 301 | 200 | 2 | `B301` | 301 |
| **400** | 200 | **13** | `B400` | 400 |
| **401** | 200 | **16** | `B401` | 401 |
| **403** | 200 | **7** | `B403` | 403 |
| **404** | 200 | **12** | `B404` | 404 |
| 405 | 200 | 2 | `B405` | 405 |
| 408 | 200 | 2 | `B408` | 408 |
| 409 | 200 | 2 | `B409` | 409 |
| 412 | 200 | 2 | `B412` | 412 |
| 413 | 200 | 2 | `B413` | 413 |
| **429** | 200 | **14** | `B429` | 429 |
| 499 | 200 | 2 | `B499` | 499 |
| 500 | 200 | 2 | `B500` | 500 |
| 501 | 200 | 2 | `B501` | 501 |
| **502** | 200 | **14** | `B502` | 502 |
| **503** | 200 | **14** | `B503` | 503 |
| **504** | 200 | **14** | `B504` | 504 |

Every gRPC cell also returned `content-type: application/grpc`, `content-length: 0` and an EMPTY body. The empty-body `direct_response` and the HCM's own unmatched-path 404 both returned `grpc-status: 12` with **no `grpc-message` header at all** (absent, not empty).

### W-1 (cont.) — the detection matrix: RECONFIRMED, all 14 cells

| request `content-type` | detected |
|---|---|
| `application/grpc` | **YES** |
| `application/grpc+proto` | **YES** |
| `application/grpc+json` | **YES** |
| `application/grpc+` (bare) | **YES** |
| `application/grpc ` (TRAILING SPACE) | **YES** — codec OWS-strip, **not** a matcher tolerance |
| `application/grpc; charset=utf-8` | NO |
| `application/grpc;charset=utf-8` | NO |
| `APPLICATION/GRPC` | NO |
| `Application/Grpc` | NO |
| `application/grpc-web` | NO |
| `application/grpc-web+proto` | NO |
| `application/grpcfoo` | NO |
| `application/json` | NO |
| *(absent)* | NO |

Also re-measured: METHOD-INSENSITIVE (`GET`/`POST`/`PUT`/`DELETE` identical) and INDEPENDENT of `te: trailers` in both directions.

> **Do NOT build trailing-space tolerance into the comparison.** The trailing-space cell is the HTTP codec stripping optional whitespace from the field value before anything sees it. Rely on the codec's existing OWS handling exactly as every other header comparison in the tree does.

### W-5 — the encoder rule: RECONFIRMED on 9 bodies

Bodies supplied via `inline_bytes` (base64) so the source bytes are exact; each probed WITH and WITHOUT the gRPC content-type so the control gives the byte-exact original.

| source bytes (hex) | control body | `grpc-message` |
|---|---|---|
| `61 20 62 0a 63 6f 6e 74 72 6f 6c 09 74 61 62 20 c3 a9 20 25 32 35 20 65 6e 64` | `a b\ncontrol\ttab é %25 end` | `a b%0Acontrol%09tab %C3%A9 %2525 end` |
| `71 22 62 20 73 5c 6c 20 74 7e 74 20 64 7f 64` | `q"b s\l t~t d\x7fd` | `q"b s\l t%7Et d%7Fd` |
| `20 20 7e 20 2b 2c 2f 3a 3b 3d 3f 40 5b 5d 7b 7d 7c 5e 60 3c 3e 23 26 2a 28 29` | `  ~ +,/:;=?@[]{}|^` + backtick + `<>#&*()` | `%7E +,/:;=?@[]{}|^` + backtick + `<>#&*()` |
| `7e` | `~` | `%7E` |
| `7f` | `\x7f` | `%7F` |
| `25 32 35` | `%25` | `%2525` |
| `22 5c` | `"\` | `"\` |
| `7d 7e` | `}~` | `}%7E` |
| `1f 20` | `\x1f ` | `%1F ` |

A reference encoder implementing **"pass through iff `0x20..=0x7D` and not `%`; else `%` + two UPPERCASE hex digits"** predicted all nine measured strings EXACTLY. The discriminating cells are `~`→`%7E` (the parent SPEC's rule was WRONG here and said `~` passes through), `}`→passes (`0x7D` is the true upper bound), `0x7F`→`%7F`, and `%25`→`%2525`.

### W-2 — the seam census, re-derived BY TEXT on disk

- `#[cfg(test)] mod tests` in `crates/envoy-http1/src/hcm.rs` begins at **line 2532** and runs to EOF (11205). **Every hit at line ≥ 2532 is test-only.**
- `grep -rn 'synth_with' crates/` returns **exactly 7** hits: 1 definition (`hcm.rs:2239`), **4 CALLS** (`:2262` in `synth_direct_response`, `:2270` in `synth_status`, `:2410` in `synth_no_healthy_upstream`, `:2425` in `synth_overflow`), and 2 doc mentions (`:2378`, `:11103`). `synth_400`/`synth_404`/`synth_501` are depth-2 wrappers over `synth_status`. **Confirms the SPEC's CORRECTION 1; the inherited "seven callers" figure is wrong.**
- **Tokio funnel — exactly as claimed.** `crates/envoy-http1/src/hcm.rs:1457` is `if outgoing_direct {` and `:1468` is
  ```rust
  Http1Response::write_to_buf(&outgoing, &mut downstream, &mut write_buf).await?;
  ```
- **io_uring funnel — exactly 4 `write_owned` sites**, at `crates/envoy-http1/src/uring.rs:292`, `:313`, `:338`, `:389`. The **proxied** path uses a DIFFERENT writer, `write_head_body` at `uring.rs:376`. So in the uring worker `write_owned` is *exclusively* the local-reply funnel — a fact Task 7 exploits.
- `write_owned` is defined at `uring.rs:503`; its own doc calls its argument *"an owned synth `Response`"*.

### W-3 — the H1/H2 sharing edge: CONFIRMED (see Global Constraint 1)

`grep -rn 'fn build_response' crates/envoy-http2/` returns **0** — H2 defines none of its own; it uses H1's. Confirmed independently on disk by this session, not taken from the SPEC.

### W-6 — blast radius and size

- **Blast radius ZERO.** 88 fixture directories (`git ls-files 'tests/fixtures/**' | cut -d/ -f3 | sort -u | wc -l` = 88; highest `0088-runtime-fraction-route-gating`). Across ALL of `tests/` there is **not one** occurrence of `application/grpc`, `grpc-status`, `grpc-message` or `te: trailers`. The only `grpc` presence in the whole test tree is fixture `0075-upstream-grpc-health-check` and its driver, which is *upstream health-check config* — the proxy acting as a gRPC CLIENT toward a backend, never a downstream gRPC request. **No existing test or fixture can go RED from this sub-phase.**
- `grep -rn "grpc" crates/envoy-http1/src/` returns **ZERO hits** — the crate that owns local replies has no gRPC awareness at all today.
- **No percent-encoder exists** anywhere in `crates/` (`%02X`, `%02x`, `pct_encode`, `percent_encode`, `urlencode`, `form_urlencoded` all return nothing; the only `{:02` hits are zero-padded *decimal* date formatting at `crates/envoy-http1/src/date.rs:138` and `crates/envoy-accesslog/src/default_format.rs:110`). No workspace `Cargo.toml` declares `percent-encoding` or `urlencoding`. **The encoder is written from scratch — do NOT add a dependency for it** (Global Constraint 2's spirit; a new direct dep would need its own ADR).

### W-7 — the `Response` type and the serializer

`crates/envoy-http1/src/response.rs:12-18` — **four** fields (the inherited claim named only two):
```rust
pub struct Response {
    pub status: u16,                  // 100..=599
    pub reason: Option<&'static str>, // canonical reason per RFC 7231 §6.1;
    //   None falls back to a built-in table.
    pub headers: Vec<(String, String)>, // emission-order preserving.
    pub body: Bytes,                    // CL-framed in 04.1; chunked deferred.
}
```
`serialize_response_head` (`response.rs:98`) emits headers with a plain `for (name, value) in &resp.headers` at `response.rs:121` — **vector order, verbatim, no sorting, no case folding, no dedupe**. Position in the vector **is** wire order. Setting `reason: None` makes the writer fall back to `canonical_reason(200)` = `OK`, producing `HTTP/1.1 200 OK`.

### FOUR NEW MEASUREMENTS this session that no SPEC states — each is load-bearing

**N-1 — a filter-generated local reply IS transformed.** Probed with an `envoy.filters.http.rbac` ALLOW-with-no-policies chain (denies everything):

| | result |
|---|---|
| gRPC request | `200`, `content-type: application/grpc`, `grpc-status: 7`, `grpc-message: RBAC: access denied`, `content-length: 0`, empty body |
| control | `403`, `content-length: 19`, `content-type: text/plain`, body `RBAC: access denied` |

This settles the two `serve_connection` writer arms (`SynthFromDecode` at `hcm.rs:1391` and the encode-side `StopAndSend` replacement at `hcm.rs:1423`) that `110.1/SPEC.md` §1.5's family list does **not** mention. They are local replies and they ARE transformed. Task 5's `outgoing_local` bit covers them by construction.

**N-2 — the access log and the per-class stats record the TRANSFORMED status, not the original.** Probed with a `FileAccessLog` to `/dev/stdout` read back via `docker logs`, plus the admin `/stats` endpoint:

```
ALOG path=/g404 rc=200 rcd=direct_response bytes_sent=0 flags=-
ALOG path=/g404 rc=404 rcd=direct_response bytes_sent=4 flags=-
ALOG path=/g503 rc=200 rcd=direct_response bytes_sent=0 flags=-
ALOG path=/g503 rc=503 rcd=direct_response bytes_sent=4 flags=-
```
and after those 4 requests: `http.ingress.downstream_rq_2xx: 2`, `downstream_rq_4xx: 1`, `downstream_rq_5xx: 1`, `downstream_rq_completed: 4`.

**Therefore the transform MUST be applied BEFORE `crates/envoy-http1/src/hcm.rs:1447-1448`**, where `response_status_for_log` and `response_body_len` are derived and from which both the access-log record and the per-class counter dispatch at `:1480` are driven. Placing it only at the wire write (`:1457`/`:1468`) would log `404` and tick `downstream_rq_4xx` where upstream logs `200` and ticks `downstream_rq_2xx` — a silent access-log AND stats divergence. Note `%RESPONSE_CODE_DETAILS%` stays `direct_response` (UNCHANGED) and `%BYTES_SENT%` becomes `0`.

**N-3 — pass-through headers survive in their ORIGINAL position, and this generalizes beyond `location`.** A circuit-breaker overflow (`max_connections: 0`, `max_pending_requests: 0`) produced:

| | headers, in wire order |
|---|---|
| gRPC | `x-envoy-overloaded`, `content-type`, `grpc-status`, `grpc-message`, `date`, `server`, `connection`, `content-length` |
| control | `x-envoy-overloaded`, `content-length`, `content-type`, `date`, `server`, `connection` |

with `grpc-message: upstream connect error or disconnect/reset before headers. reset reason: overflow`.

**N-4 — the order rule, generalized and verified on THREE independent cases.** The single rule
> *pass-through headers (everything that is not `content-type`, `content-length`, `date`, `server`, `connection`) in original relative order; then `content-type: application/grpc`; then `grpc-status`; then `grpc-message` if the original body was non-empty; then `date`, `server`, `connection`; then `content-length: 0`*

reproduces all three measured orders EXACTLY:

| case | measured gRPC order |
|---|---|
| bodied `direct_response` 503 | `content-type, grpc-status, grpc-message, date, server, connection, content-length` |
| `redirect:` route | `location, content-type, grpc-status, date, server, connection, content-length` |
| circuit-breaker overflow | `x-envoy-overloaded, content-type, grpc-status, grpc-message, date, server, connection, content-length` |

This is strictly more precise than `110.1/SPEC.md` §1.4's `[location,] content-type, …`, which only covered `location`.

> **Order is a UNIT-TEST concern, not a differential one.** `run_http1_probe_list_arm`'s `diff_headers` builds a `BTreeSet` of LOWER-CASED header NAMES, compares the sets, then compares VALUES for every name outside the 3-entry `HEADER_ALLOW_LIST` (`server`, `date`, `x-envoy-upstream-service-time`). **Order is never read.** Matching upstream's order is good house practice and IS pinned by the tests in this plan, but a wrong order fails a unit test, not a fixture.

### Two measured, PRE-EXISTING, gRPC-ORTHOGONAL divergences — do NOT fix them here

- **CF-110-3 (already banked):** upstream emits `location: <scheme>://<authority><path>` on a `direct_response` whose status is `201` or `3xx`, in BOTH directions; envoy-rust's `synth_direct_response` does not.
- **NEW, bank as CF-110-4:** upstream's own NON-gRPC local-reply header order is `[pass-through,] content-length, content-type, date, server, connection` (measured on the `no healthy upstream` 503 and the overflow 503), whereas envoy-rust's `synth_with` (`hcm.rs:2243-2255`) emits `server, date, content-length, content-type, connection`. This is a pre-existing ORDER-only divergence, invisible to `diff_headers`, and it is **not** caused or fixed by this sub-phase. Record it; do not touch `synth_with`.

### §6.1 SPLIT GATE — re-derived bottom-up. **IT DOES NOT FIRE.**

Calibration re-measured this session from landed phases with `git diff --numstat <base> <last-impl-commit> -- . ':(exclude)docs/'`, read as `added − deleted`:

| sub-phase | base | last impl | added | deleted | **net** | its PLAN projected | error |
|---|---|---|---|---|---|---|---|
| `108.1` | `879978f` | `1829793` | 1133 | 5 | **1128** | ≈1215 | −7% |
| `108.2` | `d1760b0` | `cb0c398` | 855 | 1 | **854** | ≈905 | −6% |
| `109.1` | `a460b38` | `9a7e7f8` | 1880 | 154 | **1726** | ≈1180 | **+46%** |
| `109.2` | `e458765` | `8644fa4` | 567 | 5 | **562** | ≈745 | −25% |

Median ≈ **991**. All four figures and all eight PLAN/SPEC projection figures confirmed. `109.1` is the sole overrun, and its own successor names the cause: mechanical call-site fan-out, priced at ~3× the naive one-line-per-site estimate.

This plan's bottom-up estimate:

| bucket | non-test | test |
|---|---|---|
| T1–T4: `grpc.rs` (module doc, 3 pure fns, the transform, constants) | ≈150 | ≈330 |
| T1: `headers.rs` constants + `lib.rs` `mod` line | ≈8 | — |
| T5–T6: tokio seam (`outgoing_local`, the funnel call, comments) | ≈20 | ≈335 |
| T7: io_uring seam (`write_owned` signature + 4 call sites) | ≈14 | ≈5 |
| T8: H2 negative witness | — | ≈50 |
| **Total** | **≈192** | **≈720** |

**Central estimate ≈ 912 net LoC over 8 tasks.** Applying the worst measured PLAN overrun (+46%) gives **≈1332** — still under the ~1500 gate with ~11% headroom. The fan-out risk that drove `109.1`'s overrun is small here: 4 one-line uring call sites and one insertion point in `serve_connection`. Honest planning range **820–1330**.

**8 tasks ≪ ~25, and ≈912 (worst case ≈1332) < ~1500. §6.1 does NOT fire. No split. ADR-0179 stays UNRESERVED and is not fired by this plan.**

---

## File Structure

| file | disposition | responsibility |
|---|---|---|
| `crates/envoy-http1/src/grpc.rs` | **CREATE** | The whole gRPC local-reply surface: detection, status mapping, `grpc-message` percent-encoding, and the response transform. Private to the crate (`pub(crate) mod`) so `envoy-http2` cannot reach it — that privacy is itself part of Global Constraint 1. All unit tests for the pure functions live inline in its `#[cfg(test)] mod tests`. |
| `crates/envoy-http1/src/lib.rs` | modify (1 line + comment) | Declare `pub(crate) mod grpc;`. **No `pub use` re-export** — nothing outside the crate may call it. |
| `crates/envoy-http1/src/headers.rs` | modify (~5 lines) | Add the three header-name / value constants (`GRPC_STATUS`, `GRPC_MESSAGE`, `GRPC_CONTENT_TYPE`) beside the existing ones, so the transform never spells a header name inline. |
| `crates/envoy-http1/src/hcm.rs` | modify (~20 non-test lines, ~335 test lines) | The tokio seam inside `serve_connection`: the `outgoing_local` bit and the single transform call. Plus the seam-coverage tests in the existing `#[cfg(test)] mod tests`. |
| `crates/envoy-http1/src/uring.rs` | modify (~14 lines) | The io_uring seam: `write_owned` gains the request headers and applies the transform inside the funnel, so no call site can forget it. |
| `crates/envoy-http2/src/hcm.rs` | modify (test only, ~50 lines) | The W-4 negative witness proving H2 is untouched. |

Nothing else is touched. **No `Cargo.toml`, no `Cargo.lock`, no `tests/`, no `BEHAVIOR_CONTRACT.md`** (the `## gRPC` contract section belongs to `110.2`), no `ci.yml`, no `deny.toml`.

---

## Task ordering note

Tasks 1–4 are pure, dependency-free and independently reviewable — they build `grpc.rs` bottom-up so each has its own red→green cycle. Task 4's `apply_grpc_local_reply` is the interface every later task consumes. Tasks 5 and 6 install and then exhaustively witness the tokio seam. Task 7 does the io_uring seam (feature-gated, so it needs its own `--all-features` gate run). Task 8 proves the constraint that shaped the whole design and closes the state-3 exit gate.

**Tasks 5, 6 and 7 all touch `crates/envoy-http1/`; 5 and 6 both touch `hcm.rs`. They are NOT disjoint — do not parallelize them across worktrees.** Task 8 touches `crates/envoy-http2/src/hcm.rs` only and is genuinely independent of 5–7 once Task 4 has landed.

---

### Task 1: The `grpc` module and `is_grpc_request`

**Files:**
- Create: `crates/envoy-http1/src/grpc.rs`
- Modify: `crates/envoy-http1/src/lib.rs` (add the `mod` declaration beside the existing ones at lines 14–25)
- Modify: `crates/envoy-http1/src/headers.rs` (append constants after the existing `pub const LOCATION` at line 14)
- Test: inline `#[cfg(test)] mod tests` in `crates/envoy-http1/src/grpc.rs`

**Interfaces:**
- Consumes: `crate::headers::{find_header, CONTENT_TYPE}` — `find_header` is `pub fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str>` (`headers.rs:18`) and matches the header NAME case-insensitively via `eq_ignore_ascii_case`.
- Produces: `pub(crate) fn is_grpc_request(headers: &[(String, String)]) -> bool`; `pub const GRPC_STATUS: &str`, `GRPC_MESSAGE: &str`, `GRPC_CONTENT_TYPE: &str` in `headers.rs`.

- [ ] **Step 1: Write the failing test**

Create `crates/envoy-http1/src/grpc.rs` containing ONLY the test module for now:

```rust
//! gRPC-aware local replies over HTTP/1.1 (sub-phase 110.1).
//!
//! Upstream Envoy rewrites any LOCALLY GENERATED reply when the request that
//! provoked it carried a gRPC `content-type`: the HTTP status becomes `200`,
//! `content-type` becomes `application/grpc`, the body is DROPPED,
//! `content-length` becomes `0`, a `grpc-status` header carries a mapped code,
//! and — only when the original body was non-empty — a `grpc-message` header
//! carries that body percent-encoded.
//!
//! Every rule in this module was MEASURED against the `ENVOY_TARGET.md`-pinned
//! `envoyproxy/envoy:v1.33.0` at the 110.1 PLAN-write; the matrices are
//! tabulated in `docs/envoy-rust/phases/110.1-grpc-local-reply-transform/PLAN.md`.
//!
//! This module is `pub(crate)` ON PURPOSE. `envoy-http2` calls
//! `envoy_http1::build_response` (`crates/envoy-http2/src/hcm.rs:513-518`), so
//! anything reachable from the shared route-decision path would also rewrite
//! HTTP/2 responses while missing H2's own `synth_h2_*` upstream-failure
//! family — a partially-covered family on the H2 wire (the ADR-0049
//! silent-divergence class). HTTP/2 is CF-110-1 and stays out of scope.

#[cfg(test)]
mod tests {
    use super::*;

    fn hdrs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(n, v)| ((*n).to_string(), (*v).to_string()))
            .collect()
    }

    /// The MEASURED detection matrix, all 14 cells, from the 110.1 PLAN-write
    /// probe against the pinned image. Detection fires iff the `content-type`
    /// value is EXACTLY `application/grpc` or BEGINS WITH `application/grpc+`.
    ///
    /// Two traps live here and both are directly witnessed below: a naive
    /// `starts_with("application/grpc")` wrongly accepts `application/grpcfoo`
    /// and `application/grpc-web`; a case-insensitive or parameter-tolerant
    /// match wrongly accepts `APPLICATION/GRPC` and
    /// `application/grpc; charset=utf-8`.
    #[test]
    fn detection_matrix_matches_upstream() {
        let cells: &[(&str, bool)] = &[
            ("application/grpc", true),
            ("application/grpc+proto", true),
            ("application/grpc+json", true),
            ("application/grpc+", true),
            ("application/grpc; charset=utf-8", false),
            ("application/grpc;charset=utf-8", false),
            ("APPLICATION/GRPC", false),
            ("Application/Grpc", false),
            ("application/grpc-web", false),
            ("application/grpc-web+proto", false),
            ("application/grpcfoo", false),
            ("application/json", false),
            ("", false),
        ];
        for (value, expected) in cells {
            assert_eq!(
                is_grpc_request(&hdrs(&[("content-type", value)])),
                *expected,
                "content-type {value:?} must detect as {expected}"
            );
        }
    }

    /// An ABSENT `content-type` is the 14th measured cell and is NOT detected.
    #[test]
    fn absent_content_type_is_not_grpc() {
        assert!(!is_grpc_request(&hdrs(&[("host", "x")])));
        assert!(!is_grpc_request(&[]));
    }

    /// Header-NAME lookup stays case-insensitive (as everywhere else in the
    /// tree, via `find_header`'s `eq_ignore_ascii_case`) even though the VALUE
    /// comparison is byte-exact.
    #[test]
    fn header_name_lookup_is_case_insensitive() {
        assert!(is_grpc_request(&hdrs(&[("Content-Type", "application/grpc")])));
        assert!(is_grpc_request(&hdrs(&[("CONTENT-TYPE", "application/grpc")])));
    }

    /// MEASURED: `application/grpc ` WITH a trailing space IS detected upstream
    /// — but that is the HTTP codec stripping optional trailing whitespace
    /// (OWS) from the field value before anything sees it, NOT a tolerance in
    /// the matcher. This test pins that we deliberately do NOT build
    /// trailing-space tolerance into the comparison: by the time a value
    /// reaches here the codec has already trimmed it, so an UNTRIMMED value
    /// with a trailing space must NOT match.
    #[test]
    fn trailing_space_tolerance_is_deliberately_absent() {
        assert!(!is_grpc_request(&hdrs(&[("content-type", "application/grpc ")])));
    }

    /// First-match-wins: `find_header` returns the first matching name.
    #[test]
    fn first_content_type_wins() {
        assert!(is_grpc_request(&hdrs(&[
            ("content-type", "application/grpc"),
            ("content-type", "application/json"),
        ])));
        assert!(!is_grpc_request(&hdrs(&[
            ("content-type", "application/json"),
            ("content-type", "application/grpc"),
        ])));
    }
}
```

Add to `crates/envoy-http1/src/lib.rs`, immediately after the `pub mod date;` line (keeping the existing alphabetical-ish grouping):

```rust
// 110.1: gRPC-aware local replies. DELIBERATELY `pub(crate)` — see the module
// doc. Nothing outside this crate may reach it, because `envoy-http2` shares
// this crate's `build_response` and must stay untransformed (CF-110-1).
pub(crate) mod grpc;
```

Add to the end of the constant block in `crates/envoy-http1/src/headers.rs` (after `pub const LOCATION: &str = "location";` at line 14):

```rust
/// 110.1: the gRPC local-reply response headers and content-type value.
/// MEASURED against `envoyproxy/envoy:v1.33.0` — lower-case on the wire.
pub const GRPC_STATUS: &str = "grpc-status";
pub const GRPC_MESSAGE: &str = "grpc-message";
pub const GRPC_CONTENT_TYPE: &str = "application/grpc";
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p envoy-http1 --lib grpc:: 2>&1 | tail -40`

Expected: **compile FAILURE**, `cannot find function 'is_grpc_request' in this scope`.

> **A compile error is not a mutation RED, but it IS the correct red for a brand-new function.** What must NOT happen here is a *clean* run: `0 passed; N filtered out` is a FALSE GREEN. Assert that the `test result:` line either is absent (compile error) or shows a non-zero count.

- [ ] **Step 3: Write the minimal implementation**

Insert into `crates/envoy-http1/src/grpc.rs`, above the `#[cfg(test)] mod tests`:

```rust
use crate::headers;

/// The `content-type` value that, alone, marks a gRPC request.
const GRPC_EXACT: &str = "application/grpc";
/// The prefix form: anything after `+` (including nothing) still counts.
const GRPC_PLUS_PREFIX: &str = "application/grpc+";

/// Does this request carry a gRPC `content-type`?
///
/// MEASURED rule (all 14 cells probed against `envoyproxy/envoy:v1.33.0`):
/// true iff the `content-type` value is EXACTLY `application/grpc` or BEGINS
/// WITH `application/grpc+`. Nothing else — a parameter (`; charset=utf-8`)
/// DEFEATS it, the match is CASE-SENSITIVE on the value, and
/// `application/grpc-web`, `application/grpc-web+proto` and
/// `application/grpcfoo` are all NEGATIVE.
///
/// The header NAME lookup is case-insensitive (`find_header`), as everywhere
/// else in the tree; only the VALUE comparison is byte-exact.
///
/// No trimming happens here. Upstream detects `application/grpc ` (trailing
/// space) because the HTTP codec strips optional whitespace from field values
/// before anything sees them — that is the codec's job, not this matcher's.
pub(crate) fn is_grpc_request(headers: &[(String, String)]) -> bool {
    match headers::find_header(headers, headers::CONTENT_TYPE) {
        Some(value) => value == GRPC_EXACT || value.starts_with(GRPC_PLUS_PREFIX),
        None => false,
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-http1 --lib grpc:: 2>&1 | tail -20`

Expected: `test result: ok. 5 passed; 0 failed`. **Assert the count is 5** — a `0 passed; N filtered out` line is a false green, not a pass.

- [ ] **Step 5: Prove the tests are not vacuous (mutation check)**

Do this in a scratch worktree so it cannot collide with anything else:

```bash
git worktree add /tmp/er-mut-t1 HEAD
cd /tmp/er-mut-t1
# Mutation A: make the match case-insensitive.
sed -i 's/value == GRPC_EXACT || value.starts_with(GRPC_PLUS_PREFIX)/value.eq_ignore_ascii_case(GRPC_EXACT) || value.starts_with(GRPC_PLUS_PREFIX)/' crates/envoy-http1/src/grpc.rs
touch crates/envoy-http1/src/lib.rs   # mtime-only: forces a real rebuild, tree stays clean
cargo test -p envoy-http1 --lib grpc:: 2>&1 | tee /tmp/mutA.txt | tail -20
```

Expected: **RED**, with `detection_matrix_matches_upstream` failing on the `APPLICATION/GRPC` cell.

```bash
# Mutation B: the classic naive prefix.
git checkout -- crates/envoy-http1/src/grpc.rs
sed -i 's/value == GRPC_EXACT || value.starts_with(GRPC_PLUS_PREFIX)/value.starts_with(GRPC_EXACT)/' crates/envoy-http1/src/grpc.rs
touch crates/envoy-http1/src/lib.rs
cargo test -p envoy-http1 --lib grpc:: 2>&1 | tee /tmp/mutB.txt | tail -20
```

Expected: **RED**, failing on `application/grpcfoo` and `application/grpc-web`.

```bash
# Unmutated control from the SAME tree.
git checkout -- crates/envoy-http1/src/grpc.rs
touch crates/envoy-http1/src/lib.rs
cargo test -p envoy-http1 --lib grpc:: 2>&1 | tail -20   # must be GREEN, 5 passed
cd - && git worktree remove /tmp/er-mut-t1
```

> Gate on the **`test result:` line existing** and showing failures, not on the exit code — a compile error also exits non-zero and would be a false RED. Verify `Compiling envoy-http1` appears in each mutated run; a cached no-op gives a FALSE PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-http1/src/grpc.rs crates/envoy-http1/src/lib.rs crates/envoy-http1/src/headers.rs
git commit -m "phase 110.1 task 1: grpc module + is_grpc_request — the MEASURED 14-cell detection rule"
```

---

### Task 2: `http_to_grpc_status` — the sparse eight-entry mapping

**Files:**
- Modify: `crates/envoy-http1/src/grpc.rs`
- Test: inline `#[cfg(test)] mod tests` in the same file

**Interfaces:**
- Consumes: nothing.
- Produces: `pub(crate) fn http_to_grpc_status(status: u16) -> u8`.

- [ ] **Step 1: Write the failing test**

Append inside `mod tests` in `crates/envoy-http1/src/grpc.rs`:

```rust
    /// The MEASURED mapping matrix — a SPARSE EIGHT-ENTRY table over a DEFAULT
    /// of 2 (UNKNOWN). All 20 cells were probed against the pinned image at
    /// the 110.1 PLAN-write, each as a `direct_response` at its own distinct
    /// path with a paired non-gRPC control.
    ///
    /// The counter-intuitive cells are the point of this test: `500`, `501`,
    /// `405`, `408`, `409`, `412`, `413` and `499` all map to 2, NOT to 13/14.
    #[test]
    fn status_mapping_matches_upstream() {
        let cells: &[(u16, u8)] = &[
            (200, 2),
            (201, 2),
            (204, 2),
            (301, 2),
            (400, 13),
            (401, 16),
            (403, 7),
            (404, 12),
            (405, 2),
            (408, 2),
            (409, 2),
            (412, 2),
            (413, 2),
            (429, 14),
            (499, 2),
            (500, 2),
            (501, 2),
            (502, 14),
            (503, 14),
            (504, 14),
        ];
        for (http, grpc) in cells {
            assert_eq!(
                http_to_grpc_status(*http),
                *grpc,
                "HTTP {http} must map to grpc-status {grpc}"
            );
        }
    }

    /// The table is SPARSE: exactly eight statuses in the whole `u16` range are
    /// special, and every other one — all 65528 of them — is 2. Sweeping the
    /// full range is what makes a "helpful" extra arm (e.g. `500 => 13`, or a
    /// `4xx => 13` range arm) impossible to add unnoticed.
    #[test]
    fn every_other_status_in_the_whole_u16_range_is_unknown() {
        let special: [u16; 8] = [400, 401, 403, 404, 429, 502, 503, 504];
        let mut specials_seen = 0usize;
        for status in u16::MIN..=u16::MAX {
            if special.contains(&status) {
                specials_seen += 1;
                assert_ne!(
                    http_to_grpc_status(status),
                    2,
                    "special status {status} must not fall through to the default arm"
                );
            } else {
                assert_eq!(
                    http_to_grpc_status(status),
                    2,
                    "status {status} must map to the default 2 (UNKNOWN)"
                );
            }
        }
        assert_eq!(specials_seen, 8, "the special table must have exactly 8 entries");
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p envoy-http1 --lib grpc:: 2>&1 | tail -40`

Expected: compile FAILURE, `cannot find function 'http_to_grpc_status'`.

- [ ] **Step 3: Write the minimal implementation**

Append to the non-test part of `crates/envoy-http1/src/grpc.rs`:

```rust
/// Map an HTTP status onto a gRPC status code.
///
/// MEASURED against `envoyproxy/envoy:v1.33.0`: a SPARSE EIGHT-ENTRY table
/// over a DEFAULT of 2 (UNKNOWN). Only these eight are special —
/// `400→13`, `401→16`, `403→7`, `404→12`, `429→14`, `502→14`, `503→14`,
/// `504→14`. EVERYTHING else maps to 2, including the entire 2xx and 3xx
/// ranges and, counter-intuitively, `500`, `501`, `405`, `408`, `409`, `412`,
/// `413` and `499`.
///
/// Do NOT "improve" this with a range arm (e.g. `500..=599 => 13`). The
/// measurement says otherwise and the full-range sweep in the tests will
/// catch it.
pub(crate) fn http_to_grpc_status(status: u16) -> u8 {
    match status {
        400 => 13,
        401 => 16,
        403 => 7,
        404 => 12,
        429 | 502 | 503 | 504 => 14,
        _ => 2,
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-http1 --lib grpc:: 2>&1 | tail -20`

Expected: `test result: ok. 7 passed; 0 failed`. Assert the count is **7**.

- [ ] **Step 5: Prove the tests are not vacuous (mutation check)**

```bash
git worktree add /tmp/er-mut-t2 HEAD
cd /tmp/er-mut-t2
# Mutation: the plausible-but-wrong "5xx is UNAVAILABLE" generalisation.
sed -i 's/        429 | 502 | 503 | 504 => 14,/        429 => 14,\n        500..=599 => 14,/' crates/envoy-http1/src/grpc.rs
touch crates/envoy-http1/src/lib.rs
cargo test -p envoy-http1 --lib grpc:: 2>&1 | tail -25
```

Expected: **RED** on `status_mapping_matches_upstream` at the `500` cell (expected 2, got 14) and on the full-range sweep.

```bash
git checkout -- crates/envoy-http1/src/grpc.rs
touch crates/envoy-http1/src/lib.rs
cargo test -p envoy-http1 --lib grpc:: 2>&1 | tail -20   # control: GREEN, 7 passed
cd - && git worktree remove /tmp/er-mut-t2
```

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-http1/src/grpc.rs
git commit -m "phase 110.1 task 2: http_to_grpc_status — the MEASURED sparse-8 table over a default of 2"
```

---

### Task 3: `grpc_message_encode` — the percent-encoder

**Files:**
- Modify: `crates/envoy-http1/src/grpc.rs`
- Test: inline `#[cfg(test)] mod tests` in the same file

**Interfaces:**
- Consumes: nothing.
- Produces: `pub(crate) fn grpc_message_encode(body: &[u8]) -> String`.

- [ ] **Step 1: Write the failing test**

Append inside `mod tests`:

```rust
    /// The MEASURED encoder, on the exact nine bodies probed at the 110.1
    /// PLAN-write. Bodies were supplied to upstream as `inline_bytes` (base64)
    /// so the source bytes are exact, and each was probed WITH and WITHOUT the
    /// gRPC content-type so the control gave the byte-exact original.
    ///
    /// The DISCRIMINATING cells, each of which a plausible hand-rolled encoder
    /// gets wrong:
    ///   * `~` (0x7E) IS ESCAPED to `%7E`. The parent phase-110 SPEC claimed
    ///     `0x20..=0x7E` passes through; that was MEASURED FALSE.
    ///   * `}` (0x7D) PASSES THROUGH — it is the true upper bound.
    ///   * `%` becomes `%25`, so the input `%25` renders as `%2525`.
    ///   * multi-byte UTF-8 is encoded PER BYTE (`é` -> `%C3%A9`).
    ///   * hex digits are UPPERCASE.
    #[test]
    fn encoder_matches_upstream_on_every_measured_body() {
        let cells: &[(&[u8], &str)] = &[
            (
                b"a b\ncontrol\ttab \xc3\xa9 %25 end",
                "a b%0Acontrol%09tab %C3%A9 %2525 end",
            ),
            (b"q\"b s\\l t~t d\x7fd", "q\"b s\\l t%7Et d%7Fd"),
            (
                b"  ~ +,/:;=?@[]{}|^`<>#&*()",
                "  %7E +,/:;=?@[]{}|^`<>#&*()",
            ),
            (b"~", "%7E"),
            (b"\x7f", "%7F"),
            (b"%25", "%2525"),
            (b"\"\\", "\"\\"),
            (b"}~", "}%7E"),
            (b"\x1f ", "%1F "),
        ];
        for (input, expected) in cells {
            assert_eq!(
                grpc_message_encode(input),
                *expected,
                "encoding {input:?} must produce {expected:?}"
            );
        }
    }

    /// The rule as a property over EVERY single byte: pass through iff the byte
    /// is in `0x20..=0x7D` AND is not `%` (0x25); otherwise `%` + two UPPERCASE
    /// hex digits. Sweeping all 256 byte values pins both boundaries (0x1F/0x20
    /// at the bottom, 0x7D/0x7E at the top) and the `%` carve-out, so an
    /// off-by-one in either direction is impossible to land.
    #[test]
    fn encoder_rule_holds_for_every_byte_value() {
        for byte in 0u8..=255u8 {
            let got = grpc_message_encode(&[byte]);
            let expected = if (0x20..=0x7D).contains(&byte) && byte != b'%' {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            };
            assert_eq!(got, expected, "byte 0x{byte:02X} encoded wrongly");
        }
    }

    /// An empty body encodes to an empty string. (Whether the HEADER is emitted
    /// at all for an empty body is the transform's decision, pinned in Task 4 —
    /// upstream OMITS it entirely rather than sending an empty value.)
    #[test]
    fn empty_body_encodes_to_empty_string() {
        assert_eq!(grpc_message_encode(b""), "");
    }

    /// Hex digits are UPPERCASE, not lowercase — a `{:02x}` slip is the single
    /// most likely encoder bug and it is invisible in the ASCII-only cells.
    #[test]
    fn hex_digits_are_uppercase() {
        assert_eq!(grpc_message_encode(b"\xab\xcd\xef"), "%AB%CD%EF");
        assert_eq!(grpc_message_encode(&[0x0a, 0x1b, 0x7f]), "%0A%1B%7F");
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p envoy-http1 --lib grpc:: 2>&1 | tail -40`

Expected: compile FAILURE, `cannot find function 'grpc_message_encode'`.

- [ ] **Step 3: Write the minimal implementation**

Append to the non-test part of `crates/envoy-http1/src/grpc.rs`:

```rust
/// Percent-encode a local-reply body for the `grpc-message` header.
///
/// MEASURED rule against `envoyproxy/envoy:v1.33.0`: a byte passes through
/// UNCHANGED iff it is in `0x20..=0x7D` AND is not `%` (0x25). Every other
/// byte — every byte `< 0x20`, every byte `>= 0x7E`, and `%` itself — becomes
/// `%` followed by TWO UPPERCASE hex digits. Multi-byte UTF-8 is encoded PER
/// BYTE, so `é` (0xC3 0xA9) becomes `%C3%A9`.
///
/// Note the UPPER boundary: `}` (0x7D) passes through but `~` (0x7E) is
/// ESCAPED to `%7E`. The parent phase-110 SPEC stated the range as
/// `0x20..=0x7E`; that was MEASURED FALSE at the 110.1 PLAN-write.
///
/// The output is always ASCII, so building it as a `String` is sound: every
/// pushed byte is either an ASCII pass-through or one of `%0123456789ABCDEF`.
pub(crate) fn grpc_message_encode(body: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    // Most local-reply bodies are plain ASCII prose, so the common case is a
    // 1:1 copy; reserving `body.len()` avoids a realloc for those.
    let mut out = String::with_capacity(body.len());
    for &byte in body {
        if (0x20..=0x7D).contains(&byte) && byte != b'%' {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX[usize::from(byte >> 4)] as char);
            out.push(HEX[usize::from(byte & 0x0F)] as char);
        }
    }
    out
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-http1 --lib grpc:: 2>&1 | tail -20`

Expected: `test result: ok. 11 passed; 0 failed`. Assert the count is **11**.

- [ ] **Step 5: Prove the tests are not vacuous (mutation check)**

```bash
git worktree add /tmp/er-mut-t3 HEAD
cd /tmp/er-mut-t3
# Mutation A: the parent SPEC's WRONG upper bound.
sed -i 's/if (0x20..=0x7D).contains(&byte) \&\& byte != b'"'"'%'"'"' {/if (0x20..=0x7E).contains(\&byte) \&\& byte != b'"'"'%'"'"' {/' crates/envoy-http1/src/grpc.rs
touch crates/envoy-http1/src/lib.rs
cargo test -p envoy-http1 --lib grpc:: 2>&1 | tail -25
```
Expected: **RED** on the `~` cell (`%7E` expected, `~` produced) in both the measured-bodies test and the all-byte sweep.

```bash
# Mutation B: lowercase hex.
git checkout -- crates/envoy-http1/src/grpc.rs
sed -i 's/b"0123456789ABCDEF"/b"0123456789abcdef"/' crates/envoy-http1/src/grpc.rs
touch crates/envoy-http1/src/lib.rs
cargo test -p envoy-http1 --lib grpc:: 2>&1 | tail -25
```
Expected: **RED** on `hex_digits_are_uppercase` and on the `é`/`0x7F` cells.

```bash
# Mutation C: forget the `%` carve-out (the %2525 discriminator).
git checkout -- crates/envoy-http1/src/grpc.rs
sed -i 's/if (0x20..=0x7D).contains(&byte) \&\& byte != b'"'"'%'"'"' {/if (0x20..=0x7D).contains(\&byte) {/' crates/envoy-http1/src/grpc.rs
touch crates/envoy-http1/src/lib.rs
cargo test -p envoy-http1 --lib grpc:: 2>&1 | tail -25
```
Expected: **RED** on the `%25` cell (`%2525` expected, `%25` produced).

```bash
git checkout -- crates/envoy-http1/src/grpc.rs
touch crates/envoy-http1/src/lib.rs
cargo test -p envoy-http1 --lib grpc:: 2>&1 | tail -20   # control: GREEN, 11 passed
cd - && git worktree remove /tmp/er-mut-t3
```

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-http1/src/grpc.rs
git commit -m "phase 110.1 task 3: grpc_message_encode — the CORRECTED 0x20..=0x7D rule (~ IS escaped)"
```

---

### Task 4: `apply_grpc_local_reply` — the transform

**Files:**
- Modify: `crates/envoy-http1/src/grpc.rs`
- Test: inline `#[cfg(test)] mod tests` in the same file

**Interfaces:**
- Consumes: `is_grpc_request` (Task 1), `http_to_grpc_status` (Task 2), `grpc_message_encode` (Task 3), `crate::response::Response` (fields `status: u16`, `reason: Option<&'static str>`, `headers: Vec<(String, String)>`, `body: Bytes`), `crate::headers::{CONTENT_TYPE, CONTENT_LENGTH, DATE, SERVER, CONNECTION, GRPC_STATUS, GRPC_MESSAGE, GRPC_CONTENT_TYPE}`.
- Produces: `pub(crate) fn apply_grpc_local_reply(resp: &mut Response, req_headers: &[(String, String)])` — a no-op unless `is_grpc_request(req_headers)`. This is the ONLY entry point Tasks 5–7 call.

- [ ] **Step 1: Write the failing test**

Append inside `mod tests` (and add `use bytes::Bytes;` plus `use crate::response::Response;` to the test module's imports):

```rust
    fn resp_with(status: u16, headers: &[(&str, &str)], body: &[u8]) -> Response {
        Response {
            status,
            reason: None,
            headers: hdrs(headers),
            body: Bytes::copy_from_slice(body),
        }
    }

    fn names(resp: &Response) -> Vec<String> {
        resp.headers.iter().map(|(n, _)| n.clone()).collect()
    }

    fn value<'a>(resp: &'a Response, name: &str) -> Option<&'a str> {
        crate::headers::find_header(&resp.headers, name)
    }

    /// The wire shape of a bodied local reply, MEASURED on a `direct_response`
    /// 503 with body `B503`: status 200, `content-type: application/grpc`,
    /// `grpc-status: 14`, `grpc-message: B503`, `content-length: 0`, and the
    /// body DROPPED. Header ORDER is pinned to the measured wire order.
    #[test]
    fn bodied_local_reply_takes_the_measured_wire_shape() {
        let mut resp = resp_with(
            503,
            &[
                ("server", "envoy-rust"),
                ("date", "Mon, 17 Aug 2026 19:00:00 GMT"),
                ("content-length", "4"),
                ("content-type", "text/plain"),
                ("connection", "close"),
            ],
            b"B503",
        );
        apply_grpc_local_reply(&mut resp, &hdrs(&[("content-type", "application/grpc")]));

        assert_eq!(resp.status, 200);
        assert_eq!(resp.reason, None, "reason must fall back to the canonical 200 OK");
        assert!(resp.body.is_empty(), "the body must be DROPPED");
        assert_eq!(
            names(&resp),
            vec![
                "content-type",
                "grpc-status",
                "grpc-message",
                "date",
                "server",
                "connection",
                "content-length",
            ],
            "MEASURED upstream order for a bodied local reply"
        );
        assert_eq!(value(&resp, "content-type"), Some("application/grpc"));
        assert_eq!(value(&resp, "grpc-status"), Some("14"));
        assert_eq!(value(&resp, "grpc-message"), Some("B503"));
        assert_eq!(value(&resp, "content-length"), Some("0"));
        assert_eq!(value(&resp, "date"), Some("Mon, 17 Aug 2026 19:00:00 GMT"));
        assert_eq!(value(&resp, "server"), Some("envoy-rust"));
        assert_eq!(value(&resp, "connection"), Some("close"));
    }

    /// MEASURED on both an empty-body `direct_response` and the HCM's own
    /// unmatched-path 404: `grpc-message` is ABSENT ENTIRELY, not present with
    /// an empty value. This is the cell a "always set the header" implementation
    /// gets wrong, and it is a header-NAME-SET difference, which is exactly what
    /// the differential harness's `diff_headers` compares.
    #[test]
    fn empty_body_omits_grpc_message_entirely() {
        let mut resp = resp_with(
            404,
            &[
                ("server", "envoy-rust"),
                ("date", "D"),
                ("content-length", "0"),
                ("content-type", "text/plain"),
                ("connection", "close"),
            ],
            b"",
        );
        apply_grpc_local_reply(&mut resp, &hdrs(&[("content-type", "application/grpc")]));

        assert_eq!(resp.status, 200);
        assert_eq!(value(&resp, "grpc-status"), Some("12"));
        assert!(
            !resp.headers.iter().any(|(n, _)| n == "grpc-message"),
            "grpc-message must be ABSENT, not empty: {:?}",
            names(&resp)
        );
        assert_eq!(
            names(&resp),
            vec![
                "content-type",
                "grpc-status",
                "date",
                "server",
                "connection",
                "content-length",
            ]
        );
    }

    /// MEASURED on a `redirect:` route: the `location` header SURVIVES the
    /// transform, in its ORIGINAL leading position, and the reply still becomes
    /// 200 + `application/grpc` + `grpc-status: 2` + `content-length: 0`.
    /// Note `synth_redirect` emits NO `content-type` at all, so this also pins
    /// the "original had no content-type" branch.
    #[test]
    fn redirect_keeps_location_and_still_transforms() {
        let mut resp = resp_with(
            301,
            &[
                ("location", "http://example.com/a"),
                ("date", "D"),
                ("server", "envoy-rust"),
                ("connection", "close"),
                ("content-length", "0"),
            ],
            b"",
        );
        apply_grpc_local_reply(&mut resp, &hdrs(&[("content-type", "application/grpc")]));

        assert_eq!(resp.status, 200);
        assert_eq!(
            names(&resp),
            vec![
                "location",
                "content-type",
                "grpc-status",
                "date",
                "server",
                "connection",
                "content-length",
            ],
            "MEASURED upstream order for a redirect local reply"
        );
        assert_eq!(value(&resp, "location"), Some("http://example.com/a"));
        assert_eq!(value(&resp, "grpc-status"), Some("2"));
        assert_eq!(value(&resp, "content-type"), Some("application/grpc"));
        assert_eq!(value(&resp, "content-length"), Some("0"));
    }

    /// MEASURED on a circuit-breaker overflow: `x-envoy-overloaded` survives in
    /// its ORIGINAL leading position too. The pass-through rule is GENERAL —
    /// it is not a `location` special case.
    #[test]
    fn arbitrary_pass_through_headers_survive_in_original_position() {
        let mut resp = resp_with(
            503,
            &[
                ("x-envoy-overloaded", "true"),
                ("server", "envoy-rust"),
                ("date", "D"),
                ("content-length", "81"),
                ("content-type", "text/plain"),
                ("connection", "close"),
            ],
            b"upstream connect error or disconnect/reset before headers. reset reason: overflow",
        );
        apply_grpc_local_reply(&mut resp, &hdrs(&[("content-type", "application/grpc")]));

        assert_eq!(
            names(&resp),
            vec![
                "x-envoy-overloaded",
                "content-type",
                "grpc-status",
                "grpc-message",
                "date",
                "server",
                "connection",
                "content-length",
            ],
            "MEASURED upstream order for the overflow local reply"
        );
        assert_eq!(value(&resp, "x-envoy-overloaded"), Some("true"));
        assert_eq!(
            value(&resp, "grpc-message"),
            Some("upstream connect error or disconnect/reset before headers. reset reason: overflow")
        );
    }

    /// The body is percent-encoded into `grpc-message` per Task 3's rule.
    #[test]
    fn grpc_message_carries_the_percent_encoded_body() {
        let mut resp = resp_with(400, &[("content-type", "text/plain")], b"a b\nc %25 ~");
        apply_grpc_local_reply(&mut resp, &hdrs(&[("content-type", "application/grpc+proto")]));
        assert_eq!(value(&resp, "grpc-status"), Some("13"));
        assert_eq!(value(&resp, "grpc-message"), Some("a b%0Ac %2525 %7E"));
    }

    /// A NON-gRPC request leaves the response BYTE-FOR-BYTE untouched. This is
    /// the paired control for every cell above and the guard on non-goal 4
    /// (proxied responses must not be transformed).
    #[test]
    fn non_grpc_request_is_a_total_no_op() {
        let original = resp_with(
            404,
            &[
                ("server", "envoy-rust"),
                ("date", "D"),
                ("content-length", "4"),
                ("content-type", "text/plain"),
                ("connection", "close"),
            ],
            b"B404",
        );
        for ct in [
            "application/json",
            "application/grpc-web",
            "application/grpcfoo",
            "APPLICATION/GRPC",
            "application/grpc; charset=utf-8",
        ] {
            let mut resp = original.clone();
            apply_grpc_local_reply(&mut resp, &hdrs(&[("content-type", ct)]));
            assert_eq!(resp, original, "content-type {ct:?} must not transform");
        }
        // ...and with no content-type at all.
        let mut resp = original.clone();
        apply_grpc_local_reply(&mut resp, &hdrs(&[("host", "x")]));
        assert_eq!(resp, original);
    }

    /// The transform is IDEMPOTENT: applying it twice yields the same response
    /// as applying it once. This matters because the two wire funnels are
    /// separate code paths and a future refactor could route one reply through
    /// both; a non-idempotent transform would double-encode `grpc-message`
    /// (`%25` -> `%2525` -> `%252525`) and corrupt it silently.
    #[test]
    fn transform_is_idempotent() {
        let req = hdrs(&[("content-type", "application/grpc")]);
        let mut once = resp_with(503, &[("content-type", "text/plain")], b"100% done");
        apply_grpc_local_reply(&mut once, &req);
        let mut twice = once.clone();
        apply_grpc_local_reply(&mut twice, &req);
        assert_eq!(twice, once, "applying the transform twice must change nothing");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-http1 --lib grpc:: 2>&1 | tail -40`

Expected: compile FAILURE, `cannot find function 'apply_grpc_local_reply'`.

- [ ] **Step 3: Write the minimal implementation**

Append to the non-test part of `crates/envoy-http1/src/grpc.rs` (and add `use crate::response::Response;` and `use bytes::Bytes;` to the file's imports):

```rust
/// Rewrite a LOCALLY GENERATED response into upstream Envoy's gRPC shape, if
/// and only if the request that provoked it carried a gRPC `content-type`.
///
/// MEASURED shape (`envoyproxy/envoy:v1.33.0`): status becomes `200`,
/// `content-type` becomes `application/grpc`, the body is DROPPED,
/// `content-length` becomes `0`, `grpc-status` carries the mapped code, and
/// `grpc-message` carries the percent-encoded ORIGINAL body — but ONLY when
/// that body was non-empty; for an empty body the header is ABSENT ENTIRELY,
/// not present-and-empty.
///
/// MEASURED header order, reproduced exactly on three independent cases (a
/// bodied `direct_response`, a `redirect:` route and a circuit-breaker
/// overflow): pass-through headers first in their ORIGINAL relative order,
/// then `content-type`, `grpc-status`, `[grpc-message]`, then `date`,
/// `server`, `connection`, then `content-length: 0`. `location` and
/// `x-envoy-overloaded` are pass-throughs and both SURVIVE — the rule is
/// general, not a `location` special case.
///
/// `serialize_response_head` (`response.rs:121`) emits headers in vector order
/// verbatim, so vector position IS wire order.
///
/// # This function must only ever see a LOCAL reply
///
/// It has no way to tell a synth from a proxied upstream response — that is
/// the CALLER's job (non-goal 4 / CF-110-2). The tokio seam gates on
/// `outgoing_local`; the io_uring seam relies on `write_owned` being
/// exclusively the local-reply funnel there.
///
/// # Not installed on the shared path, deliberately
///
/// This is NOT called from `synth_with`, from any `synth_*` wrapper, or from
/// `build_response`/`build_response_in`, because `envoy-http2` calls
/// `envoy_http1::build_response` (`crates/envoy-http2/src/hcm.rs:513-518`).
/// See the module doc.
pub(crate) fn apply_grpc_local_reply(resp: &mut Response, req_headers: &[(String, String)]) {
    if !is_grpc_request(req_headers) {
        return;
    }

    let grpc_status = http_to_grpc_status(resp.status);
    let grpc_message = if resp.body.is_empty() {
        None
    } else {
        Some(grpc_message_encode(&resp.body))
    };

    // Partition the original headers. `content-type` and `content-length` are
    // REWRITTEN (so their originals are dropped here); `date`, `server` and
    // `connection` are RE-ORDERED to sit after the gRPC block; everything else
    // is a pass-through that keeps its original relative position.
    let mut passthrough: Vec<(String, String)> = Vec::with_capacity(resp.headers.len());
    let mut date: Option<(String, String)> = None;
    let mut server: Option<(String, String)> = None;
    let mut connection: Option<(String, String)> = None;

    for (name, value) in std::mem::take(&mut resp.headers) {
        if name.eq_ignore_ascii_case(headers::CONTENT_TYPE)
            || name.eq_ignore_ascii_case(headers::CONTENT_LENGTH)
        {
            // Both are REWRITTEN below, so the originals are dropped here.
            continue;
        }
        if date.is_none() && name.eq_ignore_ascii_case(headers::DATE) {
            date = Some((name, value));
        } else if server.is_none() && name.eq_ignore_ascii_case(headers::SERVER) {
            server = Some((name, value));
        } else if connection.is_none() && name.eq_ignore_ascii_case(headers::CONNECTION) {
            connection = Some((name, value));
        } else {
            passthrough.push((name, value));
        }
    }

    let mut out = passthrough;
    out.push((
        headers::CONTENT_TYPE.to_string(),
        headers::GRPC_CONTENT_TYPE.to_string(),
    ));
    out.push((headers::GRPC_STATUS.to_string(), grpc_status.to_string()));
    if let Some(message) = grpc_message {
        out.push((headers::GRPC_MESSAGE.to_string(), message));
    }
    out.extend(date);
    out.extend(server);
    out.extend(connection);
    out.push((headers::CONTENT_LENGTH.to_string(), "0".to_string()));

    resp.status = 200;
    // `None` makes `serialize_response_head` fall back to `canonical_reason(200)`
    // and emit `HTTP/1.1 200 OK`, matching the measured status line.
    resp.reason = None;
    resp.headers = out;
    resp.body = Bytes::new();
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-http1 --lib grpc:: 2>&1 | tail -20`

Expected: `test result: ok. 18 passed; 0 failed`. Assert the count is **18**.

- [ ] **Step 5: Prove the tests are not vacuous (mutation check)**

```bash
git worktree add /tmp/er-mut-t4 HEAD
cd /tmp/er-mut-t4
# Mutation A: always emit grpc-message, even for an empty body.
python3 - <<'PY'
import io
p='crates/envoy-http1/src/grpc.rs'
s=io.open(p,encoding='utf-8').read()
s=s.replace("""    let grpc_message = if resp.body.is_empty() {
        None
    } else {
        Some(grpc_message_encode(&resp.body))
    };""","""    let grpc_message = Some(grpc_message_encode(&resp.body));""")
io.open(p,'w',encoding='utf-8').write(s)
PY
touch crates/envoy-http1/src/lib.rs
cargo test -p envoy-http1 --lib grpc:: 2>&1 | tail -25
```
Expected: **RED** on `empty_body_omits_grpc_message_entirely` — the name set gains a `grpc-message` upstream does not send.

```bash
# Mutation B: drop pass-through headers instead of preserving them.
git checkout -- crates/envoy-http1/src/grpc.rs
python3 - <<'PY'
import io
p='crates/envoy-http1/src/grpc.rs'
s=io.open(p,encoding='utf-8').read()
s=s.replace("            passthrough.push((name, value));","            // mutated: drop")
io.open(p,'w',encoding='utf-8').write(s)
PY
touch crates/envoy-http1/src/lib.rs
cargo test -p envoy-http1 --lib grpc:: 2>&1 | tail -25
```
Expected: **RED** on `redirect_keeps_location_and_still_transforms` and `arbitrary_pass_through_headers_survive_in_original_position`.

```bash
# Mutation C: forget to drop the body.
git checkout -- crates/envoy-http1/src/grpc.rs
python3 - <<'PY'
import io
p='crates/envoy-http1/src/grpc.rs'
s=io.open(p,encoding='utf-8').read()
s=s.replace("    resp.body = Bytes::new();","    // mutated: body kept")
io.open(p,'w',encoding='utf-8').write(s)
PY
touch crates/envoy-http1/src/lib.rs
cargo test -p envoy-http1 --lib grpc:: 2>&1 | tail -25
```
Expected: **RED** on `bodied_local_reply_takes_the_measured_wire_shape` (`the body must be DROPPED`) and on `transform_is_idempotent` (double-encoding).

```bash
git checkout -- crates/envoy-http1/src/grpc.rs
touch crates/envoy-http1/src/lib.rs
cargo test -p envoy-http1 --lib grpc:: 2>&1 | tail -20   # control: GREEN, 18 passed
cd - && git worktree remove /tmp/er-mut-t4
```

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-http1/src/grpc.rs
git commit -m "phase 110.1 task 4: apply_grpc_local_reply — the MEASURED wire shape and header order"
```

---

### Task 5: The tokio seam in `serve_connection`

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (non-test: ~20 lines in `serve_connection`, def at `hcm.rs:750`)
- Test: `#[cfg(test)] mod tests` in `crates/envoy-http1/src/hcm.rs` (begins at line 2532)

**Interfaces:**
- Consumes: `crate::grpc::apply_grpc_local_reply` (Task 4). The existing `drive(config, req) -> Vec<u8>` test helper and the `hcm_config_single_route(path, status, body)` builder already in `hcm.rs`'s test module (see `direct_response_returns_status_and_body` at `hcm.rs:3098` for the exact usage).
- Produces: a new `outgoing_local: bool` local in `serve_connection`, and the single transform call site. Nothing new is exported.

**Design — read this before editing.** `serve_connection` has FIVE writer arms that populate `outgoing`, measured on disk at this PLAN-write:

| line | arm | local? |
|---|---|---|
| `hcm.rs:1008` | `BuildOutcome::Synth(resp, details)` — direct_response / 400 / 404 / redirect / chunked-501 | **LOCAL** |
| `hcm.rs:1070` | `outgoing = synth_overflow(close)` — request-budget rejection | **LOCAL** |
| `hcm.rs:1367` | `outgoing = final_response` from the retry loop | **EITHER** — see below |
| `hcm.rs:1391` | `SynthFromDecode(resp)` — a decode-side filter's `StopAndSend` | **LOCAL** (measurement N-1) |
| `hcm.rs:1423` | encode-side `Decision::StopAndSend(replacement)` | **LOCAL** (measurement N-1) |

Only `:1367` is ambiguous, and the tree already carries exactly the right bit. `AttemptResult.upstream_response` is documented at `hcm.rs:368-370` as *"`true` iff a real upstream RESPONSE was received … Connect/reset failures and overflow synths leave this `false`"*, and the retry loop surfaces it post-loop as `completing_upstream_response`, bound at `hcm.rs:1138-1142`:
```rust
let (final_response, completing_upstream_response, final_direct): (
    Response,
    bool,
    bool,
) = loop {
```
and produced by the `break` at `hcm.rs:1280-1284`. So **`local == !completing_upstream_response`** at that arm — which correctly marks `synth_no_healthy_upstream`, `synth_status(503)` and `synth_overflow` from `run_attempt` as local while leaving a real proxied response alone.

**Invariant worth asserting:** `outgoing_local ⟹ !outgoing_direct`. `final_direct` comes from `attempt.direct_head`, which is `true` at exactly one site (`hcm.rs:588`) — the zero-copy proxied fast path, which sets `upstream_response: true` at `hcm.rs:591`. Every synth-producing `AttemptResult` sets `direct_head: false`. So a local reply can never be carrying a pre-serialized `direct_head_buf`.

**Placement is load-bearing (measurement N-2).** The call must go AFTER the encode-filter block closes (~`hcm.rs:1437`) and BEFORE `hcm.rs:1447`:
```rust
let response_status_for_log: u16 = outgoing.status;
let response_body_len: u64 = outgoing.body.len() as u64;
```
because those two locals drive BOTH the access-log record and the per-class counter dispatch at `hcm.rs:1480`, and upstream logs `rc=200` / `bytes_sent=0` and ticks `downstream_rq_2xx` for a transformed reply. Putting the call at the wire write (`:1457`/`:1468`) instead would silently log the ORIGINAL status and tick the wrong counter class.

- [ ] **Step 1: Write the failing tests**

Append inside `hcm.rs`'s `#[cfg(test)] mod tests`:

```rust
    /// 110.1 seam: a gRPC `content-type` on a request that hits a
    /// `direct_response` route must produce upstream's MEASURED wire shape —
    /// `200`, `content-type: application/grpc`, `grpc-status`, `grpc-message`,
    /// `content-length: 0`, no body — end to end through the real tokio
    /// `serve_connection` funnel, not just through the pure transform.
    #[tokio::test]
    async fn grpc_local_reply_transforms_direct_response() {
        let config = hcm_config_single_route("/", 404, "B404").await;
        let req = b"GET /x HTTP/1.1\r\nHost: h\r\ncontent-type: application/grpc\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let s = String::from_utf8_lossy(&resp);

        assert!(s.starts_with("HTTP/1.1 200 OK\r\n"), "status line: {s}");
        assert!(s.contains("content-type: application/grpc\r\n"), "ct: {s}");
        assert!(s.contains("grpc-status: 12\r\n"), "grpc-status: {s}");
        assert!(s.contains("grpc-message: B404\r\n"), "grpc-message: {s}");
        assert!(s.contains("content-length: 0\r\n"), "cl: {s}");
        assert!(s.ends_with("\r\n\r\n"), "body must be empty: {s}");
        assert!(!s.contains("text/plain"), "old content-type must be gone: {s}");
    }

    /// The paired NON-gRPC control on the SAME route: nothing changes. Without
    /// this, a transform that fired unconditionally would still pass the test
    /// above.
    #[tokio::test]
    async fn non_grpc_request_leaves_direct_response_untouched() {
        let config = hcm_config_single_route("/", 404, "B404").await;
        let req = b"GET /x HTTP/1.1\r\nHost: h\r\ncontent-type: application/json\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let s = String::from_utf8_lossy(&resp);

        assert!(s.starts_with("HTTP/1.1 404 Not Found\r\n"), "status: {s}");
        assert!(s.contains("content-type: text/plain\r\n"), "ct: {s}");
        assert!(s.contains("content-length: 4\r\n"), "cl: {s}");
        assert!(!s.contains("grpc-status"), "no grpc-status: {s}");
        assert!(!s.contains("grpc-message"), "no grpc-message: {s}");
        assert!(s.ends_with("\r\nB404"), "body preserved: {s}");
    }

    /// The HCM's OWN unmatched-path 404 — an empty-body local reply that does
    /// NOT come from a `direct_response`. MEASURED upstream: `grpc-status: 12`
    /// and NO `grpc-message` header at all.
    #[tokio::test]
    async fn grpc_local_reply_transforms_route_not_found_without_grpc_message() {
        let config = hcm_config_single_route("/only-this", 200, "ok").await;
        let req = b"GET /nope HTTP/1.1\r\nHost: h\r\ncontent-type: application/grpc\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let s = String::from_utf8_lossy(&resp);

        assert!(s.starts_with("HTTP/1.1 200 OK\r\n"), "status: {s}");
        assert!(s.contains("grpc-status: 12\r\n"), "grpc-status: {s}");
        assert!(!s.contains("grpc-message"), "grpc-message must be ABSENT: {s}");
        assert!(s.contains("content-length: 0\r\n"), "cl: {s}");
    }

    /// The detection edges, driven through the REAL funnel rather than the pure
    /// function, so a seam that (say) lower-cased the value before matching
    /// would be caught here even though Task 1's unit tests pass.
    #[tokio::test]
    async fn grpc_detection_edges_hold_through_the_seam() {
        for (ct, transformed) in [
            ("application/grpc", true),
            ("application/grpc+proto", true),
            ("application/grpc+", true),
            ("application/grpc; charset=utf-8", false),
            ("APPLICATION/GRPC", false),
            ("application/grpc-web", false),
            ("application/grpcfoo", false),
        ] {
            let config = hcm_config_single_route("/", 404, "B404").await;
            let req = format!(
                "GET /x HTTP/1.1\r\nHost: h\r\ncontent-type: {ct}\r\nConnection: close\r\n\r\n"
            );
            let resp = drive(config, req.as_bytes()).await;
            let s = String::from_utf8_lossy(&resp);
            if transformed {
                assert!(s.starts_with("HTTP/1.1 200 OK\r\n"), "{ct} must transform: {s}");
                assert!(s.contains("grpc-status: 12\r\n"), "{ct}: {s}");
            } else {
                assert!(s.starts_with("HTTP/1.1 404 Not Found\r\n"), "{ct} must NOT transform: {s}");
                assert!(!s.contains("grpc-status"), "{ct}: {s}");
            }
        }
    }

    /// The MEASURED header ORDER, through the real funnel, byte-exact.
    ///
    /// Order is a HOUSE-CONVENTION concern, not a differential one: the
    /// harness's `diff_headers` compares a `BTreeSet` of lower-cased header
    /// NAMES plus exact VALUES outside the 3-entry `HEADER_ALLOW_LIST`, and
    /// never reads order. A wrong order therefore fails THIS test, not a
    /// fixture — which is exactly why this test has to exist.
    #[tokio::test]
    async fn grpc_local_reply_header_order_matches_upstream() {
        let config = hcm_config_single_route("/", 503, "B503").await;
        let req = b"GET /x HTTP/1.1\r\nHost: h\r\ncontent-type: application/grpc\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let s = String::from_utf8_lossy(&resp);
        let head = s.split("\r\n\r\n").next().unwrap_or_default();
        let order: Vec<&str> = head
            .lines()
            .skip(1)
            .filter_map(|l| l.split(':').next())
            .collect();
        assert_eq!(
            order,
            vec![
                "content-type",
                "grpc-status",
                "grpc-message",
                "date",
                "server",
                "connection",
                "content-length",
            ],
            "MEASURED upstream order: {s}"
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-http1 --lib grpc_local_reply 2>&1 | tee /tmp/t5-red.txt | tail -40`
and: `cargo test -p envoy-http1 --lib grpc_detection_edges 2>&1 | tail -20`

Expected: **RED** — the transform is not installed yet, so `grpc_local_reply_transforms_direct_response` reports `HTTP/1.1 404 Not Found`. `non_grpc_request_leaves_direct_response_untouched` will already be GREEN (it asserts today's behaviour) — that is expected and correct; it is the control that must stay green through the change.

**Assert the `test result:` line exists** with a non-zero failure count. Do not read the exit code alone.

- [ ] **Step 3: Write the minimal implementation**

Three edits in `crates/envoy-http1/src/hcm.rs`, all inside `serve_connection`. **Locate every one BY TEXT — line numbers drift.**

Edit 1 — beside the existing `outgoing_direct` declaration (find `let mut outgoing_direct = false;`), add immediately after it:

```rust
        // 110.1: true when `outgoing` is a LOCALLY GENERATED reply rather than
        // a real upstream response. Every writer arm below is local EXCEPT the
        // proxy arm's completing upstream response, which clears it from
        // `completing_upstream_response`. Gates the gRPC local-reply transform
        // — a proxied response must NEVER be transformed (non-goal 4 /
        // CF-110-2). Defaults to `true` so a newly added synth arm is covered
        // by omission rather than silently skipped.
        let mut outgoing_local = true;
```

Edit 2 — find `outgoing_direct = final_direct;` (the proxy arm) and add immediately after it:

```rust
                        // 110.1: `upstream_response` is the tree's existing
                        // "a real upstream RESPONSE was received" bit
                        // (`AttemptResult` doc). Its complement is exactly
                        // "this is a local reply": `synth_no_healthy_upstream`,
                        // `synth_status(503)` and `synth_overflow` from
                        // `run_attempt` all leave it false.
                        outgoing_local = !completing_upstream_response;
```

Edit 3 — find the encode-side `StopAndSend` arm's `outgoing_direct = false;` and add immediately after it:

```rust
                // 110.1: a filter's substitute response IS a local reply, even
                // when it replaced a proxied one. MEASURED upstream: an RBAC
                // deny with a gRPC content-type returns 200 + `grpc-status: 7`
                // + `grpc-message: RBAC: access denied`.
                outgoing_local = true;
```

Edit 4 — the transform call. Find the comment block ending `// (Proxy success arm) `outgoing` is the` / `// `construct_proxied_response` output ...` and insert the following IMMEDIATELY BEFORE `let response_status_for_log: u16 = outgoing.status;`:

```rust
        // 110.1: the gRPC local-reply transform, at the FIRST of the two H1
        // wire funnels (the io_uring worker has its own — `uring.rs`'s
        // `write_owned`).
        //
        // PLACEMENT IS LOAD-BEARING. This must run BEFORE
        // `response_status_for_log` / `response_body_len` are derived below,
        // because those two drive the access-log record AND the per-class
        // counter dispatch. MEASURED upstream: a transformed local reply logs
        // `%RESPONSE_CODE%` = 200 and `%BYTES_SENT%` = 0, and ticks
        // `downstream_rq_2xx` — NOT the original status's class.
        // `%RESPONSE_CODE_DETAILS%` is unchanged by the transform.
        //
        // NOT installed in `synth_with` / any `synth_*` / `build_response`:
        // `envoy-http2` calls `envoy_http1::build_response`, so a transform
        // there would rewrite H2 route-decision replies while missing H2's own
        // `synth_h2_*` family (CF-110-1; the ADR-0049 class).
        if outgoing_local {
            // A local reply never takes the zero-copy direct-head path:
            // `direct_head: true` is set at exactly one site, the successful
            // proxied attempt, which also sets `upstream_response: true`.
            debug_assert!(
                !outgoing_direct,
                "a local reply must never carry a pre-serialized direct head"
            );
            crate::grpc::apply_grpc_local_reply(&mut outgoing, &req.headers);
        }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-http1 --lib 2>&1 | tail -25`

Expected: `test result: ok.` with **zero failures** across the whole `envoy-http1` lib — the five new tests green AND every pre-existing `hcm.rs` test still green (nothing sends a gRPC content-type today, so nothing should move).

Record the passed count before and after this task; the delta must equal the number of tests you added.

- [ ] **Step 5: Prove the seam is not vacuous (mutation check)**

```bash
git worktree add /tmp/er-mut-t5 HEAD
cd /tmp/er-mut-t5
# Mutation A: gate on the wrong polarity — transform proxied, skip local.
python3 - <<'PY'
import io
p='crates/envoy-http1/src/hcm.rs'
s=io.open(p,encoding='utf-8').read()
s=s.replace("        if outgoing_local {\n            // A local reply never takes","        if !outgoing_local {\n            // A local reply never takes",1)
io.open(p,'w',encoding='utf-8').write(s)
PY
touch crates/envoy-http1/src/lib.rs
cargo test -p envoy-http1 --lib grpc_local_reply 2>&1 | tail -25
```
Expected: **RED** on all three transform tests.

```bash
# Mutation B: move the call AFTER the log locals (the N-2 placement bug).
git checkout -- crates/envoy-http1/src/hcm.rs
```
Then re-run Task 6 Step 3's access-log test after Task 6 lands; **the placement mutation is witnessed there**, because it is invisible to the wire-shape tests above. Note this explicitly in `PROGRESS.md`.

```bash
git checkout -- crates/envoy-http1/src/hcm.rs
touch crates/envoy-http1/src/lib.rs
cargo test -p envoy-http1 --lib 2>&1 | tail -20   # control: GREEN
cd - && git worktree remove /tmp/er-mut-t5
```

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 110.1 task 5: the tokio H1 wire-funnel seam — outgoing_local + apply_grpc_local_reply"
```

---

### Task 6: Family-wide seam coverage, plus the access-log and stats witnesses

**Files:**
- Test only: `#[cfg(test)] mod tests` in `crates/envoy-http1/src/hcm.rs`

**Interfaces:**
- Consumes: the Task 5 seam; the existing test builders in `hcm.rs`'s test module (`hcm_config_single_route`, `drive`, `mk_stats`, `cluster_mgr_empty`, `test_router_only_pipeline`, and the redirect-config builder used by the existing `synth_redirect` tests near `hcm.rs:11103`).
- Produces: no production code. This task's deliverable is COVERAGE — the guarantee that the whole local-reply family is covered IDENTICALLY, which is the ADR-0049 requirement that shaped the design.

**The family that must be covered identically** (re-derived BY TEXT at this PLAN-write; locate each by its function name, not by line number):

| producer | reached via | covered by |
|---|---|---|
| `synth_direct_response` (`hcm.rs:2260`) | `build_response_in:2124` | Task 5 |
| `synth_404` (`hcm.rs:2525`) ×2 sites | `build_response_in:2098`, `:2117` | Task 5 |
| `synth_400` (`hcm.rs:2522`) | `build_response_in:2078` (missing/empty Host) | **this task** |
| `synth_redirect` (`hcm.rs:2383`) | `build_response_in:2149` | **this task** |
| `synth_501` (`hcm.rs:2528`) | `serve_connection:925` (chunked rejection) | **this task** |
| `synth_no_healthy_upstream` (`hcm.rs:2409`) | `run_attempt:409` | **this task** |
| `synth_status(503)` (`hcm.rs:2269`) ×4 sites | `run_attempt:463/492/509/638` | **this task** |
| `synth_overflow` (`hcm.rs:2424`) ×2 sites | `run_attempt:470/477`, `serve_connection:1070` | **this task** |
| filter `StopAndSend` (decode + encode) | `serve_connection:1391/1423` | **this task** |
| a real PROXIED response | `serve_connection:1367` with `upstream_response == true` | **this task (negative)** |

- [ ] **Step 1: Write the failing tests**

Append inside `hcm.rs`'s `#[cfg(test)] mod tests`. Model each config on the nearest existing test for that path — e.g. the redirect builder used by `synth_redirect_emits_five_names_and_no_content_type` (near `hcm.rs:11103`), and the no-healthy-upstream builder used by the `synth_no_healthy_upstream` test at `hcm.rs:7158`.

```rust
    /// Helper: assert the standard gRPC local-reply wire shape on a raw
    /// response buffer, with the mapped code and an optional expected
    /// `grpc-message`. Keeps the family tests below to one line of intent each,
    /// so a missing family member is visible at a glance.
    fn assert_grpc_shape(resp: &[u8], grpc_status: &str, grpc_message: Option<&str>) {
        let s = String::from_utf8_lossy(resp);
        assert!(s.starts_with("HTTP/1.1 200 OK\r\n"), "status: {s}");
        assert!(s.contains("content-type: application/grpc\r\n"), "ct: {s}");
        assert!(s.contains(&format!("grpc-status: {grpc_status}\r\n")), "gs: {s}");
        assert!(s.contains("content-length: 0\r\n"), "cl: {s}");
        assert!(s.ends_with("\r\n\r\n"), "body must be empty: {s}");
        match grpc_message {
            Some(m) => assert!(s.contains(&format!("grpc-message: {m}\r\n")), "gm: {s}"),
            None => assert!(!s.contains("grpc-message"), "gm must be absent: {s}"),
        }
    }

    /// `synth_400` — a request with a missing/empty Host. MEASURED mapping:
    /// 400 -> 13.
    #[tokio::test]
    async fn grpc_transforms_synth_400_bad_host() {
        let config = hcm_config_single_route("/", 200, "ok").await;
        let req = b"GET /x HTTP/1.1\r\nHost: \r\ncontent-type: application/grpc\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        assert_grpc_shape(&resp, "13", None);
    }

    /// `synth_redirect` — the one route-decision synth that deliberately does
    /// NOT reuse `synth_with`. MEASURED upstream: the transform DOES fire, the
    /// `location` header SURVIVES, `grpc-status` is 2 (301 is not special), and
    /// there is no `grpc-message` (the redirect body is empty).
    ///
    /// Build the config with the same redirect builder the existing
    /// `synth_redirect` tests use.
    #[tokio::test]
    async fn grpc_transforms_synth_redirect_and_keeps_location() {
        let config = hcm_config_redirect_route("/x", "example.com").await;
        let req = b"GET /x HTTP/1.1\r\nHost: h\r\ncontent-type: application/grpc\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert_grpc_shape(&resp, "2", None);
        assert!(s.contains("location: http://example.com/x\r\n"), "location must survive: {s}");
    }

    /// `synth_501` — the chunked-request rejection in `serve_connection`,
    /// which builds its `BuildOutcome::Synth` BEFORE `build_response_in` is
    /// ever called. MEASURED mapping: 501 is NOT special -> 2.
    #[tokio::test]
    async fn grpc_transforms_synth_501_chunked_rejection() {
        let config = hcm_config_single_route("/", 200, "ok").await;
        let req = b"POST /x HTTP/1.1\r\nHost: h\r\ntransfer-encoding: chunked\r\ncontent-type: application/grpc\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        assert_grpc_shape(&resp, "2", None);
    }

    /// `synth_no_healthy_upstream` — a `run_attempt` local reply with a
    /// NON-EMPTY body. MEASURED upstream: 503 -> 14, and `grpc-message` DOES
    /// appear, carrying the encoded body.
    #[tokio::test]
    async fn grpc_transforms_synth_no_healthy_upstream_with_message() {
        let config = hcm_config_route_to_empty_cluster("/x").await;
        let req = b"GET /x HTTP/1.1\r\nHost: h\r\ncontent-type: application/grpc\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        assert_grpc_shape(&resp, "14", Some("no healthy upstream"));
    }

    /// A decode-side filter's `StopAndSend`. MEASURED upstream on an RBAC deny:
    /// the filter-generated local reply IS transformed. Use the existing
    /// test-util filter that emits `StopAndSend` (the same one the phase-09
    /// LocalRateLimit tests use).
    #[tokio::test]
    async fn grpc_transforms_filter_stop_and_send() {
        let config = hcm_config_with_stop_and_send_filter(429, "over limit").await;
        let req = b"GET /x HTTP/1.1\r\nHost: h\r\ncontent-type: application/grpc\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        // 429 maps to 14; the body is plain ASCII so it encodes to itself.
        assert_grpc_shape(&resp, "14", Some("over limit"));
    }

    /// THE NEGATIVE WITNESS for non-goal 4 / CF-110-2: a PROXIED upstream
    /// response is NOT transformed, even when the request carried a gRPC
    /// content-type. Without this, an `outgoing_local` that was simply always
    /// true would pass every other test in this file.
    #[tokio::test]
    async fn grpc_does_not_transform_a_proxied_upstream_response() {
        let (config, _backend) = hcm_config_route_to_live_backend("/x", 201, "UPSTREAM").await;
        let req = b"GET /x HTTP/1.1\r\nHost: h\r\ncontent-type: application/grpc\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let s = String::from_utf8_lossy(&resp);
        assert!(s.starts_with("HTTP/1.1 201 "), "proxied status must survive: {s}");
        assert!(!s.contains("grpc-status"), "no grpc-status on a proxied reply: {s}");
        assert!(!s.contains("grpc-message"), "no grpc-message on a proxied reply: {s}");
        assert!(s.ends_with("UPSTREAM"), "proxied body must survive: {s}");
    }

    /// MEASUREMENT N-2, the placement witness: the access-log record must carry
    /// the TRANSFORMED status (200) and a zero body length, not the original.
    /// This is the ONLY test that catches the transform being installed at the
    /// wire write instead of before the log locals — the wire-shape tests are
    /// blind to it.
    #[tokio::test]
    async fn grpc_transform_is_visible_to_the_access_log() {
        let (config, log) = hcm_config_single_route_with_access_log("/", 404, "B404").await;
        let req = b"GET /x HTTP/1.1\r\nHost: h\r\ncontent-type: application/grpc\r\nConnection: close\r\n\r\n";
        let _ = drive(config, req).await;
        let record = log.records().await.pop().expect("one access-log record");
        assert_eq!(
            record.response_status, 200,
            "MEASURED upstream logs %RESPONSE_CODE% = 200 for a transformed local reply"
        );
        assert_eq!(
            record.response_body_len, 0,
            "MEASURED upstream logs %BYTES_SENT% = 0 for a transformed local reply"
        );
    }

    /// MEASUREMENT N-2, the stats half: the per-class counter must tick
    /// `downstream_rq_2xx`, not `downstream_rq_4xx`.
    #[tokio::test]
    async fn grpc_transform_ticks_the_2xx_response_class() {
        let config = hcm_config_single_route("/", 404, "B404").await;
        let stats = config.stats.clone();
        let req = b"GET /x HTTP/1.1\r\nHost: h\r\ncontent-type: application/grpc\r\nConnection: close\r\n\r\n";
        let _ = drive(config, req).await;
        assert_eq!(stats.downstream_rq_2xx().value(), 1, "transformed reply is a 2xx");
        assert_eq!(stats.downstream_rq_4xx().value(), 0, "the original 404 must NOT tick");
    }
```

> **Adapt the config-builder names to what actually exists.** `hcm_config_single_route` and `drive` are confirmed present (`hcm.rs:3098` uses both). The other builders (`hcm_config_redirect_route`, `hcm_config_route_to_empty_cluster`, `hcm_config_with_stop_and_send_filter`, `hcm_config_route_to_live_backend`, `hcm_config_single_route_with_access_log`) are NAMES THIS PLAN CHOOSES. Before writing them, grep the test module for an existing equivalent — `grep -n 'async fn hcm_config' crates/envoy-http1/src/hcm.rs` — and REUSE it if one exists (the redirect, no-healthy-upstream, filter-synth and access-log paths all already have tests, so builders very likely exist under other names). Only write a new builder where none exists, and keep it beside its siblings. Report in `PROGRESS.md` which you reused and which you wrote.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-http1 --lib grpc_ 2>&1 | tee /tmp/t6-red.txt | tail -50`

Expected: compile failures for any builder you have not yet written, then RED on the family members whose coverage is genuinely new. `grpc_does_not_transform_a_proxied_upstream_response` should be GREEN from the start — it asserts the guard already installed in Task 5, and it must STAY green.

- [ ] **Step 3: Make them pass**

**No production change should be required.** If any family member is RED after the builders compile, that is a REAL GAP in the Task 5 seam — investigate with `superpowers:systematic-debugging` before touching anything. The most likely genuine gap is a synth path that reaches the wire by some route other than the `outgoing` funnel; if you find one, it must be covered, not excluded (ADR-0049).

Run: `cargo test -p envoy-http1 --lib 2>&1 | tail -25` — the whole crate green.

- [ ] **Step 4: Run the placement mutation deferred from Task 5**

```bash
git worktree add /tmp/er-mut-t6 HEAD
cd /tmp/er-mut-t6
```
Move the `if outgoing_local { ... }` block from before `let response_status_for_log` to immediately before `if outgoing_direct {`, then:
```bash
touch crates/envoy-http1/src/lib.rs
cargo test -p envoy-http1 --lib grpc_transform_is_visible_to_the_access_log grpc_transform_ticks 2>&1 | tail -25
```
Expected: **RED** on both — the log records 404 and the 4xx counter ticks. This is the witness that measurement N-2 is enforced.

```bash
git checkout -- crates/envoy-http1/src/hcm.rs
touch crates/envoy-http1/src/lib.rs
cargo test -p envoy-http1 --lib 2>&1 | tail -20   # control: GREEN
cd - && git worktree remove /tmp/er-mut-t6
```

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 110.1 task 6: family-wide seam coverage + the access-log/stats placement witnesses"
```

---

### Task 7: The io_uring seam

**Files:**
- Modify: `crates/envoy-http1/src/uring.rs`

**Interfaces:**
- Consumes: `crate::grpc::apply_grpc_local_reply` (Task 4).
- Produces: `async fn write_owned(down: &mut TcpStream, resp: &mut Response, req_headers: &[(String, String)], buf: &mut Vec<u8>) -> Result<(), Http1Error>` — the transform moves INSIDE the funnel so no call site can forget it.

**Why inside the funnel, not at the call sites.** Measured at this PLAN-write: `write_owned` has exactly FOUR call sites (`uring.rs:292`, `:313`, `:338`, `:389`) and **every one writes a synthetic local reply**. The proxied path uses a DIFFERENT writer, `write_head_body` (`uring.rs:376`). So in the io_uring worker `write_owned` IS the local-reply funnel, and putting the transform inside it makes the coverage structural rather than a four-way discipline. `uring.rs` has **no `#[cfg(test)]` and no `#[test]` at all** — there is no unit-test harness in this file — which is precisely why the seam must be unmissable by construction.

- [ ] **Step 1: Establish the red — confirm the seam is absent and the file compiles today**

```bash
cargo clippy -p envoy-http1 --all-targets --all-features -- -D warnings 2>&1 | tail -20
grep -n 'apply_grpc_local_reply' crates/envoy-http1/src/uring.rs   # must print NOTHING
```

Expected: clippy clean, and zero `apply_grpc_local_reply` hits in `uring.rs`. That zero IS the red: the io_uring worker currently serves untransformed local replies.

> `clippy` prints `Checking`, not `Compiling`, and a warm cache gives exit 0 with ZERO `Checking` lines — a cached no-op. Force a real dirty set with an mtime-only `touch crates/envoy-http1/src/lib.rs` first, and assert the `Checking envoy-http1` line appears.

- [ ] **Step 2: Change the funnel's signature and install the transform**

In `crates/envoy-http1/src/uring.rs`, replace the `write_owned` definition (currently at `:503`, locate by text `async fn write_owned`):

```rust
/// Serialize + write an owned synth `Response` (same head serializer and
/// coalescing rule as `Http1Response::write_to_buf`).
///
/// 110.1: this is the io_uring worker's LOCAL-REPLY WIRE FUNNEL, and the
/// gRPC transform lives INSIDE it deliberately. All four call sites write a
/// synthetic local reply; the proxied path uses `write_head_body` instead.
/// Installing the transform here rather than at the call sites makes the
/// coverage structural — a fifth local-reply site added later cannot forget it.
///
/// The tokio path has its OWN funnel in `hcm.rs`'s `serve_connection`, which
/// bypasses this function entirely; a transform installed at only one of the
/// two silently misses the other.
async fn write_owned(
    down: &mut TcpStream,
    resp: &mut Response,
    req_headers: &[(String, String)],
    buf: &mut Vec<u8>,
) -> Result<(), Http1Error> {
    crate::grpc::apply_grpc_local_reply(resp, req_headers);
    serialize_response_head(resp, buf);
    write_head_body(down, buf, &resp.body).await
}
```

Then update each of the four call sites. **Locate them by text, not by line number.**

Site 1 — the `BuildOutcome::Synth` arm (currently `:291-293`): bind the response mutably and pass the request headers.
```rust
            BuildOutcome::Synth(mut resp, _details) => {
                write_owned(&mut down, &mut resp, &req.headers, &mut write_buf).await?;
                tick_class(&config, resp.status);
            }
```

Site 2 — the `pick_endpoint() -> None` arm (currently `:312-314`):
```rust
                    let mut resp = synth_no_healthy_upstream(close);
                    write_owned(&mut down, &mut resp, &req.headers, &mut write_buf).await?;
                    tick_class(&config, resp.status);
```

Site 3 — the pool-acquire failure arm (currently `:336-339`):
```rust
                        let mut resp = synth_status(503, close);
                        cluster.record_response(endpoint, resp.status);
                        write_owned(&mut down, &mut resp, &req.headers, &mut write_buf).await?;
                        tick_class(&config, resp.status);
```

Site 4 — the upstream send/recv failure arm (currently `:387-390`): identical shape to site 3.

> **Two ordering facts to preserve, both already correct in the code above.**
> 1. `cluster.record_response(endpoint, resp.status)` is called BEFORE `write_owned` at sites 3 and 4. It must STAY before, so outlier detection keeps recording the ORIGINAL `503` — that is an upstream-health fact, not a downstream-reply fact.
> 2. `tick_class(&config, resp.status)` is called AFTER `write_owned` at all four sites. It must STAY after, so it sees the TRANSFORMED `200` and ticks `downstream_rq_2xx` — matching measurement N-2 and the tokio path's behaviour.

- [ ] **Step 3: Verify it compiles clean under the feature that gates it**

```bash
touch crates/envoy-http1/src/lib.rs
cargo clippy -p envoy-http1 --all-targets --all-features -- -D warnings 2>&1 | tee /tmp/t7-clippy.txt | tail -30
grep -c 'Checking envoy-http1' /tmp/t7-clippy.txt   # must be >= 1 — otherwise it was a cached no-op
```

Expected: clean, with at least one `Checking envoy-http1` line proving the run was real.

Then the workspace gate, which is what CI runs:
```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -20
cargo test --workspace 2>&1 | tail -20
```

- [ ] **Step 4: Verify the coverage claim structurally**

```bash
# Every write_owned call site must now pass request headers; there must be no
# local-reply writer in uring.rs that bypasses the funnel.
grep -n 'write_owned\|write_head_body' crates/envoy-http1/src/uring.rs
grep -c 'apply_grpc_local_reply' crates/envoy-http1/src/uring.rs   # must be exactly 1
```

Expected: exactly 4 `write_owned` call sites plus its definition and the one call inside it; exactly 1 `write_head_body` call site (the proxied path, untransformed); exactly 1 `apply_grpc_local_reply`.

Record the counts in `PROGRESS.md`. **This structural check is the io_uring seam's coverage evidence** — there is no test harness in this file, and saying so plainly is required rather than implying test coverage that does not exist. Bank the absence as a carry-forward (see Task 8 Step 4).

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-http1/src/uring.rs
git commit -m "phase 110.1 task 7: the io_uring wire-funnel seam — transform inside write_owned"
```

---

### Task 8: The W-4 HTTP/2 negative witness, and the state-3 exit gate

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs` (test only)
- Create: `docs/envoy-rust/phases/110.1-grpc-local-reply-transform/PROGRESS.md`

**Interfaces:**
- Consumes: `envoy_http1::{build_response, BuildOutcome, Request, Response}` — already imported at `crates/envoy-http2/src/hcm.rs:18`.
- Produces: no production code. The deliverable is the WITNESS that Global Constraint 1 holds.

**Why this test must exist.** The entire seam design — three separate insertion points instead of one obvious one in `synth_with` — exists solely to keep HTTP/2 untransformed. Without a positive assertion, that constraint is unwitnessed and a later refactor that "simplifies" the transform down into `build_response` would break H2 silently, with every test in this plan still green.

- [ ] **Step 1: Write the failing test**

Append inside `crates/envoy-http2/src/hcm.rs`'s `#[cfg(test)] mod tests`, beside the existing `h2_shared_seam_serves_the_redirect_arm` test (near `hcm.rs:7233`), which is the model for building an H2-side `build_response` call:

```rust
    /// 110.1 W-4 — THE NEGATIVE WITNESS THAT SHAPES THE WHOLE 110.1 DESIGN.
    ///
    /// HTTP/2 has no route-action dispatch of its own: it calls
    /// `envoy_http1::build_response` (`crates/envoy-http2/src/hcm.rs:513-518`).
    /// 110.1 makes envoy-rust rewrite LOCALLY GENERATED HTTP/1.1 replies into
    /// upstream's gRPC shape when the request carries a gRPC `content-type` —
    /// and it must do so at the H1 WIRE FUNNELS only, NEVER inside
    /// `synth_with`, any `synth_*` wrapper, or `build_response`.
    ///
    /// If anyone ever "simplifies" the transform down onto the shared path,
    /// this test goes RED. Without it, that refactor would silently transform
    /// H2's route-decision replies (direct_response / 400 / 404 / redirect)
    /// while leaving H2's own `synth_h2_*` upstream-failure family untouched —
    /// a PARTIALLY covered family on the H2 wire, exactly the ADR-0049
    /// silent-divergence class.
    ///
    /// HTTP/2 gRPC-aware local replies are CF-110-1 and are OUT OF SCOPE for
    /// 110.1. Their upstream shape IS measured (headers-only, no trailers,
    /// `content-length` OMITTED rather than `0`), so this test asserts
    /// envoy-rust's CURRENT untransformed H2 behaviour, not upstream parity.
    #[tokio::test]
    async fn h2_route_decision_reply_is_not_grpc_transformed() {
        let h1cfg = h2_direct_response_h1_config(404, "B404").await;
        let mut req = Request {
            method: "GET".to_string(),
            path: "/x".to_string(),
            version: envoy_http1::HttpVersion::Http11,
            headers: vec![
                ("host".to_string(), "envoy-rust.test".to_string()),
                ("content-type".to_string(), "application/grpc".to_string()),
            ],
            bytes_consumed: 0,
            body: None,
        };
        match build_response(&h1cfg, &mut req, false) {
            BuildOutcome::Synth(resp, _details) => {
                assert_eq!(
                    resp.status, 404,
                    "H2 must keep the CONFIGURED status — 110.1's transform must not reach the shared path"
                );
                assert_eq!(
                    resp.headers
                        .iter()
                        .find(|(n, _)| n.eq_ignore_ascii_case("content-type"))
                        .map(|(_, v)| v.as_str()),
                    Some("text/plain"),
                    "H2 must keep text/plain, not application/grpc"
                );
                assert!(
                    !resp.headers.iter().any(|(n, _)| n.eq_ignore_ascii_case("grpc-status")),
                    "H2 must carry NO grpc-status: {:?}",
                    resp.headers
                );
                assert!(
                    !resp.headers.iter().any(|(n, _)| n.eq_ignore_ascii_case("grpc-message")),
                    "H2 must carry NO grpc-message: {:?}",
                    resp.headers
                );
                assert_eq!(&resp.body[..], b"B404", "H2 must keep the body");
            }
            _other => panic!("expected BuildOutcome::Synth from the shared seam"),
        }
    }
```

> Reuse the existing H2 config builder if one matches — `grep -n 'async fn h2_.*_h1_config' crates/envoy-http2/src/hcm.rs`. `h2_redirect_h1_config` exists; a `direct_response` sibling may too. Only write `h2_direct_response_h1_config` if none exists, modelling it on `h2_redirect_h1_config`.

- [ ] **Step 2: Run the test to verify it passes, then MUTATE to prove it is not vacuous**

Run: `cargo test -p envoy-http2 --lib h2_route_decision_reply_is_not_grpc_transformed 2>&1 | tail -20`

Expected: **GREEN immediately.** That is correct and expected — this is a CHARACTERIZATION PIN, not a feature test. Its red comes from the mutation, which is the actual evidence:

```bash
git worktree add /tmp/er-mut-t8 HEAD
cd /tmp/er-mut-t8
```
Install the transform on the shared path — append to `build_response` in `crates/envoy-http1/src/hcm.rs` so it transforms unconditionally:
```rust
pub fn build_response(config: &HCMConfig, req: &mut Request, close: bool) -> BuildOutcome {
    let mut out = build_response_in(&config.current_route_config(), req, close, &config.runtime);
    if let BuildOutcome::Synth(ref mut r, _) = out {
        crate::grpc::apply_grpc_local_reply(r, &req.headers);
    }
    out
}
```
```bash
touch crates/envoy-http1/src/lib.rs crates/envoy-http2/src/lib.rs
cargo test -p envoy-http2 --lib h2_route_decision_reply_is_not_grpc_transformed 2>&1 | tail -25
```
Expected: **RED** — `H2 must keep the CONFIGURED status`. This is the proof that the constraint is enforced rather than merely documented.

```bash
git checkout -- crates/envoy-http1/src/hcm.rs
touch crates/envoy-http1/src/lib.rs crates/envoy-http2/src/lib.rs
cargo test -p envoy-http2 --lib h2_route_decision 2>&1 | tail -20   # control: GREEN
cd - && git worktree remove /tmp/er-mut-t8
```

- [ ] **Step 3: Run the full state-3 exit gate**

Run each and capture the FULL output to a file (never pipe a verification run through `tail` — it truncates the `failures:` block):

```bash
cargo build --workspace --all-targets           > /tmp/g-build.txt 2>&1; echo "build=$?"
cargo clippy --workspace --all-targets --all-features -- -D warnings > /tmp/g-clippy.txt 2>&1; echo "clippy=$?"
cargo fmt --all -- --check                      > /tmp/g-fmt.txt 2>&1; echo "fmt=$?"
cargo test --workspace --no-fail-fast           > /tmp/g-test.txt 2>&1; echo "test=$?"
cargo deny check                                > /tmp/g-deny.txt 2>&1; echo "deny=$?"
```

Census the test run with the standing recipe (the `ok`-only form makes `failed=0` tautological):
```bash
grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed' /tmp/g-test.txt \
  | awk '{p+=$4; f+=$6; n++} END {print "binaries="n, "passed="p, "failed="f}'
grep -c 'test result: FAILED' /tmp/g-test.txt
```

**Classify every RED by ISOLATION, never by text.** Re-run each failing test alone; a test that PASSES alone is a known local flake (ADR-0164: a stable core of five — four `access_log_*_upstream_reset` plus `admin_config_dump_server_info`, all LOCAL-only — plus an open-ended startup-race / container-readiness tail whose membership and size both move run to run). A test that fails DETERMINISTICALLY in isolation is a real regression. CI is authoritative for the backend-routing differential fixtures, which go RED on this host's `192.168.65.2` bridge.

Also assert the sub-phase's structural non-goals:
```bash
git ls-files 'tests/fixtures/**' | cut -d/ -f3 | sort -u | wc -l   # must still be 88
git status --porcelain -- Cargo.toml Cargo.lock                    # must be EMPTY
grep -rn 'grpc' crates/envoy-config/src/lib.rs | grep -i 'configerror\|GrpcLocalReply'  # no new variant
```

- [ ] **Step 4: Write `PROGRESS.md`**

Create `docs/envoy-rust/phases/110.1-grpc-local-reply-transform/PROGRESS.md` with, per task: what landed, the command outputs QUOTED (not summarized), the mutation RED evidence with its unmutated control, and honest statements of what is NOT covered. It must record at minimum:

- The per-task test counts and the workspace census (`binaries=`/`passed=`/`failed=`), with every RED classified by isolation.
- **That `crates/envoy-http1/src/uring.rs` has NO test harness**, so the io_uring seam's evidence is the Task 7 Step 4 structural check plus `clippy --all-features`, not a test. **Bank this as a NEW carry-forward CF-110-5: the io_uring local-reply seam is unwitnessed by any test.**
- **Bank CF-110-4** (from this PLAN's measurements): envoy-rust's non-gRPC `synth_with` header order differs from upstream's — pre-existing, order-only, invisible to `diff_headers`, not fixed here.
- That CF-110-3 (upstream's `location` on a `201`/`3xx` `direct_response`) is unchanged and remains a hazard `110.2`'s fixture must avoid.
- That NO fixture, NO `BEHAVIOR_CONTRACT.md` edit, NO config surface, NO fuzz target and NO dependency change landed.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-http2/src/hcm.rs docs/envoy-rust/phases/110.1-grpc-local-reply-transform/PROGRESS.md
git commit -m "phase 110.1 task 8: W-4 H2 negative witness + state-3 exit gate [PROGRESS.md]"
```

---

## Definition of done — the §7.5 gate, instantiated for 110.1

- **(a)** No new differential fixture — none is in scope (Global Constraint 3). **N/A.**
- **(b)** All **88** pre-existing differential fixtures still green. Blast radius was MEASURED ZERO (W-6): no fixture or test anywhere under `tests/` sends a gRPC `content-type` or asserts on `grpc-status`/`grpc-message`. CI is authoritative for the backend-routing fixtures.
- **(c)** Conformance unchanged — h2spec threshold untouched, `known-failures.txt` untouched at 21 lines / ONE real entry. Assert `grep -c 'h2spec not found'` = 0 over the CI build log WITH `test h2spec_pass_rate_gate ... ok` present; that gate self-skips SILENTLY on a developer host, so a local green proves nothing (ADR-0163).
- **(d)** **No new fuzz target** — no parser, no codec, no filter, no config surface, so §7.4's trigger does not fire. The five existing targets stay green.
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace` and `cargo deny check` all clean at WORKSPACE scope. **`--all-features` is not optional** — it is the only gate that compiles the io_uring seam (Global Constraint 8).
- **(f)** `REVIEW.md` APPROVED — written by a SEPARATE state-5 session (§5.1; ADR-0127).

## What this sub-phase does NOT do

1. **No differential fixture.** `0089` is `110.2`'s.
2. **No HTTP/2 gRPC local replies** — CF-110-1. Shape measured, out of scope; Task 8 witnesses that H2 is untouched.
3. **No trailer API of any kind.**
4. **No proxied/upstream-originated gRPC responses** — CF-110-2.
5. **No `grpc_web` / gRPC bridge / gRPC-JSON transcoding / `grpc_stats`.**
6. **No `grpc_status_filter`** (access-log) — rejected at ADR-0154 DECISION 7.
7. **No `fault.grpc_status` abort** — the deferral at `crates/envoy-config/src/bootstrap.rs:1296` stays.
8. **No config surface**, therefore no `ConfigError` variant, no corpus seed, no fuzz target.
9. **No `BEHAVIOR_CONTRACT.md` edit** — the `## gRPC` section is `110.2`'s.
10. **No fix to any banked finding**, and no change to `synth_with`'s existing non-gRPC header order (CF-110-4).

## Next state

At this plan's completion the sub-phase sits at §5 **state 3 complete** (`SPEC.md` + `PLAN.md` + `PROGRESS.md`, no `REVIEW.md`). The next session runs §5 **state 4** (`superpowers:verification-before-completion`) — a SEPARATE session per §5.1 and ADR-0127. Sibling `110.2` (fixture `0089` + the `BEHAVIOR_CONTRACT.md` `## gRPC` section + the parent-110 close) follows only after `110.1` is `done`.
