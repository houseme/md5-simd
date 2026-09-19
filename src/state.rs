//! Copyable multi-stream lane state (shares [`crate::core::Raw`] with [`crate::Md5`]).

use crate::backend;
use crate::core::Raw;

/// Incremental state for one MD5 message in a multi-stream workload.
///
/// `finalize` is a **snapshot**: the state is not modified and may receive
/// more data afterwards.
#[derive(Clone, Copy, Debug)]
pub struct Md5State {
    raw: Raw,
}

impl Default for Md5State {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Md5State {
    /// Empty lane state.
    #[inline]
    pub const fn new() -> Self {
        Self { raw: Raw::new() }
    }

    /// Append bytes to this lane.
    #[inline]
    pub fn update(&mut self, input: &[u8]) {
        self.raw.update(input, backend::compress_block);
    }

    /// Snapshot digest; state is left unchanged.
    #[inline]
    #[must_use]
    pub fn finalize(&self) -> [u8; 16] {
        self.raw.digest_snapshot(backend::compress_block)
    }

    /// Reset to empty.
    #[inline]
    pub fn reset(&mut self) {
        self.raw.reset();
    }

    /// Bytes absorbed so far.
    #[inline]
    pub const fn bytes_hashed(&self) -> u64 {
        self.raw.count
    }

    /// Whether nothing has been absorbed since construction/reset.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.raw.count == 0
    }
}
