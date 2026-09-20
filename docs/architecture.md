# Architecture and invariants

## Data flow

```text
Md5 / Md5State ---- Raw::update ------- backend::compress_block
                       |                        |
                       |                 textbook / x86 asm / Rust
                       |
                       +-- snapshot ---- frame::finalize_with
                                              |
DigestMd5 -- Digest buffer -- Md5Core ----------+
                                              |
one-shot -- complete input blocks ------------+
                                              |
                                    build_final_blocks

Md5Engine::hash_many -- contiguous equal-length runs
                       |
                       +-- scalar / optional pair fallback
                       +-- platform selection -- hash_equal_wide<ISA>
                                                   |
                                            hardware transpose
                                            shared 64-step kernel
                                            build_final_blocks
```

## Ownership and finalization

`Raw` owns the four chaining words, a partial 64-byte block, its length, and a
wrapping `u64` byte count. `Md5` and `Md5State` use the same state machine.
Snapshot finalization copies only chaining state into the framing operation;
it leaves the stream available for further writes.

Digest 0.11 supplies its own partial-block buffer. `Md5Core` uses the chaining
state and complete-block count from `Raw`; its internal partial buffer stays
empty. Finalization passes the outer tail directly to shared framing. It does
not mutate the core to absorb the tail. This removes a second buffer transition
and the previously unreachable partial-buffer update fallback.

`frame::build_final_blocks` alone appends `0x80`, zeros, and the little-endian
bit length modulo 2^64. A tail of 0–55 bytes uses one final block; 56–63 uses two.
All one-shot, streaming, pair, and SIMD finalization paths use this builder.
`Md5::from_parts` requires a caller-supplied block-aligned state and byte count;
it is not a serialized state or an authentication primitive.

## Compression and dispatch

`consts.rs` owns IV, K, S, message-word order, and the destination cycle. Scalar
and SIMD Rust kernels use these tables. The vendored assembly retains its fixed
instruction schedule but imports K from the same table. Its immediate rotate
amounts and memory operands are part of that audited assembly program.

The textbook compressor keeps independent Boolean expressions for differential
checking; it intentionally shares RFC constants. RustCrypto `md-5` is the
independent test oracle. An internal oracle alone would not detect a corrupted
shared constant.

`backend::compress_block` is the only production single-stream selector:
`force-portable` wins, followed by x86_64 `opt` assembly, then little-endian
aarch64 `opt` (`single_aarch`, adapted from fast-md5), then portable Rust.
The batch scheduler calls that backend for fallback messages. Feature `simd`
does not enter this single-stream decision.

`hash_many_dispatch` validates output capacity and groups adjacent equal-length
inputs without allocation or reordering. The `std`-only `Md5Engine::hash_many_grouped`
entry point is an explicit scheduler option: it sorts temporary indices by
length, dispatches grouped runs, and scatters results back to original order.
`platform` chooses an available ISA.
The same generic wide kernel is inlined into the x86 `target_feature` entries;
removing that inlining can change AVX code generation significantly.

A wide call handles at most four SIMD groups. Full blocks gather active groups
then compress them. On little-endian aarch64, runs with **3+ groups** advance
the 64-step chains with instruction-level step interleaving and pipeline the
next block's gather behind the current compress. x86 keeps compresses inside
`#[target_feature]` entries and uses sequential per-group compress (measured
faster than an out-of-line interleaved helper). Single- and two-group shapes
use the tight gather→compress loop on all SIMD targets.

NEON8 uses two four-lane vectors and shares its transpose and rotate operations
with NEON4. AVX-512 gather uses AVX2 transposes plus AVX-512F inserts. Unused lanes
duplicate the last valid input pointer; they never read beyond a message or
write beyond its corresponding output slot.

## Unsafe boundary

- Scalar word reads are unaligned little-endian loads confined to a 64-byte block.
- SIMD gathers read complete 64-byte blocks; final tails are copied into local
  padded blocks before vector loads. Caller input alignment is unrestricted.
- x86 dispatch checks CPU and OS support for AVX2 and for AVX-512F plus AVX2 before
  entering those kernels. Without `std`, x86 batching conservatively falls back.
- aarch64 adapters use baseline NEON on little-endian targets. Big-endian
  targets select scalar Rust because the vector word loads do not perform
  little-endian conversion. Tests exercise Apple Silicon; big-endian fallback
  selection was checked statically, not executed.
- Internal `Wide` operations are private implementation details invoked within
  the selected ISA entry. No vector kernel is publicly callable.
- Hex conversion constructs only ASCII bytes before creating a `String`.

## API limits

`hash_many` is one-shot SIMD. `update_many` and `finalize_many` are scalar loops
over `Md5State`; neither batches incremental compression. The experimental pair
path stays disabled by default and does not replace the SIMD kernel.

The native `zeroize` method is best-effort clearing, not a cryptographic secret
storage guarantee. `Md5State` remains `Copy`, and Digest owns an external buffer.
The `zeroize` feature affects native `Md5` drop only. Manual clearing requires
`reset()` before normal hashing resumes.

## Regression coverage

Tests check RFC vectors, million-a, random inputs and stream splits, Clone and
snapshot behavior, Digest reset/reuse, modulo length encoding, resumed block
states, SIMD lane/group limits, unaligned inputs, mixed runs, untouched output
sentinels, full hex encoding, and reader interruptions/errors. Feature matrices
must exercise accelerated and forced-portable code separately.
