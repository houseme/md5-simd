//! Scalar MD5 compression with optimized and textbook Boolean expressions.
//!
//! - [`compress`]: unrolled scalar kernel with equivalent F/G identities
//! - [`compress_textbook`]: RFC textbook F/G oracle (`force-portable` / CI)
//! - [`load_m`] / [`mix_*`] / [`step_dest`]: shared by multi-buffer interleave
//!
//! Message-word order and dest-register cycle live in [`crate::consts`].

use crate::consts::{DEST, K, MSG, S, STATE_INIT};

/// Load 16 little-endian message words from a 64-byte block.
///
/// The input need not be aligned; each word is decoded as little-endian.
#[inline(always)]
pub(crate) fn load_m(block: &[u8; 64]) -> [u32; 16] {
    let mut m = [0u32; 16];
    let p = block.as_ptr();
    let mut i = 0;
    while i < 16 {
        // SAFETY: i < 16; the unaligned four-byte load stays inside block.
        let q = unsafe { p.add(i * 4).cast::<u32>() };
        m[i] = u32::from_le(unsafe { q.read_unaligned() });
        i += 1;
    }
    m
}

/// Production F identity: `F(x,y,z) ≡ z ^ (x & (y ^ z))`.
#[inline(always)]
pub(crate) fn mix_f(x: u32, y: u32, z: u32) -> u32 {
    z ^ (x & (y ^ z))
}

/// Production G identity: `(x & z) + (y & !z)`.
///
/// Masks are disjoint, so addition matches textbook `(x & z) | (y & !z)`.
/// The add form can schedule better than the xor-mask identity on aarch64.
#[inline(always)]
pub(crate) fn mix_g(x: u32, y: u32, z: u32) -> u32 {
    (x & z).wrapping_add(y & !z)
}

/// Production H identity.
#[inline(always)]
pub(crate) fn mix_h(x: u32, y: u32, z: u32) -> u32 {
    x ^ y ^ z
}

/// Production I identity.
#[inline(always)]
pub(crate) fn mix_i(x: u32, y: u32, z: u32) -> u32 {
    y ^ (x | !z)
}

/// Apply production mix + one MD5 step to `v` in place (used by pair path).
#[inline(always)]
pub(crate) fn step_dest(v: &mut [u32; 4], m: &[u32; 16], step: usize) {
    let dest = DEST[step & 3];
    let x = v[(dest + 1) & 3];
    let y = v[(dest + 2) & 3];
    let z = v[(dest + 3) & 3];
    let mix = match step >> 4 {
        0 => mix_f(x, y, z),
        1 => mix_g(x, y, z),
        2 => mix_h(x, y, z),
        _ => mix_i(x, y, z),
    };
    v[dest] = x.wrapping_add(
        v[dest]
            .wrapping_add(mix)
            .wrapping_add(K[step])
            .wrapping_add(m[MSG[step]])
            .rotate_left(S[step]),
    );
}

/// Compress a block using equivalent Boolean identities and 64 unrolled steps.
#[inline(always)]
pub fn compress(state: &mut [u32; 4], block: &[u8; 64]) {
    let m = load_m(block);
    let a0 = state[0];
    let b0 = state[1];
    let c0 = state[2];
    let d0 = state[3];
    let mut a = a0;
    let mut b = b0;
    let mut c = c0;
    let mut d = d0;

    macro_rules! step {
        ($mix:expr, $a:ident, $b:ident, $c:ident, $d:ident, $k:expr) => {{
            $a = $b.wrapping_add(
                $a.wrapping_add($mix)
                    .wrapping_add(K[$k])
                    .wrapping_add(m[MSG[$k]])
                    .rotate_left(S[$k]),
            );
        }};
    }

    // Round 1 — F
    step!(d ^ (b & (c ^ d)), a, b, c, d, 0);
    step!(c ^ (a & (b ^ c)), d, a, b, c, 1);
    step!(b ^ (d & (a ^ b)), c, d, a, b, 2);
    step!(a ^ (c & (d ^ a)), b, c, d, a, 3);
    step!(d ^ (b & (c ^ d)), a, b, c, d, 4);
    step!(c ^ (a & (b ^ c)), d, a, b, c, 5);
    step!(b ^ (d & (a ^ b)), c, d, a, b, 6);
    step!(a ^ (c & (d ^ a)), b, c, d, a, 7);
    step!(d ^ (b & (c ^ d)), a, b, c, d, 8);
    step!(c ^ (a & (b ^ c)), d, a, b, c, 9);
    step!(b ^ (d & (a ^ b)), c, d, a, b, 10);
    step!(a ^ (c & (d ^ a)), b, c, d, a, 11);
    step!(d ^ (b & (c ^ d)), a, b, c, d, 12);
    step!(c ^ (a & (b ^ c)), d, a, b, c, 13);
    step!(b ^ (d & (a ^ b)), c, d, a, b, 14);
    step!(a ^ (c & (d ^ a)), b, c, d, a, 15);

    // Round 2 — G: (x&z)+(y&!z) on the RFC operand order for each step.
    step!((b & d).wrapping_add(c & !d), a, b, c, d, 16);
    step!((a & c).wrapping_add(b & !c), d, a, b, c, 17);
    step!((d & b).wrapping_add(a & !b), c, d, a, b, 18);
    step!((c & a).wrapping_add(d & !a), b, c, d, a, 19);
    step!((b & d).wrapping_add(c & !d), a, b, c, d, 20);
    step!((a & c).wrapping_add(b & !c), d, a, b, c, 21);
    step!((d & b).wrapping_add(a & !b), c, d, a, b, 22);
    step!((c & a).wrapping_add(d & !a), b, c, d, a, 23);
    step!((b & d).wrapping_add(c & !d), a, b, c, d, 24);
    step!((a & c).wrapping_add(b & !c), d, a, b, c, 25);
    step!((d & b).wrapping_add(a & !b), c, d, a, b, 26);
    step!((c & a).wrapping_add(d & !a), b, c, d, a, 27);
    step!((b & d).wrapping_add(c & !d), a, b, c, d, 28);
    step!((a & c).wrapping_add(b & !c), d, a, b, c, 29);
    step!((d & b).wrapping_add(a & !b), c, d, a, b, 30);
    step!((c & a).wrapping_add(d & !a), b, c, d, a, 31);

    // Round 3 — H
    step!(b ^ c ^ d, a, b, c, d, 32);
    step!(a ^ b ^ c, d, a, b, c, 33);
    step!(d ^ a ^ b, c, d, a, b, 34);
    step!(c ^ d ^ a, b, c, d, a, 35);
    step!(b ^ c ^ d, a, b, c, d, 36);
    step!(a ^ b ^ c, d, a, b, c, 37);
    step!(d ^ a ^ b, c, d, a, b, 38);
    step!(c ^ d ^ a, b, c, d, a, 39);
    step!(b ^ c ^ d, a, b, c, d, 40);
    step!(a ^ b ^ c, d, a, b, c, 41);
    step!(d ^ a ^ b, c, d, a, b, 42);
    step!(c ^ d ^ a, b, c, d, a, 43);
    step!(b ^ c ^ d, a, b, c, d, 44);
    step!(a ^ b ^ c, d, a, b, c, 45);
    step!(d ^ a ^ b, c, d, a, b, 46);
    step!(c ^ d ^ a, b, c, d, a, 47);

    // Round 4 — I
    step!(c ^ (b | !d), a, b, c, d, 48);
    step!(b ^ (a | !c), d, a, b, c, 49);
    step!(a ^ (d | !b), c, d, a, b, 50);
    step!(d ^ (c | !a), b, c, d, a, 51);
    step!(c ^ (b | !d), a, b, c, d, 52);
    step!(b ^ (a | !c), d, a, b, c, 53);
    step!(a ^ (d | !b), c, d, a, b, 54);
    step!(d ^ (c | !a), b, c, d, a, 55);
    step!(c ^ (b | !d), a, b, c, d, 56);
    step!(b ^ (a | !c), d, a, b, c, 57);
    step!(a ^ (d | !b), c, d, a, b, 58);
    step!(d ^ (c | !a), b, c, d, a, 59);
    step!(c ^ (b | !d), a, b, c, d, 60);
    step!(b ^ (a | !c), d, a, b, c, 61);
    step!(a ^ (d | !b), c, d, a, b, 62);
    step!(d ^ (c | !a), b, c, d, a, 63);

    state[0] = a0.wrapping_add(a);
    state[1] = b0.wrapping_add(b);
    state[2] = c0.wrapping_add(c);
    state[3] = d0.wrapping_add(d);
}

/// Textbook RFC boolean functions — independent oracle for CI / `force-portable`.
#[inline(always)]
pub fn compress_textbook(state: &mut [u32; 4], block: &[u8; 64]) {
    #[inline(always)]
    fn f(x: u32, y: u32, z: u32) -> u32 {
        (x & y) | (!x & z)
    }
    #[inline(always)]
    fn g(x: u32, y: u32, z: u32) -> u32 {
        (x & z) | (y & !z)
    }
    #[inline(always)]
    fn h(x: u32, y: u32, z: u32) -> u32 {
        x ^ y ^ z
    }
    #[inline(always)]
    fn i(x: u32, y: u32, z: u32) -> u32 {
        y ^ (x | !z)
    }

    let m = load_m(block);
    let initial = *state;
    let [mut a, mut b, mut c, mut d] = initial;

    macro_rules! step {
        ($mix:ident, $a:ident, $b:ident, $c:ident, $d:ident, $k:expr) => {{
            $a = $b.wrapping_add(
                $a.wrapping_add($mix($b, $c, $d))
                    .wrapping_add(K[$k])
                    .wrapping_add(m[MSG[$k]])
                    .rotate_left(S[$k]),
            );
        }};
    }

    step!(f, a, b, c, d, 0);
    step!(f, d, a, b, c, 1);
    step!(f, c, d, a, b, 2);
    step!(f, b, c, d, a, 3);
    step!(f, a, b, c, d, 4);
    step!(f, d, a, b, c, 5);
    step!(f, c, d, a, b, 6);
    step!(f, b, c, d, a, 7);
    step!(f, a, b, c, d, 8);
    step!(f, d, a, b, c, 9);
    step!(f, c, d, a, b, 10);
    step!(f, b, c, d, a, 11);
    step!(f, a, b, c, d, 12);
    step!(f, d, a, b, c, 13);
    step!(f, c, d, a, b, 14);
    step!(f, b, c, d, a, 15);

    step!(g, a, b, c, d, 16);
    step!(g, d, a, b, c, 17);
    step!(g, c, d, a, b, 18);
    step!(g, b, c, d, a, 19);
    step!(g, a, b, c, d, 20);
    step!(g, d, a, b, c, 21);
    step!(g, c, d, a, b, 22);
    step!(g, b, c, d, a, 23);
    step!(g, a, b, c, d, 24);
    step!(g, d, a, b, c, 25);
    step!(g, c, d, a, b, 26);
    step!(g, b, c, d, a, 27);
    step!(g, a, b, c, d, 28);
    step!(g, d, a, b, c, 29);
    step!(g, c, d, a, b, 30);
    step!(g, b, c, d, a, 31);

    step!(h, a, b, c, d, 32);
    step!(h, d, a, b, c, 33);
    step!(h, c, d, a, b, 34);
    step!(h, b, c, d, a, 35);
    step!(h, a, b, c, d, 36);
    step!(h, d, a, b, c, 37);
    step!(h, c, d, a, b, 38);
    step!(h, b, c, d, a, 39);
    step!(h, a, b, c, d, 40);
    step!(h, d, a, b, c, 41);
    step!(h, c, d, a, b, 42);
    step!(h, b, c, d, a, 43);
    step!(h, a, b, c, d, 44);
    step!(h, d, a, b, c, 45);
    step!(h, c, d, a, b, 46);
    step!(h, b, c, d, a, 47);

    step!(i, a, b, c, d, 48);
    step!(i, d, a, b, c, 49);
    step!(i, c, d, a, b, 50);
    step!(i, b, c, d, a, 51);
    step!(i, a, b, c, d, 52);
    step!(i, d, a, b, c, 53);
    step!(i, c, d, a, b, 54);
    step!(i, b, c, d, a, 55);
    step!(i, a, b, c, d, 56);
    step!(i, d, a, b, c, 57);
    step!(i, c, d, a, b, 58);
    step!(i, b, c, d, a, 59);
    step!(i, a, b, c, d, 60);
    step!(i, d, a, b, c, 61);
    step!(i, c, d, a, b, 62);
    step!(i, b, c, d, a, 63);

    state[0] = initial[0].wrapping_add(a);
    state[1] = initial[1].wrapping_add(b);
    state[2] = initial[2].wrapping_add(c);
    state[3] = initial[3].wrapping_add(d);
}

/// Compress complete 64-byte blocks with zero per-block copies.
#[inline(always)]
pub fn compress_blocks_with<F>(state: &mut [u32; 4], data: &[u8], mut compress: F)
where
    F: FnMut(&mut [u32; 4], &[u8; 64]),
{
    assert_eq!(data.len() % 64, 0, "input must contain complete MD5 blocks");
    let (chunks, _rest) = data.as_chunks::<64>();
    // Local state avoids repeated `&mut state` reloads in long 1 MiB loops.
    // Tight iteration (no manual prefetch): sequential HW prefetch covers this
    // path; measured parity or better than `prfm` on Apple Silicon.
    let mut local = *state;
    for block in chunks {
        compress(&mut local, block);
    }
    *state = local;
}

/// Serialize state as 16 LE bytes.
#[inline(always)]
pub fn state_to_bytes(state: [u32; 4]) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[0..4].copy_from_slice(&state[0].to_le_bytes());
    out[4..8].copy_from_slice(&state[1].to_le_bytes());
    out[8..12].copy_from_slice(&state[2].to_le_bytes());
    out[12..16].copy_from_slice(&state[3].to_le_bytes());
    out
}

/// IV for fresh hashers.
#[inline]
pub const fn iv() -> [u32; 4] {
    STATE_INIT
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::hash_with;
    use alloc::vec::Vec;

    #[test]
    fn production_matches_textbook() {
        for len in [0usize, 1, 55, 56, 64, 65, 128, 256, 1024] {
            let data: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(19)).collect();
            assert_eq!(
                hash_with(&data, compress),
                hash_with(&data, compress_textbook),
                "len={len}"
            );
        }
    }

    #[test]
    fn step_dest_loop_matches_unrolled_compress() {
        let block: [u8; 64] = core::array::from_fn(|i| (i as u8).wrapping_add(9));
        let m = load_m(&block);
        let mut v = iv();
        for step in 0..64 {
            step_dest(&mut v, &m, step);
        }
        let mut state = iv();
        compress(&mut state, &block);
        // step_dest mutates working regs; feed final add like compress does.
        // After 64 steps `v` holds working A/B/C/D; compress adds IV.
        v[0] = iv()[0].wrapping_add(v[0]);
        v[1] = iv()[1].wrapping_add(v[1]);
        v[2] = iv()[2].wrapping_add(v[2]);
        v[3] = iv()[3].wrapping_add(v[3]);
        // Wait: compress starts from state=IV, works, then adds IV to working.
        // step_dest starts from iv() as working regs — same as compress's a,b,c,d init.
        // After 64 steps both should equal working; compress then adds IV.
        assert_eq!(v, state);
    }
}
