//! 28 Task 8 (ADR-0069/0070): in-process RING_HASH BACKSTOP — the deterministic
//! complement to the `0036-lb-ring-hash` cross-proxy differential fixture.
//!
//! The differential (fixture 0036) proves RING_HASH consistent-hashing selection
//! BILATERALLY, but only for a single shape: a PLAIN STATIC cluster where the
//! `x-hash-key` header is ALWAYS present (so the request always carries a hash
//! key). It cannot reach the paths this backstop covers:
//!
//!   1. ring determinism + the §6.2 oracle subset (key→host mapping ground truth);
//!   2. SPREAD over a sweep of keys (both hosts selected at least once);
//!   3. the NO-HASH-KEY fallback — `pick_endpoint(None)` on a RING_HASH cluster
//!      returns a VALID host (the absent-header path the differential, which
//!      always sends the header, never exercises);
//!   4. the EMPTY-header-value-is-HASHED distinction — `Some(hash(""))` is a
//!      DETERMINISTIC hashed selection (`xxh64("")`), NOT the random/None
//!      fallback (the integration companion to Task 6's extraction-helper test);
//!   5. the SINGLE-HOST ring — every key (and `None`) routes to the one host;
//!   6. the ROUND_ROBIN-ignores-key regression — `Some(123)` behaves exactly like
//!      `None` (the key is inert on the cursor path).
//!
//! Construction goes through the PUBLIC production path — `parse_bootstrap` →
//! `load_dynamic_resources` → `envoy_cluster::from_bootstrap` → `manager.get(name)`
//! → `ClusterHandle::pick_endpoint` — so this is a faithful integration backstop
//! (NOT the in-crate `mk_ring_hash_handle` unit fixture). The request hash is
//! computed via the PUBLIC `envoy_cluster::hash_request_key` helper (the same
//! `xxh64` the ring uses internally), mirroring how the HCM threads the key.
//!
//! These behaviors already landed (Tasks 5/6), so the assertions are
//! CHARACTERIZATION/REGRESSION: they PASS against the current ring/fallback logic
//! and would FAIL if it regressed. The §6.2 oracle (the same one Task 5 pinned) is
//! the regression ground truth — host 0 = `172.22.0.2:5678`, host 1 =
//! `172.22.0.3:5678`, `minimum_ring_size: 1024`.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use envoy_cluster::{ClusterHandle, hash_request_key};

/// host 0 of the §6.2 oracle (the "ONE" backend).
const HOST_0: &str = "172.22.0.2:5678";
/// host 1 of the §6.2 oracle (the "TWO" backend).
const HOST_1: &str = "172.22.0.3:5678";

fn host0() -> SocketAddr {
    HOST_0.parse().unwrap()
}
fn host1() -> SocketAddr {
    HOST_1.parse().unwrap()
}

/// Build a `ClusterHandle` for a single cluster via the PUBLIC production path.
/// `clusters_block` is the `clusters:` list body (each `- name: ...`). A
/// kernel-ephemeral admin block (`port_value: 0`) satisfies the config
/// NoRuntime gate (no listener is needed since the backstop drives the cluster
/// directly via `pick_endpoint`, not over the data plane).
async fn build_cluster(clusters_block: &str, name: &str) -> ClusterHandle {
    let yaml = format!(
        "node: {{ id: lb-ring-hash-backstop, cluster: envoy-rust-phase-28 }}\n\
         admin: {{ address: {{ socket_address: {{ address: 127.0.0.1, port_value: 0 }} }} }}\n\
         static_resources:\n  clusters:\n{clusters_block}"
    );
    let mut bootstrap = envoy_config::parse_bootstrap(&yaml).expect("parse_bootstrap");
    envoy_config::load_dynamic_resources(&mut bootstrap).expect("load_dynamic_resources");
    let registry = Arc::new(envoy_stats::StatsRegistry::new());
    let manager = envoy_cluster::from_bootstrap(&bootstrap, registry)
        .await
        .expect("from_bootstrap");
    manager
        .get(name)
        .unwrap_or_else(|| panic!("cluster {name:?} present in manager"))
}

/// A two-host PLAIN STATIC RING_HASH cluster over the §6.2 oracle addresses
/// (`minimum_ring_size: 1024`, the fixture-0036 / Task-5 ring size).
fn ring_hash_two_host_block() -> String {
    r#"    - name: ring_cluster
      type: STATIC
      lb_policy: RING_HASH
      ring_hash_lb_config:
        minimum_ring_size: 1024
      load_assignment:
        cluster_name: ring_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 172.22.0.2, port_value: 5678 }
              - endpoint:
                  address:
                    socket_address: { address: 172.22.0.3, port_value: 5678 }
"#
    .to_string()
}

/// A SINGLE-host RING_HASH cluster (one endpoint = host 0).
fn ring_hash_single_host_block() -> String {
    r#"    - name: ring_cluster
      type: STATIC
      lb_policy: RING_HASH
      ring_hash_lb_config:
        minimum_ring_size: 1024
      load_assignment:
        cluster_name: ring_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 172.22.0.2, port_value: 5678 }
"#
    .to_string()
}

/// A two-host ROUND_ROBIN cluster over the SAME two oracle addresses (the
/// key-inert regression baseline).
fn round_robin_two_host_block() -> String {
    r#"    - name: rr_cluster
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: rr_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 172.22.0.2, port_value: 5678 }
              - endpoint:
                  address:
                    socket_address: { address: 172.22.0.3, port_value: 5678 }
"#
    .to_string()
}

// ── 1: ring determinism + the §6.2 oracle subset ─────────────────────────────

/// The §6.2 oracle is the regression ground truth: the SAME six key→host pins
/// Task 5 locked, now asserted through the PUBLIC `hash_request_key` +
/// `pick_endpoint` integration surface. Do NOT adjust the oracle to match output.
#[tokio::test(flavor = "multi_thread")]
async fn ring_determinism_matches_section_6_2_oracle() {
    let rh = build_cluster(&ring_hash_two_host_block(), "ring_cluster").await;

    // (key, expected host) — the §6.2 oracle subset.
    let oracle: &[(&[u8], SocketAddr)] = &[
        (b"key-0", host0()),
        (b"key-2", host1()),
        (b"key-10", host0()),
        (b"key-11", host1()),
        (b"user-alice", host0()),
        (b"1.2.3.4", host1()),
    ];

    for (key, expected) in oracle {
        let hash = hash_request_key(key);
        let got = rh.pick_endpoint(Some(hash), None);
        assert_eq!(
            got,
            Some(*expected),
            "oracle: key {:?} → {expected} (got {got:?})",
            std::str::from_utf8(key).unwrap()
        );
        // Determinism: a SECOND pick with the same key lands on the same host.
        assert_eq!(
            rh.pick_endpoint(Some(hash), None),
            Some(*expected),
            "ring pick is deterministic for key {:?}",
            std::str::from_utf8(key).unwrap()
        );
    }
}

// ── 2: spread over a sweep of keys (both hosts selected) ─────────────────────

/// Over a sweep of ~16 keys, BOTH hosts are selected at least once — the ring
/// distributes keys across the membership (it does not collapse to one host).
#[tokio::test(flavor = "multi_thread")]
async fn ring_spread_selects_both_hosts() {
    let rh = build_cluster(&ring_hash_two_host_block(), "ring_cluster").await;

    // The 16 keys are a FIXED characterization set observed (in the §6.2 recon /
    // locally) to cover both hosts; the runtime SPREAD assertions below are the
    // safety net should a future distribution change shift which keys land where.
    let mut saw_host0 = false;
    let mut saw_host1 = false;
    for i in 0..16u32 {
        let key = format!("spread-key-{i}");
        let pick = rh
            .pick_endpoint(Some(hash_request_key(key.as_bytes())), None)
            .expect("RING_HASH pick yields a host for a healthy cluster");
        assert!(
            pick == host0() || pick == host1(),
            "pick {pick} must be one of the two members"
        );
        saw_host0 |= pick == host0();
        saw_host1 |= pick == host1();
    }
    assert!(
        saw_host0,
        "spread must select host 0 at least once over 16 keys"
    );
    assert!(
        saw_host1,
        "spread must select host 1 at least once over 16 keys"
    );
}

// ── 3: the no-hash-key fallback (absent header) ──────────────────────────────

/// `pick_endpoint(None)` on a RING_HASH cluster returns a VALID host — NOT a
/// panic, NOT `None` for a healthy cluster. This is the ABSENT-header path the
/// differential never exercises (fixture 0036 always sends `x-hash-key`).
#[tokio::test(flavor = "multi_thread")]
async fn no_hash_key_falls_back_to_valid_host() {
    let rh = build_cluster(&ring_hash_two_host_block(), "ring_cluster").await;

    let pick = rh
        .pick_endpoint(None, None)
        .expect("RING_HASH None-key fallback must yield a valid host (cursor path)");
    assert!(
        pick == host0() || pick == host1(),
        "None-key fallback {pick} must be one of the two members"
    );
}

// ── 4: empty-header-value is HASHED (deterministic), not the None fallback ───

/// An EMPTY header value hashes to `xxh64("")` — a NORMAL deterministic hashed
/// selection, NOT the random/None fallback (ADR-0070). The integration companion
/// to Task 6's extraction-helper test (c): `Some(hash(""))` lands on the SAME
/// host every time, and that host equals an explicit `xxh64("")` ring lookup.
///
/// This proves the load-bearing ADR-0070 distinction DIRECTLY by asserting BOTH
/// paths side-by-side on the SAME freshly-built two-host ring:
///   - `Some(hash(""))` is HASHED: it pins to ONE host, stable across repeats
///     (the ring path — an empty-but-PRESENT header value is hashed, not random);
///   - `None` is the ABSENT-header FALLBACK: it ROTATES host0→host1→host0… (the
///     round-robin cursor path), so it is NOT pinned to one host.
/// The `Some(hash(""))` picks take the ring path and do NOT touch the cursor, so
/// the `None` rotation below starts from a clean cursor at 0 (host0 first) and the
/// host0→host1 rotation is observable on this one cluster. Stability of the hashed
/// pick CONTRASTED with the rotation of the fallback is what makes "hashed, not
/// fallback" self-evident (rather than implied by stability alone).
#[tokio::test(flavor = "multi_thread")]
async fn empty_header_value_is_hashed_deterministically() {
    let rh = build_cluster(&ring_hash_two_host_block(), "ring_cluster").await;

    // HASHED path: Some(hash("")) pins to one host, stable across repeats.
    let empty_hash = hash_request_key(b"");
    let first = rh
        .pick_endpoint(Some(empty_hash), None)
        .expect("empty-value hashed pick yields a host");
    // Deterministic across repeated picks (it's a fixed key, not randomized).
    for _ in 0..8 {
        assert_eq!(
            rh.pick_endpoint(Some(empty_hash), None),
            Some(first),
            "Some(hash(\"\")) is a deterministic hashed selection, not a random fallback"
        );
    }
    assert!(
        first == host0() || first == host1(),
        "empty-value pick {first} must be one of the two members"
    );

    // FALLBACK path: None rotates across hosts — it is NOT pinned. The ring picks
    // above did not advance the round-robin cursor, so it starts clean at 0: the
    // first None lands host0, the second host1 (the cursor advancing), proving the
    // absent-header fallback is the rotating cursor path, distinct from the pinned
    // hashed selection of an empty-but-present value above (ADR-0070).
    assert_eq!(
        rh.pick_endpoint(None, None),
        Some(host0()),
        "None fallback (cursor 0) — absent-header path, NOT the hashed empty-value path"
    );
    assert_eq!(
        rh.pick_endpoint(None, None),
        Some(host1()),
        "None fallback rotates to host1 (cursor 1) — it is NOT pinned to one host"
    );
}

// ── 5: single-host ring routes every key (and None) to the one host ──────────

/// A RING_HASH cluster with ONE endpoint: every key — AND the `None` fallback —
/// routes to that single host (the ring has exactly one member).
#[tokio::test(flavor = "multi_thread")]
async fn single_host_ring_routes_everything_to_the_one_host() {
    let rh = build_cluster(&ring_hash_single_host_block(), "ring_cluster").await;

    for i in 0..16u32 {
        let key = format!("single-key-{i}");
        assert_eq!(
            rh.pick_endpoint(Some(hash_request_key(key.as_bytes())), None),
            Some(host0()),
            "single-host ring: key {key} → the one host"
        );
    }
    // The empty-value key and the None fallback also land on the one host.
    assert_eq!(
        rh.pick_endpoint(Some(hash_request_key(b"")), None),
        Some(host0()),
        "single-host ring: empty-value key → the one host"
    );
    assert_eq!(
        rh.pick_endpoint(None, None),
        Some(host0()),
        "single-host ring: None fallback → the one host"
    );
}

// ── 6: ROUND_ROBIN ignores the key (regression) ─────────────────────────────

/// A ROUND_ROBIN cluster IGNORES the hash key: `pick_endpoint(Some(123))` behaves
/// exactly like the cursor path — identical to `pick_endpoint(None)`. The key is
/// inert (no ring is built for a non-RING_HASH cluster).
#[tokio::test(flavor = "multi_thread")]
async fn round_robin_ignores_hash_key() {
    let rr = build_cluster(&round_robin_two_host_block(), "rr_cluster").await;

    // NOTE: these assertions depend on a FRESH, unshared cursor starting at 0
    // (host0 first) — a freshly-built cluster has not advanced its round-robin
    // cursor, so the first pick is endpoints[0].
    // The cursor advances regardless of Some/None — Some(123) is inert. Starting
    // from a fresh cursor: pick 0 → endpoints[0], pick 1 → endpoints[1], pick 2 →
    // endpoints[0] (2 % 2). A `Some(key)` call advances the cursor identically to
    // a `None` call.
    assert_eq!(
        rr.pick_endpoint(Some(123), None),
        Some(host0()),
        "ROUND_ROBIN cursor 0, key ignored"
    );
    assert_eq!(
        rr.pick_endpoint(None, None),
        Some(host1()),
        "ROUND_ROBIN cursor 1, key ignored (None advances the cursor)"
    );
    assert_eq!(
        rr.pick_endpoint(Some(123), None),
        Some(host0()),
        "ROUND_ROBIN cursor 2 % 2 = 0, key ignored"
    );
}
