//! From-scratch xxHash64 (seed 0) for the ring-hash load balancer.
//!
//! Project doctrine D-3.2 forbids adding a hashing crate — load-balancing
//! primitives are written from scratch in pure safe Rust (the crate root carries
//! `#![forbid(unsafe_code)]`). This is the standard xxHash64 algorithm with the
//! seed fixed to 0, which is Envoy's `RING_HASH` default.
//!
//! The canonical test vectors are LOCKED by project decision ADR-0070 (empirically
//! validated 36/36 against live upstream Envoy v1.33.0). The implementation MUST
//! reproduce them byte-for-byte.

// The ring-hash load balancer that consumes `xxh64` lands in a later task of this
// phase; until then the symbol is exercised only by the unit tests below, so the
// dead-code lint would otherwise fire under the non-test build.
#![allow(dead_code)]

const PRIME64_1: u64 = 0x9E37_79B1_85EB_CA87;
const PRIME64_2: u64 = 0xC2B2_AE3D_27D4_EB4F;
const PRIME64_3: u64 = 0x1656_67B1_9E37_79F9;
const PRIME64_4: u64 = 0x85EB_CA77_C2B2_AE63;
const PRIME64_5: u64 = 0x27D4_EB2F_1656_67C5;

/// One xxHash64 round: fold a 64-bit lane input into an accumulator.
#[inline]
fn round(acc: u64, input: u64) -> u64 {
    let acc = acc.wrapping_add(input.wrapping_mul(PRIME64_2));
    acc.rotate_left(31).wrapping_mul(PRIME64_1)
}

/// Merge a finished accumulator lane into the running hash.
#[inline]
fn merge_round(mut h: u64, acc: u64) -> u64 {
    let val = round(0, acc);
    h ^= val;
    h.wrapping_mul(PRIME64_1).wrapping_add(PRIME64_4)
}

/// xxHash64 with the seed fixed to 0 (Envoy `RING_HASH` default).
pub(crate) fn xxh64(data: &[u8]) -> u64 {
    const SEED: u64 = 0;
    let len = data.len();
    let mut input = data;

    let mut h: u64 = if len >= 32 {
        let mut v1 = SEED.wrapping_add(PRIME64_1).wrapping_add(PRIME64_2);
        let mut v2 = SEED.wrapping_add(PRIME64_2);
        let mut v3 = SEED;
        let mut v4 = SEED.wrapping_sub(PRIME64_1);

        // Process 32-byte stripes (4 lanes of 8 bytes each).
        while input.len() >= 32 {
            v1 = round(v1, u64::from_le_bytes(input[0..8].try_into().unwrap()));
            v2 = round(v2, u64::from_le_bytes(input[8..16].try_into().unwrap()));
            v3 = round(v3, u64::from_le_bytes(input[16..24].try_into().unwrap()));
            v4 = round(v4, u64::from_le_bytes(input[24..32].try_into().unwrap()));
            input = &input[32..];
        }

        let mut acc = v1
            .rotate_left(1)
            .wrapping_add(v2.rotate_left(7))
            .wrapping_add(v3.rotate_left(12))
            .wrapping_add(v4.rotate_left(18));
        acc = merge_round(acc, v1);
        acc = merge_round(acc, v2);
        acc = merge_round(acc, v3);
        acc = merge_round(acc, v4);
        acc
    } else {
        SEED.wrapping_add(PRIME64_5)
    };

    h = h.wrapping_add(len as u64);

    // Tail: remaining 8-byte chunks.
    while input.len() >= 8 {
        let k1 = round(0, u64::from_le_bytes(input[0..8].try_into().unwrap()));
        h ^= k1;
        h = h
            .rotate_left(27)
            .wrapping_mul(PRIME64_1)
            .wrapping_add(PRIME64_4);
        input = &input[8..];
    }

    // Tail: remaining 4-byte chunk.
    if input.len() >= 4 {
        let k1 = u32::from_le_bytes(input[0..4].try_into().unwrap()) as u64;
        h ^= k1.wrapping_mul(PRIME64_1);
        h = h
            .rotate_left(23)
            .wrapping_mul(PRIME64_2)
            .wrapping_add(PRIME64_3);
        input = &input[4..];
    }

    // Tail: remaining individual bytes.
    for &b in input {
        h ^= (b as u64).wrapping_mul(PRIME64_5);
        h = h.rotate_left(11).wrapping_mul(PRIME64_1);
    }

    // Final avalanche.
    h ^= h >> 33;
    h = h.wrapping_mul(PRIME64_2);
    h ^= h >> 29;
    h = h.wrapping_mul(PRIME64_3);
    h ^= h >> 32;
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    // LOCKED canonical vectors (ADR-0070): empirically validated 36/36 against
    // live upstream Envoy v1.33.0. These two exercise the empty-input and
    // short-tail (<32 byte) paths.
    #[test]
    fn locked_empty() {
        assert_eq!(xxh64(b""), 0xEF46_DB37_51D8_E999);
    }

    #[test]
    fn locked_abc() {
        assert_eq!(xxh64(b"abc"), 0x44BC_2CF5_AD77_0999);
    }

    // >=32-byte input: exercises the 4-lane block loop PLUS the tail.
    // Expected value generated with the python `xxhash` library (xxhash 3.7.0):
    //   python3 -c "import xxhash; print(hex(xxhash.xxh64(
    //       b'The quick brown fox jumps over the lazy dog', seed=0).intdigest()))"
    //   -> 0xb242d361fda71bc
    // (43 bytes: one 32-byte stripe + 11-byte tail = 8 + ... + bytes path.)
    #[test]
    fn block_loop_plus_tail() {
        assert_eq!(
            xxh64(b"The quick brown fox jumps over the lazy dog"),
            0x0B24_2D36_1FDA_71BC
        );
    }

    // Exact multiple of 32 bytes (64 bytes = two full stripes, no tail).
    // Generated with python `xxhash` 3.7.0:
    //   xxhash.xxh64(b'0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
    //       seed=0).intdigest() -> 0x1af3ac4760fe2f85
    #[test]
    fn multiple_of_32() {
        assert_eq!(
            xxh64(b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"),
            0x1AF3_AC47_60FE_2F85
        );
    }

    // Realistic ring key: the exact string-shape the ring will hash.
    // Generated with python `xxhash` 3.7.0:
    //   xxhash.xxh64(b'172.22.0.2:5678_0', seed=0).intdigest() -> 0xfb4d13869ecafecd
    #[test]
    fn ring_key_shape() {
        assert_eq!(xxh64(b"172.22.0.2:5678_0"), 0xFB4D_1386_9ECA_FECD);
    }
}
