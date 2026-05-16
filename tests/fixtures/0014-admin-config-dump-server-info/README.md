# Fixture 0014 — admin-config-dump-server-info

**Phase:** 08.1.

**Surface:** First end-to-end bilateral assertion of the 4 new GET admin
endpoints introduced in phase 08.1 — `/config_dump` (D6), `/server_info`
(D5), `/clusters` (D7), `/listeners` (D8) — against upstream Envoy
v1.33. Drives the new `Driver::AdminScrape { scrapes: [...] }` multi-case
shape (08.1 Task 11 — see PROGRESS.md Task 11) which sequences 1..N
admin scrapes against a single bilateral proxy invocation; this fixture
declares 4 sub-cases (one per new endpoint). Per-endpoint equivalence is
expressed via the `BodyRule::JsonShape` + `BodyRule::TextLines`
strictness model wired in the same Task 11 commit.

## Configuration

- HCM listener: `ingress_http` binds `0.0.0.0:{{PORT}}` (Envoy) /
  `127.0.0.1:{{PORT}}` (envoy-rust). HCM `codec_type: HTTP1`. Single
  virtual host (`domains: ["*"]`) with one route (`prefix: "/"`) that
  routes to the `backend` STRICT_DNS cluster. The data-plane listener
  exists solely so `/listeners` + `/clusters` have non-trivial content to
  render — fixture 0014 drives NO data-plane traffic against it
  (`pre_requests: []`).
- Admin listener: binds `0.0.0.0:{{ADMIN_PORT}}` (Envoy) /
  `127.0.0.1:{{ADMIN_PORT}}` (envoy-rust). The `{{ADMIN_PORT}}` marker
  is satisfied by `run_fixture`'s admin-port reservation (06.1 D6.a
  branch, unchanged at 08.1).
- Backend: STRICT_DNS, single endpoint at `{{BACKEND_HOST}}:{{HTTP1_BACKEND_PORT}}`.
  The harness's `_http1_backend` spawns a real `http1-echo-server` because
  the templates reference `{{HTTP1_BACKEND_PORT}}`, but the backend is
  never reached (no data-plane traffic). Mirrors fixture 0008's bootstrap
  shape so `/config_dump` has a non-trivial `bootstrap.static_resources`
  projection.
- Cross-sub-phase rule 3 (06.1): admin is HTTP/1.1 only; this fixture
  does NOT set TLS, ALPN, or HTTP/2 on the admin listener.

## Test driver

`Driver::AdminScrape { pre_requests: [], scrapes: [...] }` with 4
sub-cases:

1. `GET /config_dump` — 200 `application/json`; `BodyRule::JsonShape`
   with `required_keys: ["configs"]` + `required_subtree` on
   `configs.0.@type` (envelope tag).
2. `GET /server_info` — 200 `application/json`; `BodyRule::JsonShape`
   with the 7 SPEC-mandated required keys + `required_subtree` on
   `state` (08.1 hardcodes the literal `"LIVE"` per architecture-decision
   lock-in #6).
3. `GET /clusters` — 200 `text/plain`; `BodyRule::TextLines` with
   `required_lines` for the 2-lines-per-cluster shape envoy-rust emits
   (architecture-decision lock-in #10).
4. `GET /listeners` — 200 `text/plain`; `BodyRule::TextLines` with
   `required_line_prefixes` (the address+port suffix differs per side —
   envoy binds `0.0.0.0`, envoy-rust binds `127.0.0.1`; both bind a
   kernel-ephemeral port the expectations.yaml cannot template-render).

## Empirical allow-list seeding (SPEC §6 signpost 12)

The `allowlist_envoy_only_*` / `allowlist_envoy_rust_only_*` /
`value_may_differ_keys` per sub-case are populated from the first
Docker-gated run's diff output, mirroring fixture 0011's 204-entry
seeding doctrine for `/stats/prometheus`. Expected divergence categories
per endpoint (each entry below was confirmed empirically before
inclusion):

- `/config_dump`:
  - `last_updated` (wall-clock; bilateral non-determinism — covered by
    `value_may_differ_keys` if/when envoy-rust hoists it to the
    top level; today envoy-rust emits it inside each `configs[N]`
    entry so it's covered by the `configs` top-level may-differ entry).
  - `configs` array content drift: envoy emits xDS-derived entries
    (`ClustersConfigDump`, `ListenersConfigDump`, etc.) that envoy-rust
    does not (BEHAVIOR_CONTRACT.md `/config_dump` row authorizes this);
    envoy-rust emits the bootstrap projection in a slightly different
    JSON-renaming-set than envoy's protobuf-to-JSON projection
    (`bootstrap.<field>` shape divergence). Both are absorbed via
    `configs` ∈ `value_may_differ_keys`; the `required_subtree` check on
    `configs.0.@type` is what locks down the envelope.
- `/server_info`:
  - `version` — envoy-rust emits `envoy-rust <pkg-version>`; envoy emits
    its build version.
  - `hot_restart_version` — envoy emits a value; envoy-rust emits
    `"disabled"`.
  - `command_line_options` — each side emits its own command-line
    introspection (envoy-rust emits a `BTreeMap<String, serde_yaml::Value>`
    constructed at handler-init time, per PLAN lock-in #7; envoy emits
    its full options).
  - `uptime_current_epoch_seconds` / `uptime_all_epochs_seconds` —
    wall-clock divergence.
  - `node` — envoy emits OS-introspected fields beyond the parsed
    `node:` block (e.g. `build.version`, `metadata`); envoy-rust emits
    only what was in the bootstrap. Covered by `value_may_differ_keys`.
- `/clusters`:
  - Per-endpoint numeric-counter lines (`success_rate`, `host_weight`,
    etc.) that upstream Envoy adds per cluster member; envoy-rust at 08.1
    emits ONLY the minimum two lines per cluster (lock-in #10). Empirical
    count + categorization documented in `allowlist_envoy_only_lines`
    after the first Docker-gated run.
- `/listeners`:
  - Per-side address line shape: envoy emits one shape
    (`ingress_http::0.0.0.0:<ephemeral-port>`); envoy-rust emits another
    (`ingress_http::127.0.0.1:<ephemeral-port>`). Each lands in the
    other side's allow-list as a per-side-only line.

## Cross-references

- Phase 08.1 SPEC §3 D17.1 — fixture deliverable.
- Phase 08.1 PLAN.md Task 11 — fixture authoring + empirical seeding.
- SPEC §6 signpost 12 — allow-list seeding doctrine.
- BEHAVIOR_CONTRACT.md "Admin endpoint body shapes" — the 4 rows
  authorizing the equivalence dispositions asserted against here.
- Phase 06.1 fixture 0011 — `Driver::AdminScrape` precedent (the
  single-scrape parent shape; widened to `Vec<AdminScrapeCase>` at
  08.1 Task 11).
- Phase 04.3 fixture 0008 — HCM + STRICT_DNS bootstrap precedent (the
  data-plane scaffolding mirrored here).

**Acceptance signal:** the fixture is green at the Docker-gated CI
level (`tests/differential/tests/admin_config_dump_server_info.rs`). No
in-process backstop exists for this fixture (the differential level is
the only level that exercises the upstream-Envoy admin output).
