# Changelog

## Unreleased

### Performance

- `Md5Engine::update_many` now advances live streams through the SIMD kernels
  instead of looping over them on the single-stream backend. Signature and
  results are unchanged. Streams may be unequal, unaligned, or idle in a call;
  fewer than four streams with a complete block, tails, and non-SIMD targets keep
  the single-stream path.
- Same-host ABBA against updating the same `Md5State`s one by one (64 KiB chunks,
  1 MiB per stream): Apple Silicon NEON 3.11× (8 streams) and 4.10× (16);
  Intel Core i7-9700 AVX2, against the x86_64 assembly backend, 2.92× (4),
  5.64× (8) and 5.47× (16).
- x86_64 incremental windows hold one SIMD group; ABBA against the four-group
  window measured 1.17× at 16 and 32 streams on AVX2.
- `hash_equal_wide` and the new `update_equal_wide` share one full-block loop.
  ABBA of `hash_many_equal_16/1048576` before and after the extraction:
  1.005× on Apple Silicon, 1.001× on i7-9700 (no change).

### Validation

- Added `tests/update_many.rs`: lockstep, near-equal, stalled/short, arbitrary
  chunking, more streams than one scheduling window, and mixed single/batch
  updates, all against RustCrypto `md-5`.
- Added the `update_many_{4,8,16,32}x1mib` Criterion groups.
- Added `examples/streaming_uploads.rs`: the server loop for hashing uploads in
  flight (admit, one chunk per upload per tick, `update_many`, retire), timed
  against per-upload updates with every digest checked.

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

- Bound debug SIMD compression stack usage by reusing one round's stack frame;
  retain release-mode inlining. Add a 1 MiB thread-stack regression check for
  batch sizes and padding boundaries, including the Windows README example path.
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
