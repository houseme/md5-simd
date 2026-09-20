//! Shared final-block construction and one-shot framing.

use crate::compress::state_to_bytes;
use crate::consts::STATE_INIT;

/// Build final blocks from a total byte count (including `tail`) and a tail
/// shorter than 64 bytes. Complete preceding blocks have already been compressed.
///
/// Returns how many blocks were written (1 or 2).
#[inline(always)]
pub fn build_final_blocks(total_bytes: u64, tail: &[u8], blocks: &mut [[u8; 64]; 2]) -> usize {
    debug_assert!(tail.len() < 64);
    let bit_len = total_bytes.wrapping_mul(8).to_le_bytes();
    if tail.len() <= 55 {
        // One block: do not touch `blocks[1]`.
        blocks[0] = [0u8; 64];
        blocks[0][..tail.len()].copy_from_slice(tail);
        blocks[0][tail.len()] = 0x80;
        blocks[0][56..64].copy_from_slice(&bit_len);
        1
    } else {
        blocks[0] = [0u8; 64];
        blocks[1] = [0u8; 64];
        blocks[0][..tail.len()].copy_from_slice(tail);
        blocks[0][tail.len()] = 0x80;
        blocks[1][56..64].copy_from_slice(&bit_len);
        2
    }
}

/// One-shot framing: full blocks + RFC padding through `compress`.
#[inline(always)]
pub fn hash_with<F>(input: &[u8], mut compress: F) -> [u8; 16]
where
    F: FnMut(&mut [u32; 4], &[u8; 64]),
{
    // Empty message: single padding block; skip generic multi-block setup.
    if input.is_empty() {
        return finalize_with(STATE_INIT, 0, &[], compress);
    }
    let mut state = STATE_INIT;
    let (blocks, tail) = input.as_chunks::<64>();
    for block in blocks {
        compress(&mut state, block);
    }
    finalize_with(state, input.len() as u64, tail, compress)
}

/// Finalize from (state, total_bytes, tail) using `compress`.
#[inline(always)]
pub fn finalize_with<F>(state: [u32; 4], total_bytes: u64, tail: &[u8], mut compress: F) -> [u8; 16]
where
    F: FnMut(&mut [u32; 4], &[u8; 64]),
{
    let mut st = state;
    let mut blocks = [[0u8; 64]; 2];
    let used = build_final_blocks(total_bytes, tail, &mut blocks);
    for block in &blocks[..used] {
        compress(&mut st, block);
    }
    state_to_bytes(st)
}
