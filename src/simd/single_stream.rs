//! Vendored **single-stream** compression core (feature `opt`).
//!
//! - x86_64 → [`super::single_x86`] (assembly)
//! - aarch64 → [`super::single_aarch`] (fast-md5-adapted Rust + `ror` asm)
//!
//! Compiled only for `opt` without `force-portable`. Fallback selection lives
//! in backend; source attribution is retained in NOTICE.
//!
//! ## License
//!
//! - x86 glue / asm: **BSD-2-Clause**, Copyright (c) 2026, Latigo LLC.
//!   Asm schedules: animetosho/md5-optimisation (PD / CC0-1.0).
//! - aarch64 glue: **BSD-2-Clause**, Copyright (c) 2026, Latigo LLC
//!   (fast-md5 `src/aarch64.rs`).
//! Full text in crate-root `NOTICE`.

/// MD5 compresses 64-byte blocks.
pub const BLOCK_SIZE: usize = 64;

/// State words in the running MD5 state.
pub const STATE_WORDS: usize = 4;

/// Compress one 64-byte block into the running MD5 state.
#[inline]
pub fn transform(state: &mut [u32; STATE_WORDS], block: &[u8; BLOCK_SIZE]) {
    #[cfg(target_arch = "x86_64")]
    super::single_x86::transform(state, block);
    #[cfg(target_arch = "aarch64")]
    super::single_aarch::transform(state, block);
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        let _ = (state, block);
        unreachable!("single_stream::transform is only built for x86_64/aarch64 opt");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::hash_with;
    use alloc::vec::Vec;

    #[test]
    fn rfc_vectors_via_single_opt() {
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
    fn matches_in_tree_oracle() {
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
