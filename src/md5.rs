//! Streaming MD5 hasher (`Clone` + optional `io::Write`).

use crate::backend;
use crate::core::Raw;
use alloc::string::String;

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Streaming MD5 hasher.
///
/// `Clone` supports independent snapshots (`clone().finalize()`); prefer
/// [`Self::finalize_snapshot`] or [`Self::digest_so_far`] when the hasher
/// must continue absorbing data.
#[derive(Clone, Debug)]
pub struct Md5 {
    raw: Raw,
}

impl Default for Md5 {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Md5 {
    /// New hasher with the RFC 1321 IV.
    #[inline]
    pub const fn new() -> Self {
        Self { raw: Raw::new() }
    }

    /// Resume from a block-aligned state that already absorbed `count_bytes`.
    ///
    /// The caller must supply the chaining state after exactly that many bytes,
    /// with `count_bytes` a multiple of 64. No partial block is restored.
    #[inline]
    pub const fn from_parts(state: [u32; 4], count_bytes: u64) -> Self {
        Self {
            raw: Raw::from_parts(state, count_bytes),
        }
    }

    /// Absorb additional bytes.
    #[inline]
    pub fn update(&mut self, data: &[u8]) {
        self.raw.update(data, backend::compress_block);
    }

    /// Builder-style update.
    #[inline]
    #[must_use]
    pub fn chain_update(mut self, data: &[u8]) -> Self {
        self.update(data);
        self
    }

    /// Consume and return the 16-byte digest.
    #[inline]
    pub fn finalize(self) -> [u8; 16] {
        self.raw.digest_snapshot(backend::compress_block)
    }

    /// Non-consuming digest (same as `self.clone().finalize()` without clone).
    #[inline]
    pub fn finalize_snapshot(&self) -> [u8; 16] {
        self.raw.digest_snapshot(backend::compress_block)
    }

    /// Finalize then reset in place (no full hasher clone).
    #[inline]
    pub fn finalize_reset(&mut self) -> [u8; 16] {
        let d = self.raw.digest_snapshot(backend::compress_block);
        self.raw.reset();
        d
    }

    /// Reset to IV / empty buffer.
    #[inline]
    pub fn reset(&mut self) {
        self.raw.reset();
    }

    /// Bytes absorbed so far (mod 2^64).
    #[inline]
    pub const fn bytes_hashed(&self) -> u64 {
        self.raw.count
    }

    /// Clear the native state on a best-effort basis.
    ///
    /// Call [`Self::reset`] before hashing another message. This does not erase
    /// previously made copies or guarantee removal of compiler-generated copies.
    #[inline]
    pub fn zeroize(&mut self) {
        self.raw.zeroize();
    }

    /// Non-consuming lowercase hex of the current digest (`std`).
    ///
    /// Prefer this over `clone().finalize()` when only the snapshot digest is needed.
    #[cfg(feature = "std")]
    #[inline]
    pub fn finalize_hex(&self) -> String {
        hex_encode(&self.raw.digest_snapshot(backend::compress_block))
    }

    /// Non-consuming digest bytes (same as [`Self::finalize_snapshot`]).
    #[inline]
    pub fn digest_so_far(&self) -> [u8; 16] {
        self.finalize_snapshot()
    }
}

#[cfg(feature = "zeroize")]
impl Drop for Md5 {
    #[inline]
    fn drop(&mut self) {
        self.raw.zeroize();
    }
}

/// One-shot hashing (active backend).
#[inline]
pub fn digest(data: &[u8]) -> [u8; 16] {
    backend::hash(data)
}

/// Hash a message and return its 32-character lowercase hexadecimal digest.
pub fn digest_hex(data: &[u8]) -> String {
    hex_encode(&digest(data))
}

/// Encode every input byte as two lowercase hexadecimal characters.
pub fn hex_encode(bytes: &[u8]) -> String {
    let len = bytes.len().checked_mul(2).expect("hex length overflow");
    let mut buf = alloc::vec![0u8; len];
    hex_encode_into(bytes, &mut buf);
    // SAFETY: hex alphabet is valid ASCII.
    unsafe { String::from_utf8_unchecked(buf) }
}

/// Alias for [`digest_hex`] for applications using MD5-shaped ETags.
///
/// This does not add HTTP quoting or implement multipart ETag composition.
#[inline]
pub fn etag_hex(data: &[u8]) -> String {
    digest_hex(data)
}

/// Lowercase hex of a 16-byte digest into a fixed 32-byte array (no alloc).
#[inline]
pub fn hex_encode_digest(digest: &[u8; 16]) -> [u8; 32] {
    let mut out = [0u8; 32];
    hex_encode_into(digest, &mut out);
    out
}

/// Write lowercase hex into `out`; returns bytes written (`2 * bytes.len()`).
///
/// `out.len()` must be at least `2 * bytes.len()`.
///
/// # Panics
/// Panics before writing if the output is too short.
#[inline]
pub fn hex_encode_into(bytes: &[u8], out: &mut [u8]) -> usize {
    assert!(bytes.len() <= out.len() / 2, "hex output is too short");
    for (i, b) in bytes.iter().enumerate() {
        out[i * 2] = HEX[(b >> 4) as usize];
        out[i * 2 + 1] = HEX[(b & 0x0f) as usize];
    }
    bytes.len() * 2
}

/// Hash a reader to lowercase hexadecimal until EOF.
///
/// `chunk` is clamped to 4 KiB–1 MiB for the allocated read buffer.
///
/// # Errors
/// Returns reader errors other than [`std::io::ErrorKind::Interrupted`], which
/// is retried. On error, no partial digest is returned.
#[cfg(feature = "std")]
pub fn etag_hex_streaming<R: std::io::Read>(reader: R, chunk: usize) -> std::io::Result<String> {
    let chunk = chunk.clamp(4096, 1024 * 1024);
    let mut buf = vec![0u8; chunk];
    digest_reader_with_buffer(reader, &mut buf).map(|digest| hex_encode(&digest))
}

/// Hash a reader until EOF using a 64 KiB stack buffer.
///
/// # Errors
/// Returns reader errors other than [`std::io::ErrorKind::Interrupted`], which
/// is retried. On error, no partial digest is returned.
#[cfg(feature = "std")]
pub fn digest_reader<R: std::io::Read>(reader: R) -> std::io::Result<[u8; 16]> {
    let mut buf = [0u8; 65536];
    digest_reader_with_buffer(reader, &mut buf)
}

#[cfg(feature = "std")]
fn digest_reader_with_buffer<R: std::io::Read>(
    mut reader: R,
    buf: &mut [u8],
) -> std::io::Result<[u8; 16]> {
    let mut h = Md5::new();
    loop {
        match reader.read(buf) {
            Ok(0) => return Ok(h.finalize()),
            Ok(n) => h.update(&buf[..n]),
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        }
    }
}

#[cfg(feature = "std")]
impl std::io::Write for Md5 {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.update(buf);
        Ok(buf.len())
    }

    #[inline]
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
