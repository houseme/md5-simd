//! Single-stream assembly and multi-message SIMD dispatch.
//!
//! ```text
//! single_stream.rs + single_x86.rs   single-stream asm (feature `opt`)
//! wide.rs                            fused multi-buffer kernel
//! neon / neon8 / avx2 / avx512       batch ISA adapters
//! platform.rs                        CPUID + equal-length dispatch
//! mod.rs                             hash_many scheduler
//! ```
//!
//! Single-stream uses `opt` on x86_64; `hash_many`
//! equal-length runs use fused SIMD groups (see `docs/performance.md`).

use crate::backend;
use crate::multibuf;
const MAX_GROUPS: usize = 4;

mod platform;
#[cfg(all(
    feature = "simd",
    target_endian = "little",
    any(target_arch = "aarch64", target_arch = "x86_64"),
    not(feature = "force-portable")
))]
mod wide;

/// Vendored single-stream asm body (feature `opt`).
#[cfg(all(
    feature = "opt",
    target_arch = "x86_64",
    not(feature = "force-portable")
))]
mod single_x86;

/// Vendored single-stream entry (`transform`).
#[cfg(all(
    feature = "opt",
    target_arch = "x86_64",
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

/// Equal-run size handed to one fused `hash_equal_n` call.
///
/// `0` → sequential `backend::hash` (inherits `opt`).
#[inline]
fn pick_batch(left: usize, max: usize, msg_len: usize) -> usize {
    if left < 4 || max < 4 || msg_len < 32 {
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
        assert_eq!(pick_batch(4, 8, 64), 4);
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
    fn simd_hash_many_matches_backend() {
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

    #[cfg(feature = "opt")]
    #[test]
    fn opt_profile_batch_matches_single_stream() {
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
