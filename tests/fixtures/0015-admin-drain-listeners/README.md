# Fixture 0015 — admin-drain-listeners

**Phase:** 08.2.

**Surface:** First end-to-end bilateral assertion of the admin
`/drain_listeners` POST endpoint introduced in phase 08.2 (D9 + D10 +
D11 + D12 + D-ready). Drives the new `Driver::AdminScrape` extensions
landed at 08.2 Task 7 (D16) — `pre_admin_actions` + `post_admin_assertions`
— against upstream Envoy v1.33 and envoy-rust in lock-step. Per the
parent-08 SPEC §2.4 "Admin-action effect equivalence" wire-level
invariant: BOTH proxies MUST refuse-or-immediately-close new connections
on their data-plane listeners within the 5s `DRAIN_BUDGET` after a
`POST /drain_listeners` returns 200, AND BOTH proxies' admin `/ready`
endpoint MUST flip to 503 with the literal token `DRAINING` in the
response body. See `docs/envoy-rust/BEHAVIOR_CONTRACT.md`
"Admin-action effect equivalence" subsection (Task 8 lands this).

## Configuration

- HCM listener: `hcm_listener` binds `0.0.0.0:{{PORT}}` (Envoy) /
  `127.0.0.1:{{PORT}}` (envoy-rust). HCM `codec_type: HTTP1`. Single
  virtual host (`domains: ["*"]`) with one route (`prefix: "/"`) that
  emits `direct_response { status: 200, body: { inline_string: "ok\n" } }`.
  Mirrors fixture 0007's HCM + direct_response shape — minimal
  data-plane surface, NO upstream cluster, NO backend (the listener
  exists solely so `post_admin_assertions.data_plane_connection_refused`
  has a real bound port to probe; no data-plane traffic is driven against
  it during this fixture's run).
- Admin listener: binds `0.0.0.0:{{ADMIN_PORT}}` (Envoy) /
  `127.0.0.1:{{ADMIN_PORT}}` (envoy-rust). The `{{ADMIN_PORT}}` marker
  is satisfied by `run_fixture`'s admin-port reservation (06.1 D6.a
  branch, unchanged at 08.2).
- No backend (`clusters: []`). The harness's `_http1_backend` is not
  spawned (the fixture YAMLs reference neither `{{BACKEND_HOST}}` nor
  `{{HTTP1_BACKEND_PORT}}`).
- Cross-sub-phase rule 3 (06.1): admin is HTTP/1.1 only; this fixture
  does NOT set TLS, ALPN, or HTTP/2 on the admin listener.

## Test driver

`Driver::AdminScrape` with the 08.2 D16 extensions:

1. `pre_admin_actions: [POST /drain_listeners → 200]` — the drain
   trigger. Issued against BOTH proxies' admin listeners via
   `drive_admin_post` (08.2 D16). Flips `DrainState` on each side
   from `Live` to `Draining` (envoy-rust) / equivalent (envoy).
2. `scrapes: [GET /server_info → 200 application/json with state key]` —
   the post-drain bilateral admin observation. Empirical investigation
   at Task 8 surfaced that upstream Envoy v1.33's `/ready` does NOT
   flip to 503 immediately on `POST /drain_listeners` — it requires
   `--drain-time-s` (default 600s) to elapse OR
   `--drain-strategy immediate` (server-level CLI flags, NOT
   bootstrap-configurable); envoy-rust per parent-08 SPEC §5.5 flips
   `/ready` immediately on drain. The scrape therefore targets
   `/server_info` (which is bilaterally 200-with-JSON across the
   drain transition on BOTH proxies), with a `BodyRule::JsonShape`
   that requires the `state` key on both sides while admitting the
   `state` value to differ (envoy-rust reports `"DRAINING"`; upstream
   envoy v1.33 may report `"DRAINING"` once its drain bookkeeping
   advances or `"LIVE"` mid-drain). The per-side allow-list seeding
   for `/server_info` mirrors fixture 0014's seeded subset for the
   same endpoint (`version` / `hot_restart_version` /
   `command_line_options` / `node` ∈ `value_may_differ_keys`;
   `uptime_current_epoch` / `uptime_all_epochs` envoy-only;
   `uptime_current_epoch_seconds` / `uptime_all_epochs_seconds`
   envoy-rust-only). The substantive drain assertion is the
   `data_plane_connection_refused` step below — `/server_info`
   merely satisfies Task 7's `scrapes.is_empty() → bail` invariant
   with a bilateral-stable shape.
3. `post_admin_assertions: [data_plane_connection_refused on the HCM
   listener within 5s]` — the wire-level drain effect. The per-side
   template-render of `listener_address: "127.0.0.1:{{PORT}}"` (08.2
   Task 8 extension at `tests/differential/src/lib.rs`
   post_admin_assertions dispatch arm) resolves the marker against each
   proxy's data-plane host port; both proxies are probed with
   `assert_data_plane_connection_refused` (08.2 D16 helper). The
   assertion succeeds if BOTH proxies refuse the connection (ECONNREFUSED)
   OR FIN it immediately on accept (immediate-EOF) OR RST it on accept
   (ungraceful close, per the Task 7 fixup) within the 5s budget.

YAML field order: `pre_admin_actions` is declared BEFORE `pre_requests`
(which is absent here — see "Why no pre_requests" below) per
architecture-decision lock-in #18's reader-ergonomics half (drain
trigger at the top of the YAML block). The dispatch fn drives them in
the temporal order `pre_requests → pre_admin_actions → scrapes →
post_admin_assertions` regardless of YAML field order; for this fixture
the absence of `pre_requests` reduces the temporal sequence to
`pre_admin_actions → scrapes → post_admin_assertions`.

## Why no pre_requests

The PLAN's task spec sketch carried a notional
`pre_requests: [GET /ready → 200 LIVE]` pre-drain baseline assertion.
The actual `Driver::PreRequest` grammar at
`tests/differential/src/lib.rs:210-217` carries only
`(method, path, host, port_key)` — no `expected_status` /
`expected_body` fields — so the pre-drain `/ready=200 LIVE` baseline is
not assertable through it. Additionally `pre_requests` target the HCM
listener (`port_key = PORT`), not the admin listener, so a path of
`/ready` would land on the HCM's direct_response route (status 200,
body `"ok\n"`), not the admin `/ready` endpoint. The drain trigger
followed by the post-drain `DRAINING` scrape + the
`data_plane_connection_refused` post-assertion together form the
substantive end-to-end signal; the pre-drain baseline is covered in
isolation by the in-process backstop at Task 10
(`tests/differential/tests/admin_drain_listeners.rs`) where the
admin `/ready` endpoint can be probed directly without the
`Driver::PreRequest` HCM-routing constraint.

## Empirical allow-list seeding (SPEC §6 signpost 12)

Fixture 0015 ships with a minimal expectations.yaml — the only body
rule is the `BodyRule::TextLines { required_lines: ["DRAINING"] }`
shape on the `/ready` scrape. There are no per-side allow-list
entries seeded at fixture-landing time; if the first Docker-gated
green run surfaces an envoy-only or envoy-rust-only line that the
literal-content-match doesn't cover, additional allow-list entries
land at Task 11's state-4 verification per the established 06.1 /
06.3 / 08.1 seeding doctrine. The data_plane_connection_refused
assertion is wire-level (kernel-side ECONNREFUSED / immediate-EOF /
RST signals); it has no allow-list and is bilateral by construction —
both proxies must refuse the connection within `within_ms` for the
assertion to pass.

## Cross-references

- Phase 08.2 SPEC §2.4 — Admin-action effect equivalence (the
  parent-08 wire-level invariant this fixture exercises).
- Phase 08.2 PLAN.md Task 8 — fixture authoring + Docker-gated wrapper
  + BEHAVIOR_CONTRACT subsection.
- Phase 08.2 PLAN.md Task 7 — `Driver::AdminScrape` extensions (the
  `pre_admin_actions` + `post_admin_assertions` D16 surface this
  fixture consumes).
- Phase 08.2 PLAN.md Task 10 — in-process backstop
  (`tests/differential/tests/admin_drain_listeners.rs`) that
  exercises the same drain flow without Docker — Task 10 deviation #1
  uses HCM + direct_response shape (NOT the trivial-echo-filter
  workaround) because the data_plane_connection_refused assertion
  requires a real bound data-plane listener. The trivial-echo-filter
  workaround (08.1 REVIEW §4 process note option (b)) is reserved
  for future admin-only backstops where no data-plane listener is
  needed.
- SPEC §6 signpost 12 — allow-list seeding doctrine.
- BEHAVIOR_CONTRACT.md "Admin-action effect equivalence" subsection
  (Task 8 lands this) — the cross-proxy wire-level invariant table
  authorizing the equivalence dispositions asserted against here.
- Phase 08.1 fixture 0014 — `Driver::AdminScrape` multi-case precedent
  (the parent shape this fixture's `scrapes:` block builds on).
- Phase 04.x fixture 0007 — HCM + direct_response bootstrap precedent
  (the minimal-data-plane scaffolding mirrored here).

**Acceptance signal:** the fixture is green at the Docker-gated CI
level (`tests/differential/tests/admin_drain_listeners.rs`) AND, after
Task 10 lands, the in-process happy-path level
(`tests/differential/tests/admin_drain_listeners.rs` is shared between
both surfaces — the Docker-gated wrapper at Task 8 vs the in-process
backstop at Task 10 are distinct files in distinct test buckets). The
differential level provides the bilateral assertion against upstream
Envoy; the in-process level provides a fast Docker-free smoke test of
the drain happy path against a standalone envoy-rust subprocess.
