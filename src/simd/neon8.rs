//! NEON 8-lane multi-buffer via dual `uint32x4_t` (aarch64).
//!
//! Clean-room `Wide` adapter. Gather applies a 4×4 transpose to each half
//! (lanes 0..4 and 4..8). Edition 2024: inner `unsafe` blocks in `unsafe fn`.

use super::neon::{Neon4, transpose4};
use super::wide::Wide;
use core::arch::aarch64::*;

pub const LANES: usize = 8;

/// Dual-NEON backend: `[lo, hi]` = lanes 0..4 and 4..8.
#[derive(Clone, Copy)]
pub struct Pair4(pub uint32x4_t, pub uint32x4_t);

pub struct Neon8;

impl Wide for Neon8 {
    const LANES: usize = LANES;
    type V = Pair4;

    #[inline(always)]
    fn splat(x: u32) -> Self::V {
        unsafe { Pair4(vdupq_n_u32(x), vdupq_n_u32(x)) }
    }

    #[inline(always)]
    fn splat_ref(value: &u32) -> Self::V {
        // SAFETY: value is a valid aligned word; the caller has selected this ISA.
        unsafe { Pair4(vld1q_dup_u32(value), vld1q_dup_u32(value)) }
    }

    #[inline(always)]
    fn add(a: Self::V, b: Self::V) -> Self::V {
        unsafe { Pair4(vaddq_u32(a.0, b.0), vaddq_u32(a.1, b.1)) }
    }

    #[inline(always)]
    fn xor(a: Self::V, b: Self::V) -> Self::V {
        unsafe { Pair4(veorq_u32(a.0, b.0), veorq_u32(a.1, b.1)) }
    }

    #[inline(always)]
    fn and(a: Self::V, b: Self::V) -> Self::V {
        unsafe { Pair4(vandq_u32(a.0, b.0), vandq_u32(a.1, b.1)) }
    }

    #[inline(always)]
    fn or(a: Self::V, b: Self::V) -> Self::V {
        unsafe { Pair4(vorrq_u32(a.0, b.0), vorrq_u32(a.1, b.1)) }
    }

    #[inline(always)]
    fn not(a: Self::V) -> Self::V {
        unsafe { Pair4(vmvnq_u32(a.0), vmvnq_u32(a.1)) }
    }

    #[inline(always)]
    fn rotl(a: Self::V, r: u32) -> Self::V {
        Pair4(Neon4::rotl(a.0, r), Neon4::rotl(a.1, r))
    }

    #[inline(always)]
    fn to_lanes(v: Self::V, out: &mut [u32]) {
        debug_assert!(out.len() >= LANES);
        unsafe {
            vst1q_u32(out.as_mut_ptr(), v.0);
            vst1q_u32(out.as_mut_ptr().add(4), v.1);
        }
    }

    #[inline(always)]
    fn from_lanes(words: &[u32]) -> Self::V {
        assert!(words.len() >= LANES);
        // SAFETY: the assertion guarantees LANES readable words.
        unsafe { Pair4(vld1q_u32(words.as_ptr()), vld1q_u32(words.as_ptr().add(4))) }
    }

    #[inline(always)]
    unsafe fn gather_block(ptrs: &[*const u8], n: usize) -> [Self::V; 16] {
        debug_assert!((1..=LANES).contains(&n));
        unsafe {
            let last = n - 1;
            let mut p = [ptrs[last]; 8];
            // A bounded pointer loop avoids a variable-size memcpy in every gather.
            #[allow(
                clippy::manual_memcpy,
                reason = "per-block gather; measured memcpy regression"
            )]
            for i in 0..n {
                p[i] = ptrs[i];
            }
            let mut out = [Pair4(vdupq_n_u32(0), vdupq_n_u32(0)); 16];
            let mut w = 0usize;
            while w < 16 {
                let mut lo_rows = [vdupq_n_u32(0); 4];
                let mut hi_rows = [vdupq_n_u32(0); 4];
                for i in 0..4 {
                    lo_rows[i] = vld1q_u32(p[i].add(w * 4).cast::<u32>());
                    hi_rows[i] = vld1q_u32(p[i + 4].add(w * 4).cast::<u32>());
                }
                let lt = transpose4(lo_rows);
                let ht = transpose4(hi_rows);
                for j in 0..4 {
                    out[w + j] = Pair4(lt[j], ht[j]);
                }
                w += 4;
            }
            out
        }
    }
}

/// Equal-length fused batch (groups of 8 when `n > 8`).
#[inline]
pub fn hash_equal(inputs: &[&[u8]], outputs: &mut [[u8; 16]]) {
    super::wide::hash_equal_wide::<Neon8>(inputs, outputs);
}

/// Incremental counterpart of [`hash_equal`]: advance caller chaining values
/// over `nblocks` complete blocks (groups of 8 when `n > 8`).
#[inline]
pub fn update_equal(chain: &mut [[u32; 4]], inputs: &[&[u8]], nblocks: usize) {
    super::wide::update_equal_wide::<Neon8>(chain, inputs, nblocks);
}
