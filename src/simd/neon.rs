//! NEON 4-lane `Wide` + zip/trn gather.
//!
//! Clean-room; RFC 1321 schedule. Edition 2024: intrinsic calls sit in inner
//! `unsafe` blocks inside `unsafe fn` bodies.

use super::wide::Wide;
use core::arch::aarch64::*;

pub const LANES: usize = 4;

/// NEON `uint32x4_t` backend for the generic multi-buffer kernel.
pub struct Neon4;

/// 4×4 u32 transpose via `vtrn` (rows = messages).
#[inline(always)]
pub(super) unsafe fn transpose4(r: [uint32x4_t; 4]) -> [uint32x4_t; 4] {
    unsafe {
        let t01 = vtrnq_u32(r[0], r[1]);
        let t23 = vtrnq_u32(r[2], r[3]);
        let c0 = vcombine_u32(vget_low_u32(t01.0), vget_low_u32(t23.0));
        let c1 = vcombine_u32(vget_low_u32(t01.1), vget_low_u32(t23.1));
        let c2 = vcombine_u32(vget_high_u32(t01.0), vget_high_u32(t23.0));
        let c3 = vcombine_u32(vget_high_u32(t01.1), vget_high_u32(t23.1));
        [c0, c1, c2, c3]
    }
}

/// # Safety
/// Each `ptrs[0..n]` readable for 64 bytes.
#[inline(always)]
unsafe fn gather_neon4(ptrs: &[*const u8], n: usize) -> [uint32x4_t; 16] {
    unsafe {
        let last = n - 1;
        let mut p = [ptrs[last]; 4];
        // A bounded pointer loop avoids a variable-size memcpy in every gather.
        #[allow(
            clippy::manual_memcpy,
            reason = "per-block gather; measured memcpy regression"
        )]
        for i in 0..n {
            p[i] = ptrs[i];
        }
        let mut m = [vdupq_n_u32(0); 16];
        let mut words_left = 0usize;
        while words_left < 16 {
            let mut rows = [vdupq_n_u32(0); 4];
            for i in 0..4 {
                rows[i] = vld1q_u32(p[i].add(words_left * 4).cast::<u32>());
            }
            let t = transpose4(rows);
            m[words_left..words_left + 4].copy_from_slice(&t);
            words_left += 4;
        }
        m
    }
}

impl Wide for Neon4 {
    const LANES: usize = LANES;
    type V = uint32x4_t;

    #[inline(always)]
    fn splat(x: u32) -> Self::V {
        unsafe { vdupq_n_u32(x) }
    }

    #[inline(always)]
    fn splat_ref(value: &u32) -> Self::V {
        // SAFETY: value is a valid aligned word; the caller has selected this ISA.
        unsafe { vld1q_dup_u32(value) }
    }

    #[inline(always)]
    fn add(a: Self::V, b: Self::V) -> Self::V {
        unsafe { vaddq_u32(a, b) }
    }

    #[inline(always)]
    fn xor(a: Self::V, b: Self::V) -> Self::V {
        unsafe { veorq_u32(a, b) }
    }

    #[inline(always)]
    fn and(a: Self::V, b: Self::V) -> Self::V {
        unsafe { vandq_u32(a, b) }
    }

    #[inline(always)]
    fn or(a: Self::V, b: Self::V) -> Self::V {
        unsafe { vorrq_u32(a, b) }
    }

    #[inline(always)]
    fn not(a: Self::V) -> Self::V {
        unsafe { vmvnq_u32(a) }
    }

    #[inline(always)]
    fn rotl(a: Self::V, r: u32) -> Self::V {
        unsafe {
            macro_rules! ro {
                ($n:expr) => {{ vorrq_u32(vshlq_n_u32(a, $n), vshrq_n_u32(a, 32 - $n)) }};
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
                    let left = vshlq_u32(a, vdupq_n_s32(r as i32));
                    let right = vshlq_u32(a, vdupq_n_s32(r as i32 - 32));
                    vorrq_u32(left, right)
                }
            }
        }
    }

    #[inline(always)]
    fn to_lanes(v: Self::V, out: &mut [u32]) {
        debug_assert!(out.len() >= LANES);
        unsafe { vst1q_u32(out.as_mut_ptr(), v) }
    }

    #[inline(always)]
    unsafe fn gather_block(ptrs: &[*const u8], n: usize) -> [Self::V; 16] {
        unsafe { gather_neon4(ptrs, n) }
    }
}

/// Equal-length fused batch (groups of 4 when `n > 4`).
#[inline]
pub fn hash_equal(inputs: &[&[u8]], outputs: &mut [[u8; 16]]) {
    super::wide::hash_equal_wide::<Neon4>(inputs, outputs);
}
