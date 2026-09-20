//! Shared hasher state machine used by `Md5` and `Md5State`.

#[cfg(not(all(
    feature = "opt",
    not(feature = "force-portable"),
    any(
        target_arch = "x86_64",
        all(target_arch = "aarch64", target_endian = "little")
    )
)))]
use crate::compress::compress_blocks_with;
use crate::compress::iv;
use crate::frame::finalize_with;

/// Raw MD5 state: IV words + partial block buffer + byte count.
#[derive(Clone, Copy, Debug)]
pub struct Raw {
    /// Running MD5 state words.
    pub state: [u32; 4],
    /// Partial-block buffer.
    pub buf: [u8; 64],
    /// Valid bytes in `buf` (0..64).
    pub buf_len: u32,
    /// Total bytes absorbed (for length encoding).
    pub count: u64,
}

impl Default for Raw {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Raw {
    #[inline]
    pub const fn new() -> Self {
        Self {
            state: iv(),
            buf: [0u8; 64],
            buf_len: 0,
            count: 0,
        }
    }

    #[inline]
    pub const fn from_parts(state: [u32; 4], count_bytes: u64) -> Self {
        Self {
            state,
            buf: [0u8; 64],
            buf_len: 0,
            count: count_bytes,
        }
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
    fn update(&mut self, mut data: &[u8], mut compress: impl FnMut(&mut [u32; 4], &[u8; 64])) {
        self.count = self.count.wrapping_add(data.len() as u64);
        if self.buf_len > 0 {
            let have = self.buf_len as usize;
            let take = (64 - have).min(data.len());
            self.buf[have..have + take].copy_from_slice(&data[..take]);
            self.buf_len = (have + take) as u32;
            data = &data[take..];
            if self.buf_len == 64 {
                let block = self.buf;
                compress(&mut self.state, &block);
                self.buf_len = 0;
            } else {
                return;
            }
        }
        if data.len() >= 64 {
            let full = data.len() & !63;
            compress_blocks_with(&mut self.state, &data[..full], &mut compress);
            data = &data[full..];
        }
        if !data.is_empty() {
            self.buf[..data.len()].copy_from_slice(data);
            self.buf_len = data.len() as u32;
        }
    }

    /// Absorb bytes using the vendored `opt` single-stream kernel when built.
    ///
    /// Same semantics as [`Self::update`]; the multi-block loop calls
    /// `single_stream::transform` directly (no `FnMut` boundary).
    #[inline(always)]
    pub fn update_opt(&mut self, data: &[u8]) {
        #[cfg(all(
            feature = "opt",
            not(feature = "force-portable"),
            any(
                target_arch = "x86_64",
                all(target_arch = "aarch64", target_endian = "little")
            )
        ))]
        {
            let mut data = data;
            self.count = self.count.wrapping_add(data.len() as u64);
            if self.buf_len > 0 {
                let have = self.buf_len as usize;
                let take = (64 - have).min(data.len());
                self.buf[have..have + take].copy_from_slice(&data[..take]);
                self.buf_len = (have + take) as u32;
                data = &data[take..];
                if self.buf_len == 64 {
                    let block = self.buf;
                    crate::simd::single_stream::transform(&mut self.state, &block);
                    self.buf_len = 0;
                } else {
                    return;
                }
            }
            if data.len() >= 64 {
                let full = data.len() & !63;
                let (chunks, _) = data[..full].as_chunks::<64>();
                for block in chunks {
                    crate::simd::single_stream::transform(&mut self.state, block);
                }
                data = &data[full..];
            }
            if !data.is_empty() {
                self.buf[..data.len()].copy_from_slice(data);
                self.buf_len = data.len() as u32;
            }
        }
        #[cfg(not(all(
            feature = "opt",
            not(feature = "force-portable"),
            any(
                target_arch = "x86_64",
                all(target_arch = "aarch64", target_endian = "little")
            )
        )))]
        {
            self.update(data, crate::backend::compress_block);
        }
    }

    /// Compute a digest without modifying the buffered stream.
    #[inline]
    pub fn digest_snapshot<F>(&self, compress: F) -> [u8; 16]
    where
        F: FnMut(&mut [u32; 4], &[u8; 64]),
    {
        finalize_with(
            self.state,
            self.count,
            &self.buf[..self.buf_len as usize],
            compress,
        )
    }

    /// Reset to IV / empty buffer.
    #[inline]
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Best-effort scrub of hasher material.
    #[inline]
    pub fn zeroize(&mut self) {
        self.state = [0; 4];
        self.buf = [0u8; 64];
        self.buf_len = 0;
        self.count = 0;
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
    }
}
