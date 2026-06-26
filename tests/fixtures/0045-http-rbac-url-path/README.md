# Fixture 0045 — `0045-http-rbac-url-path`

Phase 37 differential acceptance fixture for the RBAC **`url_path`** condition
(Envoy `type.matcher.v3.PathMatcher`, `url_path: { path: { <StringMatcher> } }`)
on the existing phase-10 `envoy.filters.http.rbac` filter.

## Chain

A plain `[envoy.filters.http.rbac, envoy.filters.http.router]` HCM chain — **no
producer** (unlike the phase-35/36 `metadata` fixtures `0043`/`0044`, which need a
`header_to_metadata` producer-before-consumer). `url_path` is self-contained: it
reads the request path directly. The route is a single
`direct_response { status: 200, body: "ok\n" }`, so the fixture is fully
upstream-independent (no backend cluster; `clusters: []`).

RBAC is `action: ALLOW`, one policy `allow_path`:
`url_path: { path: { exact: "/allowed" } }` Permission, `any: true` Principal.

## Probes (the match / miss / query-strip trio)

| probe | request | verdict | response |
|---|---|---|---|
| 1 | `GET /allowed` | exact match | `200` + `ok\n` |
| 2 | `GET /denied` | no match | `403` + `RBAC: access denied` (19B) |
| 3 | `GET /allowed?x=1` | query `?x=1` **stripped** → matches `/allowed` | `200` + `ok\n` |

**Probe 3 is the load-bearing differential** (ADR-0090 §B): Envoy matches
`url_path` against the request-target with everything from the first `?` removed.
envoy-rust's route matcher compares the raw request-target byte-for-byte
(`crates/envoy-http1/src/hcm.rs`), so a naive whole-`:path` compare would see
`/allowed?x=1 ≠ /allowed` and 403 it — probe 3 proves the new strip-at-first-`?`
helper (`crates/envoy-filter/src/rbac.rs::strip_query`) is genuinely query-stripping.

The `403` body is `RBAC: access denied` (19 bytes, **no** trailing newline) per
phase-10 ADR-0034.

## Out of scope (carry-forwards)

- **`#fragment` (M37-1, ADR-0090 R1):** `/allowed#frag` is rejected at the H1 codec
  as a `400` before `url_path` matching — a separate codec request-target surface.
- **Path normalization beyond query-strip (ADR-0090 §B):** Envoy at this pin applies
  NO percent-decode / dot-segment / slash-merge / case-fold by default; the probes
  are already-normalized so the cross-proxy verdict is byte-portable. A faithful
  normalization slice (with `normalize_path`/`merge_slashes` ON) is a future phase.
- **Unanchored `safe_regex` partial-vs-full (M36-1):** the backstop locks an anchored
  `^…$` pattern (partial==full); the cross-cutting full-match fix stays deferred.

## Per-side YAML asymmetry

`envoy.yaml` (upstream reference) has the `admin` block, binds `0.0.0.0`, and sets
`generate_request_id: false`. `envoy-rust.yaml` omits all three (binds `127.0.0.1`,
no admin probe for a `direct_response` fixture, envoy-rust never injects
`x-request-id`). The HCM body is otherwise byte-identical. The `{{PORT}}` token is
substituted per run by the harness. Follows the fixture-0017/0043 precedent.

## Locality

LOCALLY authoritative (a normal request/response — no file-watch/reload trigger,
unlike phases 26/27). Docker-gated by the differential harness at the cluster level.
Run: `cargo test -p differential rbac_url_path` (filter by the test NAME, not `0045`).
