//! Single-stream backend dispatch.
//!
//! ```text
//! force-portable           → textbook oracle
//! opt (x86_64)             → simd::single_stream assembly
//! opt (little-endian aarch64) → simd::single_stream (fast-md5-adapted)
//! otherwise                → in-tree compress
//! ```
//!
//! See `docs/performance.md` for same-host gates.

use crate::compress::{compress, compress_blocks_with, compress_textbook, state_to_bytes};
use crate::consts::STATE_INIT;
use crate::frame::{build_final_blocks, hash_with};

/// RFC 1321 digest of the empty message.
#[inline]
pub const fn empty_digest() -> [u8; 16] {
    [
        0xd4, 0x1d, 0x8c, 0xd9, 0x8f, 0x00, 0xb2, 0x04, 0xe9, 0x80, 0x09, 0x98, 0xec, 0xf8, 0x42,
        0x7e,
    ]
}

/// True when this build’s single-stream path uses a vendored `opt` kernel.
const fn has_opt_kernel() -> bool {
    cfg!(all(
        feature = "opt",
        not(feature = "force-portable"),
        any(
            target_arch = "x86_64",
            all(target_arch = "aarch64", target_endian = "little")
        )
    ))
}

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
        any(
            target_arch = "x86_64",
            all(target_arch = "aarch64", target_endian = "little")
        )
    ))]
    {
        crate::simd::single_stream::transform(state, block);
    }
    #[cfg(all(
        not(feature = "force-portable"),
        any(
            not(feature = "opt"),
            not(any(
                target_arch = "x86_64",
                all(target_arch = "aarch64", target_endian = "little")
            ))
        )
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
///
/// Fast paths: empty → RFC constant; `1..=55` bytes → one padded block;
/// larger inputs use a monomorphic compress loop when an `opt` kernel exists
/// (avoids a generic `FnMut` boundary on multi-block 1 MiB work).
#[inline(always)]
pub fn hash(input: &[u8]) -> [u8; 16] {
    if input.is_empty() {
        return empty_digest();
    }
    if input.len() <= 55 {
        let mut blocks = [[0u8; 64]; 2];
        let used = build_final_blocks(input.len() as u64, input, &mut blocks);
        debug_assert_eq!(used, 1);
        let mut state = STATE_INIT;
        compress_block(&mut state, &blocks[0]);
        return state_to_bytes(state);
    }
    if has_opt_kernel() && input.len() >= 64 {
        return hash_opt_bulk(input);
    }
    hash_with(input, compress_block)
}

/// Monomorphic multi-block oneshot for `opt` single-stream kernels.
#[cfg(all(
    feature = "opt",
    not(feature = "force-portable"),
    any(
        target_arch = "x86_64",
        all(target_arch = "aarch64", target_endian = "little")
    )
))]
#[inline(always)]
fn hash_opt_bulk(input: &[u8]) -> [u8; 16] {
    let mut state = STATE_INIT;
    let (blocks, tail) = input.as_chunks::<64>();
    // Direct kernel calls: no `FnMut` in the 16k-block 1 MiB loop.
    for block in blocks {
        crate::simd::single_stream::transform(&mut state, block);
    }
    let mut pad = [[0u8; 64]; 2];
    let used = build_final_blocks(input.len() as u64, tail, &mut pad);
    for block in pad.iter().take(used) {
        crate::simd::single_stream::transform(&mut state, block);
    }
    state_to_bytes(state)
}

#[cfg(not(all(
    feature = "opt",
    not(feature = "force-portable"),
    any(
        target_arch = "x86_64",
        all(target_arch = "aarch64", target_endian = "little")
    )
)))]
#[inline(always)]
fn hash_opt_bulk(input: &[u8]) -> [u8; 16] {
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
    } else if cfg!(all(
        feature = "opt",
        target_arch = "aarch64",
        target_endian = "little"
    )) {
        "single-aarch(aarch64)"
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
    } else if cfg!(all(
        feature = "opt",
        target_arch = "aarch64",
        target_endian = "little"
    )) {
        &["single-aarch", "in-tree", "portable(force-portable)"]
    } else {
        &["in-tree", "portable(force-portable)"]
    }
}

/// Hazmat: raw compress entry points.
pub mod hazmat {
    pub use super::{compress_block, compress_blocks, empty_digest};
}

/// Textbook-oracle one-shot (ignores accelerated backends).
#[inline]
pub fn hash_textbook(input: &[u8]) -> [u8; 16] {
    if input.is_empty() {
        return empty_digest();
    }
    hash_with(input, compress_textbook)
}

/// In-tree production-kernel one-shot (ignores opt asm / force-portable).
#[inline]
pub fn hash_in_tree(input: &[u8]) -> [u8; 16] {
    if input.is_empty() {
        return empty_digest();
    }
    hash_with(input, compress)
}
