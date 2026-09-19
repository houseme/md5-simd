#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(missing_docs)]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

mod backend;
mod compress;
mod consts;
mod core;
mod engine;
mod frame;
mod md5;
mod multibuf;
mod simd;
mod state;

#[cfg(feature = "digest")]
mod digest_compat;

pub use backend::{available_backends, backend_name, is_portable_only};
pub use engine::{Md5Engine, md5_many};
pub use md5::{Md5, digest, digest_hex, etag_hex, hex_encode, hex_encode_digest, hex_encode_into};
pub use simd::{runtime_lanes, simd_active, simd_lanes, simd_name};
pub use state::Md5State;

#[cfg(feature = "std")]
pub use md5::{digest_reader, etag_hex_streaming};

pub use consts::{K, S, STATE_INIT};

#[cfg(feature = "digest")]
pub use digest::{self, Digest};
#[cfg(feature = "digest")]
pub use digest_compat::{Md5 as DigestMd5, Md5Core};

/// Hazmat: raw compress entry points (custom framing).
pub use backend::hazmat;

/// Software multi-buffer: pair-interleaved hashing for independent messages.
pub use multibuf::{hash_pair, pair_path_active};

/// MD5 compresses 64-byte blocks.
pub const BLOCK_SIZE: usize = 64;

/// MD5 produces a 16-byte digest.
pub const DIGEST_LEN: usize = 16;

/// Textbook-oracle one-shot (CI / differential). Ignores accelerated backends.
#[inline]
pub fn digest_portable(data: &[u8]) -> [u8; 16] {
    backend::hash_textbook(data)
}

/// In-tree production-kernel one-shot (ignores `opt` / `force-portable`).
#[inline]
pub fn digest_opt_scalar(data: &[u8]) -> [u8; 16] {
    backend::hash_in_tree(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn rfc_empty_and_abc() {
        assert_eq!(hex_encode(&digest(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(
            hex_encode(&digest(b"abc")),
            "900150983cd24fb0d6963f7d28e17f72"
        );
    }

    #[test]
    fn clone_finalize_pattern() {
        let mut h = Md5::new();
        h.update(b"message ");
        let mid = h.clone().finalize();
        h.update(b"digest");
        assert_eq!(
            hex_encode(&h.finalize()),
            "f96b697d7cb7938d525a2f31aaf161d0"
        );
        assert_eq!(hex_encode(&mid), hex_encode(&digest(b"message ")));
        assert_ne!(mid, digest(b"message digest"));
    }

    #[test]
    fn all_kernels_agree() {
        for len in [0usize, 1, 55, 56, 64, 100, 1024] {
            let data: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(13)).collect();
            let active = digest(&data);
            let book = digest_portable(&data);
            let tree = digest_opt_scalar(&data);
            assert_eq!(active, book, "active==textbook len={len}");
            assert_eq!(tree, book, "in-tree==textbook len={len}");
        }
    }

    #[test]
    fn finalize_reset_no_clone_semantics() {
        let mut h = Md5::new();
        h.update(b"abc");
        let d1 = h.finalize_reset();
        assert_eq!(hex_encode(&d1), "900150983cd24fb0d6963f7d28e17f72");
        h.update(b"");
        let d2 = h.finalize_reset();
        assert_eq!(hex_encode(&d2), "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn hazmat_matches_update() {
        let data: Vec<u8> = (0..256).map(|i| i as u8).collect();
        let mut state = STATE_INIT;
        hazmat::compress_blocks(&mut state, &data);
        let mut h = Md5::new();
        h.update(&data);
        let via_md5 = h.finalize();
        // Manual padding for 256-byte message: one 0x80 block with length.
        let mut st = state;
        let mut block = [0u8; 64];
        block[0] = 0x80;
        block[56..64].copy_from_slice(&(256u64 * 8).to_le_bytes());
        hazmat::compress_block(&mut st, &block);
        let mut out = [0u8; 16];
        for (i, w) in st.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
        }
        assert_eq!(out, via_md5);
    }

    #[cfg(feature = "zeroize")]
    #[test]
    fn zeroize_clears_count() {
        let mut h = Md5::new();
        h.update(b"x");
        h.zeroize();
        assert_eq!(h.bytes_hashed(), 0);
    }

    #[cfg(feature = "digest")]
    #[test]
    fn digest_trait_roundtrip() {
        use digest::Digest;
        let mut h = DigestMd5::new();
        Digest::update(&mut h, b"abc");
        assert_eq!(
            hex_encode(h.finalize().as_slice()),
            "900150983cd24fb0d6963f7d28e17f72"
        );
    }
}
