//! Software multi-buffer: interleave two independent MD5 compressions.
//!
//! Uses shared schedule + mix forms from [`crate::compress`] / [`crate::consts`].
//! Default **off** (`pair-interleave`): measured slower than sequential on
//! Apple Silicon aarch64 — see `docs/performance.md`.

use crate::compress::{iv, load_m, state_to_bytes, step_dest};
use crate::frame::build_final_blocks;

/// Whether `hash_many` should use the software pair path.
#[inline]
pub const fn pair_path_active() -> bool {
    cfg!(feature = "pair-interleave") && !cfg!(feature = "force-portable") && !cfg!(feature = "opt")
}

/// Interleave one 64-byte block for two independent MD5 states.
#[inline]
pub fn compress_pair(
    state0: &mut [u32; 4],
    block0: &[u8; 64],
    state1: &mut [u32; 4],
    block1: &[u8; 64],
) {
    let m = [load_m(block0), load_m(block1)];
    let mut v = [*state0, *state1];
    let init = v;

    for step in 0..64 {
        step_dest(&mut v[0], &m[0], step);
        step_dest(&mut v[1], &m[1], step);
    }

    for lane in 0..2 {
        v[lane][0] = init[lane][0].wrapping_add(v[lane][0]);
        v[lane][1] = init[lane][1].wrapping_add(v[lane][1]);
        v[lane][2] = init[lane][2].wrapping_add(v[lane][2]);
        v[lane][3] = init[lane][3].wrapping_add(v[lane][3]);
    }

    *state0 = v[0];
    *state1 = v[1];
}

/// Hash two messages; equal-length pairs use interleaved compress when active.
#[inline]
pub fn hash_pair(a: &[u8], b: &[u8]) -> ([u8; 16], [u8; 16]) {
    if !pair_path_active() || a.len() != b.len() {
        return (crate::backend::hash(a), crate::backend::hash(b));
    }

    let mut s0 = iv();
    let mut s1 = iv();

    let (c0, rem0) = a.as_chunks::<64>();
    let (c1, rem1) = b.as_chunks::<64>();
    for (b0, b1) in c0.iter().zip(c1.iter()) {
        compress_pair(&mut s0, b0, &mut s1, b1);
    }

    let mut fb0 = [[0u8; 64]; 2];
    let mut fb1 = [[0u8; 64]; 2];
    let u0 = build_final_blocks(a.len() as u64, rem0, &mut fb0);
    let u1 = build_final_blocks(b.len() as u64, rem1, &mut fb1);
    debug_assert_eq!(u0, u1);
    for i in 0..u0 {
        compress_pair(&mut s0, &fb0[i], &mut s1, &fb1[i]);
    }

    (state_to_bytes(s0), state_to_bytes(s1))
}

/// Hash many independent messages (pair equal-length neighbors when active).
pub fn hash_many_dispatch(inputs: &[&[u8]], outputs: &mut [[u8; 16]]) {
    assert!(
        outputs.len() >= inputs.len(),
        "outputs.len() ({}) < inputs.len() ({})",
        outputs.len(),
        inputs.len()
    );

    if !pair_path_active() {
        for (input, output) in inputs.iter().zip(outputs.iter_mut()) {
            *output = crate::backend::hash(input);
        }
        return;
    }

    let mut i = 0;
    while i + 1 < inputs.len() {
        let (a, b) = (inputs[i], inputs[i + 1]);
        if a.len() == b.len() && a.len() >= 64 {
            let (d0, d1) = hash_pair(a, b);
            outputs[i] = d0;
            outputs[i + 1] = d1;
            i += 2;
        } else {
            outputs[i] = crate::backend::hash(a);
            i += 1;
        }
    }
    if i < inputs.len() {
        outputs[i] = crate::backend::hash(inputs[i]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compress::{compress, compress_textbook};
    use alloc::vec::Vec;

    #[test]
    fn pair_block_matches_single() {
        let b0: [u8; 64] = core::array::from_fn(|i| i as u8);
        let b1: [u8; 64] = core::array::from_fn(|i| (i as u8).wrapping_mul(3));

        let mut s0 = iv();
        let mut s1 = iv();
        compress_pair(&mut s0, &b0, &mut s1, &b1);

        let mut t0 = iv();
        let mut t1 = iv();
        compress(&mut t0, &b0);
        compress(&mut t1, &b1);
        assert_eq!(s0, t0);
        assert_eq!(s1, t1);

        let mut u0 = iv();
        compress_textbook(&mut u0, &b0);
        assert_eq!(s0, u0);
    }

    #[test]
    fn hash_pair_matches_backend_hash() {
        for len in [64usize, 128, 256, 1024] {
            let a: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_add(1)).collect();
            let b: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_add(9)).collect();
            let (d0, d1) = hash_pair(&a, &b);
            assert_eq!(d0, crate::backend::hash(&a));
            assert_eq!(d1, crate::backend::hash(&b));
        }
    }
}
