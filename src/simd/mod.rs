//! Single-stream assembly / aarch64 opt core and multi-message SIMD dispatch.
//!
//! ```text
//! single_stream.rs + single_x86.rs / single_aarch.rs   single-stream opt
//! wide.rs                            fused multi-buffer kernel
//! neon / neon8 / avx2 / avx512       batch ISA adapters
//! platform.rs                        CPUID + equal-length dispatch
//! mod.rs                             hash_many + update_many schedulers
//! ```
//!
//! Single-stream uses `opt` on x86_64 (asm) and little-endian aarch64
//! (fast-md5-adapted kernel); `hash_many` equal-length runs use fused SIMD
//! groups (see `docs/performance.md`).

use crate::backend;
use crate::multibuf;
use crate::state::Md5State;
const MAX_GROUPS: usize = 4;
/// Most streams one fused kernel call accepts (widest register × `MAX_GROUPS`).
const MAX_BATCH: usize = 16 * MAX_GROUPS;

mod platform;
#[cfg(all(
    feature = "simd",
    target_endian = "little",
    any(target_arch = "aarch64", target_arch = "x86_64"),
    not(feature = "force-portable")
))]
mod wide;

/// Vendored single-stream asm body (feature `opt`, x86_64).
#[cfg(all(
    feature = "opt",
    target_arch = "x86_64",
    not(feature = "force-portable")
))]
mod single_x86;

/// Vendored single-stream aarch64 body (feature `opt`).
#[cfg(all(
    feature = "opt",
    target_arch = "aarch64",
    target_endian = "little",
    not(feature = "force-portable")
))]
mod single_aarch;

/// Vendored single-stream entry (`transform`).
#[cfg(all(
    feature = "opt",
    any(
        target_arch = "x86_64",
        all(target_arch = "aarch64", target_endian = "little")
    ),
    not(feature = "force-portable")
))]
pub mod single_stream;

#[cfg(all(
    feature = "simd",
    target_endian = "little",
    target_arch = "aarch64",
    not(feature = "force-portable")
))]
mod neon;

#[cfg(all(
    feature = "simd",
    target_endian = "little",
    target_arch = "aarch64",
    not(feature = "force-portable")
))]
mod neon8;

#[cfg(all(
    feature = "simd",
    target_endian = "little",
    target_arch = "x86_64",
    not(feature = "force-portable")
))]
mod avx2;

#[cfg(all(
    feature = "simd",
    target_endian = "little",
    target_arch = "x86_64",
    not(feature = "force-portable")
))]
mod avx512;

/// Compile-time max SIMD width for this target (runtime CPUID may be lower).
#[inline]
pub const fn simd_lanes() -> usize {
    platform::COMPILE_MAX
}

/// Best usable batch width on this process (cached CPUID on x86_64).
#[inline]
pub fn runtime_lanes() -> usize {
    platform::lanes()
}

/// Whether SIMD multi-buffer can run on this process.
#[inline]
pub fn simd_active() -> bool {
    platform::lanes() > 1
}

/// Best SIMD kernel label for reports.
#[inline]
pub fn simd_name() -> &'static str {
    platform::name()
}

/// Hash adjacent equal-length runs using the configured SIMD selection policy.
pub fn hash_many_dispatch(inputs: &[&[u8]], outputs: &mut [[u8; 16]]) {
    assert!(
        outputs.len() >= inputs.len(),
        "outputs.len() ({}) < inputs.len() ({})",
        outputs.len(),
        inputs.len()
    );

    let max = platform::lanes();
    if max <= 1 {
        multibuf::hash_many_dispatch(inputs, outputs);
        return;
    }

    let mut i = 0;
    while i < inputs.len() {
        let len0 = inputs[i].len();
        let mut run = 1usize;
        while i + run < inputs.len() && inputs[i + run].len() == len0 {
            run += 1;
        }

        let mut done = 0usize;
        while done < run {
            let left = run - done;
            let batch = pick_batch(left, max, len0);
            if batch == 0 {
                outputs[i + done] = backend::hash(inputs[i + done]);
                done += 1;
            } else {
                platform::hash_equal_n(
                    batch,
                    &inputs[i + done..i + done + batch],
                    &mut outputs[i + done..i + done + batch],
                );
                done += batch;
            }
        }
        i += run;
    }
}

/// Append `inputs[i]` to `states[i]`, sharing SIMD registers between streams.
///
/// Streams need not be aligned, equally long, or all have data. Each stream is
/// first brought to a block boundary on the single-stream backend. Then, while
/// at least four streams still hold a complete block, those streams advance
/// together over the block count they have in common; a stream that runs out
/// simply leaves the batch. Everything left over (tails, or fewer than four
/// streams) goes through the single-stream backend, so no input shape is slower
/// than the scalar loop this replaces.
///
/// # Panics
/// Panics if `states.len() != inputs.len()`.
pub fn update_many_dispatch(states: &mut [Md5State], inputs: &[&[u8]]) {
    assert_eq!(
        states.len(),
        inputs.len(),
        "states.len() ({}) != inputs.len() ({})",
        states.len(),
        inputs.len()
    );

    let max = platform::lanes();
    if pick_batch(states.len(), max, 64) == 0 {
        for (state, input) in states.iter_mut().zip(inputs) {
            state.update(input);
        }
        return;
    }

    let window = max * MAX_GROUPS;
    for (states, inputs) in states.chunks_mut(window).zip(inputs.chunks(window)) {
        update_window(states, inputs, max);
    }
}

/// One scheduling window of at most `max * MAX_GROUPS` streams.
fn update_window(states: &mut [Md5State], inputs: &[&[u8]], max: usize) {
    let n = states.len();
    debug_assert!(n <= MAX_BATCH);

    // Bring every stream to a block boundary. A stream whose buffer is still
    // partial afterwards has no input left, so `rest` alone decides liveness.
    let mut rest: [&[u8]; MAX_BATCH] = [&[]; MAX_BATCH];
    for ((state, input), rest) in states.iter_mut().zip(inputs).zip(&mut rest) {
        let raw = state.raw_mut();
        let head = if raw.buf_len == 0 {
            0
        } else {
            (64 - raw.buf_len as usize).min(input.len())
        };
        raw.update_opt(&input[..head]);
        *rest = &input[head..];
    }

    let mut live = [0usize; MAX_BATCH];
    let mut chain = [[0u32; 4]; MAX_BATCH];
    loop {
        let mut n_live = 0;
        let mut nblocks = usize::MAX;
        for (i, rest) in rest[..n].iter().enumerate() {
            let blocks = rest.len() / 64;
            if blocks != 0 {
                live[n_live] = i;
                n_live += 1;
                nblocks = nblocks.min(blocks);
            }
        }
        let batch = pick_batch(n_live, max, 64);
        if batch == 0 {
            break;
        }
        debug_assert_eq!(batch, n_live);

        let mut blocks: [&[u8]; MAX_BATCH] = [&[]; MAX_BATCH];
        for (slot, &i) in live[..batch].iter().enumerate() {
            chain[slot] = states[i].raw_mut().state;
            blocks[slot] = rest[i];
        }
        if !platform::update_equal_n(&mut chain[..batch], &blocks[..batch], nblocks) {
            break;
        }
        let advanced = nblocks * 64;
        for (slot, &i) in live[..batch].iter().enumerate() {
            let raw = states[i].raw_mut();
            raw.state = chain[slot];
            raw.count = raw.count.wrapping_add(advanced as u64);
            rest[i] = &rest[i][advanced..];
        }
    }

    for (state, rest) in states.iter_mut().zip(rest) {
        if !rest.is_empty() {
            state.raw_mut().update_opt(rest);
        }
    }
}

/// Equal-run size handed to one fused `hash_equal_n` call.
///
/// `0` → sequential `backend::hash` (inherits `opt`).
///
/// Measured crossovers (aarch64 NEON + x86 AVX probe):
/// - `msg_len < 32`: gather never amortizes
/// - `n = 4..7` and `msg_len < 64`: SIMD loses on aarch64 NEON8
#[inline]
fn pick_batch(left: usize, max: usize, msg_len: usize) -> usize {
    if left < 4 || max < 4 || msg_len < 32 {
        return 0;
    }
    if left < 8 && msg_len < 64 {
        return 0;
    }
    left.min(max * MAX_GROUPS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    #[test]
    fn runtime_lanes_valid() {
        let l = runtime_lanes();
        assert!(l == 1 || l == 4 || l == 8 || l == 16, "lanes={l}");
        let _ = simd_name();
        let _ = simd_lanes();
        let _ = simd_active();
    }

    #[test]
    fn pick_batch_policy() {
        assert_eq!(pick_batch(3, 8, 1024), 0);
        assert_eq!(pick_batch(8, 8, 16), 0);
        assert_eq!(pick_batch(4, 8, 32), 0); // small n × short msg: scalar
        assert_eq!(pick_batch(4, 8, 63), 0);
        assert_eq!(pick_batch(4, 8, 64), 4);
        assert_eq!(pick_batch(8, 8, 32), 8); // larger n may use 32-byte SIMD
        assert_eq!(pick_batch(8, 8, 4096), 8);
        assert_eq!(pick_batch(9, 8, 1024), 9);
        assert_eq!(pick_batch(32, 8, 1024), 32);
        assert_eq!(pick_batch(40, 8, 1024), 32); // 8 * MAX_GROUPS
        #[cfg(all(target_arch = "x86_64", feature = "simd"))]
        {
            assert_eq!(pick_batch(8, 16, 1024), 8);
            assert_eq!(pick_batch(32, 16, 1024), 32);
            assert_eq!(pick_batch(80, 16, 1024), 64); // 16 * MAX_GROUPS
        }
        assert_eq!(pick_batch(8, 1, 1024), 0);
    }

    #[test]
    fn mixed_lengths_match_backend() {
        let storage: Vec<Vec<u8>> = vec![
            Vec::new(),
            vec![1u8; 3],
            vec![2u8; 64],
            vec![3u8; 65],
            vec![4u8; 256],
            vec![5u8; 128],
        ];
        let inputs: Vec<&[u8]> = storage.iter().map(|v| v.as_slice()).collect();
        let mut outputs = vec![[0u8; 16]; inputs.len()];
        hash_many_dispatch(&inputs, &mut outputs);
        for (msg, out) in storage.iter().zip(outputs.iter()) {
            assert_eq!(*out, backend::hash(msg));
        }
    }

    #[test]
    fn simd_hash_many_matches_backend() {
        #[cfg(feature = "std")]
        let handle = std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(simd_hash_many_matches_backend_body)
            .expect("spawn test thread");
        #[cfg(feature = "std")]
        handle.join().expect("test thread");
        #[cfg(not(feature = "std"))]
        simd_hash_many_matches_backend_body();
    }

    fn simd_hash_many_matches_backend_body() {
        for count in [1usize, 2, 3, 4, 5, 7, 8, 9, 15, 16, 17, 24, 32] {
            for len in [0usize, 1, 55, 64, 65, 256, 1024] {
                let storage: Vec<Vec<u8>> = (0..count)
                    .map(|lane| {
                        (0..len)
                            .map(|i| (i as u8).wrapping_add((lane as u8).wrapping_mul(17)))
                            .collect()
                    })
                    .collect();
                let inputs: Vec<&[u8]> = storage.iter().map(|v| v.as_slice()).collect();
                let mut outputs = vec![[0u8; 16]; count];
                hash_many_dispatch(&inputs, &mut outputs);
                for (msg, out) in storage.iter().zip(outputs.iter()) {
                    assert_eq!(*out, backend::hash(msg), "count={count} len={len}");
                }
            }
        }
    }

    #[cfg(feature = "opt")]
    #[test]
    fn opt_profile_batch_matches_single_stream() {
        #[cfg(feature = "std")]
        let handle = std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(opt_profile_batch_matches_single_stream_body)
            .expect("spawn test thread");
        #[cfg(feature = "std")]
        handle.join().expect("test thread");
        #[cfg(not(feature = "std"))]
        opt_profile_batch_matches_single_stream_body();
    }

    #[cfg(feature = "opt")]
    fn opt_profile_batch_matches_single_stream_body() {
        let count = runtime_lanes().max(4);
        let len = 200usize;
        let storage: Vec<Vec<u8>> = (0..count)
            .map(|l| (0..len).map(|i| (i as u8).wrapping_add(l as u8)).collect())
            .collect();
        let inputs: Vec<&[u8]> = storage.iter().map(|v| v.as_slice()).collect();
        let mut outputs = vec![[0u8; 16]; count];
        hash_many_dispatch(&inputs, &mut outputs);
        for (msg, out) in storage.iter().zip(outputs.iter()) {
            assert_eq!(*out, backend::hash(msg));
        }
    }
}
