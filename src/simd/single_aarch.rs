//! Vendored aarch64 scalar compression (feature `opt`).
//!
//! Adapted from fast-md5 `src/aarch64.rs`. Copyright (c) 2026, Latigo LLC,
//! BSD-2-Clause. Full license and provenance: crate-root `NOTICE`.
//!
//! Techniques retained from the reference:
//! - on-demand little-endian message-word loads (`mi`) so LLVM can interleave
//!   loads with the previous round's arithmetic;
//! - per-round `ror` via `asm!` to pin the accumulator in a 32-bit register;
//! - F/G/H/I identities chosen for AArch64 ALU patterns (BIC/AND, ORN).
//!
//! RFC schedule tables live in [`crate::consts`] (K/S/MSG are not duplicated).

use super::single_stream::{BLOCK_SIZE, STATE_WORDS};
use crate::consts::K;

/// Unrolled 64-step AArch64 kernel.
///
/// - `inline_always`: the compress body must sit inside `hash_with` / update
///   loops; a mid-size `#[inline]` regresses 1 MiB oneshot on Apple Silicon.
/// - `too_many_lines` / `many_single_char_names`: the RFC schedule is expanded
///   in place so each rotate amount is a literal; splitting the rounds can
///   inhibit the unroll. `a`/`b`/`c`/`d`/`m` follow RFC 1321 naming.
#[allow(
    clippy::inline_always,
    clippy::too_many_lines,
    clippy::many_single_char_names
)]
#[inline(always)]
pub(crate) fn transform(state: &mut [u32; STATE_WORDS], block: &[u8; BLOCK_SIZE]) {
    let m = block.as_ptr().cast::<u32>();
    let mi = |i: usize| -> u32 {
        // SAFETY: 64-byte block; i in 0..16.
        u32::from_le(unsafe { m.add(i).read_unaligned() })
    };

    let (a0, b0, c0, d0) = (state[0], state[1], state[2], state[3]);
    let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);

    // rotate_left(r) == ror(32-r); asm keeps the value in a w register.
    // `pure, nomem, nostack` lets LLVM schedule surrounding ALU freely.
    macro_rules! ror {
        ($x:expr, $r:expr) => {{
            let mut v: u32 = $x;
            // SAFETY: pure rotate of a local u32; no memory effects.
            unsafe {
                core::arch::asm!(
                    "ror {v:w}, {v:w}, #{n}",
                    v = inout(reg) v,
                    n = const (32u32 - $r),
                    options(pure, nomem, nostack),
                );
            }
            v
        }};
    }

    // F(b,c,d) = D ^ (B & (C^D))
    macro_rules! f {
        ($a:ident, $b:ident, $c:ident, $d:ident, $m:expr, $k:expr, $r:expr) => {
            $a = $a
                .wrapping_add($d ^ ($b & ($c ^ $d)))
                .wrapping_add($m)
                .wrapping_add($k);
            $a = ror!($a, $r).wrapping_add($b);
        };
    }
    // G(b,c,d) = (~D & C) + (D & B)  — BIC + AND on AArch64
    macro_rules! g {
        ($a:ident, $b:ident, $c:ident, $d:ident, $m:expr, $k:expr, $r:expr) => {
            $a = $a
                .wrapping_add((!$d & $c).wrapping_add($d & $b))
                .wrapping_add($m)
                .wrapping_add($k);
            $a = ror!($a, $r).wrapping_add($b);
        };
    }
    // H(b,c,d) = B ^ C ^ D
    macro_rules! h {
        ($a:ident, $b:ident, $c:ident, $d:ident, $m:expr, $k:expr, $r:expr) => {
            $a = $a
                .wrapping_add($b ^ $c ^ $d)
                .wrapping_add($m)
                .wrapping_add($k);
            $a = ror!($a, $r).wrapping_add($b);
        };
    }
    // I(b,c,d) = C ^ (B | ~D)  — ORN on AArch64
    macro_rules! i {
        ($a:ident, $b:ident, $c:ident, $d:ident, $m:expr, $k:expr, $r:expr) => {
            $a = $a
                .wrapping_add($c ^ ($b | !$d))
                .wrapping_add($m)
                .wrapping_add($k);
            $a = ror!($a, $r).wrapping_add($b);
        };
    }

    f!(a, b, c, d, mi(0), K[0], 7);
    f!(d, a, b, c, mi(1), K[1], 12);
    f!(c, d, a, b, mi(2), K[2], 17);
    f!(b, c, d, a, mi(3), K[3], 22);
    f!(a, b, c, d, mi(4), K[4], 7);
    f!(d, a, b, c, mi(5), K[5], 12);
    f!(c, d, a, b, mi(6), K[6], 17);
    f!(b, c, d, a, mi(7), K[7], 22);
    f!(a, b, c, d, mi(8), K[8], 7);
    f!(d, a, b, c, mi(9), K[9], 12);
    f!(c, d, a, b, mi(10), K[10], 17);
    f!(b, c, d, a, mi(11), K[11], 22);
    f!(a, b, c, d, mi(12), K[12], 7);
    f!(d, a, b, c, mi(13), K[13], 12);
    f!(c, d, a, b, mi(14), K[14], 17);
    f!(b, c, d, a, mi(15), K[15], 22);

    g!(a, b, c, d, mi(1), K[16], 5);
    g!(d, a, b, c, mi(6), K[17], 9);
    g!(c, d, a, b, mi(11), K[18], 14);
    g!(b, c, d, a, mi(0), K[19], 20);
    g!(a, b, c, d, mi(5), K[20], 5);
    g!(d, a, b, c, mi(10), K[21], 9);
    g!(c, d, a, b, mi(15), K[22], 14);
    g!(b, c, d, a, mi(4), K[23], 20);
    g!(a, b, c, d, mi(9), K[24], 5);
    g!(d, a, b, c, mi(14), K[25], 9);
    g!(c, d, a, b, mi(3), K[26], 14);
    g!(b, c, d, a, mi(8), K[27], 20);
    g!(a, b, c, d, mi(13), K[28], 5);
    g!(d, a, b, c, mi(2), K[29], 9);
    g!(c, d, a, b, mi(7), K[30], 14);
    g!(b, c, d, a, mi(12), K[31], 20);

    h!(a, b, c, d, mi(5), K[32], 4);
    h!(d, a, b, c, mi(8), K[33], 11);
    h!(c, d, a, b, mi(11), K[34], 16);
    h!(b, c, d, a, mi(14), K[35], 23);
    h!(a, b, c, d, mi(1), K[36], 4);
    h!(d, a, b, c, mi(4), K[37], 11);
    h!(c, d, a, b, mi(7), K[38], 16);
    h!(b, c, d, a, mi(10), K[39], 23);
    h!(a, b, c, d, mi(13), K[40], 4);
    h!(d, a, b, c, mi(0), K[41], 11);
    h!(c, d, a, b, mi(3), K[42], 16);
    h!(b, c, d, a, mi(6), K[43], 23);
    h!(a, b, c, d, mi(9), K[44], 4);
    h!(d, a, b, c, mi(12), K[45], 11);
    h!(c, d, a, b, mi(15), K[46], 16);
    h!(b, c, d, a, mi(2), K[47], 23);

    i!(a, b, c, d, mi(0), K[48], 6);
    i!(d, a, b, c, mi(7), K[49], 10);
    i!(c, d, a, b, mi(14), K[50], 15);
    i!(b, c, d, a, mi(5), K[51], 21);
    i!(a, b, c, d, mi(12), K[52], 6);
    i!(d, a, b, c, mi(3), K[53], 10);
    i!(c, d, a, b, mi(10), K[54], 15);
    i!(b, c, d, a, mi(1), K[55], 21);
    i!(a, b, c, d, mi(8), K[56], 6);
    i!(d, a, b, c, mi(15), K[57], 10);
    i!(c, d, a, b, mi(6), K[58], 15);
    i!(b, c, d, a, mi(13), K[59], 21);
    i!(a, b, c, d, mi(4), K[60], 6);
    i!(d, a, b, c, mi(11), K[61], 10);
    i!(c, d, a, b, mi(2), K[62], 15);
    i!(b, c, d, a, mi(9), K[63], 21);

    state[0] = a0.wrapping_add(a);
    state[1] = b0.wrapping_add(b);
    state[2] = c0.wrapping_add(c);
    state[3] = d0.wrapping_add(d);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::hash_with;
    use alloc::vec::Vec;

    #[test]
    fn rfc_vectors() {
        let vectors: &[(&[u8], &str)] = &[
            (b"", "d41d8cd98f00b204e9800998ecf8427e"),
            (b"abc", "900150983cd24fb0d6963f7d28e17f72"),
            (
                b"abcdefghijklmnopqrstuvwxyz",
                "c3fcd3d76192e4007dfb496cca67e13b",
            ),
        ];
        for (input, want) in vectors {
            assert_eq!(&crate::md5::hex_encode(&hash_with(input, transform)), want);
        }
    }

    #[test]
    fn matches_in_tree() {
        for len in [0usize, 1, 55, 56, 64, 65, 128, 1024] {
            let data: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(31)).collect();
            assert_eq!(
                hash_with(&data, transform),
                crate::backend::hash_in_tree(&data),
                "len={len}"
            );
        }
    }
}
