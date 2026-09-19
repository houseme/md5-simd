//! Single-stream backend dispatch.
//!
//! ```text
//! force-portable  → textbook oracle
//! opt (x86_64)    → simd::single_stream assembly
//! otherwise       → in-tree compress (aarch64 keeps this even with `opt`)
//! ```
//!
//! See `docs/performance.md` for same-host gates.

use crate::compress::{compress, compress_blocks_with, compress_textbook};
use crate::frame::hash_with;

/// Compress one 64-byte block into the running state.
#[inline(always)]
pub fn compress_block(state: &mut [u32; 4], block: &[u8; 64]) {
    #[cfg(feature = "force-portable")]
    {
        compress_textbook(state, block);
    }
    #[cfg(all(
        feature = "opt",
        not(feature = "force-portable"),
        target_arch = "x86_64"
    ))]
    {
        crate::simd::single_stream::transform(state, block);
    }
    #[cfg(all(
        not(feature = "force-portable"),
        any(not(feature = "opt"), not(target_arch = "x86_64"))
    ))]
    {
        compress(state, block);
    }
}

/// Compress complete 64-byte blocks without applying padding.
///
/// # Panics
/// Panics if `data.len()` is not a multiple of 64.
#[inline]
pub fn compress_blocks(state: &mut [u32; 4], data: &[u8]) {
    compress_blocks_with(state, data, compress_block);
}

/// One-shot hash through the active single-stream backend.
#[inline]
pub fn hash(input: &[u8]) -> [u8; 16] {
    hash_with(input, compress_block)
}

/// True when the **selected** compressor is the textbook oracle.
pub const fn is_portable_only() -> bool {
    cfg!(feature = "force-portable")
}

/// Active single-stream backend name.
pub fn backend_name() -> &'static str {
    if cfg!(feature = "force-portable") {
        "portable(force-portable)"
    } else if cfg!(all(feature = "opt", target_arch = "x86_64")) {
        "single-asm(x86_64)"
    } else if cfg!(target_arch = "x86_64") {
        "in-tree(x86_64)"
    } else if cfg!(target_arch = "aarch64") {
        "in-tree(aarch64)"
    } else {
        "in-tree"
    }
}

/// Single-stream backends compiled into this build.
pub fn available_backends() -> &'static [&'static str] {
    if cfg!(feature = "force-portable") {
        &["portable(force-portable)", "in-tree"]
    } else if cfg!(all(feature = "opt", target_arch = "x86_64")) {
        &["single-asm", "in-tree", "portable(force-portable)"]
    } else {
        &["in-tree", "portable(force-portable)"]
    }
}

/// Hazmat: raw compress entry points.
pub mod hazmat {
    pub use super::{compress_block, compress_blocks};
}

/// Textbook-oracle one-shot (ignores accelerated backends).
#[inline]
pub fn hash_textbook(input: &[u8]) -> [u8; 16] {
    hash_with(input, compress_textbook)
}

/// In-tree production-kernel one-shot (ignores opt asm / force-portable).
#[inline]
pub fn hash_in_tree(input: &[u8]) -> [u8; 16] {
    hash_with(input, compress)
}
