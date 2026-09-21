//! Incremental multi-stream `update_many` against the independent RustCrypto oracle.
//!
//! `update_many` advances live streams through the SIMD kernels, so every shape
//! that changes which streams share a register is checked here: whole groups in
//! lockstep, streams that stall or run short, arbitrary unaligned chunking, more
//! streams than one scheduling window, and single-stream updates mixed in.

use md5::Digest as _;
use md5_simd::{Md5Engine, Md5State};

mod common;
use common::run_with_large_stack;

fn oracle(data: &[u8]) -> [u8; 16] {
    md5::Md5::digest(data).into()
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next() as u8).collect()
    }
}

fn assert_streams(states: &[Md5State], streams: &[Vec<u8>], what: &str) {
    for (i, (state, data)) in states.iter().zip(streams).enumerate() {
        assert_eq!(
            state.bytes_hashed(),
            data.len() as u64,
            "{what}: stream {i} byte count"
        );
        assert_eq!(
            state.finalize(),
            oracle(data),
            "{what}: stream {i} len {}",
            data.len()
        );
    }
}

/// Feed `streams` in rounds; `take(round, stream, remaining)` decides each chunk.
fn drive(streams: &[Vec<u8>], mut take: impl FnMut(usize, usize, usize) -> usize) -> Vec<Md5State> {
    let engine = Md5Engine::new();
    let mut states = vec![Md5State::new(); streams.len()];
    let mut off = vec![0usize; streams.len()];
    let mut round = 0;
    while off.iter().zip(streams).any(|(o, d)| *o < d.len()) {
        let chunks: Vec<&[u8]> = streams
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let n = take(round, i, d.len() - off[i]).min(d.len() - off[i]);
                let chunk = &d[off[i]..off[i] + n];
                off[i] += n;
                chunk
            })
            .collect();
        engine.update_many(&mut states, &chunks);
        round += 1;
        assert!(round < 1_000_000, "driver made no progress");
    }
    states
}

#[test]
fn lockstep_block_aligned_chunks() {
    run_with_large_stack(|| {
        let mut rng = Rng(0x2545_f491_4f6c_dd1d);
        // Every group shape: below the SIMD threshold, partial groups, exact
        // groups for 4/8/16-lane kernels, and more than one scheduling window.
        for n in [
            1usize, 3, 4, 5, 7, 8, 9, 15, 16, 17, 31, 32, 33, 64, 65, 100,
        ] {
            let base = 64 * (1 + rng.below(40));
            let streams: Vec<Vec<u8>> = (0..n).map(|_| rng.bytes(base + 4096)).collect();
            let states = drive(&streams, |_, _, _| base.min(1024));
            assert_streams(&states, &streams, &format!("lockstep n={n}"));
        }
    });
}

#[test]
fn near_equal_lengths_with_odd_tails() {
    run_with_large_stack(|| {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        for n in [4usize, 8, 16, 24, 40] {
            let base = rng.below(60_000);
            let streams: Vec<Vec<u8>> = (0..n)
                .map(|_| {
                    let l = base + rng.below(300);
                    rng.bytes(l)
                })
                .collect();
            let states = drive(&streams, |_, _, _| 16384);
            assert_streams(&states, &streams, &format!("near-equal n={n}"));
        }
    });
}

#[test]
fn stalled_and_short_streams_leave_the_batch() {
    run_with_large_stack(|| {
        let mut rng = Rng(0x0bad_cafe_1234_5678);
        for n in [8usize, 19, 32] {
            let streams: Vec<Vec<u8>> = (0..n)
                .map(|i| rng.bytes(if i % 5 == 0 { 40 + i } else { 30_000 + 7 * i }))
                .collect();
            let states = drive(&streams, |round, i, _| match (round + i) % 4 {
                0 => 0,       // stalled this round
                1 => 100 + i, // short and unaligned
                _ => 4096,
            });
            assert_streams(&states, &streams, &format!("ragged n={n}"));
        }
    });
}

#[test]
fn arbitrary_chunking_and_lengths() {
    run_with_large_stack(|| {
        let mut rng = Rng(0xdead_beef_cafe_f00d);
        for round in 0..30 {
            let n = 1 + rng.below(70);
            let streams: Vec<Vec<u8>> = (0..n)
                .map(|_| {
                    let l = match rng.below(4) {
                        0 => rng.below(130),
                        1 => rng.below(5_000),
                        2 => 8192 + rng.below(70),
                        _ => rng.below(40_000),
                    };
                    rng.bytes(l)
                })
                .collect();
            let mut pick = Rng(round as u64 + 1);
            let states = drive(&streams, |_, _, _| match pick.below(5) {
                0 => 0,
                1 => 1 + pick.below(63),
                2 => 64,
                3 => pick.below(3000),
                _ => 8192,
            });
            assert_streams(&states, &streams, &format!("chaotic round={round} n={n}"));
        }
    });
}

#[test]
fn mixes_with_single_stream_updates_and_snapshots() {
    run_with_large_stack(|| {
        let mut rng = Rng(0x1357_9bdf_2468_ace0);
        let engine = Md5Engine::new();
        let n = 12;
        let streams: Vec<Vec<u8>> = (0..n).map(|i| rng.bytes(20_000 + 3 * i)).collect();
        let mut states = vec![Md5State::new(); n];
        let mut off = 0;
        for step in 0.. {
            if off >= 20_000 + 3 * n {
                break;
            }
            let len = [777usize, 4096, 64, 5000][step % 4];
            let chunks: Vec<&[u8]> = streams
                .iter()
                .map(|d| &d[off.min(d.len())..(off + len).min(d.len())])
                .collect();
            if step % 3 == 0 {
                states
                    .iter_mut()
                    .zip(&chunks)
                    .for_each(|(s, c)| s.update(c));
            } else {
                engine.update_many(&mut states, &chunks);
            }
            off += len;
            // A snapshot must not disturb the stream it was taken from.
            let seen = off.min(streams[0].len());
            assert_eq!(
                states[0].finalize(),
                oracle(&streams[0][..seen]),
                "snapshot at {seen}"
            );
        }
        assert_streams(&states, &streams, "mixed single/batch");
    });
}

#[test]
fn empty_inputs_and_empty_batch_are_no_ops() {
    let engine = Md5Engine::new();
    engine.update_many(&mut [], &[]);
    let mut states = vec![Md5State::new(); 9];
    let nothing: [&[u8]; 9] = [&[]; 9];
    engine.update_many(&mut states, &nothing);
    assert!(
        states
            .iter()
            .all(|s| s.is_empty() && s.finalize() == oracle(b""))
    );
}

#[test]
#[should_panic(expected = "states.len()")]
fn length_mismatch_panics() {
    Md5Engine::new().update_many(&mut [Md5State::new(); 2], &[b"a"]);
}
