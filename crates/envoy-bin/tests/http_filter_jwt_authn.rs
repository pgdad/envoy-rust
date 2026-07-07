//! Phase-22 in-process backstop: end-to-end jwt_authn filter exercise against a
//! real envoy-bin subprocess (no Docker).
//!
//! Complements Docker fixture 0030 — adds the malformed-JWT case (probe 6) that
//! the differential harness allow-list machinery does not cover, and directly
//! asserts the `www-authenticate` header presence + exact value (phase-10 M1
//! lesson: the backstop cannot rely on the harness allow-list here).
//!
//! NOTE (M21-3/M18-9): extract-a-shared-test-support-crate is now at N≥6
//! in-process backstops (this file is the 6th). Consolidation stays deferred
//! per the standing risk-managed decision — the duplication is mechanical and
//! the refactor carries non-trivial risk relative to the value at this stage.
//!
//! Bootstrap shape: HCM (codec_type HTTP1) + [envoy.filters.http.jwt_authn,
//! envoy.filters.http.router] with provider1 (issuer:
//! "testing@secure.istio.io", audiences: ["jwt-fixture-aud"], local inline
//! JWKS, forward: false/default) and one rule `match {prefix: "/"}` →
//! `requires {provider_name: "provider1"}`. Router → direct_response 200 "ok\n".
//!
//! 6 sequential GET / probes (Host: envoy.test):
//!   probe 1 (valid JWT)           → 200, body "ok\n"
//!   probe 2 (no Authorization)    → 401, body "Jwt is missing"
//!                                    www-authenticate: Bearer realm="http://envoy.test/"
//!   probe 3 (tampered signature)  → 401, body "Jwt verification fails"
//!                                    www-authenticate contains `, error="invalid_token"`
//!   probe 4 (expired JWT)         → 401, body "Jwt is expired"
//!   probe 5 (wrong audience)      → 403, body "Audiences in Jwt are not allowed"
//!   probe 6 (malformed: not.a.jwt)→ 401, body "Jwt header is an invalid JSON"
//!            RATIONALE: "not.a.jwt" is 3 non-empty segments (passes NotInForm
//!            check). The header segment "not" base64url-decodes to 2 bytes
//!            (0x9e 0x8b) that are not valid JSON → BadHeaderJson →
//!            "Jwt header is an invalid JSON".

#![forbid(unsafe_code)]

use std::io::Write;
use std::net::SocketAddr;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

mod common;

use common::{dump_stderr_and_kill, reserve_port, wait_ready};

// ---- Static JWT tokens + JWKS (RSA-2048 / RS256, deterministic forever) -----
// Source: tests/fixtures/0030-http-filter-jwt-authn/inputs/
// The JWKS inline_string and all 4 token constants are BYTE-IDENTICAL to the
// fixture input files — copy them verbatim to keep the backstop self-contained.

const JWKS: &str = r#"{"keys":[{"kty":"RSA","kid":"k1","use":"sig","alg":"RS256","n":"rF6xHfq-E5a7xDgXOzQbUxVjiB-t-Ot2L5kOg1VZPEDZ7xh2WSnqTlzojuHpJecoiimQ-4Wu9GAM0SEWaedUypbVsWNAaDLaeNA4j9IpGcl2L5Q1EI4qA3hPpi21KRdQnv3chnAb5M9uLBwZoXfDaOTD_SqrE846LR0GOFsV_mSwHEa9Nb6aP2y_DXetyyL2i3a7OXFF-IP-35zOIorWlswnomCwnbn2YlJYTi6SvDtxwvgC3AtdzO3SFHndRT71DrRapLV0tZT0GOF9PkbsTh_E1ooqCLMXt4z4dgVKcjk4prKQ2aaeoY5qeXMEzJmzE5x8YUm30DMjdlUJFJahAQ","e":"AQAB"}]}"#;

const VALID_JWT: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6ImsxIiwidHlwIjoiSldUIn0.eyJpc3MiOiJ0ZXN0aW5nQHNlY3VyZS5pc3Rpby5pbyIsImF1ZCI6WyJqd3QtZml4dHVyZS1hdWQiXSwiZXhwIjo0MTAyNDQ0ODAwfQ.aZbLHegW9Z6sAXNL-D14IFuXrfJuj_K4vWCE4tUK0wBQacEp697raigKjp8c0585xe583iZUqoveFtqXF9qlTjXzQlChS-ba6eoDf7uEnVxmESTzEKBiSbbVrexKfvrJNW3QmnhzAA65ZOb7xhrg8vzsHvnR9f-GUGxbUI4Xxf3zOyDLhAMG6ssIal99YmvR-oWzC2_ly0XWAyVnMKNXfv8RX-D011vkgmRSuZMgjREkQOpD-cs2LUqs0BNCdd2Xkm8UJuiwsIuA8LIAKYTd3ya_ClANkUQ46qyrJsT15eTdYqrqNu4PH5LBqSxS3Bm4mrizADdwSOhVKWrqHGjRSQ";

const TAMPERED_JWT: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6ImsxIiwidHlwIjoiSldUIn0.eyJpc3MiOiJ0ZXN0aW5nQHNlY3VyZS5pc3Rpby5pbyIsImF1ZCI6WyJqd3QtZml4dHVyZS1hdWQiXSwiZXhwIjo0MTAyNDQ0ODAwfQ.aZbLHegW9Z6sAXNL-D14IFuXrfJuj_K4vWCE4tUK0wBQacEp697raigKjp8c0585xe583iZUqoveFtqXF9qlTjXzQlChS-ba6eoDf7uEnVxmESTzEKBiSbbVrexKfvrJNW3QmnhzAA65ZOb7xhrg8vzsHvnR9f-GUGxbUI4Xxf3zOyDLhAMG6ssIal99YmvR-oWzC2_ly0XWAyVnMKNXfv8RX-D011vkgmRSuZMgjREkQOpD-cs2LUqs0BNCdd2Xkm8UJuiwsIuA8LIAKYTd3ya_ClANkUQ46qyrJsT15eTdYqrqNu4PH5LBqSxS3Bm4mrizADdwSOhVKWrqHGjRSB";

const EXPIRED_JWT: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6ImsxIiwidHlwIjoiSldUIn0.eyJpc3MiOiJ0ZXN0aW5nQHNlY3VyZS5pc3Rpby5pbyIsImF1ZCI6WyJqd3QtZml4dHVyZS1hdWQiXSwiZXhwIjoxNTAwMDAwMDAwfQ.GJiQRUrdhigMHUoRYEHBh1AJ1m6qi857vx-iLrUKvDH0DJGizJWsttmLYPz-LhTalqzmCZLkIXRiM4rpt2XrZdQRfDWFnX4JxIcSJlANpQt6Bt06c-R7CEikPk2Jqsyeskf5QEoRTM2y900bPEVZddWaGrQhkBInxkyM_Y0VhQHWeHrC_BzIm76hFSGDcSs3GzuR2pVs1YHNmI_3KMmJ0HtU3LgFPwqr4dC9_j85truS-AOlpLkxZL_hLti1eT2Fvc0o89vF_zg0mwgRnT5UpJPGNPvDXsjQv_Ok287yd2HzTtKQVBY5a9kyzdan2pZeLMyNYh6Kj6qEpxbhizY4pg";

const WRONG_AUD_JWT: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6ImsxIiwidHlwIjoiSldUIn0.eyJpc3MiOiJ0ZXN0aW5nQHNlY3VyZS5pc3Rpby5pbyIsImF1ZCI6WyJvdGhlci1hdWQiXSwiZXhwIjo0MTAyNDQ0ODAwfQ.etnKuLttA3_ODRK4sd5IVNWh8tf_xWXQjV0nfhIUQaqdmOkey8JN-TbP4fQujesa3TGIPZOE7D5euTjiPjKriGIfDFs4OVDNqYWAbIky3lFhLw2hiOeTallecUsCFn8sXIn_2UonZIFxtB_DMUCbv3aUE1gop4vXKFjB0ZBKH5nPoX0J_NHgQoZZ3tyt8p1LsCxIEyYb3ew9OByaumLkOqrT5m7sBW3K7urFRf_SZkt6V49HK6WU3DLn7aVsZInicyoUr33XgnrWdfjMjoPG_MVjj_JLymZWLxQd7Oc4qCZo0avjTG1p2c3fhFbJJvd6-tELa44adyGwuHGG00o4DQ";

/// Open a fresh TCP connection to `addr`, write an HTTP/1.1 GET with
/// `Connection: close` and `Host: envoy.test` (and optionally an
/// `Authorization` header), read-to-end, split head/body at `\r\n\r\n`, parse
/// the status code from the status line, parse response header name/value
/// pairs, and return `(status, headers, body)`. Panics on any I/O or parse
/// failure.
///
/// Extends the rbac backstop's `probe` (which returned only `(status, body)`)
/// to surface the parsed response headers so the `www-authenticate` assertions
/// (phase-10 M1 lesson) can be applied — mirroring the fault backstop's
/// `http1_get` pattern.
async fn probe(
    addr: SocketAddr,
    authorization: Option<&str>,
) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let mut stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(addr))
        .await
        .expect("probe connect timeout")
        .expect("probe connect");
    let mut req = String::from("GET / HTTP/1.1\r\nHost: envoy.test\r\nConnection: close\r\n");
    if let Some(value) = authorization {
        req.push_str(&format!("Authorization: {value}\r\n"));
    }
    req.push_str("\r\n");
    stream
        .write_all(req.as_bytes())
        .await
        .expect("write request");
    let mut buf = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut buf))
        .await
        .expect("probe read timeout")
        .expect("probe read");
    let head_end = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("\\r\\n\\r\\n header terminator not found");
    let head = &buf[..head_end];
    let body = buf[head_end + 4..].to_vec();
    let head_str = std::str::from_utf8(head).expect("ASCII response head");
    let mut lines = head_str.lines();
    let status_line = lines.next().expect("status line");
    // e.g. "HTTP/1.1 401 Unauthorized"
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .expect("status code token")
        .parse()
        .expect("parse status code");
    // Remaining lines are `Name: value` header fields.
    let headers: Vec<(String, String)> = lines
        .filter(|l| !l.is_empty())
        .map(|l| {
            let (name, value) = l.split_once(':').expect("header field colon");
            (name.trim().to_string(), value.trim().to_string())
        })
        .collect();
    (status, headers, body)
}

#[tokio::test]
async fn http_filter_jwt_authn_in_process_backstop() {
    let admin_port = reserve_port();
    let listener_port = reserve_port();

    // Bootstrap YAML mirrors fixture 0030 (`tests/fixtures/0030-http-filter-jwt-authn/
    // envoy-rust.yaml`) with concrete port values substituted in.
    // `codec_type: HTTP1` is required by the envoy-config schema (all precedent
    // backstops include it; this backstop does the same).
    let bootstrap_yaml = format!(
        r#"node:
  cluster: phase-22-jwt-authn-backstop
  id: phase-22-jwt-authn-backstop
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: {admin_port}
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {listener_port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: default
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok\n" }}
                http_filters:
                  - name: envoy.filters.http.jwt_authn
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.jwt_authn.v3.JwtAuthentication
                      providers:
                        provider1:
                          issuer: "testing@secure.istio.io"
                          audiences: ["jwt-fixture-aud"]
                          local_jwks:
                            inline_string: '{JWKS}'
                      rules:
                        - match: {{ prefix: "/" }}
                          requires: {{ provider_name: "provider1" }}
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = dir.path().join("bootstrap.yaml");
    std::fs::File::create(&cfg)
        .unwrap()
        .write_all(bootstrap_yaml.as_bytes())
        .unwrap();

    // Per phase-09 REVIEW M3 + phase-10 SPEC §6.4 + rbac/fault precedent:
    // tokio::process::Command + .kill_on_drop(true). stderr is Stdio::piped()
    // (NOT Stdio::null()) so envoy-bin startup/runtime errors surface on
    // failure — load-bearing for diagnosis.
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c")
        .arg(&cfg)
        .env("ENVOY_RUST_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin");

    let listener_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();

    // Wait for the data-plane listener to bind. Dump stderr on failure so any
    // envoy-bin startup error surfaces in test output (rbac + fault precedent).
    let ready = tokio::time::timeout(
        Duration::from_secs(10),
        wait_ready(listener_addr, Duration::from_secs(10)),
    )
    .await;
    if ready.is_err() || matches!(&ready, Ok(Err(_))) {
        dump_stderr_and_kill(&mut child).await;
        panic!("envoy-bin listener never became ready at {listener_addr}");
    }

    // ---- 6 sequential probes — §6.2 L2 surface ---------------------------------

    // probe 1: valid JWT → 200, body "ok\n"
    let (s1, _h1, b1) = probe(listener_addr, Some(&format!("Bearer {VALID_JWT}"))).await;

    // probe 2: missing Authorization → 401, "Jwt is missing"
    //          www-authenticate EXACT: `Bearer realm="http://envoy.test/"`
    //          (phase-10 M1 lesson: assert header presence + exact value here
    //          since the differential allow-list machinery is not in play)
    let (s2, h2, b2) = probe(listener_addr, None).await;

    // probe 3: tampered signature → 401, "Jwt verification fails"
    //          www-authenticate CONTAINS `, error="invalid_token"`
    let (s3, h3, b3) = probe(listener_addr, Some(&format!("Bearer {TAMPERED_JWT}"))).await;

    // probe 4: expired JWT → 401, "Jwt is expired"
    let (s4, _h4, b4) = probe(listener_addr, Some(&format!("Bearer {EXPIRED_JWT}"))).await;

    // probe 5: wrong audience → 403, "Audiences in Jwt are not allowed"
    let (s5, _h5, b5) = probe(listener_addr, Some(&format!("Bearer {WRONG_AUD_JWT}"))).await;

    // probe 6: malformed token "not.a.jwt"
    // "not.a.jwt" has 3 non-empty dot-separated segments → passes NotInForm.
    // Header segment "not" base64url-decodes to 2 bytes (0x9e 0x8b) which are
    // not valid JSON → BadHeaderJson → 401, "Jwt header is an invalid JSON".
    let (s6, _h6, b6) = probe(listener_addr, Some("Bearer not.a.jwt")).await;

    // ---- Assertions -------------------------------------------------------------

    // On any failure, dump stderr so envoy-bin runtime errors surface.
    let all_ok = s1 == 200
        && b1 == b"ok\n"
        && s2 == 401
        && b2 == b"Jwt is missing"
        && s3 == 401
        && b3 == b"Jwt verification fails"
        && s4 == 401
        && b4 == b"Jwt is expired"
        && s5 == 403
        && b5 == b"Audiences in Jwt are not allowed"
        && s6 == 401
        && b6 == b"Jwt header is an invalid JSON";
    if !all_ok {
        dump_stderr_and_kill(&mut child).await;
    }

    // probe 1: valid
    assert_eq!(s1, 200, "probe-1 (valid JWT) → 200");
    assert_eq!(b1.as_slice(), b"ok\n", "probe-1 body");

    // probe 2: missing — status + body + www-authenticate exact value
    assert_eq!(s2, 401, "probe-2 (missing Authorization) → 401");
    assert_eq!(b2.as_slice(), b"Jwt is missing", "probe-2 body");
    let wa2 = h2
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("www-authenticate"))
        .map(|(_, v)| v.as_str())
        .unwrap_or("");
    assert!(
        !wa2.is_empty(),
        "probe-2: www-authenticate header must be present; headers: {h2:?}"
    );
    assert_eq!(
        wa2, r#"Bearer realm="http://envoy.test/""#,
        "probe-2: www-authenticate exact value (no error= on missing)"
    );

    // probe 3: tampered — status + body + www-authenticate contains error=invalid_token
    assert_eq!(s3, 401, "probe-3 (tampered signature) → 401");
    assert_eq!(b3.as_slice(), b"Jwt verification fails", "probe-3 body");
    let wa3 = h3
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("www-authenticate"))
        .map(|(_, v)| v.as_str())
        .unwrap_or("");
    assert!(
        wa3.contains(r#", error="invalid_token""#),
        r#"probe-3: www-authenticate must contain `, error="invalid_token"`; got: {wa3:?}"#
    );

    // probe 4: expired
    assert_eq!(s4, 401, "probe-4 (expired JWT) → 401");
    assert_eq!(b4.as_slice(), b"Jwt is expired", "probe-4 body");

    // probe 5: wrong audience → 403
    assert_eq!(s5, 403, "probe-5 (wrong audience) → 403");
    assert_eq!(
        b5.as_slice(),
        b"Audiences in Jwt are not allowed",
        "probe-5 body"
    );

    // probe 6: malformed → BadHeaderJson
    assert_eq!(s6, 401, "probe-6 (malformed: not.a.jwt) → 401");
    assert_eq!(
        b6.as_slice(),
        b"Jwt header is an invalid JSON",
        "probe-6 body"
    );

    // Explicit kill + wait on the success path (kill_on_drop is the safety net;
    // explicit kill+wait is the discipline per the rbac + fault backstop precedent).
    child.kill().await.ok();
    let _ = child.wait().await;
}
