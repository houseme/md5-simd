# Changelog

## 0.2.0 — Unreleased

### Performance follow-up

- Added `Md5Engine::hash_many_grouped` for object schedulers that need to group
  non-adjacent equal-length messages while restoring caller output order.
- Added AArch64 NEON multi-group shared-K scheduling and static 64-step expansion
  to remove repeated constant broadcasts and runtime step control flow.
- Unified `Md5State` and `DigestMd5` complete-block updates with the optimized
  bulk paths while preserving portable fallbacks.
- Added mixed-length, tail-batch, grouped-scheduler, and Digest streaming
  benchmarks with same-host ABBA evidence.
- Current Apple Silicon evidence shows about 3.10× for eight independent 1 MiB
  messages and 2.40× for sixteen; single-message AArch64 remains near
  RustCrypto `md-5` parity and is not advertised as a large speedup.
- Inlined the statically expanded AArch64 multi-group kernel; a fresh 16×1 MiB
  ABBA measured 2.40× versus sequential `md-5` with both stability gates passed.

## 0.1.0 — 2026-09-20

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
