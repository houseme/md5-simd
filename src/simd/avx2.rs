//! AVX2 8-lane `Wide` + hardware 8×8 u32 transpose gather.
//!
//! Clean-room; RFC 1321 schedule. `#[target_feature]` lives on `hash_equal*`.

use super::wide::Wide;
use core::arch::x86_64::*;

pub const LANES: usize = 8;

/// AVX2 `__m256i` backend for the generic multi-buffer kernel.
pub struct Avx2;

/// Transpose 8×8 u32 (rows = messages, cols = word index) via unpack+permute.
///
/// # Safety
/// AVX2 required. Each `r[i]` holds 8 consecutive LE u32s from message `i`.
#[inline(always)]
pub(crate) unsafe fn transpose8(r: [__m256i; 8]) -> [__m256i; 8] {
    unsafe {
        let t0 = _mm256_unpacklo_epi32(r[0], r[1]);
        let t1 = _mm256_unpackhi_epi32(r[0], r[1]);
        let t2 = _mm256_unpacklo_epi32(r[2], r[3]);
        let t3 = _mm256_unpackhi_epi32(r[2], r[3]);
        let t4 = _mm256_unpacklo_epi32(r[4], r[5]);
        let t5 = _mm256_unpackhi_epi32(r[4], r[5]);
        let t6 = _mm256_unpacklo_epi32(r[6], r[7]);
        let t7 = _mm256_unpackhi_epi32(r[6], r[7]);

        let u0 = _mm256_unpacklo_epi64(t0, t2);
        let u1 = _mm256_unpacklo_epi64(t1, t3);
        let u2 = _mm256_unpackhi_epi64(t0, t2);
        let u3 = _mm256_unpackhi_epi64(t1, t3);
        let u4 = _mm256_unpacklo_epi64(t4, t6);
        let u5 = _mm256_unpacklo_epi64(t5, t7);
        let u6 = _mm256_unpackhi_epi64(t4, t6);
        let u7 = _mm256_unpackhi_epi64(t5, t7);

        [
            _mm256_permute2x128_si256::<0x20>(u0, u4),
            _mm256_permute2x128_si256::<0x20>(u2, u6),
            _mm256_permute2x128_si256::<0x20>(u1, u5),
            _mm256_permute2x128_si256::<0x20>(u3, u7),
            _mm256_permute2x128_si256::<0x31>(u0, u4),
            _mm256_permute2x128_si256::<0x31>(u2, u6),
            _mm256_permute2x128_si256::<0x31>(u1, u5),
            _mm256_permute2x128_si256::<0x31>(u3, u7),
        ]
    }
}

/// Gather one 64-byte block from up to 8 lane pointers using transpose.
///
/// # Safety
/// AVX2; `ptrs[0..n]` readable for 64 bytes.
#[inline(always)]
unsafe fn gather_avx2(ptrs: &[*const u8], n: usize) -> [__m256i; 16] {
    unsafe {
        let last = n - 1;
        let mut p = [ptrs[last]; 8];
        // Keep pointer preparation as a bounded loop, avoiding variable-size memcpy.
        #[allow(
            clippy::manual_memcpy,
            reason = "per-block gather; see docs/performance.md"
        )]
        for i in 0..n {
            p[i] = ptrs[i];
        }
        let mut rows = [_mm256_setzero_si256(); 8];
        for i in 0..8 {
            rows[i] = _mm256_loadu_si256(p[i].cast::<__m256i>());
        }
        let w0 = transpose8(rows);
        for i in 0..8 {
            rows[i] = _mm256_loadu_si256(p[i].add(32).cast::<__m256i>());
        }
        let w8 = transpose8(rows);
        [
            w0[0], w0[1], w0[2], w0[3], w0[4], w0[5], w0[6], w0[7], w8[0], w8[1], w8[2], w8[3],
            w8[4], w8[5], w8[6], w8[7],
        ]
    }
}

impl Wide for Avx2 {
    const LANES: usize = LANES;
    type V = __m256i;

    #[inline(always)]
    fn splat(x: u32) -> Self::V {
        unsafe { _mm256_set1_epi32(x as i32) }
    }

    #[inline(always)]
    fn splat_ref(value: &u32) -> Self::V {
        // SAFETY: value is a valid aligned word; the caller has selected this ISA.
        unsafe { _mm256_set1_epi32(*value as i32) }
    }

    #[inline(always)]
    fn add(a: Self::V, b: Self::V) -> Self::V {
        unsafe { _mm256_add_epi32(a, b) }
    }

    #[inline(always)]
    fn xor(a: Self::V, b: Self::V) -> Self::V {
        unsafe { _mm256_xor_si256(a, b) }
    }

    #[inline(always)]
    fn and(a: Self::V, b: Self::V) -> Self::V {
        unsafe { _mm256_and_si256(a, b) }
    }

    #[inline(always)]
    fn or(a: Self::V, b: Self::V) -> Self::V {
        unsafe { _mm256_or_si256(a, b) }
    }

    #[inline(always)]
    fn not(a: Self::V) -> Self::V {
        unsafe { _mm256_xor_si256(a, _mm256_set1_epi32(-1)) }
    }

    #[inline(always)]
    fn rotl(a: Self::V, r: u32) -> Self::V {
        unsafe {
            macro_rules! ro {
                ($n:expr) => {{ _mm256_or_si256(_mm256_slli_epi32(a, $n), _mm256_srli_epi32(a, 32 - $n)) }};
            }
            match r {
                4 => ro!(4),
                5 => ro!(5),
                6 => ro!(6),
                7 => ro!(7),
                9 => ro!(9),
                10 => ro!(10),
                11 => ro!(11),
                12 => ro!(12),
                14 => ro!(14),
                15 => ro!(15),
                16 => ro!(16),
                17 => ro!(17),
                20 => ro!(20),
                21 => ro!(21),
                22 => ro!(22),
                23 => ro!(23),
                _ => {
                    let n = _mm256_set1_epi32(r as i32);
                    let n_inv = _mm256_set1_epi32((32 - r) as i32);
                    _mm256_or_si256(_mm256_sllv_epi32(a, n), _mm256_srlv_epi32(a, n_inv))
                }
            }
        }
    }

    #[inline(always)]
    fn to_lanes(v: Self::V, out: &mut [u32]) {
        debug_assert!(out.len() >= LANES);
        unsafe { _mm256_storeu_si256(out.as_mut_ptr().cast::<__m256i>(), v) }
    }

    #[inline(always)]
    fn from_lanes(words: &[u32]) -> Self::V {
        assert!(words.len() >= LANES);
        // SAFETY: the assertion guarantees LANES readable words; the load is unaligned.
        unsafe { _mm256_loadu_si256(words.as_ptr().cast::<__m256i>()) }
    }

    #[inline(always)]
    unsafe fn gather_block(ptrs: &[*const u8], n: usize) -> [Self::V; 16] {
        unsafe { gather_avx2(ptrs, n) }
    }
}

/// Equal-length fused batch (any `n`, groups of 8).
///
/// # Safety
/// CPU must support AVX2.
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn hash_equal(inputs: &[&[u8]], outputs: &mut [[u8; 16]]) {
    super::wide::hash_equal_wide::<Avx2>(inputs, outputs);
}

/// Incremental counterpart of [`hash_equal`]: advance caller chaining values
/// over `nblocks` complete blocks (any `n`, groups of 8).
///
/// # Safety
/// CPU must support AVX2.
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn update_equal(chain: &mut [[u32; 4]], inputs: &[&[u8]], nblocks: usize) {
    super::wide::update_equal_wide::<Avx2>(chain, inputs, nblocks);
}
