# Fixture 0010 — http2-router-upstream

**Phase:** 05.3.

**Surface:** HTTP/2 cleartext (H2C) downstream listener proxying to an HTTP/2 cleartext upstream cluster. The first H2-on-H2 round-trip in the project.

**Configuration:**

- Downstream listener: `http2_listener` binds `0.0.0.0:{{PORT}}` (Envoy) / `127.0.0.1:{{PORT}}` (envoy-rust); HCM `codec_type: HTTP2`; single virtual host (`domains: ["*"]`); single route (`prefix: "/"`) routing to cluster `backend`.
- Upstream cluster: `backend` of `type: STRICT_DNS` (per 05.1's schema growth) with `dns_lookup_family: V4_ONLY` (per 05.4 REVIEW R-2 for `host.docker.internal` reachability), resolving `{{BACKEND_HOST}}:{{HTTP2_BACKEND_PORT}}`. The `typed_extension_protocol_options.HttpProtocolOptions.explicit_http_config.http2_protocol_options: {}` block selects upstream H2 (per 05.3 D2.a).

**Test driver:** `Driver::Http2 { method: GET, path: "/", host: "envoy-rust.test", ... }` (drives via `tests/differential/src/lib.rs::drive_http2`).

**Backend:** `tests/helpers/http2-echo-server` (binary at `target/<profile>/http2-echo-server`); spawned by `Http2EchoBackend` per 05.3 D6.

**Cross-references:**

- Phase 05.3 SPEC §3 D7 — fixture surface.
- Parent-05 SPEC §3 D15.3 — fixture deliverable in the parent split.
- Phase 05.1 SPEC §3 D3 — `STRICT_DNS` cluster type.
- Phase 05.4 SPEC §3 D2 — `dns_lookup_family: V4_ONLY` posture.
- Phase 05.4 REVIEW §4 R-2 + R-4 — `body_is_nonempty` predicate template (informs H2 client codec emission decisions on empty-body GETs); R-2 V4_ONLY for `host.docker.internal`.
- Phase 05.2 fixture 0009 — H2 listener-side direct-response sibling.
- Phase 04.3 fixture 0008 — H1 router-upstream sibling.

**Acceptance signal:** the fixture is green at the Docker-gated CI level (`tests/differential/tests/http2_router_upstream.rs`) AND at the in-process integration backstop level (`crates/envoy-bin/tests/http2_router_upstream.rs`).
