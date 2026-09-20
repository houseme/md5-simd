# Changelog

## 0.2.0 — 2026-09-20

### Performance follow-up

- Added the optimized little-endian AArch64 scalar backend while retaining
  shared framing, portable fallbacks, and third-party attribution.
- Added `Md5Engine::hash_many_grouped` for object schedulers that need to group
  non-adjacent equal-length messages while restoring caller output order.
- Added AArch64 NEON multi-group shared-K scheduling and static 64-step expansion
  to remove repeated constant broadcasts and runtime step control flow.
- Unified `Md5State` and `DigestMd5` complete-block updates with the optimized
  bulk paths while preserving portable fallbacks.
- Added mixed-length, tail-batch, grouped-scheduler, and Digest streaming
  benchmarks with same-host ABBA evidence.
- Current Apple Silicon evidence shows about 3.10× for eight independent 1 MiB
  messages and 4.22× for sixteen; single-message AArch64 remains near
  RustCrypto `md-5` parity and is not advertised as a large speedup.
- Specialized the multi-group kernel for 2/3/4 groups without inlining the full
  compressor into the caller; fresh 16×1 MiB ABBA reached 4.22×.

### Validation and release tooling

- Added immutable before/after executable comparisons to the ABBA runner.
- Added independent RustCrypto checks for 1 MiB unaligned inputs, final padding
  boundaries, and the streaming APIs. Single-message instruction-scheduling
  experiments did not meet the promotion threshold; the production kernel was
  retained.
- Included the curated performance evidence referenced by the documentation in
  version control so clean-checkout release packages retain those records.
- Moved wide-SIMD integration tests to explicit 8 MiB test stacks so debug
  x86_64 validation no longer fails from the test runner's default stack size.

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
