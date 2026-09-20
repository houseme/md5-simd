//! One-shot platform detection + equal-length kernel selection.
//!
//! All ISA `cfg` walls live here so `simd/mod.rs` stays a thin scheduler.

use crate::backend;

#[cfg(all(
    feature = "simd",
    target_endian = "little",
    target_arch = "aarch64",
    not(feature = "force-portable")
))]
use super::{neon, neon8};

#[cfg(all(
    feature = "simd",
    target_endian = "little",
    target_arch = "x86_64",
    not(feature = "force-portable")
))]
use super::{avx2, avx512};

/// Whether batch SIMD is compiled in for this target/feature set.
#[inline]
pub(crate) const fn compiled() -> bool {
    cfg!(all(
        feature = "simd",
        target_endian = "little",
        any(target_arch = "aarch64", target_arch = "x86_64"),
        not(feature = "force-portable")
    ))
}

/// Compile-time max lanes for this target (runtime CPUID may be lower).
pub(crate) const COMPILE_MAX: usize = {
    #[cfg(all(
        feature = "simd",
        target_endian = "little",
        target_arch = "aarch64",
        not(feature = "force-portable")
    ))]
    {
        neon8::LANES
    }
    #[cfg(all(
        feature = "simd",
        target_endian = "little",
        target_arch = "x86_64",
        not(feature = "force-portable")
    ))]
    {
        avx512::LANES
    }
    #[cfg(not(all(
        feature = "simd",
        target_endian = "little",
        any(target_arch = "aarch64", target_arch = "x86_64"),
        not(feature = "force-portable")
    )))]
    {
        1
    }
};

/// Best usable batch width on this process (1 = sequential fallback).
pub(crate) fn lanes() -> usize {
    if !compiled() {
        return 1;
    }
    #[cfg(all(
        feature = "simd",
        target_endian = "little",
        target_arch = "aarch64",
        not(feature = "force-portable")
    ))]
    {
        neon8::LANES
    }
    #[cfg(all(
        feature = "simd",
        target_endian = "little",
        target_arch = "x86_64",
        not(feature = "force-portable")
    ))]
    {
        if x86_has_avx512() {
            avx512::LANES
        } else if x86_has_avx2() {
            avx2::LANES
        } else {
            1
        }
    }
    #[cfg(not(all(
        feature = "simd",
        target_endian = "little",
        any(target_arch = "aarch64", target_arch = "x86_64"),
        not(feature = "force-portable")
    )))]
    {
        1
    }
}

/// Human-readable batch kernel name.
pub(crate) fn name() -> &'static str {
    if lanes() <= 1 {
        return "none";
    }
    #[cfg(all(
        feature = "simd",
        target_endian = "little",
        target_arch = "aarch64",
        not(feature = "force-portable")
    ))]
    {
        "simd-neon8-fused"
    }
    #[cfg(all(
        feature = "simd",
        target_endian = "little",
        target_arch = "x86_64",
        not(feature = "force-portable")
    ))]
    {
        if x86_has_avx512() {
            "simd-avx512-fused"
        } else if x86_has_avx2() {
            "simd-avx2-fused"
        } else {
            "none"
        }
    }
    #[cfg(not(all(
        feature = "simd",
        target_endian = "little",
        any(target_arch = "aarch64", target_arch = "x86_64"),
        not(feature = "force-portable")
    )))]
    {
        "none"
    }
}

/// Hash `n` equal-length messages on the best fitting fused kernel.
///
/// `n` may exceed one register width — kernels fuse multiple groups per block.
pub(crate) fn hash_equal_n(n: usize, inputs: &[&[u8]], outputs: &mut [[u8; 16]]) {
    debug_assert_eq!(inputs.len(), n);
    debug_assert!(outputs.len() >= n);
    if n == 0 {
        return;
    }

    #[cfg(all(
        feature = "simd",
        target_endian = "little",
        target_arch = "aarch64",
        not(feature = "force-portable")
    ))]
    {
        if n >= 5 {
            neon8::hash_equal(inputs, outputs);
        } else {
            neon::hash_equal(inputs, outputs);
        }
        return;
    }

    #[cfg(all(
        feature = "simd",
        target_endian = "little",
        target_arch = "x86_64",
        not(feature = "force-portable")
    ))]
    {
        // n ≤ 8 → AVX2 (8-lane transpose gather).
        // n > 8 + AVX-512 → ZMM fused (16-lane groups).
        // n > 8 + AVX2 only → AVX2 fused multi-group.
        if n <= 8 && x86_has_avx2() {
            // SAFETY: AVX2 probed.
            unsafe { avx2::hash_equal(inputs, outputs) };
        } else if x86_has_avx512() {
            // SAFETY: AVX-512F + AVX2 probed on this path.
            unsafe { avx512::hash_equal(inputs, outputs) };
        } else if x86_has_avx2() {
            // SAFETY: AVX2 probed.
            unsafe { avx2::hash_equal(inputs, outputs) };
        } else {
            sequential(n, inputs, outputs);
        }
        return;
    }

    #[allow(unreachable_code)]
    {
        sequential(n, inputs, outputs);
    }
}

/// Advance `n` chaining values over `nblocks` complete blocks on the kernel
/// [`hash_equal_n`] would choose for `n` messages.
///
/// Returns `false` without touching `chain` when no SIMD kernel is usable; the
/// caller then advances each stream on the single-stream backend.
pub(crate) fn update_equal_n(chain: &mut [[u32; 4]], inputs: &[&[u8]], nblocks: usize) -> bool {
    let n = chain.len();
    debug_assert_eq!(inputs.len(), n);
    if n == 0 || nblocks == 0 {
        return true;
    }

    #[cfg(all(
        feature = "simd",
        target_endian = "little",
        target_arch = "aarch64",
        not(feature = "force-portable")
    ))]
    {
        if n >= 5 {
            neon8::update_equal(chain, inputs, nblocks);
        } else {
            neon::update_equal(chain, inputs, nblocks);
        }
        return true;
    }

    #[cfg(all(
        feature = "simd",
        target_endian = "little",
        target_arch = "x86_64",
        not(feature = "force-portable")
    ))]
    {
        if n <= 8 && x86_has_avx2() {
            // SAFETY: AVX2 probed.
            unsafe { avx2::update_equal(chain, inputs, nblocks) };
        } else if x86_has_avx512() {
            // SAFETY: AVX-512F + AVX2 probed on this path.
            unsafe { avx512::update_equal(chain, inputs, nblocks) };
        } else if x86_has_avx2() {
            // SAFETY: AVX2 probed.
            unsafe { avx2::update_equal(chain, inputs, nblocks) };
        } else {
            return false;
        }
        return true;
    }

    #[allow(unreachable_code)]
    {
        let _ = (chain, inputs, nblocks);
        false
    }
}

#[inline]
fn sequential(n: usize, inputs: &[&[u8]], outputs: &mut [[u8; 16]]) {
    for i in 0..n {
        outputs[i] = backend::hash(inputs[i]);
    }
}

#[cfg(all(
    feature = "simd",
    target_endian = "little",
    target_arch = "x86_64",
    not(feature = "force-portable"),
    feature = "std"
))]
#[inline]
fn x86_has_avx2() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| std::arch::is_x86_feature_detected!("avx2"))
}

#[cfg(all(
    feature = "simd",
    target_endian = "little",
    target_arch = "x86_64",
    not(feature = "force-portable"),
    feature = "std"
))]
#[inline]
fn x86_has_avx512() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| std::arch::is_x86_feature_detected!("avx512f") && x86_has_avx2())
}

#[cfg(all(
    feature = "simd",
    target_endian = "little",
    target_arch = "x86_64",
    not(feature = "force-portable"),
    not(feature = "std")
))]
#[inline]
fn x86_has_avx2() -> bool {
    false
}

#[cfg(all(
    feature = "simd",
    target_endian = "little",
    target_arch = "x86_64",
    not(feature = "force-portable"),
    not(feature = "std")
))]
#[inline]
fn x86_has_avx512() -> bool {
    false
}
