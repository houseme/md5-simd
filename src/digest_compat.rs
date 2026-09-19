//! RustCrypto `digest` 0.11 compatibility (feature = `"digest"`).
//!
//! Wraps the shared [`crate::core::Raw`] state machine so Digest call sites
//! use the same compressor/padding as [`crate::Md5`].

use core::fmt;

use crate::backend;
use crate::core::Raw;
use digest::block_api::{
    AlgorithmName, Block, BlockSizeUser, Buffer, BufferKindUser, Eager, FixedOutputCore,
    OutputSizeUser, Reset, UpdateCore,
};
use digest::typenum::{U16, U64};
use digest::{HashMarker, Output};

/// Core MD5 state for the Digest trait wrapper.
#[derive(Clone)]
pub struct Md5Core {
    raw: Raw,
}

impl Default for Md5Core {
    #[inline]
    fn default() -> Self {
        Self { raw: Raw::new() }
    }
}

impl HashMarker for Md5Core {}

impl BlockSizeUser for Md5Core {
    type BlockSize = U64;
}

impl BufferKindUser for Md5Core {
    type BufferKind = Eager;
}

impl OutputSizeUser for Md5Core {
    type OutputSize = U16;
}

impl UpdateCore for Md5Core {
    #[inline]
    fn update_blocks(&mut self, blocks: &[Block<Self>]) {
        debug_assert_eq!(self.raw.buf_len, 0);
        let mut state = self.raw.state;
        for block in blocks {
            let block: &[u8; 64] = block.as_slice().try_into().expect("MD5 block = 64 bytes");
            backend::compress_block(&mut state, block);
        }
        self.raw.state = state;
        self.raw.count = self
            .raw
            .count
            .wrapping_add((blocks.len() as u64).wrapping_mul(64));
    }
}

impl FixedOutputCore for Md5Core {
    #[inline]
    fn finalize_fixed_core(&mut self, buffer: &mut Buffer<Self>, out: &mut Output<Self>) {
        let tail = buffer.get_data();
        let digest = crate::frame::finalize_with(
            self.raw.state,
            self.raw.count.wrapping_add(tail.len() as u64),
            tail,
            backend::compress_block,
        );
        out.copy_from_slice(&digest);
    }
}

impl Reset for Md5Core {
    #[inline]
    fn reset(&mut self) {
        self.raw.reset();
    }
}

impl AlgorithmName for Md5Core {
    fn write_alg_name(f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Md5")
    }
}

impl fmt::Debug for Md5Core {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Md5Core { .. }")
    }
}

digest::buffer_fixed!(
    /// Streaming MD5 hasher compatible with RustCrypto `Digest` (0.11).
    pub struct Md5(Md5Core);
    impl: BaseFixedTraits AlgorithmName Default Clone HashMarker Reset FixedOutputReset;
);
