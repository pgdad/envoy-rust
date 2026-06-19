//! Maglev consistent-hashing lookup table for the `MAGLEV` load balancer.
//!
//! §6.2-LOCKED (ADR-0072; a from-scratch replica reproduced live Envoy v1.33.0
//! 80/80 at the default table_size, 64/64 at M=17). Per-host permutation from TWO
//! xxHash64 invocations of the host `ip:port` string (NO `_i` suffix — unlike the
//! ring): offset = xxh64_seed(key, 0) % M; skip = xxh64_seed(key, 1) % (M-1) + 1
//! (seed 1 is load-bearing); permutation[j] = (offset + j*skip) % M. Populate by
//! the round-robin claim loop in host (config) order; earlier host wins
//! contention. Lookup = table[request_hash % M]. See the pinned-oracle test.

// The maglev table is consumed by `cluster.rs` in Task 5; until then the symbol
// is exercised only by the unit tests below, so the dead-code lint would
// otherwise fire under the non-test build. (Mirrors xxhash.rs / ring_hash.rs.)
#![allow(dead_code)]

use crate::xxhash::xxh64_seed;

#[derive(Debug)]
pub(crate) struct MaglevTable {
    table: Vec<usize>, // length M; entry = host index into the build address slice
    table_size: u64,   // M (prime)
}

impl MaglevTable {
    /// Build over `addresses` (host index i = addresses[i]) for prime `table_size`.
    /// Empty `addresses` (or table_size 0) → empty table (lookup returns None).
    pub(crate) fn build(addresses: &[String], table_size: u64) -> MaglevTable {
        let m = table_size as usize;
        let n = addresses.len();
        if n == 0 || m == 0 {
            return MaglevTable {
                table: Vec::new(),
                table_size,
            };
        }
        let offset: Vec<u64> = addresses
            .iter()
            .map(|a| xxh64_seed(a.as_bytes(), 0) % table_size)
            .collect();
        let skip: Vec<u64> = addresses
            .iter()
            .map(|a| xxh64_seed(a.as_bytes(), 1) % (table_size - 1) + 1)
            .collect();
        let mut next = vec![0u64; n]; // per-host permutation cursor j
        let mut table = vec![usize::MAX; m];
        let mut filled = 0usize;
        loop {
            for host in 0..n {
                // Claim this host's next unclaimed permutation slot.
                // Overflow safety: `next[host] * skip[host]` — both u64; skip < M ≤
                // 5_000_011, and total claims ≤ M ≈ 5M so per-host `next` advances at
                // most ~M times. Worst case ~5M * 5M ≈ 2.5e13 ≪ u64::MAX (1.8e19): no
                // overflow. The `% table_size` keeps the result identical to §A.
                let mut c = ((offset[host] + next[host] * skip[host]) % table_size) as usize;
                while table[c] != usize::MAX {
                    next[host] += 1;
                    c = ((offset[host] + next[host] * skip[host]) % table_size) as usize;
                }
                table[c] = host;
                next[host] += 1;
                filled += 1;
                if filled == m {
                    return MaglevTable { table, table_size };
                }
            }
        }
    }

    /// Look up the host index for `key_hash` (the `xxh64` seed-0 of the request
    /// hash material). Returns `None` only for an empty table.
    pub(crate) fn lookup(&self, key_hash: u64) -> Option<usize> {
        if self.table.is_empty() {
            return None;
        }
        Some(self.table[(key_hash % self.table_size) as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xxhash::xxh64;

    fn two_hosts() -> Vec<String> {
        vec![
            "172.31.0.2:5678".to_string(), // host 0
            "172.31.0.3:5678".to_string(), // host 1
        ]
    }

    // THE PINNED ORACLE — live-Envoy v1.33.0-validated ground truth (§A, ADR-0072).
    // host 0 = 172.31.0.2:5678, host 1 = 172.31.0.3:5678, M = 65537 (default).
    // If ANY key maps to the wrong host, the algorithm is wrong — debug the
    // algorithm, do NOT adjust these expected values.
    #[test]
    fn pinned_oracle_matches_live_envoy() {
        let table = MaglevTable::build(&two_hosts(), 65537);
        let cases: &[(&str, usize)] = &[
            ("key-0", 0),
            ("key-2", 1),
            ("key-7", 1),
            ("key-10", 0),
            ("key-11", 0),
            ("key-14", 1),
            ("key-19", 0),
            ("key-23", 0),
            ("key-33", 1),
            ("key-41", 1),
            ("key-50", 0),
            ("key-63", 1),
            ("user-alice", 1),
            ("user-bob", 0),
            ("1.2.3.4", 0),
            ("session-abc", 1),
            ("foo", 0),
            ("bar", 1),
            ("baz", 0),
            ("a", 1),
            ("hello", 1),
            ("world", 1),
            ("0", 1),
            ("", 0), // empty-but-present value hashes to xxh64(b"") — NOT a fallback
        ];
        for (key, expected_host) in cases {
            let got = table.lookup(xxh64(key.as_bytes()));
            assert_eq!(
                got,
                Some(*expected_host),
                "oracle key {key:?} must map to host {expected_host}"
            );
        }
    }

    // Full-table host-slot distribution at M=65537 (near-perfect split).
    #[test]
    fn distribution_is_near_perfect() {
        let table = MaglevTable::build(&two_hosts(), 65537);
        assert_eq!(table.table.len(), 65537, "table length == M");
        let host0 = table.table.iter().filter(|&&h| h == 0).count();
        let host1 = table.table.iter().filter(|&&h| h == 1).count();
        assert_eq!(host0, 32769, "host 0 slot count");
        assert_eq!(host1, 32768, "host 1 slot count");
        assert_eq!(host0 + host1, 65537);
    }

    // Single-host table (n=1) → every lookup returns host 0.
    #[test]
    fn single_host_always_returns_host_zero() {
        let table = MaglevTable::build(&["10.0.0.1:80".to_string()], 65537);
        assert_eq!(table.table.len(), 65537);
        for key in ["", "a", "key-0", "1.2.3.4", "anything"] {
            assert_eq!(table.lookup(xxh64(key.as_bytes())), Some(0));
        }
    }

    // Empty build (n=0) → lookup returns None.
    #[test]
    fn empty_build_lookup_returns_none() {
        let table = MaglevTable::build(&[], 65537);
        assert!(table.table.is_empty());
        assert_eq!(table.lookup(0), None);
        assert_eq!(table.lookup(u64::MAX), None);
    }

    // Determinism: building twice yields byte-identical tables.
    #[test]
    fn build_is_deterministic() {
        let a = MaglevTable::build(&two_hosts(), 65537);
        let b = MaglevTable::build(&two_hosts(), 65537);
        assert_eq!(a.table, b.table, "two builds must be identical");
    }
}
