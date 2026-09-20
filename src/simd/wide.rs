//! Shared multi-buffer MD5 kernel — **one** schedule, **one** fused batch path.
//!
//! ```text
//! Wide trait      ISA vector ops + gather_block (hardware transpose when available)
//! compress_wide   unrolled 64-step (consts::{K,S,MSG,DEST})
//! hash_equal_wide fused equal-length groups (n may exceed LANES)
//! ```
//!
//! Fused groups: each block index gathers active groups, then compresses them.
//! aarch64 NEON with 3+ groups interleaves 64-step chains and pipelines the
//! next gather; x86 keeps compress inside `#[target_feature]` (sequential).
//!
//! Compiled only alongside the enabled ISA adapters.

#![allow(clippy::needless_range_loop)]

use super::MAX_GROUPS;
use crate::consts::{DEST, K, MSG, S, STATE_INIT};
use crate::frame::build_final_blocks;

/// Max lanes in one SIMD register.
pub(crate) const MAX_LANES: usize = 16;

/// Architecture-specific u32 vector ops + block gather.
pub(crate) trait Wide {
    const LANES: usize;
    type V: Copy;

    fn splat(x: u32) -> Self::V;
    fn splat_ref(value: &u32) -> Self::V;
    fn add(a: Self::V, b: Self::V) -> Self::V;
    fn xor(a: Self::V, b: Self::V) -> Self::V;
    fn and(a: Self::V, b: Self::V) -> Self::V;
    fn or(a: Self::V, b: Self::V) -> Self::V;
    fn not(a: Self::V) -> Self::V;
    fn rotl(a: Self::V, r: u32) -> Self::V;
    fn to_lanes(v: Self::V, out: &mut [u32]);

    /// Gather one 64-byte block from each of `n` pointers (`1..=LANES`).
    /// Unused lanes reuse the last valid pointer.
    ///
    /// # Safety
    /// `ptrs[0..n]` each point to ≥64 readable bytes.
    unsafe fn gather_block(ptrs: &[*const u8], n: usize) -> [Self::V; 16];
}

/// One MD5 step; `step` is a literal at each unrolled call site.
///
/// G uses the disjoint-mask add identity `(x&z)+(y&!z)` shared with the
/// scalar production kernel (see `compress::mix_g`).
#[inline(always)]
fn wide_step_with_k<W: Wide>(v: &mut [W::V; 4], m: &[W::V; 16], step: usize, kv: W::V) {
    let dest = DEST[step & 3];
    let x = v[(dest + 1) & 3];
    let y = v[(dest + 2) & 3];
    let z = v[(dest + 3) & 3];
    let mix = match step >> 4 {
        0 => W::xor(z, W::and(x, W::xor(y, z))),
        1 => W::xor(y, W::and(z, W::xor(x, y))),
        2 => W::xor(W::xor(x, y), z),
        _ => W::xor(y, W::or(x, W::not(z))),
    };
    let t = W::add(W::add(W::add(v[dest], mix), kv), m[MSG[step]]);
    v[dest] = W::add(x, W::rotl(t, S[step]));
}

#[inline(always)]
fn wide_step<W: Wide>(v: &mut [W::V; 4], m: &[W::V; 16], step: usize) {
    wide_step_with_k::<W>(v, m, step, W::splat_ref(&K[step]));
}

/// 64-step MD5 compression — unrolled so `S[step]` is a literal.
#[inline(always)]
pub(crate) fn compress_wide<W: Wide>(state: &mut [W::V; 4], m: &[W::V; 16]) {
    let mut v = *state;
    macro_rules! st {
        ($i:expr) => {
            wide_step::<W>(&mut v, m, $i)
        };
    }
    st!(0);
    st!(1);
    st!(2);
    st!(3);
    st!(4);
    st!(5);
    st!(6);
    st!(7);
    st!(8);
    st!(9);
    st!(10);
    st!(11);
    st!(12);
    st!(13);
    st!(14);
    st!(15);
    st!(16);
    st!(17);
    st!(18);
    st!(19);
    st!(20);
    st!(21);
    st!(22);
    st!(23);
    st!(24);
    st!(25);
    st!(26);
    st!(27);
    st!(28);
    st!(29);
    st!(30);
    st!(31);
    st!(32);
    st!(33);
    st!(34);
    st!(35);
    st!(36);
    st!(37);
    st!(38);
    st!(39);
    st!(40);
    st!(41);
    st!(42);
    st!(43);
    st!(44);
    st!(45);
    st!(46);
    st!(47);
    st!(48);
    st!(49);
    st!(50);
    st!(51);
    st!(52);
    st!(53);
    st!(54);
    st!(55);
    st!(56);
    st!(57);
    st!(58);
    st!(59);
    st!(60);
    st!(61);
    st!(62);
    st!(63);
    state[0] = W::add(v[0], state[0]);
    state[1] = W::add(v[1], state[1]);
    state[2] = W::add(v[2], state[2]);
    state[3] = W::add(v[3], state[3]);
}

#[inline(always)]
fn iv_state<W: Wide>() -> [W::V; 4] {
    [
        W::splat(STATE_INIT[0]),
        W::splat(STATE_INIT[1]),
        W::splat(STATE_INIT[2]),
        W::splat(STATE_INIT[3]),
    ]
}

#[inline(always)]
fn store_digests<W: Wide>(state: &[W::V; 4], lo: usize, n: usize, outputs: &mut [[u8; 16]]) {
    let mut words = [[0u32; MAX_LANES]; 4];
    for w in 0..4 {
        W::to_lanes(state[w], &mut words[w][..W::LANES]);
    }
    for lane in 0..n {
        let mut out = [0u8; 16];
        for w in 0..4 {
            out[w * 4..w * 4 + 4].copy_from_slice(&words[w][lane].to_le_bytes());
        }
        outputs[lo + lane] = out;
    }
}

/// Gather one 64-byte block index for every active SIMD group.
///
/// Must stay `#[inline(always)]` on x86 so gathers run inside
/// `#[target_feature]` entries. Debug stack growth is handled by widening
/// test thread stacks.
///
/// # Safety
/// For each group, every active lane pointer at `start` has 64 readable bytes.
#[inline(always)]
unsafe fn gather_groups_at<W: Wide>(
    inputs: &[&[u8]],
    start: usize,
    n: usize,
    lanes: usize,
    ngroups: usize,
) -> [[W::V; 16]; MAX_GROUPS] {
    let mut ms = [[W::splat(0); 16]; MAX_GROUPS];
    for g in 0..ngroups {
        let lo = g * lanes;
        let hi = (lo + lanes).min(n);
        let mut ptrs = [core::ptr::null(); MAX_LANES];
        for (i, lane) in (lo..hi).enumerate() {
            // SAFETY: start identifies a complete 64-byte input block.
            ptrs[i] = unsafe { inputs[lane].as_ptr().add(start) };
        }
        // SAFETY: every active pointer has 64 readable bytes; 1 <= hi-lo <= lanes.
        ms[g] = unsafe { W::gather_block(&ptrs, hi - lo) };
    }
    ms
}

/// Compress multi-group blocks with instruction-level step interleaving.
///
/// Independent 64-step chains are advanced together so rotate/add latency on
/// one group is covered by other groups' vector ops.
#[inline(never)]
fn compress_groups_interleaved<W: Wide>(
    states: &mut [[W::V; 4]; MAX_GROUPS],
    ms: &[[W::V; 16]; MAX_GROUPS],
    ngroups: usize,
) {
    debug_assert!(ngroups >= 2);
    let mut vs = [[W::splat(0); 4]; MAX_GROUPS];
    vs[..ngroups].copy_from_slice(&states[..ngroups]);
    macro_rules! st {
        ($step:literal) => {{
            // One K broadcast serves every independent group in this block.
            let kv = W::splat_ref(&K[$step]);
            for g in 0..ngroups {
                wide_step_with_k::<W>(&mut vs[g], &ms[g], $step, kv);
            }
        }};
    }
    st!(0);
    st!(1);
    st!(2);
    st!(3);
    st!(4);
    st!(5);
    st!(6);
    st!(7);
    st!(8);
    st!(9);
    st!(10);
    st!(11);
    st!(12);
    st!(13);
    st!(14);
    st!(15);
    st!(16);
    st!(17);
    st!(18);
    st!(19);
    st!(20);
    st!(21);
    st!(22);
    st!(23);
    st!(24);
    st!(25);
    st!(26);
    st!(27);
    st!(28);
    st!(29);
    st!(30);
    st!(31);
    st!(32);
    st!(33);
    st!(34);
    st!(35);
    st!(36);
    st!(37);
    st!(38);
    st!(39);
    st!(40);
    st!(41);
    st!(42);
    st!(43);
    st!(44);
    st!(45);
    st!(46);
    st!(47);
    st!(48);
    st!(49);
    st!(50);
    st!(51);
    st!(52);
    st!(53);
    st!(54);
    st!(55);
    st!(56);
    st!(57);
    st!(58);
    st!(59);
    st!(60);
    st!(61);
    st!(62);
    st!(63);
    for g in 0..ngroups {
        states[g][0] = W::add(vs[g][0], states[g][0]);
        states[g][1] = W::add(vs[g][1], states[g][1]);
        states[g][2] = W::add(vs[g][2], states[g][2]);
        states[g][3] = W::add(vs[g][3], states[g][3]);
    }
}

/// Whether multi-group full blocks should interleave 64-step chains.
///
/// Measured: NEON8 benefits at `ngroups >= 3`; x86 AVX must keep every
/// compress inside `#[target_feature]` entries, so it stays sequential.
const fn prefer_group_interleave() -> bool {
    cfg!(all(target_arch = "aarch64", target_endian = "little"))
}

/// Fused equal-length multi-buffer hash (`n` may exceed `W::LANES`).
///
/// `#[inline(always)]` is load-bearing on x86: `#[target_feature]` entries must
/// absorb this body or AVX codegen degrades.
#[inline(always)]
pub(crate) fn hash_equal_wide<W: Wide>(inputs: &[&[u8]], outputs: &mut [[u8; 16]]) {
    let n = inputs.len();
    debug_assert!(n >= 1);
    debug_assert!(outputs.len() >= n);
    debug_assert!(inputs.iter().all(|s| s.len() == inputs[0].len()));

    let lanes = W::LANES;
    let ngroups = n.div_ceil(lanes);
    debug_assert!(ngroups <= MAX_GROUPS, "n={n} lanes={lanes}");

    let len = inputs[0].len();
    let nfull = len / 64;
    let mut states = [iv_state::<W>(); MAX_GROUPS];

    if nfull > 0 {
        if ngroups == 1 {
            // Single-group tight loop.
            for bi in 0..nfull {
                let start = bi * 64;
                let mut ptrs = [core::ptr::null(); MAX_LANES];
                for (i, lane) in (0..n).enumerate() {
                    // SAFETY: start identifies a complete 64-byte input block.
                    ptrs[i] = unsafe { inputs[lane].as_ptr().add(start) };
                }
                // SAFETY: every pointer has 64 readable bytes.
                let m = unsafe { W::gather_block(&ptrs, n) };
                compress_wide::<W>(&mut states[0], &m);
            }
        } else if prefer_group_interleave() && ngroups >= 2 {
            // aarch64 NEON only: share K broadcasts across groups, and for
            // three or more groups also interleave chains + pipeline gather.
            // SAFETY: block 0 is a complete 64-byte block in every lane.
            let mut cur = unsafe { gather_groups_at::<W>(inputs, 0, n, lanes, ngroups) };
            for bi in 0..nfull {
                let next = if bi + 1 < nfull {
                    // SAFETY: start identifies a complete 64-byte input block.
                    Some(unsafe { gather_groups_at::<W>(inputs, (bi + 1) * 64, n, lanes, ngroups) })
                } else {
                    None
                };
                compress_groups_interleaved::<W>(&mut states, &cur, ngroups);
                match next {
                    Some(ms) => cur = ms,
                    None => break,
                }
            }
        } else {
            // Sequential per-group compress (all x86; also NEON ngroups<=2).
            for bi in 0..nfull {
                let start = bi * 64;
                // SAFETY: start identifies a complete 64-byte input block.
                let ms = unsafe { gather_groups_at::<W>(inputs, start, n, lanes, ngroups) };
                for g in 0..ngroups {
                    compress_wide::<W>(&mut states[g], &ms[g]);
                }
            }
        }
    }

    for g in 0..ngroups {
        let lo = g * lanes;
        let hi = (lo + lanes).min(n);
        let gn = hi - lo;
        let mut pad = [[[0u8; 64]; 2]; MAX_LANES];
        let mut used = [1usize; MAX_LANES];
        for (lane, src) in (lo..hi).enumerate() {
            let rem = &inputs[src][len - (len % 64)..];
            used[lane] = build_final_blocks(len as u64, rem, &mut pad[lane]);
        }
        for pi in 0..used[0] {
            let mut ptrs = [core::ptr::null(); MAX_LANES];
            for i in 0..gn {
                ptrs[i] = pad[i][pi].as_ptr();
            }
            // SAFETY: each active pointer addresses a complete local padding block.
            let m = unsafe { W::gather_block(&ptrs, gn) };
            compress_wide::<W>(&mut states[g], &m);
        }
        store_digests::<W>(&states[g], lo, gn, outputs);
    }
}
