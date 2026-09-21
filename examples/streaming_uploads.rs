//! Hashing many uploads that are still in flight, with `Md5Engine::update_many`.
//!
//! `hash_many` needs complete messages. An upload server never has one: at any moment it holds
//! only the latest chunk of each connection, and every connection's MD5 chaining value has to
//! survive until its next chunk arrives. `update_many` is the batch API for that shape.
//!
//! The loop below is the pattern to copy:
//!
//! 1. Keep one `Md5State` per live upload in a contiguous `Vec`, with the per-upload bookkeeping
//!    in a parallel `Vec`. Retire a finished upload with `swap_remove` on both.
//! 2. Once per tick, take one chunk from every upload that has data and make a single
//!    `update_many` call. Uploads with nothing to give pass an empty slice; they cost nothing and
//!    do not push the others out of SIMD.
//! 3. Hand out block-aligned chunks (a multiple of 64 bytes; 64 KiB to 1 MiB works well) so the
//!    streams stay on a block boundary and can share registers. Only the last chunk is odd.
//! 4. Aim for at least `Md5Engine::lanes()` live uploads per call. Below four, `update_many`
//!    quietly does what a per-stream loop would.
//!
//! The demo drives the same synthetic workload twice, once with a per-upload `Md5State::update`
//! loop and once with `update_many`, checks every digest against the one-shot `digest`, and
//! prints aggregate throughput per concurrency level.
//!
//! ```text
//! cargo run --release --example streaming_uploads
//! ```

use md5_simd::{Md5Engine, Md5State, digest};
use std::time::Instant;

/// Bytes handed to the hasher per upload per tick.
const CHUNK: usize = 256 * 1024;

/// One upload as the server sees it: a body that arrives in pieces.
struct Upload<'a> {
    id: usize,
    body: &'a [u8],
    received: usize,
}

impl<'a> Upload<'a> {
    /// The bytes that arrived since the last tick. Every 7th tick of an upload is a stall.
    fn next_chunk(&mut self, tick: usize) -> &'a [u8] {
        if (tick + self.id).is_multiple_of(7) {
            return &[];
        }
        let body: &'a [u8] = self.body;
        let end = (self.received + CHUNK).min(body.len());
        let chunk = &body[self.received..end];
        self.received = end;
        chunk
    }

    fn is_complete(&self) -> bool {
        self.received == self.body.len()
    }
}

/// Run all `bodies` through the server loop with at most `concurrency` uploads in flight.
/// `batched` selects `update_many`; otherwise each state is updated on its own.
fn serve(
    engine: Md5Engine,
    bodies: &[Vec<u8>],
    concurrency: usize,
    batched: bool,
) -> Vec<[u8; 16]> {
    let mut digests = vec![[0u8; 16]; bodies.len()];
    let mut uploads: Vec<Upload<'_>> = Vec::with_capacity(concurrency);
    let mut states: Vec<Md5State> = Vec::with_capacity(concurrency);
    let mut chunks: Vec<&[u8]> = Vec::with_capacity(concurrency);
    let mut queued = bodies.iter().enumerate();

    for tick in 0.. {
        // Admit new uploads up to the concurrency limit.
        while uploads.len() < concurrency {
            let Some((id, body)) = queued.next() else {
                break;
            };
            uploads.push(Upload {
                id,
                body,
                received: 0,
            });
            states.push(Md5State::new());
        }
        if uploads.is_empty() {
            break;
        }

        // One chunk per live upload, then one call for all of them.
        chunks.clear();
        chunks.extend(uploads.iter_mut().map(|upload| upload.next_chunk(tick)));
        if batched {
            engine.update_many(&mut states, &chunks);
        } else {
            for (state, chunk) in states.iter_mut().zip(&chunks) {
                state.update(chunk);
            }
        }

        // Retire finished uploads; `swap_remove` keeps both vectors dense and in step.
        let mut slot = 0;
        while slot < uploads.len() {
            if uploads[slot].is_complete() {
                digests[uploads[slot].id] = states[slot].finalize();
                uploads.swap_remove(slot);
                states.swap_remove(slot);
            } else {
                slot += 1;
            }
        }
    }
    digests
}

/// Deterministic pseudo-random bodies: most around `typical` bytes, a few tiny, none aligned.
fn make_bodies(count: usize, typical: usize) -> Vec<Vec<u8>> {
    let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    (0..count)
        .map(|i| {
            let len = if i % 9 == 0 {
                next() as usize % 4096
            } else {
                typical / 2 + next() as usize % typical
            };
            (0..len).map(|_| next() as u8).collect()
        })
        .collect()
}

fn main() {
    // Small enough for an unoptimized CI run, large enough to time in release.
    let (count, typical) = if cfg!(debug_assertions) {
        (24, 400 * 1024)
    } else {
        (192, 4 * 1024 * 1024)
    };
    let engine = Md5Engine::new();
    let bodies = make_bodies(count, typical);
    let total: usize = bodies.iter().map(Vec::len).sum();
    let expected: Vec<[u8; 16]> = bodies.iter().map(|body| digest(body)).collect();

    println!(
        "backend={} simd={} lanes={}",
        engine.backend_name(),
        engine.simd_name(),
        engine.lanes()
    );
    println!(
        "{count} uploads, {:.1} MiB in total, {} KiB chunks, every 7th tick of an upload stalls",
        total as f64 / 1048576.0,
        CHUNK >> 10
    );
    println!(
        "{:>11} | {:>16} | {:>16} | {:>7}",
        "concurrency", "per-upload MiB/s", "update_many MiB/s", "speedup"
    );

    for concurrency in [1usize, 2, 4, 8, 16, 32, 64] {
        let mut rates = [0f64; 2];
        for (slot, batched) in [false, true].into_iter().enumerate() {
            for _ in 0..if cfg!(debug_assertions) { 1 } else { 3 } {
                let started = Instant::now();
                let digests = serve(engine, &bodies, concurrency, batched);
                let rate = total as f64 / started.elapsed().as_secs_f64() / 1048576.0;
                assert_eq!(
                    digests, expected,
                    "concurrency={concurrency} batched={batched}"
                );
                rates[slot] = rates[slot].max(rate);
            }
        }
        println!(
            "{concurrency:>11} | {:>16.0} | {:>16.0} | {:>6.2}x",
            rates[0],
            rates[1],
            rates[1] / rates[0]
        );
    }
    println!("all {count} digests match the one-shot digest in every run");
}
