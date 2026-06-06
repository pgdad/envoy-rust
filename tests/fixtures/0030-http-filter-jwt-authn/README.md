# Fixture 0030: HTTP filter — JWT authn

Phase 22 differential acceptance fixture for `envoy.filters.http.jwt_authn`.
Both upstream Envoy (v1.33.0) and envoy-rust must produce the deterministic
status sequence `[200, 401, 401, 401, 403]` for 5 sequential `GET /` requests
(`Host: envoy.test`) against a single direct_response route, given an HCM
filter chain of `[envoy.filters.http.jwt_authn, envoy.filters.http.router]`
with a single provider `provider1` required on the `prefix: "/"` route.

This is the FIRST crypto-in-a-filter differential fixture (RS256 verify via
the new `envoy-jwt` crate over `aws-lc-rs`). References ADR-0055 (phase-22
SPEC; the `envoy-jwt` crate) and ADR-0056 (phase-22 PLAN).

## Filter chain

```
http_filters:
  - envoy.filters.http.jwt_authn (provider1; rule prefix "/" requires provider1)
  - envoy.filters.http.router (terminus)
```

Decode-side iteration: jwt_authn invokes first (declaration order). It selects
the first `rules[]` entry whose `RouteMatch` matches, extracts the JWT from
`Authorization: Bearer`, verifies RS256 against the provider's inline JWKS, and
validates `iss` / `aud` / `exp`. On success: `Decision::Continue` falls through
to router → direct_response 200 + `"ok\n"`; `http.ingress_http.jwt_authn.allowed`
increments. On failure: `Decision::StopAndSend` with a byte-exact local reply
and `http.ingress_http.jwt_authn.denied` increments.

## Probe burst

| # | probe          | Authorization                | status | body                                |
|---|----------------|------------------------------|--------|-------------------------------------|
| 1 | valid          | `Bearer <valid.jwt>`         | 200    | `ok\n`                              |
| 2 | missing         | (none)                       | 401    | `Jwt is missing`                    |
| 3 | tampered        | `Bearer <tampered.jwt>`      | 401    | `Jwt verification fails`            |
| 4 | expired         | `Bearer <expired.jwt>`       | 401    | `Jwt is expired`                    |
| 5 | wrong-audience  | `Bearer <wrong_aud.jwt>`     | 403    | `Audiences in Jwt are not allowed`  |

The bodies are byte-exact across proxies: upstream Envoy v1.33's
`envoy.extensions.filters.http.jwt_authn.v3.JwtAuthentication` source-hardcodes
these strings; envoy-rust matches them in
`crates/envoy-filter/src/jwt_authn.rs` (`error_reply` / `missing_reply`).

### `www-authenticate`

Every denied probe carries a `www-authenticate` response header:

- Probe 2 (missing): `Bearer realm="http://envoy.test/"` (NO `error=`).
- Probes 3/4/5 (non-missing failure): `Bearer realm="http://envoy.test/", error="invalid_token"`.

The realm is `http://{Host}{path}` (scheme `http`, port-independent per the
SPEC §6.2 L3 lock). Because BOTH proxies receive the SAME `Host: envoy.test`
and path `/`, the realm `http://envoy.test/` is identical on both sides, so the
`www-authenticate` value is **byte-exact cross-proxy**. It is deliberately NOT
on the harness header allow-list (`docs/envoy-rust/BEHAVIOR_CONTRACT.md`), so
`set_equal_modulo_allow_list` compares it value-exact — the bilateral proof of
the realm-construction contract.

## Static test data (`inputs/`)

`inputs/gen.sh` is a reproducible bash + openssl + python3-stdlib generator
(NO PyJWT / `cryptography`). It:

1. `openssl genrsa 2048` (the `RSA_PKCS1_2048_8192_SHA256` floor — RSA-2048
   MANDATORY).
2. Extracts the modulus `n` (raw big-endian bytes, base64url) + `e = AQAB`
   (65537) and writes `jwks.json` (`kty: RSA`, `kid: k1`, `alg: RS256`).
3. Builds + RS256-signs 4 tokens via `openssl dgst -sha256 -sign`:
   - `valid.jwt`     — `iss` ok, `aud: [jwt-fixture-aud]`, `exp: 4102444800` (year 2100).
   - `expired.jwt`   — same iss/aud, `exp: 1500000000` (year 2017).
   - `wrong_aud.jwt` — `aud: [other-aud]`, far-future `exp`.
   - `tampered.jwt`  — `valid.jwt` with its LAST signature char flipped (still
     base64url-decodable; signature no longer verifies).

The tokens + JWKS are committed STATIC and DETERMINISTIC forever: every `exp`
is far-future (year 2100) or far-past (year 2017), so there is **zero clock
sensitivity** — the 5 verdicts never drift with wall-clock time, on either
proxy. Only the PUBLIC `jwks.json` + the 4 tokens + the generator are
committed; the RSA private key is generated in `/tmp` and never committed.

## Inline JWKS on both sides

The provider's `local_jwks.inline_string` is the EXACT single-line contents of
`inputs/jwks.json`, and is BYTE-IDENTICAL in `envoy.yaml` and `envoy-rust.yaml`
(verified). Both proxies verify against the same public key, so an accepted
token (probe 1 → 200) on BOTH proxies is the cross-proxy proof that the
`n`/`e`/signature set is internally consistent.

## Assertion strategy

5 sequential `Http1Probe` entries (`Driver::Http1ProbeList`) with per-probe
`extra_headers` carrying the `authorization` value. Each probe asserts:

- `expected_status` exact (200 / 401 / 401 / 401 / 403).
- `expected_body: { kind: byte_exact }` (the table above; both proxies emit
  identical bytes).
- `expected_headers: set_equal_modulo_allow_list` — cross-proxy header-set
  equality modulo the `server` + `date` allow-list rows; the remaining standard
  headers (`content-length` / `content-type` / `connection`) plus
  `www-authenticate` are value-exact across proxies under the harness's
  `Connection: close` request framing.

Top-level `equivalence: { response_status: exact, response_body: { kind:
byte_exact } }`.

## Stats wired (per BEHAVIOR_CONTRACT.md `Stat-name mapping`)

- `http.ingress_http.jwt_authn.allowed` — 1 (probe 1 verified).
- `http.ingress_http.jwt_authn.denied`  — 4 (probes 2–5 denied).

## Per-side YAML asymmetry

`envoy.yaml` (upstream) carries an `admin` block (`port_value: 0`;
kernel-ephemeral), bind `0.0.0.0:{{PORT}}` (Docker container public bind), and
`generate_request_id: false` (envoy-rust does not inject `x-request-id`;
disable upstream injection for header-set parity). `envoy-rust.yaml` carries
the symmetric narrow shape: no `admin` block, bind `127.0.0.1:{{PORT}}`, no
`generate_request_id` field (envoy-rust's HCM config does not model it). The
HCM body — `http_filters`, `route_config`, `stat_prefix`, `codec_type`, the
inline JWKS — is otherwise identical between the two files.
