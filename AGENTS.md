# AGENTS.md — contributor guide

`md5-simd` provides streaming and batch MD5 checksums. MD5 is unsuitable for
passwords, signatures, or authenticity checks.

## Project references

- [README](README.md): public APIs and feature selection.
- [Architecture](docs/architecture.md): state, framing, dispatch, and unsafe boundaries.
- [Performance](docs/performance.md): measurement methodology and evidence.
- [Release guide](docs/releasing.md): validation and package preparation.
- [NOTICE](NOTICE): third-party provenance and license text.

## Implementation invariants

1. `frame::build_final_blocks` is the only production padding builder.
2. `consts` owns the RFC constants and message schedule.
3. `backend::compress_block` selects the single-stream compressor.
4. `simd::hash_many_dispatch` schedules one-shot batches and
   `simd::update_many_dispatch` schedules incremental ones; `wide::hash_equal_wide`
   and `wide::update_equal_wide` share `wide::compress_full_blocks`, the only SIMD
   full-block loop. Keep ISA-specific operations in their adapters.
5. `Md5` supports independent clones; snapshot methods and `Md5State::finalize`
   preserve the stream. Digest 0.11 uses the same compressor and framing.
6. Every backend must match RFC 1321 and the independent RustCrypto test oracle.
7. Preserve third-party source headers and NOTICE when editing vendored code.

Defaults are `std,opt,digest,simd`. `force-portable` overrides acceleration.
Do not change public API, feature defaults, license metadata, or declare an MSRV
without an explicit decision. Document unsafe preconditions and API error/panic
contracts; avoid historical implementation narratives in source comments.

## Verification

```bash
cargo test --locked
cargo test --locked --features force-portable
cargo test --locked --no-default-features
cargo test --locked --no-default-features --features std
cargo test --locked --no-default-features --features simd,digest
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps
cargo run --locked --example etag_stream
cargo run --locked --release --example checksum_profile
cargo package --locked --list
cargo package --locked
```

Use `--allow-dirty` for an intentional local package preview only. Run both
clippy profiles: all-features enables force-portable and excludes SIMD adapters.
On Apple Silicon, also check x86_64 code with
`cargo check --locked --target x86_64-apple-darwin --features opt,simd,digest`.
Cross-compilation is not native instruction execution or performance evidence.

## Performance and release discipline

Use same-host ABBA comparisons with immutable binaries and predetermined
stability thresholds. Keep pair-interleave off by default. Preserve measured
hot-path choices unless replacement code passes correctness and performance
checks; do not introduce duplicate compression or padding implementations.

Keep Cargo.lock, tests, documentation, and curated validation data versioned.
The manifest's package allowlist excludes local state and maintainer tooling.
Inspect the archive before publication. Release preparation does not itself
request a tag, push, or registry upload.
