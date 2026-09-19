//! Batch hashing and scalar incremental multi-stream operations.
//!
//! MD5 is for legacy interoperability and non-adversarial checksums only.

use crate::backend;
use crate::md5::Md5;
use crate::multibuf;
use crate::simd;
use crate::state::Md5State;
#[cfg(feature = "std")]
use alloc::string::String;

/// Batch MD5 engine. Construct once and reuse.
#[derive(Clone, Copy, Debug, Default)]
pub struct Md5Engine;

impl Md5Engine {
    /// Create an engine using the process-default backend.
    #[inline]
    pub const fn new() -> Self {
        Self
    }

    /// Active single-stream backend name.
    #[inline]
    pub fn backend_name(self) -> &'static str {
        backend::backend_name()
    }

    /// Multi-buffer kernel label (`none` unless feature `simd` is on and active).
    #[inline]
    pub fn simd_name(self) -> &'static str {
        simd::simd_name()
    }

    /// Effective batch width for `hash_many`.
    ///
    /// Maximum usable SIMD width, or 2 for the experimental scalar pair
    /// path, or 1 for sequential hashing. Individual calls may use fewer lanes.
    #[inline]
    pub fn lanes(self) -> usize {
        let s = simd::runtime_lanes();
        if s > 1 {
            s
        } else if multibuf::pair_path_active() {
            2
        } else {
            1
        }
    }

    /// Hash independent messages into `outputs`.
    ///
    /// Contiguous equal-length runs may use SIMD when enabled and available.
    /// Other inputs use the active single-stream backend. Order is preserved;
    /// extra output slots are left unchanged. No allocation or thread spawning.
    ///
    /// # Panics
    /// Panics if `outputs.len() < inputs.len()`.
    #[inline]
    pub fn hash_many(self, inputs: &[&[u8]], outputs: &mut [[u8; 16]]) {
        simd::hash_many_dispatch(inputs, outputs);
    }

    /// Hash independent messages to lowercase hex (ETag-shaped).
    ///
    /// # Panics
    /// Panics if `outputs.len() < inputs.len()`.
    #[cfg(feature = "std")]
    pub fn hash_many_hex(self, inputs: &[&[u8]], outputs: &mut [String]) {
        assert!(
            outputs.len() >= inputs.len(),
            "outputs.len() ({}) < inputs.len() ({})",
            outputs.len(),
            inputs.len()
        );
        let mut digests = std::vec![[0u8; 16]; inputs.len()];
        self.hash_many(inputs, &mut digests);
        for (d, output) in digests.iter().zip(outputs.iter_mut()) {
            *output = crate::md5::hex_encode(d);
        }
    }

    /// Hash one message via the active single-stream path.
    #[inline]
    pub fn hash_one(self, input: &[u8]) -> [u8; 16] {
        backend::hash(input)
    }

    /// Increment several independent streams through the scalar backend.
    ///
    /// # Panics
    /// Panics if `states.len() != inputs.len()`.
    #[inline]
    pub fn update_many(self, states: &mut [Md5State], inputs: &[&[u8]]) {
        assert_eq!(states.len(), inputs.len());
        for (state, input) in states.iter_mut().zip(inputs.iter()) {
            state.update(input);
        }
    }

    /// Snapshot-finalize several streams sequentially (states are not consumed).
    ///
    /// # Panics
    /// Panics if `outputs.len() < states.len()`.
    #[inline]
    pub fn finalize_many(self, states: &[Md5State], outputs: &mut [[u8; 16]]) {
        assert!(
            outputs.len() >= states.len(),
            "outputs.len() ({}) < states.len() ({})",
            outputs.len(),
            states.len()
        );
        for (state, output) in states.iter().zip(outputs.iter_mut()) {
            *output = state.finalize();
        }
    }

    /// Streaming hasher on the active backend.
    #[inline]
    pub fn hasher(self) -> Md5 {
        Md5::new()
    }
}

/// Hash many independent messages with the process-default engine.
#[inline]
pub fn md5_many(inputs: &[&[u8]], outputs: &mut [[u8; 16]]) {
    Md5Engine::new().hash_many(inputs, outputs);
}
