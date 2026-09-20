# md5-simd

[![CI](https://github.com/houseme/md5-simd/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/houseme/md5-simd/actions/workflows/ci.yml)
[![Audit](https://github.com/houseme/md5-simd/actions/workflows/audit.yml/badge.svg?branch=main)](https://github.com/houseme/md5-simd/actions/workflows/audit.yml)
[![Crates](https://img.shields.io/crates/v/md5-simd.svg)](https://crates.io/crates/md5-simd)
[![Documentation](https://docs.rs/md5-simd/badge.svg)](https://docs.rs/md5-simd)
[![Dependency status](https://deps.rs/repo/github/houseme/md5-simd/status.svg)](https://deps.rs/repo/github/houseme/md5-simd)
[![Crates.io Total Downloads](https://img.shields.io/crates/d/md5-simd)](https://crates.io/crates/md5-simd)
[![Crates.io License](https://img.shields.io/crates/l/md5-simd)](https://crates.io/crates/md5-simd)

MD5 checksums for object-storage workloads: streaming uploads, snapshot ETags,
RustCrypto integration, and batches of independent messages.

The default build uses optimized x86_64 and little-endian AArch64 single-stream
compressors with SIMD batch kernels. Other targets retain a portable Rust
implementation.
Single-stream hashing and batch hashing use separate dispatch paths.

> MD5 is cryptographically broken. Use it only for legacy interoperability and
> non-adversarial checksums, never for passwords, signatures, or authenticity.

## Getting started

```toml
[dependencies]
md5-simd = "0.2"
```

The library import is `md5_simd`. Default features are `std`, `opt`, `digest`, and
`simd`. These features can also be selected individually.

```rust
use md5_simd::{Md5, digest, hex_encode};

assert_eq!(hex_encode(&digest(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
let mut hasher = Md5::new();
hasher.update(b"a");
assert_eq!(hasher.finalize_snapshot(), digest(b"a"));
hasher.update(b"bc");
assert_eq!(hasher.finalize(), digest(b"abc"));
```

## Streaming and snapshots

`Md5::finalize()` consumes the hasher. `finalize_snapshot()` and `digest_so_far()`
return the current digest without consuming or changing it. `clone().finalize()`
remains supported. `finalize_reset()` returns the digest and resets the hasher
for another message.

With `std`, the hasher implements `std::io::Write`:

```rust
# #[cfg(feature = "std")]
# {
use md5_simd::{Md5, digest, digest_reader};
use std::io::Write;

let mut hasher = Md5::new();
hasher.write_all(b"hello ")?;
let prefix_hex = hasher.finalize_hex(); // Non-consuming lowercase hex.
assert_eq!(prefix_hex, md5_simd::digest_hex(b"hello "));
hasher.write_all(b"world")?;
assert_eq!(hasher.finalize(), digest(b"hello world"));
assert_eq!(digest_reader(&b"abc"[..])?, digest(b"abc"));
# }
# Ok::<(), std::io::Error>(())
```

Reader helpers hash until EOF, retry interrupted reads, and propagate other I/O
errors. `etag_hex_streaming(reader, chunk)` clamps its buffer size to 4 KiB–1 MiB.

| API | Output | Allocation |
| --- | --- | --- |
| `digest`, native byte finalizers | `[u8; 16]` | None |
| `hex_encode_digest` | `[u8; 32]` | None |
| `hex_encode_into` | Caller-provided byte slice | None |
| `hex_encode`, `digest_hex`, `etag_hex` | Lowercase `String` | One output allocation |
| `Md5::finalize_hex` (`std`) | Snapshot `String` | One output allocation |

`hex_encode` encodes the entire slice. `hex_encode_into` requires at least twice
as many output bytes as input bytes and preserves unused output bytes.

An MD5 hex string has the shape of an ordinary single-part ETag. This crate does
not implement multipart ETag composition, HTTP quoting, Content-MD5 Base64
encoding, or storage-specific encryption semantics.

## RustCrypto Digest

The `digest` feature provides `DigestMd5` and re-exports the **Digest 0.11** traits.
Use `DigestMd5` for generic Digest consumers and `Md5` for the native snapshot API.
Traits from Digest 0.10 are a different API and are not interchangeable.

```rust
# #[cfg(feature = "digest")]
# {
use md5_simd::{Digest, DigestMd5, digest};

let mut hasher = DigestMd5::new();
Digest::update(&mut hasher, b"abc");
assert_eq!(hasher.finalize_reset().as_slice(), &digest(b"abc"));
Digest::update(&mut hasher, b"next message");
assert_eq!(hasher.finalize().as_slice(), &digest(b"next message"));
# }
```

Both interfaces share the selected compressor and padding builder. Digest owns
its partial-block buffer; complete blocks are compressed directly.

## Batch hashing

```rust
use md5_simd::{Md5Engine, digest};

let messages = [[0x5a; 1024]; 8];
let inputs: Vec<&[u8]> = messages.iter().map(|m| m.as_slice()).collect();
let mut outputs = [[0u8; 16]; 8];
Md5Engine::new().hash_many(&inputs, &mut outputs);
assert!(outputs.iter().all(|out| *out == digest(&messages[0])));
```

`hash_many` preserves input order and processes contiguous equal-length runs.
Runs with at least four messages of at least 32 bytes can use SIMD; small runs,
short messages, and unsupported targets use the active single-stream backend.
Inputs need no special alignment. The caller supplies output storage, and the
byte batch API performs no heap allocation or thread spawning.

When an object scheduler has non-adjacent equal-size messages, the `std`
feature also provides `Md5Engine::hash_many_grouped`. It sorts internal indices
by message length, runs the same SIMD dispatcher, and scatters digests back to
the original order. This method allocates temporary indices and digest storage;
use it only when the expected SIMD work outweighs that scheduling cost.

| Target | Batch selection with `simd` |
| --- | --- |
| x86_64 with `std` | Runtime detection: AVX2 (8 lanes), AVX-512F + AVX2 (16 lanes), or scalar |
| Little-endian aarch64 | NEON (4 lanes) or paired NEON (8 lanes) |
| Other targets (including big-endian); x86_64 without `std` | Scalar fallback |
| Any target with `force-portable` | Textbook scalar fallback |

On an AVX-512 host, runs of up to eight messages prefer AVX2. Larger runs can
process up to four vector groups per kernel call. `runtime_lanes()` and
`simd_name()` report available capability; a particular call may use fewer lanes
or fall back. SIMD accelerates independent messages, not one message's sequential
MD5 chain.

`Md5State` is a copyable incremental state, and `Md5Engine::update_many` is the
incremental counterpart of `hash_many`: it advances live streams whose messages
are not complete yet, which is the shape of a streaming upload.

```rust
use md5_simd::{Md5Engine, Md5State, digest};

let uploads = [[0x5a; 4096]; 8];
let engine = Md5Engine::new();
let mut states = [Md5State::new(); 8];
for round in 0..4 {
    // One 1 KiB chunk per live stream; chaining values survive between calls.
    let chunks: Vec<&[u8]> = uploads.iter().map(|u| &u[round * 1024..(round + 1) * 1024]).collect();
    engine.update_many(&mut states, &chunks);
}
assert!(states.iter().all(|s| s.finalize() == digest(&uploads[0])));
```

With `simd`, streams that hold complete blocks share SIMD registers. Streams need
not be equally long, block-aligned, or all supplied with data in a call: each is
first brought to a block boundary, then every stream that still has a complete
block advances over the block count they have in common, and a stream that runs
out leaves the batch. Fewer than four such streams, tails, and targets without a
SIMD kernel use the active single-stream backend, so no shape is slower than
updating the states one by one. `finalize_many` remains a sequential snapshot
loop; `Md5State::finalize()` is a non-consuming snapshot.

### Measured AArch64 batch results

On Apple Silicon, same-host ABBA measurements show the intended batch shape:

| Workload | NEON result vs sequential `md-5` |
| --- | ---: |
| 8 independent × 1 MiB | **3.10×** |
| 16 independent × 1 MiB | **4.22×** |
| 7 independent × 1 MiB tail batch | **2.67×** |
| 9 independent × 1 MiB with one scalar tail | **1.73×** |
| grouped mixed lengths | **1.60×** |

These are independent-message throughput results. A single MD5 message remains
serial and is measured separately; current AArch64 single-message oneshot and
streaming are approximately at RustCrypto `md-5` parity. For non-adjacent object
sizes, `hash_many_grouped` can recover equal-length runs when its temporary sort
and output scatter cost is justified.

## Features and portability

| Feature | Default | Effect |
| --- | :---: | --- |
| `std` | Yes | I/O helpers, native snapshot hex, batch hex, x86 runtime detection |
| `opt` | Yes | x86_64 single-stream assembly; little-endian aarch64 `single-aarch` kernel |
| `digest` | Yes | Optional RustCrypto Digest 0.11 dependency |
| `simd` | Yes | SIMD batches; single-stream dispatch is unchanged |
| `force-portable` | No | Overrides the selected single-stream and batch acceleration |
| `pair-interleave` | No | Experimental scalar pairs when `opt` and `force-portable` are off |
| `zeroize` | No | Best-effort clearing of native `Md5` on drop |

```toml
# Portable scalar with I/O support:
md5-simd = { path = "../md5-simd", default-features = false, features = ["std"] }

# no_std library (uses alloc for String-returning helpers):
# md5-simd = { path = "../md5-simd", default-features = false }
```

Byte hashing APIs do not allocate. `no_std` builds use `alloc`; applications using
allocating helpers need an allocator. `zeroize` does not cover `Md5State` copies,
Digest buffers, or all compiler-made copies. Call `reset()` before reusing a
manually cleared `Md5`.

The project uses edition 2024. No minimum supported Rust version is declared;
the release checks use Rust 1.98.1. Older compiler versions are not guaranteed.

## Performance

Performance depends on CPU, message size, batch shape, features, and compiler.
[Performance notes](https://github.com/houseme/md5-simd/blob/main/docs/performance.md)
record the measured aarch64 refactor comparison, validation limits, and reproduction
commands. No result is a universal throughput guarantee.

Applications should validate their own checksum formatting, I/O error handling,
and workload performance when adopting a new implementation.

## Development

```bash
cargo test --locked
cargo test --locked --features force-portable
cargo test --locked --no-default-features --features std
cargo test --locked --no-default-features
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --all-features -- -D warnings
cargo doc --no-deps
cargo package --locked
cargo run --release --example checksum_profile
```

CI covers tests, feature combinations, documentation, and package verification.
The Audit workflow checks locked dependencies on changes and weekly. Version tags
run the release checks; registry upload requires an explicit manual dispatch.

Run both clippy profiles: `--all-features` enables `force-portable` and excludes
SIMD adapters. README examples run as rustdoc tests.

[Architecture](https://github.com/houseme/md5-simd/blob/main/docs/architecture.md) describes state ownership, dispatch, padding,
and unsafe boundaries. See the [changelog](https://github.com/houseme/md5-simd/blob/main/CHANGELOG.md)
for release notes and the [release guide](https://github.com/houseme/md5-simd/blob/main/docs/releasing.md)
for maintainer checks. Repository tooling and local files are excluded from the
published package by an explicit manifest allowlist.

## License

Apache-2.0. The vendored scalar assembly and its glue retain their BSD-2-Clause
and assembly provenance notices. See [LICENSE](https://github.com/houseme/md5-simd/blob/main/LICENSE) and [NOTICE](https://github.com/houseme/md5-simd/blob/main/NOTICE).

## Acknowledgements

Thank you to [fast-md5](https://github.com/ogital-net/fast-md5) and
[md5-many](https://github.com/Small-Ku/md5-many) for their open-source work on MD5
performance and multi-buffer design. Third-party code attribution is retained
in [NOTICE](https://github.com/houseme/md5-simd/blob/main/NOTICE) and the relevant source headers.
