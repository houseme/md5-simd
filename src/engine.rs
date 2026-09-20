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
#[cfg(feature = "std")]
use alloc::vec::Vec;

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

    /// Hash messages after grouping equal lengths while restoring output order.
    ///
    /// This is for callers that own an object scheduling queue and can pay one
    /// allocation plus an index sort to expose non-adjacent equal-length runs.
    /// The original [`Self::hash_many`] remains allocation-free and preserves
    /// the input order during dispatch. Grouping is used only when the sorted
    /// workload contains a SIMD-sized run; otherwise this falls back to
    /// [`Self::hash_many`].
    ///
    /// # Panics
    /// Panics if `outputs.len() < inputs.len()`.
    #[cfg(feature = "std")]
    pub fn hash_many_grouped(self, inputs: &[&[u8]], outputs: &mut [[u8; 16]]) {
        assert!(
            outputs.len() >= inputs.len(),
            "outputs.len() ({}) < inputs.len() ({})",
            outputs.len(),
            inputs.len()
        );
        if inputs.len() < 4 || has_simd_sized_adjacent_run(inputs) {
            self.hash_many(inputs, outputs);
            return;
        }

        let mut order: Vec<usize> = (0..inputs.len()).collect();
        order.sort_unstable_by_key(|&index| inputs[index].len());
        let mut max_run = 1usize;
        let mut run = 1usize;
        for pair in order.windows(2) {
            if inputs[pair[0]].len() == inputs[pair[1]].len() {
                run += 1;
                max_run = max_run.max(run);
            } else {
                run = 1;
            }
        }
        if max_run < 4 {
            self.hash_many(inputs, outputs);
            return;
        }

        let sorted_inputs: Vec<&[u8]> = order.iter().map(|&index| inputs[index]).collect();
        let mut sorted_outputs = vec![[0u8; 16]; inputs.len()];
        self.hash_many(&sorted_inputs, &mut sorted_outputs);
        for (sorted_index, &original_index) in order.iter().enumerate() {
            outputs[original_index] = sorted_outputs[sorted_index];
        }
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

#[cfg(feature = "std")]
fn has_simd_sized_adjacent_run(inputs: &[&[u8]]) -> bool {
    let mut run = 1usize;
    for pair in inputs.windows(2) {
        if pair[0].len() == pair[1].len() {
            run += 1;
            if run >= 4 {
                return true;
            }
        } else {
            run = 1;
        }
    }
    false
}

/// Hash many independent messages with the process-default engine.
#[inline]
pub fn md5_many(inputs: &[&[u8]], outputs: &mut [[u8; 16]]) {
    Md5Engine::new().hash_many(inputs, outputs);
}
