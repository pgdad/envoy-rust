# Fixture 0012 — access-log file sink

The first fixture exercising envoy-rust's HCM access-log subsystem. An H1
direct_response listener with a `file` access-log emits one access-log line
per request to a configured path; the harness reads each proxy's file and
diffs per-token per the rules in `expectations.yaml`.

## Surface

- HCM with `codec_type: HTTP1`, single virtual host `domains: ["*"]`,
  single route `prefix: "/"` `direct_response { status: 200, body:
  "ok\n" }`.
- Access log configured with the file logger; path declared in each
  side's YAML. The harness reads from the declared path post-request.

## Per-side divergences

| Side | bind address | admin block | access-log path                      | `generate_request_id` |
|------|--------------|-------------|--------------------------------------|-----------------------|
| envoy | `0.0.0.0`   | yes (port 0)| `/tmp/0012-envoy-access.log`         | `false` (load-bearing) |
| envoy-rust | `127.0.0.1` | omitted | `/tmp/0012-envoy-rust-access.log`    | omitted (never injects) |

The `generate_request_id: false` on the Envoy side prevents Envoy from
injecting an `x-request-id` header at HCM time; envoy-rust never injects
that header (per 04.3 SPEC §4 non-goal). Without this knob, the
`%REQ(X-REQUEST-ID)%` token would emit a UUID on Envoy's line and `-` on
envoy-rust's line, breaking the value-exact equivalence. Mirrors the
posture used in fixture 0008.

## Per-token equivalence

See the per-token rules in `expectations.yaml`. The authoritative
disposition table is `docs/envoy-rust/BEHAVIOR_CONTRACT.md` `Access log
field mapping` section (populated for the first time by this fixture's
commit).

## Driver

`Driver::Http1WithAccessLog` (06.2 NEW). Wire-protocol leg via the 04.1-
landed `drive_http1` helper; access-log assertion is a post-request
file-content read + per-token diff. The harness waits up to 5s for both
files to appear, then yields 100ms to let the OS flush, before reading.

## Cross-references

- Phase 06.2 SPEC: `docs/envoy-rust/phases/06.2-access-log/SPEC.md`
- BEHAVIOR_CONTRACT row table: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` `Access log field mapping`
- ADR: none (06.2 lands no ADRs under the recommended posture)
- Related fixtures: 0007 (H1 direct_response baseline), 0011 (admin stats baseline)
