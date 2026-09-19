//! AVX-512F 16-lane `Wide` + dual-8 transpose gather.
//!
//! Clean-room; RFC 1321 schedule. Gather = two AVX2-style 8×8 transposes
//! combined with AVX-512F `inserti64x4`. Dispatch requires both AVX-512F
//! and AVX2, including OS support for their register state.

use super::avx2::transpose8;
use super::wide::Wide;
use core::arch::x86_64::*;

pub const LANES: usize = 16;

/// AVX-512 `__m512i` backend for the generic multi-buffer kernel.
pub struct Avx512;

/// Gather 16 lane blocks using 8-wide transposes + ZMM insert.
///
/// # Safety
/// AVX-512F (+ AVX2 for the 256-bit transpose helpers).
#[inline(always)]
unsafe fn gather_avx512(ptrs: &[*const u8], n: usize) -> [__m512i; 16] {
    unsafe {
        let last = n - 1;
        let mut p = [ptrs[last]; 16];
        // Keep pointer preparation as a bounded loop, avoiding variable-size memcpy.
        #[allow(
            clippy::manual_memcpy,
            reason = "per-block gather; see docs/performance.md"
        )]
        for i in 0..n {
            p[i] = ptrs[i];
        }

        let mut lo_rows = [_mm256_setzero_si256(); 8];
        let mut hi_rows = [_mm256_setzero_si256(); 8];
        for i in 0..8 {
            lo_rows[i] = _mm256_loadu_si256(p[i].cast::<__m256i>());
            hi_rows[i] = _mm256_loadu_si256(p[i + 8].cast::<__m256i>());
        }
        let lo = transpose8(lo_rows);
        let hi = transpose8(hi_rows);
        let mut m = [_mm512_setzero_si512(); 16];
        for j in 0..8 {
            // inserti64x4 is AVX-512F (inserti32x8 needs DQ).
            m[j] = _mm512_inserti64x4(_mm512_castsi256_si512(lo[j]), hi[j], 1);
        }

        for i in 0..8 {
            lo_rows[i] = _mm256_loadu_si256(p[i].add(32).cast::<__m256i>());
            hi_rows[i] = _mm256_loadu_si256(p[i + 8].add(32).cast::<__m256i>());
        }
        let lo = transpose8(lo_rows);
        let hi = transpose8(hi_rows);
        for j in 0..8 {
            m[j + 8] = _mm512_inserti64x4(_mm512_castsi256_si512(lo[j]), hi[j], 1);
        }
        m
    }
}

impl Wide for Avx512 {
    const LANES: usize = LANES;
    type V = __m512i;

    #[inline(always)]
    fn splat(x: u32) -> Self::V {
        unsafe { _mm512_set1_epi32(x as i32) }
    }

    #[inline(always)]
    fn splat_ref(value: &u32) -> Self::V {
        // SAFETY: value is a valid aligned word; the caller has selected this ISA.
        unsafe { _mm512_set1_epi32(*value as i32) }
    }

    #[inline(always)]
    fn add(a: Self::V, b: Self::V) -> Self::V {
        unsafe { _mm512_add_epi32(a, b) }
    }

    #[inline(always)]
    fn xor(a: Self::V, b: Self::V) -> Self::V {
        unsafe { _mm512_xor_si512(a, b) }
    }

    #[inline(always)]
    fn and(a: Self::V, b: Self::V) -> Self::V {
        unsafe { _mm512_and_si512(a, b) }
    }

    #[inline(always)]
    fn or(a: Self::V, b: Self::V) -> Self::V {
        unsafe { _mm512_or_si512(a, b) }
    }

    #[inline(always)]
    fn not(a: Self::V) -> Self::V {
        unsafe { _mm512_xor_si512(a, _mm512_set1_epi32(-1)) }
    }

    #[inline(always)]
    fn rotl(a: Self::V, r: u32) -> Self::V {
        unsafe {
            macro_rules! ro {
                ($n:expr) => {{ _mm512_or_si512(_mm512_slli_epi32(a, $n), _mm512_srli_epi32(a, 32 - $n)) }};
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
                    let n = _mm512_set1_epi32(r as i32);
                    let n_inv = _mm512_set1_epi32((32 - r) as i32);
                    _mm512_or_si512(_mm512_sllv_epi32(a, n), _mm512_srlv_epi32(a, n_inv))
                }
            }
        }
    }

    #[inline(always)]
    fn to_lanes(v: Self::V, out: &mut [u32]) {
        debug_assert!(out.len() >= LANES);
        unsafe { _mm512_storeu_si512(out.as_mut_ptr().cast::<__m512i>(), v) }
    }

    #[inline(always)]
    unsafe fn gather_block(ptrs: &[*const u8], n: usize) -> [Self::V; 16] {
        unsafe { gather_avx512(ptrs, n) }
    }
}

/// Equal-length fused batch (any `n`, groups of 16).
///
/// # Safety
/// CPU must support AVX-512F and AVX2 (transpose helpers).
#[target_feature(enable = "avx512f")]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn hash_equal(inputs: &[&[u8]], outputs: &mut [[u8; 16]]) {
    super::wide::hash_equal_wide::<Avx512>(inputs, outputs);
}
