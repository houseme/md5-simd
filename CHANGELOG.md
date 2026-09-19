# Changelog

## 0.1.0 — Unreleased

Initial release candidate.

### APIs

- One-shot MD5 and streaming `Md5` with independent clones, snapshots, and reset.
- RustCrypto Digest 0.11 adapter through `DigestMd5`.
- Ordered batch hashing and incremental per-message state APIs.
- Reader and `std::io::Write` support, allocating hex helpers, and fixed-buffer hex.
- `no_std` support with `alloc` for allocating helpers.

### Backends

- x86_64 scalar assembly, portable scalar, and textbook reference compressors.
- Runtime-selected AVX2/AVX-512 batches and little-endian aarch64 NEON batches.
- Shared constants, padding, and SIMD framing, with conservative scalar fallback.

### Correctness and packaging

- Full-slice hexadecimal encoding, interrupted-read retries, and complete-block
  validation for raw compression.
- Differential coverage of padding, streaming, snapshots, batch boundaries,
  unaligned input, Digest reset, and byte-count wrapping.
- Runnable README examples, documented feature behavior, and package file allowlist.
- Retained third-party attribution and reproducible development lockfile.
