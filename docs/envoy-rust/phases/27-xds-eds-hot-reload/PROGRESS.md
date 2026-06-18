# Phase 27 (`27-xds-eds-hot-reload`) — PROGRESS

> State-by-state + task-by-task progress log for phase 27 (file-based EDS endpoint hot-reload).
> The state-2 PLAN-write entry (incl. the §6.2 empirical-verification transcript) is below;
> each state-3 task appends its own entry (one code commit + one PROGRESS commit per task,
> per `feedback_execution_style`).
> **NOTE:** UNLIKE phase 26 (whose §6.2 was deferred to state-3 Task 1), phase 27's §6.2
> verification was run AT the state-2 PLAN-write — this dev host is native Linux (Docker
> Desktop), so the ADR-0066 container-internal `docker exec` atomic-rename methodology makes
> the reload observable. See the §6.2 transcript below + the PLAN STATUS banner.

---

## State-2 PLAN-write (this commit) — §6.2 VERIFIED at PLAN-write, ADR-0068 FIRED

- **Skill:** `superpowers:writing-plans`.
- **Authored:** `PLAN.md` (header + goal + architecture + the §6.2-VERIFIED facts V1–V6 + the PLAN-time design decisions D1-handle-type / D2-watcher-placement / D3-mirror-empty / D4-update_rejected / EDS+HC/OD-no-watcher + the SPEC-correction anchors C1–C5 + the §6.1 single-phase confirmation + the file structure + Tasks 1–9 + self-review) + this `PROGRESS.md` (skeleton + the §6.2 transcript + the Task-2 preamble) + **ADR-0068** (the §6.2 reconciliation).
- **§6.2 empirical verification: DONE this session** (transcript below). **ADR-0068 FIRES** — the empty-assignment disposition and the config_dump `last_updated` projection diverged from the SPEC; reconciled (empty-assignment MIRRORED to a MATCH; config_dump `last_updated` correction; `update_rejected` promoted to emitted; EDS+HC/OD = no watcher; §6.1 single-phase confirmed).
- **ADR posture:** **ADR-0068 FIRED** at this PLAN-write (DECISIONS.md ledger head → ADR-0068, count 69). **ADR-0069 (the §6.1 split) reserved-but-UNFIRED** (single phase confirmed). ADR-0014 in force; ADR-0028 open.
- **ROADMAP:** row `27` flips `planned → in-progress` at this commit (STATE now points at it). **STATE:** advances to `27` state-2-complete / state-3-next (next skill `superpowers:subagent-driven-development`). Superseded state-1/state-2 top-section blocks relocated to `STATE_HISTORY.md` (ADR-0035 / §4.1 inv. 9).
- **No production/test change at state-2** (docs-only PLAN-write commit — PLAN + PROGRESS + ADR-0068 + ROADMAP + STATE).

### State-2 §6.2 empirical-verification transcript (the PROTOCOL + the locked findings)

Ran the SPEC §6.2 6-item checklist against `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`) via the **ADR-0066 Docker-Desktop methodology** — this dev host runs Docker Desktop (linuxkit VM), where host bind-mounts deliver no inotify (the `host-docker-desktop-virtiofs-no-inotify` finding), so real reloads were obtained by placing the EDS file on the Envoy container's OWN overlayfs (written by an entrypoint shell script, NO host bind-mount) and reloading it via `docker exec` write-temp-then-`mv` (atomic-rename) inside the container.

**Rig:** docker network `eds-probe-net`; `backend1`/`backend2` = `hashicorp/http-echo -text=BACKEND_ONE|BACKEND_TWO` (port 5678, single endpoint each), discovered by numeric IP (172.22.0.2 / 172.22.0.3); a 3 s-sleep python backend for the in-flight test; `envoy-probe` = `envoyproxy/envoy:v1.33.0` on the same network (`--entrypoint /bin/sh` writing `/tmp/bootstrap.yaml` + the initial `/tmp/eds/eds.yaml` then `exec envoy`); admin `:9901`, H1 listener `:10000`; `type: EDS` cluster `eds_backend` (`eds_cluster_config.eds_config.path_config_source.path: /tmp/eds/eds.yaml`, NO HC / NO OD), route `/`→`eds_backend`, router-only chain. Host left clean (all containers + network removed).

**Findings (the 6-item checklist — LOCKED; full reconciliation in ADR-0068):**

- **V1 (reload + readiness) — MATCHES.** Atomic-rename `[backend1]→[backend2]` re-targets live traffic, NO restart; `/probe` body flips `BACKEND_ONE`→`BACKEND_TWO` on the first 50 ms poll; settle **~7–8 ms** (`mv`→effective; a recovery flip ~6.7 ms); **zero 503 gap** (400-request hammer across a live swap = 0 non-200s); cluster stays up. → harness wait bound = 50 ms poll grid, ~2 s ceiling.
- **V2 (observability) — second-backend body-marker swap CONFIRMED over liveness-flip.** Markers cleanly distinguish; the swap re-targets observably. Liveness-flip (→ unreachable `127.0.0.1:1` → 503 in ~55 ms) IS observable but noisier (request-time connect-refused, membership stays `1/1`, does not prove the new endpoint serves). **D6 uses the second backend**; 503 reserved as the empty-assignment expected result.
- **V3 (`update_*` values) — MATCHES.** `update_attempt`/`update_success` = `1/1`→`2/2`→`3/3`; failure/rejected/empty = 0 on success. **Membership Envoy-only:** Envoy emits `membership_total`/`_healthy`/`_change` (`membership_change` ticks on every set change; `membership_total`→0 on empty-apply) — envoy-rust emits no `membership_total` (`cluster.rs:963`), so assert ONLY `update_*`.
- **V4 (bad-reload taxonomy) — ONE MATERIAL DIVERGENCE (empty-assignment), reconciled to a MATCH:**
  - (a) malformed YAML → `update_failure` +1, last-good 200 (`Filesystem config update failure: ... yaml-cpp: error`).
  - (b) wrong/absent `cluster_name` (`NOT_eds_backend`) → `update_rejected` +1, last-good 200 (`Filesystem config update rejected: Unexpected EDS cluster (expecting eds_backend): NOT_eds_backend`).
  - (c) unparseable address (`not-an-ip`) → `update_rejected` +1, last-good 200 (`malformed IP address: not-an-ip`).
  - (d) **EMPTY endpoint list (CLA present, `endpoints: []`)** → Envoy **APPLIES** it: `update_success` +1, `membership_total` 1→0, `/probe` → **503 "no healthy upstream"**. DIVERGES from the SPEC warm-reject projection. **RECONCILED — envoy-rust MIRRORS** (apply-empty → `update_success` → `pick()` None → `synth_no_healthy_upstream` 503; safe because `pick()` already handles empty + the 503 path exists; D-3.3 prefers the faithful mirror). See ADR-0068 V4(d).
  - (e) truly-empty envelope (`resources: []`) → Envoy `update_empty` +1, last-good 200. Envoy DISTINGUISHES (e) from (d); envoy-rust mirrors.
  - Recovery: a final good reload restored 200 on the first poll (~6.7 ms), `update_success` resumed ticking.
- **V5 (config_dump) — MATCHES wire shape; SPEC `last_updated` projection WRONG.** EDS only under `?include_eds`, under `static_endpoint_configs[]`; the embedded assignment reflects the new endpoints. **NO `last_updated`, NO `version_info`** on the EndpointsConfigDump entry (already the phase-21 shape). → D5 shrinks to read-through-handle. Post-reload subtree: `{"@type":"…EndpointsConfigDump","static_endpoint_configs":[{"endpoint_config":{"@type":"…ClusterLoadAssignment","cluster_name":"eds_backend","endpoints":[{…new addr…}],"policy":{"overprovisioning_factor":140}}}]}`.
- **V6 (in-flight isolation) — MATCHES.** A request begun pre-reload (3 s-sleep backend) completed against the OLD endpoint; the next request hit the NEW endpoint. Cursor-bounds on a shrinking set is envoy-rust-side (`i % total` over the read-once snapshot, empty short-circuited) — D8 backstop-asserted.

**Divergence summary (drives ADR-0068):** (1) empty-assignment apply-vs-reject — reconciled to a MATCH by mirroring; (2) config_dump has no `last_updated` (projection wrong) — D5 shrinks; (3) `membership_*` Envoy-only — assert `update_*` only; (4) `update_rejected` must be registered (phase-21 omitted it). Items V1/V2/V3/V6 match the projection.

### Task-2 preamble — the first state-3 implementation task (D1 endpoint-handle migration)

The state-3 arc dispatches PLAN Tasks 2–9 to fresh subagents SERIALLY (`feedback_serial_subagent_dispatch`), each with two-stage review (spec-compliance THEN code-quality), TDD per task, one code commit + one PROGRESS commit per task. **Task 1 (§6.2) is DONE** (this state-2 commit). The next unstarted task is **Task 2** (the D1 endpoint-set-handle migration — `Cluster.endpoints: Vec<SocketAddr>` → `RwLock<Arc<Vec<SocketAddr>>>`, the §6.2-independent foundation; regression witness = fixtures 0020–0029 green incl. 0029's idle watcher). Tasks 2 and 3 are §6.2-independent and may run in either order; Task 4 depends on both.

---
