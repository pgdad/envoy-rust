//! Consistent-hashing ring for the `RING_HASH` load balancer.
//!
//! The ring algorithm is §6.2-LOCKED (decision ADR-0070; validated 36/36 against
//! live upstream Envoy v1.33.0). It MUST reproduce the recon oracle BYTE-FOR-BYTE
//! — in particular the `{address}_{i}` per-replica key shape (the `_` separator is
//! LOAD-BEARING) and the xxHash64 (seed 0) over that key. See the module tests'
//! pinned-oracle case for the empirical ground truth.
//!
//! Ring build (per host): `replicas = minimum_ring_size / num_hosts` entries
//! (integer division). Entry `i` (decimal `0..replicas`) for a host hashes
//! `xxh64(format!("{address}_{i}").as_bytes())`. All `(hash, host_index)` entries
//! are collected then sorted ascending by hash. Lookup binary-searches for the
//! first entry with `entry.hash >= request_hash`, wrapping to index 0 (clockwise)
//! when the request hash exceeds every entry.
//!
//! Address-string scope: the build keys are the host `ip:port` strings produced by
//! `SocketAddr`'s `Display` (e.g. `172.22.0.2:5678`), matching Envoy's
//! `address()->asString()`. This is exact for IPv4 (the differential fixture). IPv6
//! `SocketAddr` Display is bracketed (`[::1]:5678`); IPv6 ring hosts are an UNTESTED
//! non-goal — the cross-proxy guarantee is scoped to IPv4.
//!
//! M28-1 (documented bound vs Envoy's ketama): `RingHashLbConfig.maximum_ring_size` is
//! parse-validation-only — the ring build is governed solely by `minimum_ring_size`
//! (`minimum_ring_size / num_hosts` replicas per host). envoy-rust does NOT scale
//! replicas up toward `maximum_ring_size` for small host counts, validated for the
//! 2-host/1024 oracle.

use crate::xxhash::xxh64;

/// A consistent-hashing ring over a host set. Each entry stores the ring hash and
/// the index of the host (into the SAME ordered host slice that built the ring),
/// keeping the entry cheap (no address clone). `entries` is sorted ascending by
/// hash so [`HashRing::lookup`] can `partition_point`-search it.
#[derive(Debug)]
pub(crate) struct HashRing {
    entries: Vec<(u64, usize)>,
}

impl HashRing {
    /// Build the ring over `addresses` (host index `i` = `addresses[i]`) with
    /// `min_ring_size` total target entries. Each host contributes
    /// `min_ring_size / num_hosts` entries (integer division), keyed
    /// `xxh64(format!("{address}_{i}"))`. The collected entries are sorted
    /// ascending by hash.
    ///
    /// Empty `addresses` yields an empty ring; [`HashRing::lookup`] then returns
    /// `None`. A `RING_HASH` cluster has >= 1 endpoint by construction
    /// (`from_bootstrap` rejects empty clusters), so the empty-ring case is only
    /// reachable via a hot-reload that applies an empty set — the caller's
    /// no-host path then handles it.
    pub(crate) fn build(addresses: &[String], min_ring_size: u64) -> HashRing {
        let num_hosts = addresses.len();
        if num_hosts == 0 {
            return HashRing {
                entries: Vec::new(),
            };
        }
        // Integer division per the §6.2-LOCKED algorithm (e.g. 1024/2 = 512).
        let replicas = min_ring_size / num_hosts as u64;
        let mut entries: Vec<(u64, usize)> = Vec::with_capacity(num_hosts * replicas as usize);
        for (host_index, address) in addresses.iter().enumerate() {
            for i in 0..replicas {
                // The `_` separator is LOAD-BEARING — a one-character change here
                // breaks the cross-proxy differential. `{i}` is the plain decimal index.
                let hash = xxh64(format!("{address}_{i}").as_bytes());
                entries.push((hash, host_index));
            }
        }
        // Sort ascending by hash (stable on the hash key; ties keep insertion order).
        entries.sort_by_key(|(hash, _)| *hash);
        HashRing { entries }
    }

    /// Look up the host index for `key_hash` (the `xxh64` of the request hash key).
    /// Returns the host of the first ring entry with `entry.hash >= key_hash`
    /// (clockwise), wrapping to the index-0 (smallest-hash) entry's host when
    /// `key_hash` exceeds every entry. Returns `None` only for an empty ring.
    pub(crate) fn lookup(&self, key_hash: u64) -> Option<usize> {
        if self.entries.is_empty() {
            return None;
        }
        // `partition_point` returns the index of the first entry whose hash is
        // NOT `< key_hash`, i.e. the first `entry.hash >= key_hash` (bisect_left).
        let pos = self.entries.partition_point(|(hash, _)| *hash < key_hash);
        // Wrap to index 0 when no entry is >= key_hash (request hash past the max).
        let idx = if pos == self.entries.len() { 0 } else { pos };
        Some(self.entries[idx].1)
    }

    /// Number of ring entries (host_index slots). Test-only introspection.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_hosts() -> Vec<String> {
        vec![
            "172.22.0.2:5678".to_string(), // host 0 = the "ONE" backend
            "172.22.0.3:5678".to_string(), // host 1 = the "TWO" backend
        ]
    }

    // (a) 1024 total entries (512 per host), sorted ascending by hash.
    #[test]
    fn build_produces_min_ring_size_entries_sorted() {
        let ring = HashRing::build(&two_hosts(), 1024);
        assert_eq!(ring.len(), 1024, "512 entries per host * 2 hosts");
        assert!(
            ring.entries.windows(2).all(|w| w[0].0 <= w[1].0),
            "entries must be sorted ascending by hash"
        );
    }

    // (b) lookup returns the first entry.hash >= request_hash, and wraps to the
    // index-0 entry's host when request_hash exceeds the max.
    #[test]
    fn lookup_finds_first_ge_and_wraps_past_max() {
        // Hand-built ring with known small hashes; construct directly so we can
        // assert the wrap explicitly.
        let ring = HashRing {
            entries: vec![(10, 7), (20, 8), (30, 9)],
        };
        // Exact / between: first entry >= key.
        assert_eq!(ring.lookup(0), Some(7), "below all → first entry");
        assert_eq!(ring.lookup(10), Some(7), "== smallest → that entry");
        assert_eq!(ring.lookup(11), Some(8), "between → next entry up");
        assert_eq!(ring.lookup(30), Some(9), "== largest → that entry");
        // Past the max → wrap to index 0 (the smallest-hash entry's host).
        assert_eq!(ring.lookup(31), Some(7), "above all → wrap to index 0");
        assert_eq!(ring.lookup(u64::MAX), Some(7), "u64::MAX → wrap to index 0");
    }

    // (c) THE PINNED ORACLE — empirical ground truth from the §6.2 recon against
    // live Envoy v1.33.0 (PROGRESS.md oracle table). host 0 = 172.22.0.2:5678
    // (ONE), host 1 = 172.22.0.3:5678 (TWO). If ANY fail, the ring impl is wrong.
    #[test]
    fn pinned_oracle_matches_live_envoy() {
        let ring = HashRing::build(&two_hosts(), 1024);
        let cases: &[(&str, usize)] = &[
            ("key-0", 0),
            ("key-2", 1),
            ("key-10", 0),
            ("key-11", 1),
            ("key-14", 0),
            ("key-19", 1),
            ("user-alice", 0),
            ("1.2.3.4", 1),
        ];
        for (key, expected_host) in cases {
            let got = ring.lookup(xxh64(key.as_bytes()));
            assert_eq!(
                got,
                Some(*expected_host),
                "oracle key {key:?} must map to host {expected_host}"
            );
        }
    }

    // (d) single-host ring → every lookup returns host 0.
    #[test]
    fn single_host_always_returns_host_zero() {
        let ring = HashRing::build(&["10.0.0.1:80".to_string()], 1024);
        assert_eq!(ring.len(), 1024, "1024 entries for the sole host");
        for key in ["", "a", "key-0", "1.2.3.4", "anything"] {
            assert_eq!(ring.lookup(xxh64(key.as_bytes())), Some(0));
        }
    }

    // (e) determinism: same key → same host across repeated lookups.
    #[test]
    fn lookup_is_deterministic() {
        let ring = HashRing::build(&two_hosts(), 1024);
        for key in ["key-0", "key-19", "user-alice"] {
            let h = xxh64(key.as_bytes());
            let first = ring.lookup(h);
            for _ in 0..10 {
                assert_eq!(ring.lookup(h), first, "key {key:?} must be stable");
            }
        }
    }

    // Empty ring → lookup returns None (the hot-reload-applies-empty edge).
    #[test]
    fn empty_ring_lookup_returns_none() {
        let ring = HashRing::build(&[], 1024);
        assert_eq!(ring.len(), 0);
        assert_eq!(ring.lookup(0), None);
        assert_eq!(ring.lookup(u64::MAX), None);
    }
}
